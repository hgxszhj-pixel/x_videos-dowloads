//! 下载历史和书签模块

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use uuid::Uuid;

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
        if self.history_file.exists() {
            let content = std::fs::read_to_string(&self.history_file)?;
            self.entries = serde_json::from_str(&content).unwrap_or_default();
        }
        if self.bookmark_file.exists() {
            let content = std::fs::read_to_string(&self.bookmark_file)?;
            self.bookmarks = serde_json::from_str(&content).unwrap_or_default();
        }
        Ok(())
    }

    /// 保存历史记录
    fn save(&self) -> Result<()> {
        std::fs::create_dir_all(self.history_file.parent().unwrap())?;
        std::fs::write(&self.history_file, serde_json::to_string_pretty(&self.entries)?)?;
        std::fs::write(&self.bookmark_file, serde_json::to_string_pretty(&self.bookmarks)?)?;
        Ok(())
    }

    /// 添加下载历史
    pub fn add_entry(&mut self, url: String, title: Option<String>, file_path: Option<PathBuf>, file_size: Option<u64>, duration_secs: Option<f64>) -> Result<()> {
        let entry = HistoryEntry {
            id: Uuid::new_v4(),
            url,
            title,
            file_path,
            file_size,
            downloaded_at: Utc::now(),
            duration_secs,
        };
        self.entries.insert(0, entry); // 最近的在前面
        self.save()?;
        Ok(())
    }

    /// 获取最近 N 条历史
    pub fn get_recent(&self, count: usize) -> &[HistoryEntry] {
        &self.entries[..count.min(self.entries.len())]
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
    pub fn history_count(&self) -> usize {
        self.entries.len()
    }

    /// 获取书签条数
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

        history.add_entry(
            "https://example.com/video1".to_string(),
            Some("Video 1".to_string()),
            Some(PathBuf::from("/path/video1.mp4")),
            Some(1024),
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
}
