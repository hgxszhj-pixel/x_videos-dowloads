//! 下载队列管理模块
//!
//! 提供结构化的下载队列状态管理，支持暂停/恢复/取消/重试

use crate::types::{DownloadProgress, DownloadResult, VideoInfo};
use std::collections::VecDeque;
use std::sync::RwLock;
use std::time::Instant;

/// 下载项状态
#[derive(Debug, Clone, PartialEq)]
pub enum QueueItemStatus {
    /// 等待中
    Pending,
    /// 下载中
    Downloading,
    /// 已暂停
    Paused,
    /// 已完成
    Completed,
    /// 失败
    Failed(String),
    /// 已取消
    Cancelled,
}

/// 下载队列中的单个项
#[derive(Debug, Clone)]
pub struct QueueItem {
    /// 原始 URL
    pub url: String,
    /// 解析后的视频信息
    pub video_info: Option<VideoInfo>,
    /// 当前状态
    pub status: QueueItemStatus,
    /// 下载进度 (0.0 - 100.0)
    pub progress: f64,
    /// 下载速度 (bytes/s)
    pub speed: u64,
    /// 已下载字节数
    pub bytes_downloaded: u64,
    /// 总字节数
    pub total_bytes: u64,
    /// 加入队列时间
    pub enqueued_at: Instant,
    /// 开始下载时间
    pub started_at: Option<Instant>,
    /// 完成时间
    pub completed_at: Option<Instant>,
    /// 错误信息
    pub error_message: Option<String>,
    /// 下载结果 (完成后)
    pub result: Option<DownloadResult>,
}

impl QueueItem {
    /// 创建新的队列项
    pub fn new(url: String, video_info: Option<VideoInfo>) -> Self {
        Self {
            url,
            video_info,
            status: QueueItemStatus::Pending,
            progress: 0.0,
            speed: 0,
            bytes_downloaded: 0,
            total_bytes: 0,
            enqueued_at: Instant::now(),
            started_at: None,
            completed_at: None,
            error_message: None,
            result: None,
        }
    }

    /// 更新进度
    pub fn update_progress(&mut self, progress: DownloadProgress) {
        self.progress = progress.percent * 100.0;
        self.speed = progress.speed as u64;
        self.bytes_downloaded = progress.bytes_downloaded;
        if self.total_bytes == 0 && progress.total_bytes > 0 {
            self.total_bytes = progress.total_bytes;
        }
    }

    /// 标记为下载中
    pub fn start(&mut self) {
        self.status = QueueItemStatus::Downloading;
        self.started_at = Some(Instant::now());
    }

    /// 标记为完成
    pub fn complete(&mut self, result: DownloadResult) {
        let success = result.success;
        self.status = if success {
            QueueItemStatus::Completed
        } else {
            QueueItemStatus::Failed(result.error_message.clone().unwrap_or_default())
        };
        self.completed_at = Some(Instant::now());
        self.result = Some(result);
        if success {
            self.progress = 100.0;
        }
    }

    /// 暂停
    pub fn pause(&mut self) {
        if self.status == QueueItemStatus::Downloading {
            self.status = QueueItemStatus::Paused;
        }
    }

    /// 恢复
    pub fn resume(&mut self) {
        if self.status == QueueItemStatus::Paused {
            self.status = QueueItemStatus::Downloading;
        }
    }

    /// 取消
    pub fn cancel(&mut self) {
        self.status = QueueItemStatus::Cancelled;
        self.completed_at = Some(Instant::now());
    }

    /// 获取预估剩余时间 (秒)
    pub fn eta(&self) -> Option<u64> {
        if self.speed == 0 || self.progress >= 100.0 {
            return None;
        }
        let remaining = self.total_bytes.saturating_sub(self.bytes_downloaded);
        Some(remaining / self.speed)
    }
}

/// 下载队列管理器
#[derive(Debug)]
pub struct DownloadQueue {
    /// 队列项列表
    items: RwLock<VecDeque<QueueItem>>,
    /// 最大队列长度 (0 = 无限制)
    max_size: usize,
    /// 并行下载数
    parallel_downloads: usize,
    /// 当前活跃下载数
    active_count: RwLock<usize>,
}

impl Default for DownloadQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl DownloadQueue {
    /// 创建新的下载队列
    pub fn new() -> Self {
        Self {
            items: RwLock::new(VecDeque::new()),
            max_size: 0, // 无限制
            parallel_downloads: 3,
            active_count: RwLock::new(0),
        }
    }

    /// 创建带配置的下载队列
    pub fn with_config(max_size: usize, parallel_downloads: usize) -> Self {
        Self {
            items: RwLock::new(VecDeque::new()),
            max_size,
            parallel_downloads,
            active_count: RwLock::new(0),
        }
    }

    /// 入队 (添加到队列末尾)
    pub fn enqueue(&self, url: String, video_info: Option<VideoInfo>) -> bool {
        if self.max_size > 0 {
            let len = self.items.read().ok().map(|i| i.len()).unwrap_or(0);
            if len >= self.max_size {
                return false;
            }
        }
        let item = QueueItem::new(url, video_info);
        if let Ok(mut items) = self.items.write() {
            items.push_back(item);
            true
        } else {
            false
        }
    }

    /// 入队 (添加到队列开头)
    pub fn enqueue_front(&self, url: String, video_info: Option<VideoInfo>) -> bool {
        if self.max_size > 0 {
            let len = self.items.read().ok().map(|i| i.len()).unwrap_or(0);
            if len >= self.max_size {
                return false;
            }
        }
        let item = QueueItem::new(url, video_info);
        if let Ok(mut items) = self.items.write() {
            items.push_front(item);
            true
        } else {
            false
        }
    }

    /// 出队 (获取下一个待下载项)
    pub fn dequeue(&self) -> Option<QueueItem> {
        {
            let active = self.active_count.read().ok()?;
            if *active >= self.parallel_downloads {
                return None;
            }
        }

        let mut items = self.items.write().ok()?;

        for i in 0..items.len() {
            if items[i].status == QueueItemStatus::Pending {
                items[i].start();
                *self.active_count.write().ok()? += 1;
                return Some(items[i].clone());
            }
        }
        None
    }

    /// 根据 URL 获取队列项的可变引用
    pub fn get_mut_by_url(&self, url: &str) -> Option<crate::types::DownloadResult> {
        let mut items = self.items.write().ok()?;
        for item in items.iter_mut() {
            if item.url == url {
                if item.status == QueueItemStatus::Downloading {
                    *self.active_count.write().ok()? = self.active_count.read().ok().map(|g| g.saturating_sub(1)).unwrap_or(0);
                }
                if item.status == QueueItemStatus::Completed || item.status == QueueItemStatus::Failed("".to_string()) {
                    return item.result.clone();
                }
                return None;
            }
        }
        None
    }

    /// 更新项状态
    pub fn update_item<F>(&self, url: &str, f: F) -> bool
    where
        F: FnOnce(&mut QueueItem),
    {
        if let Ok(mut items) = self.items.write() {
            for item in items.iter_mut() {
                if item.url == url {
                    f(item);
                    return true;
                }
            }
        }
        false
    }

    /// 完成项
    pub fn complete_item(&self, url: &str, result: DownloadResult) {
        self.update_item(url, |item| {
            item.complete(result.clone());
        });
        if let Ok(mut active) = self.active_count.write() {
            *active = active.saturating_sub(1);
        }
    }

    /// 更新进度
    pub fn update_progress(&self, url: &str, progress: DownloadProgress) {
        self.update_item(url, |item| {
            item.update_progress(progress);
        });
    }

    /// 暂停项
    pub fn pause_item(&self, url: &str) -> bool {
        let paused = self.update_item(url, |item| {
            item.pause();
        });
        if paused {
            if let Ok(mut active) = self.active_count.write() {
                *active = active.saturating_sub(1);
            }
        }
        paused
    }

    /// 恢复项
    pub fn resume_item(&self, url: &str) -> bool {
        self.update_item(url, |item| {
            item.resume();
        })
    }

    /// 取消项
    pub fn cancel_item(&self, url: &str) -> bool {
        let cancelled = self.update_item(url, |item| {
            item.cancel();
        });
        if cancelled {
            if let Ok(mut active) = self.active_count.write() {
                *active = active.saturating_sub(1);
            }
        }
        cancelled
    }

    /// 重试失败项
    pub fn retry_item(&self, url: &str) -> bool {
        self.update_item(url, |item| {
            if matches!(item.status, QueueItemStatus::Failed(_) | QueueItemStatus::Cancelled) {
                item.status = QueueItemStatus::Pending;
                item.progress = 0.0;
                item.speed = 0;
                item.bytes_downloaded = 0;
                item.error_message = None;
            }
        })
    }

    /// 获取所有项的快照
    pub fn get_all(&self) -> Vec<QueueItem> {
        self.items
            .read()
            .ok()
            .map(|items| items.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// 获取待下载项 (Pending + Paused)
    pub fn get_pending(&self) -> Vec<QueueItem> {
        self.items
            .read()
            .ok()
            .map(|items| {
                items.iter()
                    .filter(|i| matches!(i.status, QueueItemStatus::Pending | QueueItemStatus::Paused))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// 获取已完成项
    pub fn get_completed(&self) -> Vec<QueueItem> {
        self.items
            .read()
            .ok()
            .map(|items| {
                items.iter()
                    .filter(|i| i.status == QueueItemStatus::Completed)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// 获取失败项
    pub fn get_failed(&self) -> Vec<QueueItem> {
        self.items
            .read()
            .ok()
            .map(|items| {
                items.iter()
                    .filter(|i| matches!(i.status, QueueItemStatus::Failed(_)))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// 获取队列统计
    pub fn stats(&self) -> QueueStats {
        let items_guard = self.items.read().ok();
        let items_opt = items_guard.as_ref();

        QueueStats {
            total: items_opt.map(|i| i.len()).unwrap_or(0),
            pending: items_opt.map(|i| i.iter().filter(|x| x.status == QueueItemStatus::Pending).count()).unwrap_or(0),
            downloading: items_opt.map(|i| i.iter().filter(|x| x.status == QueueItemStatus::Downloading).count()).unwrap_or(0),
            paused: items_opt.map(|i| i.iter().filter(|x| x.status == QueueItemStatus::Paused).count()).unwrap_or(0),
            completed: items_opt.map(|i| i.iter().filter(|x| x.status == QueueItemStatus::Completed).count()).unwrap_or(0),
            failed: items_opt.map(|i| i.iter().filter(|x| matches!(x.status, QueueItemStatus::Failed(_))).count()).unwrap_or(0),
            active: self.active_count.read().ok().map(|g| *g).unwrap_or(0),
        }
    }

    /// 清空已完成项
    pub fn clear_completed(&self) {
        if let Ok(mut items) = self.items.write() {
            items.retain(|i| !matches!(i.status, QueueItemStatus::Completed));
        }
    }

    /// 清空失败项
    pub fn clear_failed(&self) {
        if let Ok(mut items) = self.items.write() {
            items.retain(|i| !matches!(i.status, QueueItemStatus::Failed(_)));
        }
    }

    /// 清空所有项
    pub fn clear(&self) {
        if let Ok(mut items) = self.items.write() {
            items.clear();
        }
        if let Ok(mut active) = self.active_count.write() {
            *active = 0;
        }
    }

    /// 移除指定项
    pub fn remove(&self, url: &str) -> bool {
        if let Ok(mut items) = self.items.write() {
            let was_active = items.iter().any(|i| i.url == url && i.status == QueueItemStatus::Downloading);
            let removed = items.iter().position(|i| i.url == url).map(|idx| items.remove(idx));
            if was_active {
                if let Ok(mut active) = self.active_count.write() {
                    *active = active.saturating_sub(1);
                }
            }
            return removed.is_some();
        }
        false
    }

    /// 获取队列长度
    pub fn len(&self) -> usize {
        self.items.read().ok().map(|i| i.len()).unwrap_or(0)
    }

    /// 检查队列是否为空
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 检查是否有活跃下载
    pub fn has_active(&self) -> bool {
        self.active_count.read().ok().map(|a| *a > 0).unwrap_or(false)
    }
}

/// 队列统计信息
#[derive(Debug, Clone, Default)]
pub struct QueueStats {
    pub total: usize,
    pub pending: usize,
    pub downloading: usize,
    pub paused: usize,
    pub completed: usize,
    pub failed: usize,
    pub active: usize,
}

impl QueueStats {
    /// 成功率
    pub fn success_rate(&self) -> f64 {
        let finished = self.completed + self.failed;
        if finished == 0 {
            return 0.0;
        }
        self.completed as f64 / finished as f64 * 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_result(success: bool) -> DownloadResult {
        DownloadResult {
            success,
            output_path: std::path::PathBuf::from("/tmp/test.mp4"),
            file_size: 1024,
            duration_secs: 1.0,
            error_message: if success { None } else { Some("test error".to_string()) },
        }
    }

    #[test]
    fn test_queue_enqueue() {
        let queue = DownloadQueue::new();
        assert!(queue.enqueue("http://test.com".to_string(), None));
        assert_eq!(queue.len(), 1);
    }

    #[test]
    fn test_queue_dequeue() {
        let queue = DownloadQueue::new();
        queue.enqueue("http://test1.com".to_string(), None);
        queue.enqueue("http://test2.com".to_string(), None);

        let item = queue.dequeue();
        assert!(item.is_some());
        assert_eq!(item.unwrap().url, "http://test1.com");
        assert_eq!(queue.stats().active, 1);
    }

    #[test]
    fn test_queue_complete() {
        let queue = DownloadQueue::new();
        queue.enqueue("http://test.com".to_string(), None);
        queue.dequeue();

        queue.complete_item("http://test.com", make_result(true));
        let stats = queue.stats();
        assert_eq!(stats.completed, 1);
        assert_eq!(stats.active, 0);
    }

    #[test]
    fn test_queue_pause_resume() {
        let queue = DownloadQueue::new();
        queue.enqueue("http://test.com".to_string(), None);
        queue.dequeue();

        assert!(queue.pause_item("http://test.com"));
        let items = queue.get_all();
        assert_eq!(items[0].status, QueueItemStatus::Paused);

        assert!(queue.resume_item("http://test.com"));
        let items = queue.get_all();
        assert_eq!(items[0].status, QueueItemStatus::Downloading);
    }

    #[test]
    fn test_queue_cancel() {
        let queue = DownloadQueue::new();
        queue.enqueue("http://test.com".to_string(), None);
        queue.dequeue();

        assert!(queue.cancel_item("http://test.com"));
        let items = queue.get_all();
        assert!(matches!(items[0].status, QueueItemStatus::Cancelled));
    }

    #[test]
    fn test_queue_retry() {
        let queue = DownloadQueue::new();
        queue.enqueue("http://test.com".to_string(), None);
        queue.dequeue();
        queue.complete_item("http://test.com", make_result(false));

        assert!(queue.retry_item("http://test.com"));
        let items = queue.get_all();
        assert_eq!(items[0].status, QueueItemStatus::Pending);
        assert_eq!(items[0].progress, 0.0);
    }

    #[test]
    fn test_queue_stats() {
        let queue = DownloadQueue::new();
        queue.enqueue("http://test1.com".to_string(), None);
        queue.enqueue("http://test2.com".to_string(), None);
        queue.enqueue("http://test3.com".to_string(), None);

        queue.dequeue();
        queue.complete_item("http://test1.com", make_result(true));

        let stats = queue.stats();
        assert_eq!(stats.total, 3);
        assert_eq!(stats.pending, 2);
        assert_eq!(stats.completed, 1);
    }

    #[test]
    fn test_queue_clear_completed() {
        let queue = DownloadQueue::new();
        queue.enqueue("http://test1.com".to_string(), None);
        queue.dequeue();
        queue.complete_item("http://test1.com", make_result(true));

        queue.clear_completed();
        assert_eq!(queue.len(), 0);
    }

    #[test]
    fn test_queue_parallel_limit() {
        let queue = DownloadQueue::with_config(0, 2);

        queue.enqueue("http://test1.com".to_string(), None);
        queue.enqueue("http://test2.com".to_string(), None);
        queue.enqueue("http://test3.com".to_string(), None);

        // 前两个应该能出队
        assert!(queue.dequeue().is_some());
        assert!(queue.dequeue().is_some());

        // 第三个应该达到并行上限
        assert!(queue.dequeue().is_none());

        assert_eq!(queue.stats().active, 2);
    }
}