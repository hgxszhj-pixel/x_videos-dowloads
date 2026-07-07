//! 全局状态：History 单例、配置热重载版本、系统通知

use std::path::PathBuf;

use tracing::{info, warn};

use crate::config::AppConfig;
use crate::history::{DownloadStatus, History, HistoryEntry, Platform};

use super::types::HistoryItem;

/// 配置热重载状态
static CONFIG_VERSION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// 全局 History 实例（使用 Mutex 支持内部可变）
static GLOBAL_HISTORY: std::sync::Mutex<Option<History>> = std::sync::Mutex::new(None);

/// 添加下载完成记录到历史（自动清理超过 100 条的旧记录）
const MAX_HISTORY_ENTRIES: usize = 100;

/// 初始化全局 History
pub(super) fn init_global_history(history: History) {
    let mut h = GLOBAL_HISTORY.lock().expect("init_global_history: failed to lock GLOBAL_HISTORY");
    *h = Some(history);
}

pub(super) fn add_completed_download(title: String, url: String, path: Option<PathBuf>) {
    if let Ok(mut h) = GLOBAL_HISTORY.lock() {
        if let Some(history) = h.as_mut() {
            let entry = HistoryEntry {
                id: uuid::Uuid::new_v4(),
                url,
                title: Some(title.clone()),
                file_path: path,
                file_size: None,
                downloaded_at: chrono::Utc::now(),
                duration_secs: None,
                status: DownloadStatus::Completed,
                platform: Platform::Other,
                error_message: None,
            };
            let _ = history.add_entry_with_auto_cleanup(entry, MAX_HISTORY_ENTRIES);
        }
    }
}

/// 获取全局历史的 UI 项目（用于显示）
pub(super) fn get_history_items() -> Vec<HistoryItem> {
    if let Ok(h) = GLOBAL_HISTORY.lock() {
        if let Some(history) = h.as_ref() {
            let entries = history.get_entries();
            return entries.iter().rev().map(|e| {
                let duration = e.duration_secs.map(|d| format!("{}:{:02}", (d as i64) / 60, (d as i64) % 60)).unwrap_or_default();
                let completed_at = e.downloaded_at.format("%Y-%m-%d %H:%M").to_string();
                let path = e.file_path.as_ref().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
                let platform_name = format!("{:?}", e.platform);
                HistoryItem {
                    title: e.title.clone().unwrap_or_default(),
                    uploader: platform_name,
                    duration,
                    path,
                    completed_at,
                }
            }).collect();
        }
    }
    Vec::new()
}

/// 清空历史
pub(super) fn clear_global_history() {
    if let Ok(mut h) = GLOBAL_HISTORY.lock() {
        if let Some(history) = h.as_mut() {
            let _ = history.clear();
        }
    }
}

/// 重新加载配置并触发版本递增
pub(super) fn reload_config() {
    if let Ok(_config) = AppConfig::load() {
        // 更新全局配置版本，UI 会检测到变化
        CONFIG_VERSION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        info!("Configuration reloaded successfully");
    }
}

/// 发送系统通知 (使用 macOS osascript)
pub(super) fn send_notification(title: &str, body: &str) {
    let script = format!(
        "display notification \"{}\" with title \"{}\"",
        body.replace("\"", "\\\""),
        title.replace("\"", "\\\"")
    );
    if let Err(e) = std::process::Command::new("osascript")
        .args(["-e", &script])
        .output()
    {
        warn!("Failed to send notification: {}", e);
    }
}
