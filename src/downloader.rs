//! 视频下载模块
//!
//! 使用 reqwest 异步下载视频文件，支持进度跟踪和断点续传

#![allow(dead_code)]

/// 并行下载常量
const SEGMENT_SIZE_THRESHOLD: u64 = 10 * 1024 * 1024; // 10MB - 启用并行下载的最小文件大小
const SEGMENT_TIMEOUT_SECS: u64 = 300; // 5分钟 - 分段下载独立超时
const SEGMENT_VALIDATION_RATIO: u64 = 90; // 90% - 分段大小验证比例
const RESUME_MIN_DOWNLOAD_RATIO: u64 = 80; // 80% - 断点续传最小下载比例
const FILE_SIZE_VALIDATION_RATIO: u64 = 90; // 90% - 最终文件大小验证比例

use crate::types::{DownloadError, DownloadProgress, DownloadRequest, DownloadResult, DownloadStatus, DEFAULT_USER_AGENT, validate_output_path};
use anyhow::{anyhow, Context, Result};
use futures::StreamExt;
use reqwest::Client;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicBool, Ordering};
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
    /// 自适应线程控制
    pub adaptive_threads: bool,
    /// 最小线程数
    pub min_threads: u32,
    /// 最大线程数（避免封IP）
    pub max_threads: u32,
    /// 速度阈值（字节/秒），低于此值增加线程
    pub speed_threshold_bps: u64,
}

impl Default for DownloaderConfig {
    fn default() -> Self {
        Self {
            connect_timeout: 30,
            request_timeout: 600, // 大文件需要更长超时
            retry_count: 5,      // 增加重试次数
            retry_max_delay: 30,  // 最大重试间隔30秒
            chunk_size: 1024 * 1024, // 1MB chunk
            user_agent: None,
            proxy: None,
            pool_max_idle_per_host: 16,  // 适度连接池
            http2: true,
            tcp_keepalive: 120, // 减少连接断开
            parallel_segments: 4, // 默认4线程
            adaptive_threads: true, // 启用自适应线程
            min_threads: 2,        // 最少2线程
            max_threads: 8,        // 最多8线程（防封IP）
            speed_threshold_bps: 500 * 1024, // 500KB/s 低于此值增加线程
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
                if self.config.parallel_segments > 0 && size > SEGMENT_SIZE_THRESHOLD {
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
            .context("parallel download failed after all retry attempts")
    }

    /// 根据速度计算最佳线程数（自适应）
    fn calculate_optimal_threads(&self, current_speed: u64, current_threads: u32) -> u32 {
        if !self.config.adaptive_threads {
            return current_threads;
        }

        let threshold = self.config.speed_threshold_bps;
        let min = self.config.min_threads;
        let max = self.config.max_threads;

        // 速度过低，增加线程
        if current_speed < threshold && current_threads < max {
            let new_threads = (current_threads + 1).min(max);
            info!("速度过低 ({:.1} KB/s < {:.1} KB/s)，增加线程: {} -> {}",
                current_speed as f64 / 1024.0,
                threshold as f64 / 1024.0,
                current_threads,
                new_threads
            );
            return new_threads;
        }

        // 速度很好，尝试减少线程（更省资源）
        if current_speed > threshold * 3 && current_threads > min {
            let new_threads = (current_threads - 1).max(min);
            info!("速度良好 ({:.1} KB/s)，减少线程: {} -> {}",
                current_speed as f64 / 1024.0,
                current_threads,
                new_threads
            );
            return new_threads;
        }

        current_threads
    }

    /// 计算分段范围
    fn calculate_segment_ranges(&self, segments: u32, total_size: u64, chunk_size: u64) -> Vec<(u64, u64)> {
        (0..segments)
            .map(|i| {
                let start = i as u64 * chunk_size;
                let end = if i == segments - 1 {
                    total_size - 1
                } else {
                    (i + 1) as u64 * chunk_size - 1
                };
                (start, end)
            })
            .collect()
    }

    /// 设置进度跟踪
    fn setup_progress_tracking(&self, total_size: u64, task_id: &str) -> (Arc<AtomicU64>, Arc<AtomicBool>, Arc<Mutex<DownloadProgress>>) {
        let total_downloaded = Arc::new(AtomicU64::new(0));
        let download_active = Arc::new(AtomicBool::new(true));
        let progress = Arc::new(Mutex::new(
            DownloadProgress::new(task_id.to_string())
                .with_total_bytes(total_size)
                .with_status(DownloadStatus::Downloading)
        ));
        (total_downloaded, download_active, progress)
    }

    /// 合并分段文件
    async fn merge_segments(
        &self,
        output_path: &Path,
        temp_dir_path: &Path,
        segments: u32,
        total_size: u64,
        chunk_size: u64,
    ) -> Result<u64> {
        use std::fs::{File as StdFile, OpenOptions};
        use std::io::{copy as io_copy, Seek, SeekFrom};

        // Validate output path to prevent path traversal
        validate_output_path(output_path, temp_dir_path)
            .map_err(|e| anyhow!("Path traversal detected: {}", e))
            .context("output path validation failed")?;

        // 打开输出文件（不截断，支持追加写入）
        let mut output = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(output_path)?;

        let mut final_size = 0u64;

        for i in 0..segments {
            let temp_path = temp_dir_path.join(format!("part_{}.tmp", i));
            let expected_start = i as u64 * chunk_size;
            let expected_end = if i == segments - 1 {
                total_size - 1
            } else {
                (i + 1) as u64 * chunk_size - 1
            };
            let expected_segment_size = expected_end - expected_start + 1;

            // 检查分段文件是否存在且大小合理
            if let Ok(metadata) = std::fs::metadata(&temp_path) {
                let actual_size = metadata.len();
                if actual_size >= expected_segment_size * SEGMENT_VALIDATION_RATIO / 100 {
                    let mut input = StdFile::open(&temp_path)?;

                    // 移动到正确的写入位置
                    output.seek(SeekFrom::Start(expected_start))?;

                    // 使用 std::io::copy，操作系统会自动使用大缓冲区
                    match io_copy(&mut input, &mut output) {
                        Ok(n) => {
                            debug!("合并分段 {}: {} bytes", i, n);
                            final_size += n;
                        }
                        Err(e) => {
                            warn!("合并分段 {} 时出错: {:?}", i, e);
                        }
                    }
                } else {
                    warn!("分段 {} 大小异常: {} (期望 {})", i, actual_size, expected_segment_size);
                }
            }

            if let Err(e) = tokio::fs::remove_file(&temp_path).await {
                debug!("删除临时文件 {} 失败: {:?}", temp_path.display(), e);
            }
        }

        Ok(final_size)
    }

    /// 分段并行下载
    async fn parallel_download(
        &self,
        request: &DownloadRequest,
        total_size: u64,
        progress_callback: Option<Arc<dyn Fn(DownloadProgress) + Send + Sync>>,
    ) -> Result<DownloadResult> {
        let start_time = Instant::now();
        let url = request.url.clone();
        let output_path = request.output_path.clone();

        // 使用网站特征推荐的线程数（来自 website_profile）
        let segments = self.config.parallel_segments;

        info!("开始分段并行下载: {} ({} 线程)", url, segments);

        // 确保输出目录存在
        if let Some(parent) = output_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        // 创建临时目录存储各分段（使用 TempDir 自动清理）
        // 使用 spawn_blocking 避免阻塞 async runtime
        let parent_dir = output_path.parent().context("输出路径没有有效父目录")?.to_path_buf();
        let temp_dir = tokio::task::spawn_blocking(move || {
            tempfile::TempDir::new_in(parent_dir)
        }).await??;
        let temp_dir_path = temp_dir.path().to_path_buf();

        // 预创建临时文件以避免竞争条件
        for i in 0..segments {
            let temp_path = temp_dir_path.join(format!("part_{}.tmp", i));
            let _ = File::create(&temp_path).await;
        }

        let chunk_size = (total_size as f64 / segments as f64).ceil() as u64;
        let segment_ranges = self.calculate_segment_ranges(segments, total_size, chunk_size);

        // 设置进度跟踪
        let (total_downloaded, download_active, progress) =
            self.setup_progress_tracking(total_size, "parallel");

        // 克隆进度相关数据用于后台进度报告任务
        let total_downloaded_clone = total_downloaded.clone();
        let download_active_clone = download_active.clone();
        let progress_clone = progress.clone();
        let callback_clone = progress_callback.clone();
        let start_time_clone = start_time;

        // 克隆自适应线程配置值
        let adaptive = self.config.adaptive_threads;
        let min_threads = self.config.min_threads;
        let max_threads = self.config.max_threads;
        let speed_threshold = self.config.speed_threshold_bps;
        let mut current_threads = segments;

        // 启动后台进度报告任务（每 0.5 秒更新一次）
        let progress_task = tokio::spawn(async move {
            while download_active_clone.load(Ordering::Relaxed) {
                tokio::time::sleep(Duration::from_millis(100)).await;

                let downloaded = total_downloaded_clone.load(Ordering::Relaxed);
                let elapsed = start_time_clone.elapsed().as_secs_f64();
                let speed = if elapsed > 0.0 {
                    downloaded as f64 / elapsed
                } else {
                    0.0
                };

                // 动态计算推荐线程数
                if adaptive {
                    let speed_bps = speed as u64;
                    if speed_bps < speed_threshold && current_threads < max_threads {
                        current_threads = (current_threads + 1).min(max_threads);
                    } else if speed_bps > speed_threshold * 3 && current_threads > min_threads {
                        current_threads = (current_threads - 1).max(min_threads);
                    }
                }

                let mut progress = progress_clone.lock().await;
                progress.update(downloaded, speed, current_threads);

                if let Some(ref callback) = callback_clone {
                    callback(progress.clone());
                }
            }
        });

        // 并行下载各分段（每个分段有独立的较短超时）
        let mut handles = vec![];

        for i in 0..segments {
            let url = url.clone();
            let temp_path = temp_dir_path.join(format!("part_{}.tmp", i));
            let client = self.client.clone();
            let (start, end) = segment_ranges[i as usize];
            let retry_count = self.config.retry_count;
            let segment_size = chunk_size;
            let td = total_downloaded.clone();
            let cb = progress_callback.clone();

            let handle = tokio::spawn(async move {
                download_segment_with_retry(&client, &url, &temp_path, start, end, segment_size, retry_count, Some(td), cb).await
            });
            handles.push(handle);
        }

        // 等待所有分段下载完成
        let mut success_count = 0u32;
        let mut segment_sizes = vec![0u64; segments as usize];
        for (i, handle) in handles.into_iter().enumerate() {
            match handle.await {
                Ok(Ok(size)) => {
                    success_count += 1;
                    segment_sizes[i] = size;
                    total_downloaded.fetch_add(size, Ordering::Relaxed);
                }
                Ok(Err(e)) => {
                    warn!("分段 {} 下载失败: {:?}", i, e);
                    segment_sizes[i] = 0;
                }
                Err(e) => {
                    warn!("分段 {} 任务panic: {:?}", i, e);
                    segment_sizes[i] = 0;
                }
            }
        }

        info!("分段下载完成: {}/{} 成功", success_count, segments);

        // 所有分段都失败则报错
        if success_count == 0 {
            return Err(anyhow!("所有分段下载都失败了"))
            .context("all parallel segments failed to download")
        }

        // 部分成功时，检查是否应该回退到单线程断点续传
        if success_count < segments && success_count > 0 {
            warn!("部分分段下载失败 ({}/{} 成功)，尝试断点续传...", success_count, segments);

            // 计算已下载的总大小
            let downloaded_size: u64 = segment_sizes.iter().sum();
            let expected_min = total_size * success_count as u64 / segments as u64 * RESUME_MIN_DOWNLOAD_RATIO / 100;

            if downloaded_size >= expected_min {
                info!("已下载 {} bytes，尝试断点续传下载剩余部分", downloaded_size);
                match self.resume_download(&url, &output_path, downloaded_size, progress_callback.clone()).await {
                    Ok(result) => return Ok(result),
                    Err(e) => warn!("断点续传失败: {:?}，尝试合并已有分段", e),
                }
            }
        }

        // 合并分段文件
        self.merge_segments(&output_path, &temp_dir_path, segments, total_size, chunk_size).await?;

        // 验证最终文件大小
        if let Ok(final_metadata) = tokio::fs::metadata(&output_path).await {
            let final_size = final_metadata.len();
            info!("合并完成: {} bytes (期望: {} bytes)", final_size, total_size);

            if final_size < total_size * FILE_SIZE_VALIDATION_RATIO / 100 {
                warn!("最终文件大小异常: {} < {}", final_size, total_size);
            }
        }

        // TempDir 会在离开作用域时自动清理剩余的临时文件

        // 停止后台进度报告任务
        download_active.store(false, Ordering::Relaxed);
        let _ = progress_task.await;

        // 通知完成
        let mut final_progress = progress.lock().await;
        final_progress.status = DownloadStatus::Completed;
        final_progress.percent = 1.0;

        if let Some(ref callback) = progress_callback {
            callback(final_progress.clone());
        }

        info!("分段并行下载完成: {}", output_path.display());

        let elapsed = start_time.elapsed().as_secs_f64();
        Ok(DownloadResult::success(output_path, total_size, elapsed))
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

        while let Some(chunk_result) = stream.next().await {
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

            // 每次chunk都触发回调（不再等待时间间隔），确保UI实时更新
            if let Some(ref callback) = progress_callback {
                callback(progress.clone());
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
}

impl Default for VideoDownloader {
    fn default() -> Self {
        Self::new().expect("VideoDownloader: failed to create HTTP client")
    }
}

/// 下载单个分段（用于并行下载）
#[allow(clippy::too_many_arguments)]
async fn download_segment(
    client: &Client,
    url: &str,
    output_path: &PathBuf,
    start: u64,
    end: u64,
    _segment_size: u64,
    _progress_callback: Option<Arc<dyn Fn(DownloadProgress) + Send + Sync>>,
    total_downloaded: Option<Arc<AtomicU64>>,
) -> Result<u64> {
    let response = client
        .get(url)
        .header("Range", format!("bytes={}-{}", start, end))
        .send()
        .await?;

    if !response.status().is_success() && response.status() != reqwest::StatusCode::PARTIAL_CONTENT {
        return Err(anyhow!("分段下载失败: HTTP {}", response.status()))
            .context("segment download request failed");
    }

    let mut file = File::create(output_path).await?;
    let mut stream = response.bytes_stream();
    let mut downloaded: u64 = 0;
    let start_time = std::time::Instant::now();

    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result?;
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;

        let elapsed = start_time.elapsed().as_secs_f64();
        let _speed = if elapsed > 0.0 {
            downloaded as f64 / elapsed
        } else {
            0.0
        };

        // 更新 total_downloaded（供后台任务统一计算进度）
        if let Some(ref td) = total_downloaded {
            td.fetch_add(chunk.len() as u64, Ordering::Relaxed);
        }
    }

    file.flush().await?;
    Ok(downloaded)
}

/// 带重试的分段下载（使用独立超时控制）
#[allow(clippy::too_many_arguments)]
async fn download_segment_with_retry(
    client: &Client,
    url: &str,
    output_path: &PathBuf,
    start: u64,
    end: u64,
    expected_size: u64,
    retry_count: u32,
    total_downloaded: Option<Arc<AtomicU64>>,
    progress_callback: Option<Arc<dyn Fn(DownloadProgress) + Send + Sync>>,
) -> Result<u64> {
    let segment_timeout = Duration::from_secs(SEGMENT_TIMEOUT_SECS);
    let segment_size = end - start + 1;

    for attempt in 0..retry_count {
        if attempt > 0 {
            let delay = Duration::from_secs(2u64.pow(attempt - 1).min(30));
            info!("分段 [{}-{}] 重试 {} / {}，等待 {}s", start, end, attempt + 1, retry_count, delay.as_secs());
            tokio::time::sleep(delay).await;
        }

        match tokio::time::timeout(segment_timeout, download_segment(client, url, output_path, start, end, segment_size, progress_callback.clone(), total_downloaded.clone())).await {
            Ok(Ok(size)) => {
                // 验证下载大小是否合理
                if size >= expected_size * SEGMENT_VALIDATION_RATIO / 100 {
                    return Ok(size);
                } else {
                    warn!("分段 [{}-{}] 大小异常: {} (期望 {})", start, end, size, expected_size);
                    if attempt < retry_count - 1 {
                        continue;
                    }
                }
            }
            Ok(Err(e)) => {
                warn!("分段 [{}-{}] 下载错误: {:?}", start, end, e);
            }
            Err(_) => {
                warn!("分段 [{}-{}] 超时 ({}s)", start, end, segment_timeout.as_secs());
            }
        }
    }

    Err(anyhow!("分段 [{}-{}] 下载失败，已重试 {} 次", start, end, retry_count))
        .context("segment download failed after all retry attempts")
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
        active_threads: 0,
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
