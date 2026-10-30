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

/// 按当前速度调整并发线程数（纯函数，便于边界测试）。
///
/// 语义：
/// - 速度低于阈值 → 加一线程（上限 max）
/// - 速度高于阈值 ×3 → 减一线程（下限 min）
/// - 其余 → 不变
///
/// 与旧的内联实现**不是逐条等价**，有一处有意的差异：
/// 旧实现直接 `current ± 1`,不夹到 `[min, max]`；本实现两个方向都夹在
/// `[min.min(max), max]` 内。
/// 触发路径：`recommended_segments_for_size` 用 `profile.max_threads`(8),
/// 而 `with_website_profile` 在 `has_thread_limit` 时把 `config.max_threads`
/// 设成 4 —— 于是 `current=8 > max=4`,旧实现会报 `active_threads=7`（超过上限）。
/// 影响仅限显示：`active_threads` 在 `src/` 内无读取方,且 `current_threads`
/// 从不参与分段 spawn（所有分段在 `parallel_download` 里一次性起完）。
///
/// 边界处理（2026-10-30 补测时发现）：
/// - `threshold * 3` 用 `saturating_mul` —— 阈值是用户可配的 u64，
///   配到 > u64::MAX/3 时旧代码在 debug 构建里直接 panic。
/// - min > max（配置写反）时两个方向都夹在 [min.min(max), max] 内，
///   不会出现「加线程分支返回比 min 还小」的值。
///
/// 旧版 `VideoDownloader::calculate_optimal_threads` 是**死代码**
/// （全仓无调用点，真实逻辑内联在 `parallel_download` 的进度任务里），
/// 已删除并由本函数取代 —— 一份实现、纯函数、可测。
pub(crate) fn adjust_thread_count(
    speed_bps: u64,
    threshold: u64,
    min: u32,
    max: u32,
    current: u32,
) -> u32 {
    let lo = min.min(max);
    let hi = max;

    // 速度过低，增加线程
    if speed_bps < threshold && current < hi {
        return (current + 1).min(hi).max(lo);
    }

    // 速度很好，尝试减少线程（更省资源）
    if speed_bps > threshold.saturating_mul(3) && current > lo {
        return (current - 1).max(lo).min(hi);
    }

    current
}

impl VideoDownloader {
    /// 计算分段范围
    pub(super) fn calculate_segment_ranges(
        &self,
        segments: u32,
        total_size: u64,
        chunk_size: u64,
    ) -> Vec<(u64, u64)> {
        if segments == 0 {
            return Vec::new();
        }
        // 退化输入（没有内容可切）：返回 segments 个空区间而不是下溢 panic。
        // 调用方按 `segment_ranges[i]` 索引，**长度必须正好是 segments**。
        if total_size == 0 || chunk_size == 0 {
            return vec![(0, 0); segments as usize];
        }

        let last = total_size - 1;
        (0..segments)
            .map(|i| {
                // chunk 过大时 start 可能越过文件尾
                //（例如 total=10, segments=7, chunk=2 → start=12 > 9），
                // 夹到 last，否则下游 `end - start + 1` 会下溢。
                let start = (i as u64 * chunk_size).min(last);
                let end = if i == segments - 1 {
                    last
                } else {
                    ((i + 1) as u64 * chunk_size - 1).min(last)
                };
                // 保证 start <= end：下游按 `end - start + 1` 算长度，
                // 反向区间会下溢 panic。
                (start, end.max(start))
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
        let temp_dir = tokio::task::spawn_blocking(tempfile::TempDir::new).await??;
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
                    current_threads = adjust_thread_count(
                        speed_bps,
                        speed_threshold,
                        min_threads,
                        max_threads,
                        current_threads,
                    );
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
        // 用 Vec 而不是固定 [u64; 8]：这个数组按 `segment_sizes[i]`（i < segments）
        // 索引，固定 8 元素只是靠「segments ≤ 8」这个隐含前提才不越界。
        // 一旦某个网站特征把 max_threads 提到 >8，这里会**越界 panic**。
        // 长度按 segments 分配 → 与 calculate_segment_ranges 的契约一致。
        let mut segment_sizes = vec![0u64; segments as usize];
        for (i, handle) in handles.into_iter().enumerate() {
            match handle.await {
                Ok(Ok(size)) => {
                    success_count += 1;
                    segment_sizes[i] = size;
                    // 不再 fetch_add：download_segment 已按块累加进 td（core/segment.rs:75），
                    // 这里再加一次会让已下载字节翻倍（实测 100% 那行打印 73.8MB / 文件 36.9MB）
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

        // 单段没有可合并的东西：直接把 part_0.tmp rename 成输出文件，省掉一次整文件拷贝。
        // x.com/video.twimg.com 的网站特征是 max_threads=1，所以这条快路径是常态。
        // 分段尺寸不足或跨卷 rename(EXDEV) 时退回拷贝合并，行为与改动前一致。
        let mut merged = false;
        if segments == 1 {
            let only_part = temp_dir_path.join("part_0.tmp");
            let part_size = tokio::fs::metadata(&only_part)
                .await
                .map(|m| m.len())
                .unwrap_or(0);
            if part_size >= total_size * SEGMENT_VALIDATION_RATIO / 100 {
                match tokio::fs::rename(&only_part, &output_path).await {
                    Ok(()) => merged = true,
                    Err(e) => warn!("单段 rename 失败（{:?}），退回拷贝合并", e),
                }
            } else {
                warn!(
                    "单段文件不完整 ({}/{} bytes)，退回拷贝合并",
                    part_size, total_size
                );
            }
        }

        if !merged {
            self.merge_segments(
                &output_path,
                &temp_dir_path,
                segments,
                total_size,
                chunk_size,
            )
            .await?;
        }

        // 验证最终文件大小
        if let Ok(final_metadata) = tokio::fs::metadata(&output_path).await {
            let final_size = final_metadata.len();
            info!(
                "最终文件: {} bytes (期望: {} bytes)",
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
        // 收尾一律按文件真实大小汇报：累加器在分段重试时会重复计入已下过的字节，
        // 直接用它会让 100% 那行的字节数大于文件本身（历史 bug 即此）。
        let accounted = total_downloaded.load(Ordering::Relaxed);
        if accounted > total_size {
            warn!(
                "进度累加 {} bytes > 文件大小 {} bytes（含重试重复计数），收尾按文件大小汇报",
                accounted, total_size
            );
        }
        final_progress.bytes_downloaded = total_size;

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

#[cfg(test)]
mod tests {
    use super::adjust_thread_count;

    #[test]
    fn test_adjust_thread_count_slow_adds_one() {
        // 速度低于阈值 → 加一线程
        assert_eq!(adjust_thread_count(1_000, 10_000, 2, 8, 4), 5);
        // 已到上限 → 不动
        assert_eq!(adjust_thread_count(1_000, 10_000, 2, 8, 8), 8);
    }

    #[test]
    fn test_adjust_thread_count_fast_removes_one() {
        // 速度高于阈值×3 → 减一线程
        assert_eq!(adjust_thread_count(50_000, 10_000, 2, 8, 4), 3);
        // 已到下限 → 不动
        assert_eq!(adjust_thread_count(50_000, 10_000, 2, 8, 2), 2);
    }

    #[test]
    fn test_adjust_thread_count_middle_band_unchanged() {
        // 阈值到阈值×3 之间 → 不动
        assert_eq!(adjust_thread_count(10_000, 10_000, 2, 8, 4), 4);
        assert_eq!(adjust_thread_count(30_000, 10_000, 2, 8, 4), 4);
    }

    /// 阈值是用户可配的 u64 —— 配成 u64::MAX 时旧代码的 `threshold * 3`
    /// 在 debug 构建里直接溢出 panic。
    #[test]
    fn test_adjust_thread_count_threshold_overflow_does_not_panic() {
        assert_eq!(adjust_thread_count(u64::MAX, u64::MAX, 2, 8, 4), 4);
        assert_eq!(adjust_thread_count(0, u64::MAX, 2, 8, 4), 5);
    }

    /// 配置写反（min > max）时两个方向都不能越界。
    ///
    /// **这条用例必须能红**：旧的内联实现只做 `(current ± 1)`、不夹到
    /// `[min, max]` —— `current=10` 且 min=8/max=2 时旧实现返回 9（越界），
    /// 新实现返回 2。
    /// 用 `current=8` 的话旧实现根本进不了减线程分支（`current > min` 为假），
    /// 断言会「假绿」—— 这就是「已 codified」≠「有效」。
    #[test]
    fn test_adjust_thread_count_min_greater_than_max() {
        let slow = adjust_thread_count(1_000, 10_000, 8, 2, 1);
        assert!((2..=8).contains(&slow), "got {}", slow);
        // 减线程方向：旧实现返回 9（越界），新实现夹回 2
        let fast = adjust_thread_count(50_000, 10_000, 8, 2, 10);
        assert_eq!(fast, 2, "must be clamped into [min.min(max), max]");
    }

    /// 真实触发路径：`recommended_segments_for_size` 用 `profile.max_threads`(8)，
    /// 而 `with_website_profile` 在 `has_thread_limit` 时把 `config.max_threads`
    /// 设成 4。旧实现会报 7（超过上限），新实现报 4。
    #[test]
    fn test_adjust_thread_count_clamps_to_effective_max() {
        assert_eq!(adjust_thread_count(50_000, 10_000, 2, 4, 8), 4);
    }
}
