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

#[expect(dead_code)]
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

#[expect(dead_code)]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polling_tracking_initial_state() {
        let (downloaded, active, progress) = setup_progress_tracking(1024, "task-A");
        assert_eq!(downloaded.load(std::sync::atomic::Ordering::Relaxed), 0);
        assert!(active.load(std::sync::atomic::Ordering::Relaxed));
        let p = progress.blocking_lock();
        assert_eq!(p.total_bytes, 1024);
        assert_eq!(p.task_id, "task-A");
        assert_eq!(p.status, DownloadStatus::Downloading);
        assert_eq!(p.bytes_downloaded, 0);
    }

    #[test]
    fn polling_tracking_total_size_zero_is_valid() {
        let (_, _, progress) = setup_progress_tracking(0, "task-zero");
        let p = progress.blocking_lock();
        assert_eq!(p.total_bytes, 0);
    }

    #[test]
    fn polling_tracking_atomics_independent() {
        let (downloaded, active, _) = setup_progress_tracking(100, "task-B");
        downloaded.store(50, std::sync::atomic::Ordering::Relaxed);
        assert_eq!(downloaded.load(std::sync::atomic::Ordering::Relaxed), 50);
        // active 默认 true,独立于 downloaded 写入
        assert!(active.load(std::sync::atomic::Ordering::Relaxed));
        active.store(false, std::sync::atomic::Ordering::Relaxed);
        assert!(!active.load(std::sync::atomic::Ordering::Relaxed));
    }

    #[test]
    fn channel_tracking_initial_state() {
        let (tx, active, progress) = setup_channel_progress_tracking(2048, "task-C");
        assert!(active.load(std::sync::atomic::Ordering::Relaxed));
        let p = progress.blocking_lock();
        assert_eq!(p.total_bytes, 2048);
        assert_eq!(p.task_id, "task-C");
        assert_eq!(p.status, DownloadStatus::Downloading);
        // sender drop 检查 — 防止未来把 tx 改成非 Send 时编译失败
        drop(tx);
    }

    #[tokio::test]
    async fn channel_tracking_sender_is_sendable() {
        let (tx, _active, _progress) = setup_channel_progress_tracking(100, "task-D");
        // sender 是 Send,可跨 task 转移
        let tx2 = tx.clone();
        drop(tx);
        drop(tx2);
    }
}
