//! GUI 显示用的数据结构与状态定义

use super::theme::AppTheme;
use dioxus_stores::Store;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// 下载历史记录项
#[derive(Default, Clone, Store)]
pub(super) struct HistoryItem {
    pub(super) title: String,
    pub(super) uploader: String,
    pub(super) duration: String,
    pub(super) completed_at: String,
    pub(super) url: String,
}

/// Video info for display
#[derive(Default, Clone, Store)]
pub(super) struct VideoInfoDisplay {
    pub(super) url: String,
    pub(super) original_url: Option<String>, // 原始 URL（用于 yt-dlp 格式选择）
    pub(super) title: String,
    pub(super) uploader: String,
    pub(super) duration: String,
    pub(super) formats: Vec<VideoFormatDisplay>, // 存储可选择的格式列表
    pub(super) download_strategy: crate::types::DownloadStrategy,
}

#[derive(Default, Clone, Store)]
pub(super) struct VideoFormatDisplay {
    pub(super) format_id: String,
    pub(super) resolution: String,
    pub(super) ext: String,
    pub(super) filesize: String,
}

/// 格式化文件大小
pub(super) fn format_file_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// 格式化下载速度 (bytes/s -> "X.XX MiB/s")
pub(super) fn format_speed(bytes_per_sec: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    if bytes_per_sec >= GB {
        format!("{:.2} GiB/s", bytes_per_sec as f64 / GB as f64)
    } else if bytes_per_sec >= MB {
        format!("{:.2} MiB/s", bytes_per_sec as f64 / MB as f64)
    } else if bytes_per_sec >= KB {
        format!("{:.2} KiB/s", bytes_per_sec as f64 / KB as f64)
    } else {
        format!("{} B/s", bytes_per_sec)
    }
}

/// 格式化剩余时间 (seconds -> "5s" / "2m 5s" / "1h 2m")
pub(super) fn format_eta(seconds: u64) -> String {
    if seconds < 60 {
        format!("{}s", seconds)
    } else if seconds < 3600 {
        format!("{}m {}s", seconds / 60, seconds % 60)
    } else {
        format!("{}h {}m", seconds / 3600, (seconds % 3600) / 60)
    }
}

/// 格式化大小分数 ("X.XX MB / Y.YY GB")。total=0 时只显示 downloaded。
pub(super) fn format_size_fraction(downloaded: u64, total: u64) -> String {
    if total == 0 || downloaded >= total {
        format_file_size(downloaded)
    } else {
        format!(
            "{} / {}",
            format_file_size(downloaded),
            format_file_size(total)
        )
    }
}

#[derive(Store)]
pub(super) struct AppState {
    pub(super) url: String,
    pub(super) status_message: String,
    pub(super) is_fetching: bool,
    pub(super) is_downloading: bool,
    pub(super) has_video_info: bool,
    // P0-2:download_progress/speed/file_size 已迁移到顶层 Signal,
    // 避免整个 AppComponent rsx 重渲染。删除 AppState 字段。
    pub(super) cancel_flag: Option<Arc<AtomicBool>>,
    pub(super) error_display: Option<ErrorDisplay>,
    pub(super) show_settings: bool,
    pub(super) settings_output_dir: String,
    pub(super) settings_ms_token: String,
    pub(super) settings_cookie_file: String,
    pub(super) settings_concurrent: u32,
    pub(super) batch_state: BatchState,
    pub(super) batch_video_state: BatchVideoState,
    pub(super) course_collection_state: CourseCollectionState,
    pub(super) theme: AppTheme,
    pub(super) history: Vec<HistoryItem>,
    pub(super) show_history: bool,
    pub(super) selected_format_id: Option<String>,
    pub(super) is_logging_in: bool,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            url: String::new(),
            status_message: "Ready".to_string(),
            is_fetching: false,
            is_downloading: false,
            has_video_info: false,
            cancel_flag: None,
            error_display: None,
            show_settings: false,
            settings_output_dir: String::new(),
            settings_ms_token: String::new(),
            settings_cookie_file: String::new(),
            settings_concurrent: 3,
            batch_state: BatchState::default(),
            batch_video_state: BatchVideoState::default(),
            course_collection_state: CourseCollectionState::default(),
            theme: AppTheme::default(),
            history: Vec::new(),
            show_history: false,
            selected_format_id: None,
            is_logging_in: false,
        }
    }
}

/// 错误信息结构
#[derive(Default, Clone, Store)]
pub(super) struct ErrorDisplay {
    pub(super) error_type: String, // "network" | "auth" | "login" | "course" | "parse" | "download" | "unknown"
    pub(super) message: String,    // 具体错误信息
    pub(super) suggestion: String, // 解决建议
}

/// 从结构化 DownloadError 转换为 GUI ErrorDisplay
impl From<crate::types::DownloadError> for ErrorDisplay {
    fn from(err: crate::types::DownloadError) -> Self {
        use crate::types::DownloadErrorType::*;
        let error_type = match err.error_type {
            Network | Http(_) | RateLimited => "network",
            ContentUnavailable => "parse",
            Authentication => "auth",
            DiskFull | DiskIo => "download",
            Cancelled => "download",
            YtDlp(_) => "download",
            Unknown(_) => "unknown",
        }
        .to_string();
        Self {
            error_type,
            message: err.message,
            suggestion: err.action.unwrap_or_default(),
        }
    }
}

/// 通用错误转 ErrorDisplay（带类型提示）
impl ErrorDisplay {
    pub(super) fn new(
        error_type: &str,
        message: impl Into<String>,
        suggestion: impl Into<String>,
    ) -> Self {
        Self {
            error_type: error_type.to_string(),
            message: message.into(),
            suggestion: suggestion.into(),
        }
    }

    /// 根据错误类型返回对应图标
    pub(super) fn icon(&self) -> &'static str {
        match self.error_type.as_str() {
            "network" => "🌐",
            "auth" => "🔐",
            "login" => "🔑",
            "course" => "📚",
            "parse" => "🔍",
            "download" => "📥",
            _ => "⚠️",
        }
    }

    /// 根据错误类型返回类型标签(用于面板 heading,替代硬编码 "下载失败")
    pub(super) fn label(&self) -> &'static str {
        match self.error_type.as_str() {
            "network" => "网络错误",
            "auth" => "认证失败",
            "login" => "需要登录",
            "course" => "课程解析失败",
            "parse" => "URL 解析失败",
            "download" => "下载失败",
            _ => "发生错误",
        }
    }
}

/// 单个批量下载项的状态
#[derive(Clone, Store)]
pub(super) struct BatchItemState {
    pub(super) status: String, // "pending" | "resolving" | "downloading" | "completed" | "failed" | "cancelled"
    pub(super) progress: f32,  // 0.0 - 1.0
    pub(super) speed: String,
    pub(super) visible: bool,
    pub(super) speed_arc: Option<std::sync::Arc<std::sync::atomic::AtomicU64>>, // 字节/秒
    // 单项取消标志（点击行末 ✕ 触发 store(true)）
    pub(super) cancel_flag: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
}

/// 批量下载进度状态
#[derive(Default, Clone, Store)]
pub(crate) struct BatchState {
    pub(super) visible: bool,
    pub(super) progress: f32, // 总体进度
    pub(super) status: String,
    pub(super) items: Vec<BatchItemState>,
    /// 聚合下载速度（所有 active 项 speed_arc 之和 → format_speed）。
    /// 由 poller.rs:130 同步计算并写入。"" 表示无 active 下载。
    pub(super) aggregate_speed: String,
}

/// 批量视频信息状态
#[derive(Default, Clone, Store)]
pub(crate) struct BatchVideoState {
    pub(super) visible: bool,
    pub(super) items: Vec<VideoInfoDisplay>,
}

/// 课程池单条课节显示
#[derive(Default, Clone, Store)]
pub(super) struct CourseItemDisplay {
    pub(super) id: String,
    pub(super) index: u32, // 1-based 序号
    pub(super) title: String,
    pub(super) page_url: String, // 课节页 URL（用于实时 CDP 解析）
    /// 课节真实直接访问 URL(从 list_collection 的 jump_url 透传)
    /// 优先用这个而非 page_url+resource_id (会触发登录跳转)
    pub(super) jump_url: Option<String>,
    pub(super) selected: bool, // 默认全选
    pub(super) status: String, // "pending" | "resolving" | "downloading" | "completed" | "failed" | "cancelled"
    pub(super) progress: f32,
    /// 进度槽（跨 task 共享，use_effect 轮询把原子值写回 progress/status）
    pub(super) slot: Option<Arc<super::handlers::CourseItemSlot>>,
    /// 实时下载速度（polling 写回,format_speed 输出）
    pub(super) speed: String,
    /// 已下载字节数（progress × total_bytes 估算）
    pub(super) downloaded_bytes: u64,
    /// 总字节数（slot.total_bytes 写入）
    pub(super) total_bytes: u64,
}

/// 课程池整体状态
#[derive(Default, Clone, Store)]
pub(crate) struct CourseCollectionState {
    pub(super) visible: bool,
    pub(super) items: Vec<CourseItemDisplay>,
    pub(super) status: String,
}

/// fetch action 的返回值（单 URL vs 批量 URL）
#[derive(Clone)]
pub(super) enum FetchResult {
    /// 单个视频信息
    Single(VideoInfoDisplay),
    /// 批量视频信息列表
    Batch(Vec<VideoInfoDisplay>),
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ErrorDisplay::label 必须按 error_type 返回中文类型标签,未知类型回退 "发生错误"
    /// (避免 components.rs 硬编码 "下载失败" 让 login/course/parse 等错误误标)。
    #[test]
    fn test_error_display_label_covers_all_known_types() {
        assert_eq!(ErrorDisplay::new("network", "", "").label(), "网络错误");
        assert_eq!(ErrorDisplay::new("auth", "", "").label(), "认证失败");
        assert_eq!(ErrorDisplay::new("login", "", "").label(), "需要登录");
        assert_eq!(ErrorDisplay::new("course", "", "").label(), "课程解析失败");
        assert_eq!(ErrorDisplay::new("parse", "", "").label(), "URL 解析失败");
        assert_eq!(ErrorDisplay::new("download", "", "").label(), "下载失败");
    }

    #[test]
    fn test_error_display_label_falls_back_for_unknown_type() {
        // 未来新增 error_type 不会被 panic;icon() 同样回退 ⚠️,label 同样回退 "发生错误"。
        assert_eq!(
            ErrorDisplay::new("future_thing", "", "").label(),
            "发生错误"
        );
        assert_eq!(ErrorDisplay::new("", "", "").label(), "发生错误");
    }

    #[test]
    fn test_error_display_icon_and_label_are_consistent() {
        // icon() 和 label() 对同一 error_type 必须同时命中已知分支(防止 6 个值改了 label 漏改 icon)。
        let cases = ["network", "auth", "login", "course", "parse", "download"];
        for t in cases {
            let e = ErrorDisplay::new(t, "", "");
            assert_ne!(
                e.icon(),
                "⚠️",
                "{t} should have a specific icon, not fallback"
            );
            assert_ne!(
                e.label(),
                "发生错误",
                "{t} should have a specific label, not fallback"
            );
        }
    }

    /// format_eta 边界覆盖 (60s/3600s 进位规则)
    /// 锁定"为什么这样格式化":poller.rs:74 ETA 计算后直接 display,改格式必须同步改测试
    #[test]
    fn test_format_eta_subminute() {
        assert_eq!(format_eta(0), "0s");
        assert_eq!(format_eta(5), "5s");
        assert_eq!(format_eta(59), "59s");
    }

    #[test]
    fn test_format_eta_minutes() {
        assert_eq!(format_eta(60), "1m 0s"); // 60s 进位边界
        assert_eq!(format_eta(125), "2m 5s");
        assert_eq!(format_eta(3599), "59m 59s"); // 1h 进位前边界
    }

    #[test]
    fn test_format_eta_hours() {
        assert_eq!(format_eta(3600), "1h 0m"); // 1h 进位边界
        assert_eq!(format_eta(7325), "2h 2m"); // 7325s = 2*3600 + 2*60 + 5 → 显示 "2h 2m" (秒数省略)
    }

    /// format_size_fraction 三分支:total=0 fallback、normal fraction、downloaded>=total fallback
    /// 锁定"为什么"大小列在 unknown 时显示"0 B"还是"—"——本函数当前选 "0 B",UI 单独 hide 该列
    #[test]
    fn test_format_size_fraction_total_zero() {
        // 未知 total 时只显示 downloaded,UI 端单独 hide 该列(避免"0 B"误导)
        assert_eq!(format_size_fraction(0, 0), "0 B");
        assert_eq!(format_size_fraction(5 * 1024 * 1024, 0), "5.00 MB");
    }

    #[test]
    fn test_format_size_fraction_normal() {
        assert_eq!(
            format_size_fraction(5 * 1024 * 1024, 10 * 1024 * 1024),
            "5.00 MB / 10.00 MB"
        );
    }

    #[test]
    fn test_format_size_fraction_downloaded_exceeds_total() {
        // 边界:已下载 >= 总(可能 server 报 length 偏小,或 progress 计算误差)
        // → fallback 到只显示 downloaded,避免"5 MB / 3 MB"反向视觉
        assert_eq!(
            format_size_fraction(5 * 1024 * 1024, 3 * 1024 * 1024),
            "5.00 MB"
        );
    }
}
