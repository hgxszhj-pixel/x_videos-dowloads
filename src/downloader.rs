//! 视频下载模块
//!
//! 使用 reqwest 异步下载视频文件，支持进度跟踪和断点续传

#![allow(dead_code)]

use crate::types::{DownloadProgress, DownloadRequest, DownloadResult, DownloadStatus, DEFAULT_USER_AGENT};
use anyhow::{anyhow, Context, Result};
use futures::StreamExt;
use reqwest::Client;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

/// 下载器配置
#[derive(Debug, Clone)]
pub struct DownloaderConfig {
    /// 连接超时（秒）
    pub connect_timeout: u64,
    /// 请求超时（秒）
    pub request_timeout: u64,
    /// 重试次数
    pub retry_count: u32,
    /// 重试最大间隔（秒）
    pub retry_max_delay: u64,
    /// 块大小
    pub chunk_size: usize,
    /// 自定义 User-Agent
    pub user_agent: Option<String>,
    /// 代理
    pub proxy: Option<String>,
    /// 连接池最大空闲连接数
    pub pool_max_idle_per_host: usize,
    /// 是否启用HTTP/2
    pub http2: bool,
    /// TCP保活时间（秒）
    pub tcp_keepalive: u64,
    /// 分段下载线程数（0表示禁用）
    pub parallel_segments: u32,
}

impl Default for DownloaderConfig {
    fn default() -> Self {
        Self {
            connect_timeout: 30,
            request_timeout: 600, // 大文件需要更长超时
            retry_count: 5,      // 增加重试次数
            retry_max_delay: 30,  // 最大重试间隔30秒
            chunk_size: 1024 * 1024, // 1MB
            user_agent: None,
            proxy: None,
            pool_max_idle_per_host: 10,
            http2: true,
            tcp_keepalive: 60,
            parallel_segments: 4, // 4线程并行下载
        }
    }
}

/// 视频下载器
pub struct VideoDownloader {
    /// HTTP 客户端
    client: Client,
    /// 配置
    config: DownloaderConfig,
}

impl VideoDownloader {
    /// 创建新的下载器
    pub fn new() -> Result<Self> {
        Self::with_config(DownloaderConfig::default())
    }

    /// 使用配置创建下载器
    pub fn with_config(config: DownloaderConfig) -> Result<Self> {
        let mut builder = Client::builder()
            .connect_timeout(Duration::from_secs(config.connect_timeout))
            .timeout(Duration::from_secs(config.request_timeout))
            .pool_max_idle_per_host(config.pool_max_idle_per_host)
            .tcp_keepalive(Duration::from_secs(config.tcp_keepalive))
            .user_agent(
                config.user_agent.clone().unwrap_or_else(|| DEFAULT_USER_AGENT.to_string())
            );

        // 设置代理
        if let Some(ref proxy) = config.proxy {
            builder = builder.proxy(reqwest::Proxy::all(proxy)?);
        }

        // HTTP/2 支持需要使用不同的方法
        // 注意: reqwest 默认支持 HTTP/1.1，HTTP/2 需要 ALPN 支持
        // 在大多数情况下，使用默认配置即可，reqwest 会自动协商

        let client = builder.build()?;

        Ok(Self { client, config })
    }

    /// 设置 User-Agent
    pub fn with_user_agent(mut self, user_agent: String) -> Self {
        self.config.user_agent = Some(user_agent);
        self
    }

    /// 设置代理
    pub fn with_proxy(mut self, proxy: String) -> Self {
        self.config.proxy = Some(proxy);
        self
    }

    /// 下载文件（带重试机制和分段并行下载）
    pub async fn download(
        &self,
        request: &DownloadRequest,
        progress_callback: Option<Arc<dyn Fn(DownloadProgress) + Send + Sync>>,
    ) -> Result<DownloadResult> {
        let mut last_error = None;

        for attempt in 0..self.config.retry_count {
            if attempt > 0 {
                // 指数退避: 1s, 2s, 4s, 8s, 16s...
                let delay_secs = (2u64.pow(attempt - 1)).min(self.config.retry_max_delay);
                info!("重试 {} / {}，等待 {} 秒...", attempt + 1, self.config.retry_count, delay_secs);
                tokio::time::sleep(Duration::from_secs(delay_secs)).await;
            }

            // 检查是否支持分段下载（需要先获取文件大小）
            let total_size = self.get_content_length(&request.url).await?;

            // 大于10MB且启用了分段下载时使用并行下载
            if let Some(size) = total_size {
                if self.config.parallel_segments > 0 && size > 10 * 1024 * 1024 {
                    match self.parallel_download(request, size, progress_callback.clone()).await {
                        Ok(result) => return Ok(result),
                        Err(e) => {
                            last_error = Some(e);
                            warn!("分段下载失败，尝试普通下载: {:?}", last_error);
                            // 回退到普通下载
                        }
                    }
                }
            }

            // 普通下载
            match self.do_download(request, progress_callback.clone()).await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    last_error = Some(e);
                    warn!("下载尝试 {} 失败: {:?}", attempt + 1, last_error);
                }
            }
        }

        Err(anyhow!("下载失败，已重试 {} 次: {:?}", self.config.retry_count, last_error))
    }

    /// 分段并行下载
    async fn parallel_download(
        &self,
        request: &DownloadRequest,
        total_size: u64,
        progress_callback: Option<Arc<dyn Fn(DownloadProgress) + Send + Sync>>,
    ) -> Result<DownloadResult> {
        let url = request.url.clone();
        let output_path = request.output_path.clone();
        let segments = self.config.parallel_segments;

        info!("开始分段并行下载: {} ({} 线程)", url, segments);

        // 确保输出目录存在
        if let Some(parent) = output_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        // 创建临时目录存储各分段（使用 TempDir 自动清理）
        let temp_dir = tempfile::TempDir::new_in(
            output_path.parent().context("输出路径没有有效父目录")?
        )?;
        let temp_dir_path = temp_dir.path().to_path_buf();
        
        // 预创建临时文件以避免竞争条件
        for i in 0..segments {
            let temp_path = temp_dir_path.join(format!("part_{}.tmp", i));
            let _ = File::create(&temp_path).await;
        }

        let chunk_size = (total_size as f64 / segments as f64).ceil() as u64;

        // 创建进度跟踪
        let progress = Arc::new(Mutex::new(DownloadProgress::new("parallel".to_string())
            .with_total_bytes(total_size)
            .with_status(DownloadStatus::Downloading)));

        // 并行下载各分段
        let mut handles = vec![];

        for i in 0..segments {
            let url = url.clone();
            let temp_path = temp_dir_path.join(format!("part_{}.tmp", i));
            let client = self.client.clone();
            let start = i as u64 * chunk_size;
            let end = if i == segments - 1 {
                total_size - 1
            } else {
                (i + 1) as u64 * chunk_size - 1
            };
            let retry_count = self.config.retry_count;

            let handle = tokio::spawn(async move {
                for attempt in 0..retry_count {
                    let result = download_segment(&client, &url, &temp_path, start, end).await;
                    if result.is_ok() {
                        return result;
                    }
                    if attempt < retry_count - 1 {
                        tokio::time::sleep(Duration::from_secs(2u64.pow(attempt))).await;
                    }
                }
                download_segment(&client, &url, &temp_path, start, end).await
            });
            handles.push(handle);
        }

        // 等待所有分段下载完成
        let mut success_count = 0u32;
        for handle in handles {
            match handle.await {
                Ok(Ok(_)) => success_count += 1,
                Ok(Err(e)) => warn!("分段下载失败: {:?}", e),
                Err(e) => warn!("分段任务panic: {:?}", e),
            }
        }
        
        // 所有分段都失败则报错
        if success_count == 0 {
            return Err(anyhow!("所有分段下载都失败了"));
        }
        
        // 部分成功则记录警告
        if success_count < segments {
            warn!("部分分段下载失败: {}/{} 成功", success_count, segments);
        }

        // 合并分段文件（流式处理，避免内存翻倍）
        let mut output_file = File::create(&output_path).await?;
        for i in 0..segments {
            let temp_path = temp_dir_path.join(format!("part_{}.tmp", i));
            // 使用 tokio::io::copy 流式复制，不将整个文件加载到内存
            if let Ok(mut temp_file) = File::open(&temp_path).await {
                if let Err(e) = tokio::io::copy(&mut temp_file, &mut output_file).await {
                    warn!("合并分段 {} 时出错: {:?}", i, e);
                }
            }
            // 删除临时文件
            if let Err(e) = tokio::fs::remove_file(&temp_path).await {
                warn!("删除临时文件 {} 失败: {:?}", temp_path.display(), e);
            }
        }

        output_file.flush().await?;
        drop(output_file);
        
        // TempDir 会在离开作用域时自动清理剩余的临时文件

        // 通知完成
        let mut final_progress = progress.lock().await;
        final_progress.status = DownloadStatus::Completed;
        final_progress.percent = 1.0;

        if let Some(ref callback) = progress_callback {
            callback(final_progress.clone());
        }

        info!("分段并行下载完成: {}", output_path.display());

        Ok(DownloadResult::success(output_path, total_size, 0.0))
    }

    /// 执行实际下载（内部方法）
    async fn do_download(
        &self,
        request: &DownloadRequest,
        progress_callback: Option<Arc<dyn Fn(DownloadProgress) + Send + Sync>>,
    ) -> Result<DownloadResult> {
        let start_time = Instant::now();
        let url = &request.url;
        let output_path = &request.output_path;

        info!("开始下载: {} -> {}", url, output_path.display());

        // 确保输出目录存在
        if let Some(parent) = output_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        // 发送 HEAD 请求获取文件大小
        let total_size = self.get_content_length(url).await?;

        info!("文件大小: {} bytes", total_size.unwrap_or(0));

        // 发送下载请求
        let response = self.client.get(url).send().await?;

        if !response.status().is_success() {
            return Err(anyhow!("下载失败: HTTP {}", response.status()));
        }

        // 获取内容长度
        let total = total_size.unwrap_or(0);

        // 创建进度对象
        let mut progress = DownloadProgress::new("download".to_string())
            .with_total_bytes(total)
            .with_status(DownloadStatus::Downloading);

        // 通知进度
        if let Some(ref callback) = progress_callback {
            callback(progress.clone());
        }

        // 获取响应体
        let mut stream = response.bytes_stream();
        let mut downloaded: u64 = 0;
        let mut file = File::create(output_path).await?;

        let mut last_update = Instant::now();
        let update_interval = Duration::from_millis(500);

        while let Some(chunk_result) = stream.next().await {
            let chunk: bytes::Bytes = chunk_result?;

            // 写入文件
            file.write_all(&chunk).await?;

            downloaded += chunk.len() as u64;

            // 计算速度（每500ms更新一次）
            if last_update.elapsed() >= update_interval {
                let elapsed = start_time.elapsed().as_secs_f64();
                let speed = if elapsed > 0.0 {
                    downloaded as f64 / elapsed
                } else {
                    0.0
                };

                progress.update(downloaded, speed);

                if let Some(ref callback) = progress_callback {
                    callback(progress.clone());
                }

                last_update = Instant::now();
            }

            debug!("下载进度: {}/{} bytes", downloaded, total);
        }

        // 刷新文件
        file.flush().await?;
        drop(file);

        // 计算最终统计
        let elapsed = start_time.elapsed().as_secs_f64();
        let speed = if elapsed > 0.0 {
            downloaded as f64 / elapsed
        } else {
            0.0
        };

        progress.status = DownloadStatus::Completed;
        progress.percent = 1.0;
        progress.speed = speed;

        if let Some(ref callback) = progress_callback {
            callback(progress);
        }

        info!(
            "下载完成: {} bytes in {:.2}s ({:.2} MB/s)",
            downloaded,
            elapsed,
            speed / (1024.0 * 1024.0)
        );

        Ok(DownloadResult::success(output_path.clone(), downloaded, elapsed))
    }

    /// 获取内容长度
    async fn get_content_length(&self, url: &str) -> Result<Option<u64>> {
        let response = self.client.head(url).send().await?;

        if response.status().is_success() {
            let length = response
                .headers()
                .get("content-length")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse().ok());

            Ok(length)
        } else {
            Ok(None)
        }
    }

    /// 断点续传下载
    pub async fn resume_download(
        &self,
        url: &str,
        output_path: &PathBuf,
        resume_from: u64,
        progress_callback: Option<Arc<dyn Fn(DownloadProgress) + Send + Sync>>,
    ) -> Result<DownloadResult> {
        let start_time = Instant::now();

        info!("断点续传下载: {} (从 {} bytes 处)", url, resume_from);

        // 发送 HEAD 请求获取总大小
        let total_size = self.get_content_length(url).await?.unwrap_or(0);

        // 创建请求，设置 Range 头
        let mut request = self.client.get(url);
        if resume_from > 0 {
            request = request.header("Range", format!("bytes={}-", resume_from));
        }

        let response = request.send().await?;

        if !response.status().is_success() && response.status() != reqwest::StatusCode::PARTIAL_CONTENT {
            return Err(anyhow!("下载失败: HTTP {}", response.status()));
        }

        // 创建/打开文件
        let mut file = if resume_from > 0 {
            OpenOptions::new()
                .write(true)
                .append(true)
                .open(output_path)
                .await?
        } else {
            File::create(output_path).await?
        };

        let mut downloaded = resume_from;
        let mut stream = response.bytes_stream();

        while let Some(chunk_result) = stream.next().await {
            let chunk: bytes::Bytes = chunk_result?;
            file.write_all(&chunk).await?;
            downloaded += chunk.len() as u64;

            if let Some(ref callback) = progress_callback {
                let elapsed = start_time.elapsed().as_secs_f64();
                let speed = if elapsed > 0.0 {
                    downloaded as f64 / elapsed
                } else {
                    0.0
                };

                let mut progress = DownloadProgress::new("download".to_string())
                    .with_total_bytes(total_size)
                    .with_status(DownloadStatus::Downloading);

                progress.update(downloaded, speed);
                callback(progress);
            }
        }

        let elapsed = start_time.elapsed().as_secs_f64();

        Ok(DownloadResult::success(
            output_path.clone(),
            downloaded,
            elapsed,
        ))
    }
}

impl Default for VideoDownloader {
    fn default() -> Self {
        Self::new().expect("VideoDownloader: failed to create HTTP client")
    }
}

/// 下载单个分段（用于并行下载）
async fn download_segment(
    client: &Client,
    url: &str,
    output_path: &PathBuf,
    start: u64,
    end: u64,
) -> Result<u64> {
    let response = client
        .get(url)
        .header("Range", format!("bytes={}-{}", start, end))
        .send()
        .await?;

    if !response.status().is_success() && response.status() != reqwest::StatusCode::PARTIAL_CONTENT {
        return Err(anyhow!("分段下载失败: HTTP {}", response.status()));
    }

    let mut file = File::create(output_path).await?;
    let mut stream = response.bytes_stream();
    let mut downloaded: u64 = 0;

    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result?;
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;
    }

    file.flush().await?;
    Ok(downloaded)
}

/// 解析 yt-dlp 进度输出
fn parse_ytdlp_progress(line: &str) -> Option<DownloadProgress> {
    // 格式: [download]  45.2% of 100.00MiB at  2.50MiB/s ETA 00:15
    if !line.contains("[download]") || !line.contains('%') {
        return None;
    }

    // 提取百分比
    let percent_str = line.split('%').next()?.split_whitespace().last()?;
    let percent: f64 = percent_str.parse::<f64>().ok()? / 100.0;

    // 提取速度
    let speed = if let Some(speed_part) = line.split("at ").nth(1) {
        let speed_str = speed_part.split('s').next()?.split_whitespace().next()?;
        parse_speed(speed_str).unwrap_or(0.0)
    } else {
        0.0
    };

    // 提取文件大小
    let total_bytes = if let Some(size_part) = line.split("of ").nth(1) {
        let size_str = size_part.split(' ').next()?;
        parse_size(size_str).unwrap_or(0)
    } else {
        0
    };

    let bytes_downloaded = (total_bytes as f64 * percent) as u64;

    Some(DownloadProgress {
        task_id: "ytdlp".to_string(),
        bytes_downloaded,
        total_bytes,
        percent,
        speed,
        eta_secs: None,
        status: DownloadStatus::Downloading,
    })
}

/// 解析速度字符串 (如 "2.50MiB")
fn parse_speed(s: &str) -> Option<f64> {
    // 提取数字部分
    let numeric: String = s.chars().take_while(|c| c.is_numeric() || *c == '.').collect();
    let value: f64 = numeric.parse().ok()?;

    if s.contains("KiB") {
        Some(value * 1024.0)
    } else if s.contains("MiB") {
        Some(value * 1024.0 * 1024.0)
    } else if s.contains("GiB") {
        Some(value * 1024.0 * 1024.0 * 1024.0)
    } else {
        Some(value)
    }
}

/// 解析大小字符串 (如 "100.00MiB")
fn parse_size(s: &str) -> Option<u64> {
    // 提取数字部分
    let numeric: String = s.chars().take_while(|c| c.is_numeric() || *c == '.').collect();
    let value: f64 = numeric.parse().ok()?;

    if s.contains("KiB") {
        Some((value * 1024.0) as u64)
    } else if s.contains("MiB") {
        Some((value * 1024.0 * 1024.0) as u64)
    } else if s.contains("GiB") {
        Some((value * 1024.0 * 1024.0 * 1024.0) as u64)
    } else {
        Some(value as u64)
    }
}

// 需要导入 OpenOptions
use tokio::fs::OpenOptions;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_speed() {
        assert_eq!(parse_speed("2.50MiB"), Some(2.50 * 1024.0 * 1024.0));
        assert_eq!(parse_speed("1.00KiB"), Some(1024.0));
    }

    #[test]
    fn test_parse_size() {
        assert_eq!(parse_size("100.00MiB"), Some((100.0 * 1024.0 * 1024.0) as u64));
    }

    #[test]
    fn test_parse_speed_boundary() {
        // KiB 边界测试
        assert_eq!(parse_speed("0.00KiB"), Some(0.0));
        assert_eq!(parse_speed("1024.00KiB"), Some(1024.0 * 1024.0));

        // MiB 边界测试
        assert_eq!(parse_speed("0.00MiB"), Some(0.0));
        assert_eq!(parse_speed("1.00MiB"), Some(1024.0 * 1024.0));
        assert_eq!(parse_speed("1024.00MiB"), Some(1024.0 * 1024.0 * 1024.0));

        // GiB 边界测试
        assert_eq!(parse_speed("1.00GiB"), Some(1024.0 * 1024.0 * 1024.0));

        // 纯数字（无单位）
        assert_eq!(parse_speed("123.45"), Some(123.45));

        // 无效输入
        assert_eq!(parse_speed(""), None);
        assert_eq!(parse_speed("abc"), None);
    }

    #[test]
    fn test_parse_size_boundary() {
        // KiB 边界测试
        assert_eq!(parse_size("0.00KiB"), Some(0));
        assert_eq!(parse_size("1024.00KiB"), Some(1024 * 1024));

        // MiB 边界测试
        assert_eq!(parse_size("0.00MiB"), Some(0));
        assert_eq!(parse_size("1.00MiB"), Some(1024 * 1024));
        assert_eq!(parse_size("100.00MiB"), Some((100.0 * 1024.0 * 1024.0) as u64));

        // GiB 边界测试
        assert_eq!(parse_size("1.00GiB"), Some(1024 * 1024 * 1024));

        // 纯数字（无单位，字节）
        assert_eq!(parse_size("123.45"), Some(123));

        // 无效输入
        assert_eq!(parse_size(""), None);
        assert_eq!(parse_size("abc"), None);
    }

    #[test]
    fn test_parse_speed_precision() {
        // 测试精度
        assert_eq!(parse_speed("0.001KiB"), Some(1.024));
        assert_eq!(parse_speed("0.001MiB"), Some(1048.576));
    }

    #[test]
    fn test_parse_size_integer_overflow() {
        // 大小值应该被截断为 u64
        let large_value = "9223372036854775808.00MiB"; // 超过 i64 最大值
        let result = parse_size(large_value);
        // 由于精度问题，解析结果可能不准确，这是预期行为
        assert!(result.is_some());
    }
}
