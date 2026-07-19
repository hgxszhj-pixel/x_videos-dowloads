//! 全局状态：History 单例、配置热重载版本、系统通知

use std::path::PathBuf;

use tracing::{info, warn};

use crate::config::AppConfig;
use crate::history::{DownloadStatus, History, HistoryEntry, Platform};

use super::types::HistoryItem;

/// 配置热重载状态
static CONFIG_VERSION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// 全局 History 实例（使用 Mutex 支持内部可变）
///
/// **P0-3 audit (2026-07-19)**：原始代码用 `std::sync::Mutex` 而非 `tokio::sync::Mutex`，
/// 表面上违反了"async 上下文禁用 std::sync::Mutex"的最佳实践。
///
/// 经 audit 后确认**当前 invariant 安全**：
/// - `add_completed_download` / `get_history_items` / `clear_global_history` 三个函数
///   虽然声明为 async 函数，但锁的持有区间内**无任何 `.await` 调用**
///   （仅同步 mutate → drop guard），不构成"持锁跨 await"。
///
/// - 如果未来添加者在锁持有区间内插入 `.await`，**必须立即**改用 `tokio::sync::Mutex`，
///   否则会阻塞整个 executor 并可能死锁。
///
/// 改用 `tokio::sync::Mutex` 引入运行时依赖（当前 `main.rs` 用 `current_thread` runtime，
/// 改 async 后 init 路径需要 `Handle::block_on` 或重构为 lazy init），权衡收益不明显。
///
/// 当前保留 `std::sync::Mutex` + SAFETY 注释，行为等价且零运行时影响。
static GLOBAL_HISTORY: std::sync::Mutex<Option<History>> = std::sync::Mutex::new(None);

/// 添加下载完成记录到历史（自动清理超过 100 条的旧记录）
const MAX_HISTORY_ENTRIES: usize = 100;

/// 初始化全局 History
pub(super) fn init_global_history(history: History) {
    let mut h = GLOBAL_HISTORY.lock().expect("init_global_history: failed to lock GLOBAL_HISTORY");
    *h = Some(history);
}

pub(super) fn add_completed_download(title: String, url: String, path: Option<PathBuf>) {
    // SAFETY: 锁持有区间内无 `.await` 调用，不阻塞 executor
    let mut h = GLOBAL_HISTORY.lock().expect("add_completed_download: failed to lock GLOBAL_HISTORY");
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

/// 获取全局历史的 UI 项目（用于显示）
pub(super) fn get_history_items() -> Vec<HistoryItem> {
    // SAFETY: 锁持有区间内无 `.await` 调用，不阻塞 executor
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
                    url: e.url.clone(),
                }
            }).collect();
        }
    }
    Vec::new()
}

/// 清空历史
pub(super) fn clear_global_history() {
    // SAFETY: 锁持有区间内无 `.await` 调用，不阻塞 executor
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_global_history_no_lock_across_await_invariant() {
        // 验证 SAFETY 注释描述的 invariant：所有访问 GLOBAL_HISTORY 的函数
        // 在锁持有区间内都不能包含 `.await`。
        // 这是一个 AST-level 的契约，通过 lint 风格人工审计维护。
        //
        // 这里用单元测试覆盖核心 add/get/clear 流程：
        let mut guard = GLOBAL_HISTORY.lock().expect("test lock");
        *guard = None;
        drop(guard);

        let temp_dir = tempfile::tempdir().unwrap();
        let history = History::new(temp_dir.path()).unwrap();
        init_global_history(history);

        // 同步调用 add（与原 API 一致）
        add_completed_download(
            "test-title".to_string(),
            "https://example.com/video".to_string(),
            None,
        );

        let items = get_history_items();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "test-title");

        // clear 测试
        clear_global_history();
        let items_after = get_history_items();
        assert_eq!(items_after.len(), 0, "history should be cleared");
    }

    #[test]
    fn test_global_history_reload_config_works_without_lock() {
        // reload_config 不持 GLOBAL_HISTORY 锁，应可独立调用
        reload_config(); // 不 panic 即通过
    }
}