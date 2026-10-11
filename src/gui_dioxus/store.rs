//! 全局状态：History 单例、配置热重载版本、系统通知

use std::path::PathBuf;

use tracing::{info, warn};

use crate::config::AppConfig;
use crate::history::{DownloadStatus, History, HistoryEntry, Platform};

use super::types::HistoryItem;

/// 配置热重载状态
static CONFIG_VERSION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// 全局 History 实例（使用 RwLock 区分读写,UI 热路径不再互斥）
///
/// **2026-10-11 audit**：原 `std::sync::Mutex` 在读多写少场景下让 UI 重渲染
/// 与后台下载写入互相排队。读 (`get_history_items` 每次 UI frame 都调用,
/// 锁持有期间含 `clone`/`format!`/`chrono::format`) 远远多于写 (初始化 1 次 +
/// 下载完成时 `add_completed_download`)。改 `RwLock` 后多 UI 帧可并发读,
/// 写线程仅在下载完成时短暂 wait,与 Mutex 相比代价更低。
///
/// **invariant 维持**：所有访问函数仍是同步调用,锁持有区间内**无任何 `.await`**,
/// 不构成"持锁跨 await"。`RwLock::read()` 在当前调度下也不会长时间等待写锁,
/// 因为下载完成事件频率 << UI 渲染频率。
///
/// 若未来改 async,需重新评估是否换 `tokio::sync::RwLock`(同 §Mutex 推理)。
static GLOBAL_HISTORY: std::sync::RwLock<Option<History>> = std::sync::RwLock::new(None);

/// 添加下载完成记录到历史（自动清理超过 100 条的旧记录）
const MAX_HISTORY_ENTRIES: usize = 100;

/// 初始化全局 History
pub(super) fn init_global_history(history: History) {
    let mut h = GLOBAL_HISTORY
        .write()
        .expect("init_global_history: failed to lock GLOBAL_HISTORY");
    *h = Some(history);
}

pub(super) fn add_completed_download(title: String, url: String, path: Option<PathBuf>) {
    // SAFETY: 锁持有区间内无 `.await` 调用，不阻塞 executor
    let mut h = GLOBAL_HISTORY
        .write()
        .expect("add_completed_download: failed to lock GLOBAL_HISTORY");
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
    if let Ok(h) = GLOBAL_HISTORY.read() {
        if let Some(history) = h.as_ref() {
            let entries = history.get_entries();
            return entries
                .iter()
                .rev()
                .map(|e| {
                    let duration = e
                        .duration_secs
                        .map(|d| format!("{}:{:02}", (d as i64) / 60, (d as i64) % 60))
                        .unwrap_or_default();
                    let completed_at = e.downloaded_at.format("%Y-%m-%d %H:%M").to_string();
                    let platform_name = format!("{:?}", e.platform);
                    HistoryItem {
                        title: e.title.clone().unwrap_or_default(),
                        uploader: platform_name,
                        duration,
                        completed_at,
                        url: e.url.clone(),
                    }
                })
                .collect();
        }
    }
    Vec::new()
}

/// 清空历史
pub(super) fn clear_global_history() {
    // SAFETY: 锁持有区间内无 `.await` 调用，不阻塞 executor
    if let Ok(mut h) = GLOBAL_HISTORY.write() {
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

/// 持久化主题到配置文件。
/// P1-2:4 个 ThemeSwatch onclick 共享此 helper,避免重复 AppConfig::load/save 样板。
/// 注:store.write() 需要 dioxus component 上下文,无法在普通函数内调用;
/// 调用方必须先写 store 再调用此函数持久化。
pub(super) fn persist_theme(theme_name: &str) {
    if let Ok(mut cfg) = AppConfig::load() {
        cfg.theme = theme_name.to_string();
        if let Err(e) = cfg.save() {
            warn!("持久化主题失败: {}", e);
        }
    }
    info!("Theme applied: {}", theme_name);
}

/// AppleScript 字符串转义:先转反斜杠(否则后面替换 \\\" 时反斜杠会被重复 escape),
/// 再转双引号;换行/制表等控制字符替换为空格,防注入到 AppleScript 字符串外。
fn escape_applescript_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

/// 发送系统通知 (使用 macOS osascript)
pub(super) fn send_notification(title: &str, body: &str) {
    let script = format!(
        "display notification \"{}\" with title \"{}\"",
        escape_applescript_string(body),
        escape_applescript_string(title)
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
        let mut guard = GLOBAL_HISTORY.write().expect("test lock");
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

    /// escape_applescript_string 必须按顺序:反斜杠 → 双引号 → 控制字符
    /// (反斜杠先转义,否则替换 \" 时反斜杠会被重复 escape;控制字符防注入到字符串外)
    #[test]
    fn test_escape_applescript_string_handles_quotes_and_backslashes() {
        // 反斜杠优先于双引号
        assert_eq!(escape_applescript_string("a\\b"), "a\\\\b");
        // 双引号转义
        assert_eq!(escape_applescript_string("say \"hi\""), "say \\\"hi\\\"");
        // 反斜杠 + 双引号 组合:每个反斜杠必须变 \\,每个引号必须变 \"
        assert_eq!(escape_applescript_string("\\\"x"), "\\\\\\\"x");
    }

    #[test]
    fn test_escape_applescript_string_replaces_control_chars() {
        // 换行/制表符/回车 → 空格(防 AppleScript 字符串边界逃逸)
        assert_eq!(escape_applescript_string("line1\nline2"), "line1 line2");
        assert_eq!(escape_applescript_string("a\tb"), "a b");
        assert_eq!(escape_applescript_string("a\rb"), "a b");
    }

    #[test]
    fn test_escape_applescript_string_preserves_normal_text() {
        // ASCII 中文 空格 不应被改动
        assert_eq!(escape_applescript_string("下载完成"), "下载完成");
        assert_eq!(escape_applescript_string("hello world"), "hello world");
    }
}
