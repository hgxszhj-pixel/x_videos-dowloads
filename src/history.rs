//! 下载历史和书签模块

use crate::platform::url_matcher;
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Instant;
use tracing::info;
use uuid::Uuid;

/// 下载状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DownloadStatus {
    Completed,
    Failed,
    InProgress,
    Bookmarked,
}

/// 平台来源
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    X,
    YouTube,
    TikTok,
    DouYin,
    Bilibili,
    Instagram,
    Facebook,
    Weibo,
    ZhiXueYun,
    Other,
}

impl Platform {
    pub fn from_url(url: &str) -> Self {
        url_matcher::detect_platform(url)
    }

    #[allow(dead_code)]
    pub fn display_name(&self) -> &'static str {
        match self {
            Platform::X => "X/Twitter",
            Platform::YouTube => "YouTube",
            Platform::TikTok => "TikTok",
            Platform::DouYin => "DouYin",
            Platform::Bilibili => "Bilibili",
            Platform::Instagram => "Instagram",
            Platform::Facebook => "Facebook",
            Platform::Weibo => "Weibo",
            Platform::ZhiXueYun => "智学云",
            Platform::Other => "Other",
        }
    }
}

/// 下载历史条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: Uuid,
    pub url: String,
    pub title: Option<String>,
    pub file_path: Option<PathBuf>,
    pub file_size: Option<u64>,
    pub downloaded_at: DateTime<Utc>,
    pub duration_secs: Option<f64>,
    pub status: DownloadStatus,
    pub platform: Platform,
    pub error_message: Option<String>,
}

/// 历史条目构建器
#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct HistoryEntryBuilder {
    url: Option<String>,
    title: Option<String>,
    file_path: Option<PathBuf>,
    file_size: Option<u64>,
    duration_secs: Option<f64>,
    status: Option<DownloadStatus>,
    error_message: Option<String>,
}

#[allow(dead_code)]
impl HistoryEntryBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn url(mut self, url: impl Into<String>) -> Self {
        self.url = Some(url.into());
        self
    }

    pub fn title(mut self, title: Option<String>) -> Self {
        self.title = title;
        self
    }

    pub fn file_path(mut self, file_path: Option<PathBuf>) -> Self {
        self.file_path = file_path;
        self
    }

    pub fn file_size(mut self, file_size: Option<u64>) -> Self {
        self.file_size = file_size;
        self
    }

    pub fn duration_secs(mut self, duration_secs: Option<f64>) -> Self {
        self.duration_secs = duration_secs;
        self
    }

    pub fn status(mut self, status: DownloadStatus) -> Self {
        self.status = Some(status);
        self
    }

    pub fn error_message(mut self, error_message: Option<String>) -> Self {
        self.error_message = error_message;
        self
    }

    pub fn build(self) -> Result<HistoryEntry> {
        let url = self.url.ok_or_else(|| anyhow::anyhow!("url is required"))?;
        let status = self.status.ok_or_else(|| anyhow::anyhow!("status is required"))?;
        let platform = Platform::from_url(&url);

        Ok(HistoryEntry {
            id: Uuid::new_v4(),
            url,
            title: self.title,
            file_path: self.file_path,
            file_size: self.file_size,
            downloaded_at: Utc::now(),
            duration_secs: self.duration_secs,
            status,
            platform,
            error_message: self.error_message,
        })
    }
}

/// 书签条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookmarkEntry {
    pub id: Uuid,
    pub url: String,
    pub title: Option<String>,
    pub added_at: DateTime<Utc>,
    pub note: Option<String>,
}

/// 下载历史管理器
pub struct History {
    entries: Vec<HistoryEntry>,
    bookmarks: Vec<BookmarkEntry>,
    history_file: PathBuf,
    bookmark_file: PathBuf,
}

impl History {
    /// 创建历史管理器
    pub fn new(data_dir: &Path) -> Result<Self> {
        let history_file = data_dir.join("history.json");
        let bookmark_file = data_dir.join("bookmarks.json");

        let mut history = Self {
            entries: Vec::new(),
            bookmarks: Vec::new(),
            history_file,
            bookmark_file,
        };

        history.load()?;
        Ok(history)
    }

    /// 加载历史记录
    fn load(&mut self) -> Result<()> {
        let start = Instant::now();
        let history_count = if self.history_file.exists() {
            let content = std::fs::read_to_string(&self.history_file)?;
            let count = if let Ok(entries) = serde_json::from_str::<Vec<serde_json::Value>>(&content) {
                entries.len()
            } else {
                0
            };
            self.entries = serde_json::from_str(&content).unwrap_or_default();
            count
        } else {
            0
        };
        if self.bookmark_file.exists() {
            let content = std::fs::read_to_string(&self.bookmark_file)?;
            self.bookmarks = serde_json::from_str(&content).unwrap_or_default();
        }
        info!("History loaded: {} entries in {:?}", history_count, start.elapsed());
        Ok(())
    }

    /// 保存历史记录
    fn save(&self) -> Result<()> {
        if let Some(parent) = self.history_file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&self.history_file, serde_json::to_string_pretty(&self.entries)?)?;
        std::fs::write(&self.bookmark_file, serde_json::to_string_pretty(&self.bookmarks)?)?;
        Ok(())
    }

    /// 添加下载历史
    #[allow(dead_code)]
    pub fn add_entry(&mut self, entry: HistoryEntry) -> Result<()> {
        let mut entry = entry;
        entry.id = Uuid::new_v4();
        entry.downloaded_at = Utc::now();
        if entry.platform == Platform::Other {
            entry.platform = Platform::from_url(&entry.url);
        }
        self.entries.insert(0, entry); // 最近的在前面
        self.save()?;
        Ok(())
    }

    /// 添加下载历史（兼容旧接口，9参数版本）
    #[allow(clippy::too_many_arguments)]
    pub fn add_entry_with_all(
        &mut self,
        url: String,
        title: Option<String>,
        file_path: Option<PathBuf>,
        file_size: Option<u64>,
        duration_secs: Option<f64>,
        status: DownloadStatus,
        error_message: Option<String>,
    ) -> Result<()> {
        let entry = HistoryEntry {
            id: Uuid::new_v4(),
            url: url.clone(),
            title,
            file_path,
            file_size,
            downloaded_at: Utc::now(),
            duration_secs,
            status,
            platform: Platform::from_url(&url),
            error_message,
        };
        self.entries.insert(0, entry); // 最近的在前面
        self.save()?;
        Ok(())
    }

    /// 获取最近 N 条历史
    pub fn get_recent(&self, count: usize) -> &[HistoryEntry] {
        &self.entries[..count.min(self.entries.len())]
    }

    /// 添加历史条目（自动清理超过限制的旧记录）
    pub fn add_entry_with_auto_cleanup(&mut self, entry: HistoryEntry, max_entries: usize) -> Result<()> {
        self.entries.insert(0, entry);
        // 如果超过限制，删除最旧的条目
        if self.entries.len() > max_entries {
            self.entries.truncate(max_entries);
        }
        self.save()?;
        Ok(())
    }

    /// 获取历史条数
    #[allow(dead_code)]
    pub fn count(&self) -> usize {
        self.entries.len()
    }

    /// 搜索历史
    pub fn search(&self, query: &str) -> Vec<&HistoryEntry> {
        let query_lower = query.to_lowercase();
        self.entries
            .iter()
            .filter(|e| e.url.to_lowercase().contains(&query_lower) || e.title.as_ref().map(|t| t.to_lowercase().contains(&query_lower)).unwrap_or(false))
            .collect()
    }

    /// 删除历史条目
    #[allow(dead_code)]
    pub fn remove_entry(&mut self, id: Uuid) -> bool {
        let initial_len = self.entries.len();
        self.entries.retain(|e| e.id != id);
        if self.entries.len() != initial_len {
            let _ = self.save();
            true
        } else {
            false
        }
    }

    /// 清空历史
    pub fn clear(&mut self) -> Result<()> {
        self.entries.clear();
        self.save()
    }

    /// 添加书签
    pub fn add_bookmark(&mut self, url: String, title: Option<String>, note: Option<String>) -> Result<()> {
        // 检查是否已存在
        if self.bookmarks.iter().any(|b| b.url == url) {
            anyhow::bail!("URL 已存在");
        }

        let bookmark = BookmarkEntry {
            id: Uuid::new_v4(),
            url,
            title,
            added_at: Utc::now(),
            note,
        };
        self.bookmarks.push(bookmark);
        self.save()?;
        Ok(())
    }

    /// 获取所有书签
    pub fn get_bookmarks(&self) -> &[BookmarkEntry] {
        &self.bookmarks
    }

    /// 删除书签
    #[allow(dead_code)]
    pub fn remove_bookmark(&mut self, id: Uuid) -> bool {
        let initial_len = self.bookmarks.len();
        self.bookmarks.retain(|b| b.id != id);
        if self.bookmarks.len() != initial_len {
            let _ = self.save();
            true
        } else {
            false
        }
    }

    /// 获取历史条数
    #[allow(dead_code)]
    pub fn history_count(&self) -> usize {
        self.entries.len()
    }

    /// 按状态筛选历史
    #[allow(dead_code)]
    pub fn filter_by_status(&self, status: DownloadStatus) -> Vec<&HistoryEntry> {
        self.entries.iter().filter(|e| e.status == status).collect()
    }

    /// 按平台筛选历史
    #[allow(dead_code)]
    pub fn filter_by_platform(&self, platform: Platform) -> Vec<&HistoryEntry> {
        self.entries.iter().filter(|e| e.platform == platform).collect()
    }

    /// 获取所有历史
    #[allow(dead_code)]
    pub fn get_all(&self) -> &[HistoryEntry] {
        &self.entries
    }

    /// 获取所有历史条目的克隆（线程安全访问）
    pub fn get_entries(&self) -> Vec<HistoryEntry> {
        self.entries.clone()
    }

    /// 导出历史为 CSV
    #[allow(dead_code)]
    pub fn export_csv(&self, path: &Path) -> Result<()> {
        let mut wtr = csv::Writer::from_path(path)?;
        // 写入表头
        wtr.write_record(["ID", "URL", "Title", "File Path", "File Size (bytes)", "Downloaded At", "Duration (s)", "Status", "Platform", "Error"])?;
        for entry in &self.entries {
            wtr.write_record(&[
                entry.id.to_string(),
                entry.url.clone(),
                entry.title.clone().unwrap_or_default(),
                entry.file_path.as_ref().map(|p| p.to_string_lossy().to_string()).unwrap_or_default(),
                entry.file_size.map(|s| s.to_string()).unwrap_or_default(),
                entry.downloaded_at.to_rfc3339(),
                entry.duration_secs.map(|d| d.to_string()).unwrap_or_default(),
                format!("{:?}", entry.status),
                entry.platform.display_name().to_string(),
                entry.error_message.clone().unwrap_or_default(),
            ])?;
        }
        wtr.flush()?;
        Ok(())
    }

    /// 获取书签条数
    #[allow(dead_code)]
    pub fn bookmark_count(&self) -> usize {
        self.bookmarks.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_history_add_and_search() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut history = History::new(temp_dir.path()).unwrap();

        history.add_entry_with_all(
            "https://example.com/video1".to_string(),
            Some("Video 1".to_string()),
            Some(PathBuf::from("/path/video1.mp4")),
            Some(1024),
            None,
            DownloadStatus::Completed,
            None,
        ).unwrap();

        assert_eq!(history.history_count(), 1);
        assert_eq!(history.get_recent(10).len(), 1);

        let found = history.search("example");
        assert_eq!(found.len(), 1);

        let not_found = history.search("nonexistent");
        assert!(not_found.is_empty());
    }

    #[test]
    fn test_bookmarks() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut history = History::new(temp_dir.path()).unwrap();

        history.add_bookmark(
            "https://example.com/video".to_string(),
            Some("Bookmark Title".to_string()),
            None,
        ).unwrap();

        assert_eq!(history.bookmark_count(), 1);

        let bookmarks = history.get_bookmarks();
        assert_eq!(bookmarks[0].url, "https://example.com/video");
    }

    #[test]
    fn test_remove_bookmark() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut history = History::new(temp_dir.path()).unwrap();

        history.add_bookmark(
            "https://example.com/video".to_string(),
            None,
            None,
        ).unwrap();

        let bookmark = history.get_bookmarks()[0].clone();
        assert!(history.remove_bookmark(bookmark.id));
        assert_eq!(history.bookmark_count(), 0);
    }

    #[test]
    fn test_history_entry_builder() {
        let entry = HistoryEntryBuilder::new()
            .url("https://example.com/video")
            .title(Some("Test Video".to_string()))
            .file_path(Some(PathBuf::from("/path/video.mp4")))
            .file_size(Some(1024))
            .duration_secs(Some(120.5))
            .status(DownloadStatus::Completed)
            .error_message(None)
            .build()
            .unwrap();

        assert_eq!(entry.url, "https://example.com/video");
        assert_eq!(entry.title, Some("Test Video".to_string()));
        assert_eq!(entry.file_path, Some(PathBuf::from("/path/video.mp4")));
        assert_eq!(entry.file_size, Some(1024));
        assert_eq!(entry.duration_secs, Some(120.5));
        assert_eq!(entry.status, DownloadStatus::Completed);
        assert_eq!(entry.error_message, None);
    }

    #[test]
    fn test_add_entry_with_builder() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut history = History::new(temp_dir.path()).unwrap();

        let entry = HistoryEntryBuilder::new()
            .url("https://example.com/builder-test")
            .title(Some("Builder Test".to_string()))
            .status(DownloadStatus::Completed)
            .build()
            .unwrap();

        history.add_entry(entry).unwrap();

        assert_eq!(history.history_count(), 1);
        let found = history.search("builder-test");
        assert_eq!(found.len(), 1);
    }
}
