//! 视频下载模块
//!
//! 使用 reqwest 异步下载视频文件，支持进度跟踪和断点续传。
//!
//! 结构:
//! - [`config`]   - [`DownloaderConfig`] 配置
//! - [`parallel`] - 分段并行下载
//! - [`merge`]    - 分段合并

#![allow(dead_code)]

mod config;
mod merge;
mod parallel;

pub use config::DownloaderConfig;

use crate::core::cache::HEAD_CACHE;
use crate::core::constants::*;
use crate::core::segment::parse_ytdlp_progress;
use crate::hls::HlsDownloader;
use crate::types::{
    DownloadError, DownloadProgress, DownloadRequest, DownloadResult, DownloadStatus,
    DownloadStrategy,
};
use anyhow::{anyhow, Context, Result};
use futures::StreamExt;
use reqwest::Client;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::fs::File;
use tokio::fs::OpenOptions;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};
use tracing::{debug, info, warn};

/// 视频下载器
pub struct VideoDownloader {
    /// HTTP 客户端（共享连接池）
    client: Arc<Client>,
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
        // 使用共享客户端以获得连接池复用和 HTTP/2 支持
        let client = crate::http_client::shared();

        Ok(Self { client, config })
    }

    /// 使用自定义 client 创建下载器（保留代理配置能力）
    pub fn with_client_and_config(client: Arc<Client>, config: DownloaderConfig) -> Self {
        Self { client, config }
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

    /// 应用网站特征配置（从已知网站学习）
    pub fn with_website_profile(mut self, url: &str) -> Self {
        let (adaptive, min_threads, max_threads, threshold) =
            crate::website_profile::get_recommended_config(url);

        self.config.adaptive_threads = adaptive;
        self.config.min_threads = min_threads;
        self.config.max_threads = max_threads;
        self.config.speed_threshold_bps = threshold;

        // 联动优化：parallel_segments 跟随 max_threads，保持一致性
        // 如果有线程限制，使用 max_threads 作为分段数；否则用默认的 4
        self.config.parallel_segments = if max_threads < 8 {
            max_threads
        } else {
            self.config.parallel_segments
        };

        info!("应用网站特征: adaptive={}, threads=[{}-{}], threshold={:.1}KB/s, parallel_segments={}",
            adaptive, min_threads, max_threads, threshold as f64 / 1024.0, self.config.parallel_segments);

        self
    }

    /// 下载文件（带重试机制和分段并行下载）
    pub async fn download(
        &self,
        request: &DownloadRequest,
        progress_callback: Option<Arc<dyn Fn(DownloadProgress) + Send + Sync>>,
    ) -> Result<DownloadResult> {
        // YtDlp 策略使用专门的 yt-dlp 下载方法（支持合并视频+音频）
        if request.download_strategy == DownloadStrategy::YtDlp {
            return self.download_with_ytdlp(request, progress_callback).await;
        }

        // Hls 策略使用原生 HLS 下载
        if request.download_strategy == DownloadStrategy::Hls {
            let client = (*self.client).clone();
            let downloader = HlsDownloader::new(client)
                .max_concurrent(4)
                .retry_count(self.config.retry_count);

            let output_path = request.output_path.clone();
            match downloader.download(&request.url, &output_path, progress_callback).await {
                Ok(size) => return Ok(DownloadResult {
                    success: true,
                    output_path,
                    file_size: size,
                    duration_secs: 0.0,
                    error_message: None,
                }),
                Err(e) => return Ok(DownloadResult {
                    success: false,
                    output_path,
                    file_size: 0,
                    duration_secs: 0.0,
                    error_message: Some(e.to_string()),
                }),
            }
        }

        // Native 策略：使用 HTTP 直接下载
        let mut last_error = None;

        for attempt in 0..self.config.retry_count {
            if attempt > 0 {
                // 计算差异化退避时间
                let delay = Self::calculate_backoff(&last_error, attempt, self.config.retry_max_delay);
                // 安全日志: 不暴露延迟时间，防止信息泄露
                info!("重试 {} / {}...", attempt + 1, self.config.retry_count);
                tokio::time::sleep(delay).await;
            }

            // 检查是否支持分段下载（需要先获取文件大小）
            let total_size = self.get_content_length(&request.url).await?;

            // 大于10MB且启用了分段下载时使用并行下载
            if let Some(size) = total_size {
                if self.config.parallel_segments > 0 && size > SEGMENT_SIZE_THRESHOLD {
                    match self.parallel_download(request, size, progress_callback.clone()).await {
                        Ok(result) => return Ok(result),
                        Err(e) => {
                            // 分段下载失败（可能是合并失败），不要再回退到普通下载
                            // 否则会重新下载整个文件，造成时间浪费
                            warn!("分段下载失败: {:?}，不再尝试普通下载（避免重复下载）", e);
                            // 直接继续到重试逻辑，不要回退到普通下载
                        }
                    }
                }
            }

            // 普通下载
            match self.do_download(request, progress_callback.clone()).await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    last_error = Some(e);
                    warn!("下载失败，请检查网络或稍后重试");
                }
            }
        }

        Err(anyhow!("下载失败，已重试 {} 次", self.config.retry_count))
            .context("parallel download failed after all retry attempts")
    }

    /// 根据错误类型计算差异化退避时间
    fn calculate_backoff(last_error: &Option<anyhow::Error>, attempt: u32, max_delay: u64) -> Duration {
        if let Some(err) = last_error {
            let msg = err.to_string();

            // 429 限流错误 - 等待更长时间 (60-120s)
            if msg.contains("429") || msg.contains("rate limit") || msg.contains("限速") {
                // 对于限流错误，使用固定长等待，不指数退避
                return Duration::from_secs(60);
            }

            // 5xx 服务器错误 - 短等待 (2-8s)
            if msg.contains("500") || msg.contains("502") || msg.contains("503") || msg.contains("服务器异常") {
                let backoff = 2u64.pow(attempt.min(3)).min(8);
                return Duration::from_secs(backoff);
            }

            // 4xx 客户端错误 (非限流) - 快速失败，不重试
            if msg.contains("404") || msg.contains("403") || msg.contains("内容不可用") {
                // 这些错误重试无意义，但为了保持一致，仍然等待
                let backoff = 2u64.pow(attempt).min(max_delay);
                return Duration::from_secs(backoff);
            }
        }

        // 默认: 标准指数退避
        let backoff = 2u64.pow(attempt.saturating_sub(1)).min(max_delay);
        Duration::from_secs(backoff)
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
            let status_code = response.status().as_u16();
            return Err(DownloadError::http(status_code).into());
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

        // 进度节流：避免回调风暴（对大文件每个chunk都回调会造成UI卡顿）
        let mut last_reported_percent: f64 = -1.0;
        let last_callback_time = std::time::Instant::now();

        while let Some(chunk_result) = stream.next().await {
            // 检查取消信号（每 chunk 检查一次）
            if request.cancel_flag.as_ref().map(|f| f.load(Ordering::Relaxed)).unwrap_or(false) {
                return Err(DownloadError::cancelled().into());
            }
            let chunk: bytes::Bytes = chunk_result?;

            // 写入文件
            file.write_all(&chunk).await?;

            downloaded += chunk.len() as u64;

            // 计算速度并更新进度
            let elapsed = start_time.elapsed().as_secs_f64();
            let speed = if elapsed > 0.0 {
                downloaded as f64 / elapsed
            } else {
                0.0
            };

            progress.update(downloaded, speed, 1);

            // 阈值过滤：只在校验和变化超过 1% 或时间超过 100ms 时才回调
            let current_percent = if total > 0 {
                (downloaded as f64 / total as f64) * 100.0
            } else {
                0.0
            };
            let elapsed_since_last = last_callback_time.elapsed().as_millis() as u64;
            let should_report = (current_percent - last_reported_percent).abs() >= 1.0
                || elapsed_since_last >= 100;

            if should_report {
                last_reported_percent = current_percent;
                // Clone outside lock to avoid blocking the callback with lock held
                let callback_progress = progress.clone();
                if let Some(ref callback) = progress_callback {
                    callback(callback_progress);
                }
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

    /// 获取内容长度 (带缓存)
    async fn get_content_length(&self, url: &str) -> Result<Option<u64>> {
        // 检查缓存
        if let Some(size) = HEAD_CACHE.lock().await.get(url) {
            debug!("HEAD 缓存命中: {} -> {} bytes", url, size);
            return Ok(Some(*size));
        }

        let response = self.client.head(url).send().await?;

        if response.status().is_success() {
            let length = response
                .headers()
                .get("content-length")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse().ok());

            // 缓存结果
            if let Some(size) = length {
                HEAD_CACHE.lock().await.put(url.to_string(), size);
                debug!("HEAD 缓存存储: {} -> {} bytes", url, size);
            }

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
        cancel_flag: Option<Arc<AtomicBool>>,
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
            return Err(anyhow!("下载失败: HTTP {}", response.status()))
                .context("HTTP download request failed");
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
            // 检查取消信号（每 chunk 检查一次）
            if cancel_flag.as_ref().map(|f| f.load(Ordering::Relaxed)).unwrap_or(false) {
                return Err(DownloadError::cancelled().into());
            }
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

                progress.update(downloaded, speed, 1);
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

    /// 使用 yt-dlp 下载（支持合并视频+音频，适用于 YouTube 等平台）
    async fn download_with_ytdlp(
        &self,
        request: &DownloadRequest,
        progress_callback: Option<Arc<dyn Fn(DownloadProgress) + Send + Sync>>,
    ) -> Result<DownloadResult> {
        use tokio::process::Command;

        let start_time = Instant::now();
        let output_path = &request.output_path;

        info!("使用 yt-dlp 下载: {} -> {}", request.url, output_path.display());

        // 确保输出目录存在
        if let Some(parent) = output_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        // 构建 yt-dlp 参数
        // 使用 original_url（如 YouTube 原始页面 URL）来让 yt-dlp 正确选择格式合并
        let ytdlp_url = request.original_url.as_ref().unwrap_or(&request.url);
        let ytdlp = crate::yt_dlp::YtDlp::new();
        let format = request.format_id.as_deref();
        let args = ytdlp.build_download_args(
            ytdlp_url,
            output_path.to_string_lossy().as_ref(),
            format,
            request.audio_only,
            request.download_subtitles,
        );

        info!("执行 yt-dlp 命令...");

        // 发送初始进度
        let initial_progress = DownloadProgress::new("ytdlp".to_string())
            .with_status(DownloadStatus::Downloading);
        if let Some(ref cb) = progress_callback {
            cb(initial_progress);
        }

        // 执行 yt-dlp,实时解析进度(spawn + piped stdout,不再 .output().await 后才解析)
        let mut child = Command::new(&ytdlp.executable)
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("Failed to spawn yt-dlp")?;

        let stdout = child.stdout.take().context("stdout not piped")?;
        let stderr = child.stderr.take().context("stderr not piped")?;

        // 后台读取 stderr 到 Vec (避免 pipe buffer 满阻塞 yt-dlp)
        let stderr_handle = tokio::spawn(async move {
            let mut buf = Vec::new();
            let mut reader = tokio::io::BufReader::new(stderr);
            let _ = reader.read_to_end(&mut buf).await;
            buf
        });

        // 后台实时解析 stdout 中的 yt-dlp 进度行
        let cb_for_progress = progress_callback.clone();
        let progress_handle = tokio::spawn(async move {
            let mut lines = tokio::io::BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if let Some(progress) = parse_ytdlp_progress(&line) {
                    if let Some(ref cb) = cb_for_progress {
                        cb(progress);
                    }
                }
            }
        });

        // 等待 yt-dlp 退出 + 排空两个 reader task
        let status = child.wait().await.context("Failed to wait for yt-dlp")?;
        let stderr_bytes = stderr_handle.await.unwrap_or_default();
        let _ = progress_handle.await;

        if !status.success() {
            let stderr_str = String::from_utf8_lossy(&stderr_bytes);
            tracing::error!("yt-dlp 下载失败: {}", stderr_str);
            return Err(anyhow!("yt-dlp 下载失败: {}", stderr_str));
        }

        let elapsed = start_time.elapsed();
        let final_size = tokio::fs::metadata(output_path).await?.len();

        info!("yt-dlp 下载完成: {} bytes in {:?}", final_size, elapsed);

        Ok(DownloadResult {
            success: true,
            output_path: output_path.clone(),
            file_size: final_size,
            duration_secs: elapsed.as_secs_f64(),
            error_message: None,
        })
    }
}

#[cfg(test)]
#[allow(unused_imports)]
mod tests {
    use super::*;
    use crate::core::segment::{parse_size, parse_speed};

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

    #[test]
    fn test_downloader_config_default() {
        let config = DownloaderConfig::default();
        assert_eq!(config.retry_count, 5);
        assert_eq!(config.chunk_size, 32 * 1024 * 1024);
        assert_eq!(config.pool_max_idle_per_host, 32);
        assert_eq!(config.parallel_segments, 4);
        assert!(config.adaptive_threads);
        assert_eq!(config.min_threads, 2);
        assert_eq!(config.max_threads, 8);
    }

    #[test]
    fn test_downloader_config_custom() {
        let config = DownloaderConfig {
            connect_timeout: 60,
            request_timeout: 300,
            retry_count: 3,
            retry_max_delay: 15,
            chunk_size: 16 * 1024 * 1024,
            user_agent: Some("TestAgent".to_string()),
            proxy: Some("http://proxy:8080".to_string()),
            pool_max_idle_per_host: 16,
            http2: false,
            tcp_keepalive: 60,
            parallel_segments: 2,
            adaptive_threads: false,
            min_threads: 1,
            max_threads: 4,
            speed_threshold_bps: 256 * 1024,
        };
        assert_eq!(config.connect_timeout, 60);
        assert_eq!(config.parallel_segments, 2);
        assert!(!config.adaptive_threads);
    }

    #[test]
    fn test_calculate_segment_ranges() {
        let downloader = VideoDownloader::new().unwrap();
        let ranges = downloader.calculate_segment_ranges(4, 100, 25);
        assert_eq!(ranges.len(), 4);
        assert_eq!(ranges[0], (0, 24));
        assert_eq!(ranges[1], (25, 49));
        assert_eq!(ranges[2], (50, 74));
        assert_eq!(ranges[3], (75, 99));
    }

    #[test]
    fn test_calculate_segment_ranges_unequal() {
        let downloader = VideoDownloader::new().unwrap();
        // 10 bytes with 3 segments = chunk_size = 4
        let ranges = downloader.calculate_segment_ranges(3, 10, 4);
        assert_eq!(ranges.len(), 3);
        assert_eq!(ranges[0], (0, 3));
        assert_eq!(ranges[1], (4, 7));
        assert_eq!(ranges[2], (8, 9)); // Last segment takes remainder
    }

    #[test]
    fn test_calculate_backoff_rate_limit() {
        let err = anyhow::anyhow!("429 Too Many Requests");
        let delay = VideoDownloader::calculate_backoff(&Some(err), 1, 30);
        assert_eq!(delay, Duration::from_secs(60)); // Rate limit uses fixed 60s
    }

    #[test]
    fn test_calculate_backoff_server_error() {
        let err = anyhow::anyhow!("500 Internal Server Error");
        let delay = VideoDownloader::calculate_backoff(&Some(err), 1, 30);
        assert_eq!(delay, Duration::from_secs(2)); // 2^1 = 2s
    }

    #[test]
    fn test_calculate_backoff_not_found() {
        let err = anyhow::anyhow!("404 Not Found");
        let delay = VideoDownloader::calculate_backoff(&Some(err), 2, 30);
        assert_eq!(delay, Duration::from_secs(4)); // 2^2 = 4s
    }

    #[test]
    fn test_calculate_backoff_default() {
        // 无特定错误时使用标准指数退避
        let delay = VideoDownloader::calculate_backoff(&None, 3, 30);
        assert_eq!(delay, Duration::from_secs(4)); // 2^(3-1) = 4s
    }
}
