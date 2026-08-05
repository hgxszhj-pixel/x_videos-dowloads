//! 下载进度跟踪
//!
//! 提供轮询模式和 channel 推送模式两种进度跟踪方式
//!
//! 进度跟踪的实现位于 `downloader::parallel::ParallelDownloader` 的方法中。
//! 本模块保留命名空间以便未来提取共享抽象。

use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::types::{DownloadProgress, DownloadStatus};

#[allow(dead_code)]
/// 设置进度跟踪（用于轮询模式）
/// 返回: (已下载总量, 下载是否进行中, 当前进度)
pub fn setup_progress_tracking(
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

#[allow(dead_code)]
/// 设置进度跟踪（用于 channel 推送模式）
/// 返回: (进度发送端, 下载状态, 当前进度)
pub fn setup_channel_progress_tracking(
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
