//! 分段并行下载逻辑
//!
//! 多分段并发下载 + 自适应线程控制 + 后台进度聚合。

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use tokio::fs::File;
use tokio::sync::Mutex;
use tracing::{info, warn};

use super::VideoDownloader;
use crate::core::constants::*;
use crate::core::segment::download_segment_with_retry;
use crate::types::{DownloadProgress, DownloadRequest, DownloadResult, DownloadStatus};

impl VideoDownloader {
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
            info!(
                "速度过低 ({:.1} KB/s < {:.1} KB/s)，增加线程: {} -> {}",
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
            info!(
                "速度良好 ({:.1} KB/s)，减少线程: {} -> {}",
                current_speed as f64 / 1024.0,
                current_threads,
                new_threads
            );
            return new_threads;
        }

        current_threads
    }

    /// 计算分段范围
    pub(super) fn calculate_segment_ranges(
        &self,
        segments: u32,
        total_size: u64,
        chunk_size: u64,
    ) -> Vec<(u64, u64)> {
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

    /// 设置进度跟踪（用于轮询模式）
    fn setup_progress_tracking(
        &self,
        total_size: u64,
        task_id: &str,
    ) -> (
        Arc<AtomicU64>,
        Arc<AtomicBool>,
        Arc<Mutex<DownloadProgress>>,
    ) {
        let total_downloaded = Arc::new(AtomicU64::new(0));
        let download_active = Arc::new(AtomicBool::new(true));
        let progress = Arc::new(Mutex::new(
            DownloadProgress::new(task_id.to_string())
                .with_total_bytes(total_size)
                .with_status(DownloadStatus::Downloading),
        ));
        (total_downloaded, download_active, progress)
    }

    /// 设置进度跟踪（用于 channel 推送模式）
    /// 返回: (进度发送端, 下载状态, 当前进度)
    fn setup_channel_progress_tracking(
        &self,
        total_size: u64,
        task_id: &str,
    ) -> (
        tokio::sync::mpsc::Sender<u64>,
        Arc<AtomicBool>,
        Arc<Mutex<DownloadProgress>>,
    ) {
        let (tx, _rx) = tokio::sync::mpsc::channel::<u64>(100);
        let download_active = Arc::new(AtomicBool::new(true));
        let progress = Arc::new(Mutex::new(
            DownloadProgress::new(task_id.to_string())
                .with_total_bytes(total_size)
                .with_status(DownloadStatus::Downloading),
        ));
        (tx, download_active, progress)
    }

    /// 分段并行下载
    pub(super) async fn parallel_download(
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

        // 创建临时目录存储各分段（使用 TempDir 自动清理）。
        // 临时目录一律放 OS temp ($TMPDIR),不放 output_path.parent(),
        // 否则在下载期间被进程 kill 时 TempDir Drop 不跑,
        // 就会在用户输出目录 (~/Movies/) 留下 .tmpXXX/ 孤儿。
        // 用 spawn_blocking 避免阻塞 async runtime (TempDir 创建是 syscall)。
        let temp_dir = tokio::task::spawn_blocking(|| tempfile::TempDir::new()).await??;
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

        // 启动后台进度报告任务（channel 推送 + 阈值过滤）
        // 只在进度变化超过 1% 时才调用 callback，减少 UI 渲染开销
        let progress_task = tokio::spawn(async move {
            let mut last_reported_percent: f64 = -1.0;
            let mut last_downloaded: u64 = 0;

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

                // 阈值过滤：只在校验和变化超过 1% 时才回调
                let current_percent = if progress.total_bytes > 0 {
                    (progress.bytes_downloaded as f64 / progress.total_bytes as f64) * 100.0
                } else {
                    0.0
                };

                let should_report = (current_percent - last_reported_percent).abs() >= 1.0
                    || downloaded < last_downloaded; // 下载量减少说明有新数据阶段

                if !should_report {
                    continue;
                }

                last_reported_percent = current_percent;
                last_downloaded = downloaded;

                // Clone outside lock to avoid holding lock during potentially slow callback
                let callback_progress = progress.clone();
                drop(progress);

                if let Some(ref callback) = callback_clone {
                    callback(callback_progress);
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
                download_segment_with_retry(
                    &client,
                    &url,
                    &temp_path,
                    start,
                    end,
                    segment_size,
                    retry_count,
                    Some(td),
                    cb,
                    None,
                )
                .await
            });
            handles.push(handle);
        }

        // 等待所有分段下载完成
        let mut success_count = 0u32;
        // 使用固定大小数组避免每次分配 Vec（max_threads=8）
        let mut segment_sizes = [0u64; 8];
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
                .context("all parallel segments failed to download");
        }

        // 部分成功时，检查是否应该回退到单线程断点续传
        if success_count < segments && success_count > 0 {
            warn!(
                "部分分段下载失败 ({}/{} 成功)，尝试断点续传...",
                success_count, segments
            );

            // 计算已下载的总大小
            let downloaded_size: u64 = segment_sizes.iter().sum();
            let expected_min = total_size * success_count as u64 / segments as u64
                * RESUME_MIN_DOWNLOAD_RATIO
                / 100;

            if downloaded_size >= expected_min {
                info!("已下载 {} bytes，尝试断点续传下载剩余部分", downloaded_size);
                match self
                    .resume_download(
                        &url,
                        &output_path,
                        downloaded_size,
                        progress_callback.clone(),
                        None,
                    )
                    .await
                {
                    Ok(result) => return Ok(result),
                    Err(e) => warn!("断点续传失败: {:?}，尝试合并已有分段", e),
                }
            }
        }

        // 合并分段文件
        self.merge_segments(
            &output_path,
            &temp_dir_path,
            segments,
            total_size,
            chunk_size,
        )
        .await?;

        // 验证最终文件大小
        if let Ok(final_metadata) = tokio::fs::metadata(&output_path).await {
            let final_size = final_metadata.len();
            info!(
                "合并完成: {} bytes (期望: {} bytes)",
                final_size, total_size
            );

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

        let callback_progress = final_progress.clone();
        drop(final_progress);

        if let Some(ref callback) = progress_callback {
            callback(callback_progress);
        }

        info!("分段并行下载完成: {}", output_path.display());

        let elapsed = start_time.elapsed().as_secs_f64();
        Ok(DownloadResult::success(output_path, total_size, elapsed))
    }
}
