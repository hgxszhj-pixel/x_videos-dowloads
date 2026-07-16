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
    pub(super) path: String,
    pub(super) completed_at: String,
    pub(super) url: String,
}

/// Video info for display
#[derive(Default, Clone, Store)]
pub(super) struct VideoInfoDisplay {
    pub(super) url: String,
    pub(super) original_url: Option<String>,  // 原始 URL（用于 yt-dlp 格式选择）
    pub(super) title: String,
    pub(super) uploader: String,
    pub(super) duration: String,
    pub(super) formats: Vec<VideoFormatDisplay>,  // 存储可选择的格式列表
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

#[derive(Store)]
pub(super) struct AppState {
    pub(super) url: String,
    pub(super) status_message: String,
    pub(super) error_message: String,
    pub(super) is_fetching: bool,
    pub(super) is_downloading: bool,
    pub(super) has_video_info: bool,
    pub(super) download_progress: f32,
    pub(super) download_speed: String,
    pub(super) download_file_size: String,
    pub(super) cancel_flag: Option<Arc<AtomicBool>>,
    pub(super) error_display: Option<ErrorDisplay>,
    pub(super) show_settings: bool,
    pub(super) settings_output_dir: String,
    pub(super) settings_ms_token: String,
    pub(super) settings_cookie_file: String,
    pub(super) settings_concurrent: u32,
    pub(super) is_batch_mode: bool,
    pub(super) batch_state: BatchState,
    pub(super) batch_video_state: BatchVideoState,
    pub(super) theme: AppTheme,
    pub(super) history: Vec<HistoryItem>,
    pub(super) show_history: bool,
    pub(super) selected_format_id: Option<String>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            url: String::new(),
            status_message: "Ready".to_string(),
            error_message: String::new(),
            is_fetching: false,
            is_downloading: false,
            has_video_info: false,
            download_progress: 0.0,
            download_speed: String::new(),
            download_file_size: String::new(),
            cancel_flag: None,
            error_display: None,
            show_settings: false,
            settings_output_dir: String::new(),
            settings_ms_token: String::new(),
            settings_cookie_file: String::new(),
            settings_concurrent: 3,
            is_batch_mode: false,
            batch_state: BatchState::default(),
            batch_video_state: BatchVideoState::default(),
            theme: AppTheme::default(),
            history: Vec::new(),
            show_history: false,
            selected_format_id: None,
        }
    }
}

/// 错误信息结构
#[derive(Default, Clone, Store)]
pub(super) struct ErrorDisplay {
    pub(super) error_type: String,   // "network" | "auth" | "parse" | "download" | "unknown"
    pub(super) message: String,       // 具体错误信息
    pub(super) suggestion: String,    // 解决建议
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
    pub(super) fn new(error_type: &str, message: impl Into<String>, suggestion: impl Into<String>) -> Self {
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
            "parse" => "🔍",
            "download" => "📥",
            _ => "⚠️",
        }
    }
}

/// 单个批量下载项的状态
#[derive(Clone, Store)]
pub(super) struct BatchItemState {
    pub(super) url: String,
    pub(super) title: String,
    pub(super) status: String,        // "pending" | "downloading" | "completed" | "failed" | "cancelled"
    pub(super) progress: f32,         // 0.0 - 1.0
    pub(super) speed: String,
    pub(super) visible: bool,
    // 原子进度追踪（跨 task 共享）
    pub(super) progress_arc: Option<std::sync::Arc<std::sync::atomic::AtomicU32>>, // 0-10000 表示 0.00%-100.00%
    pub(super) speed_arc: Option<std::sync::Arc<std::sync::atomic::AtomicU64>>, // 字节/秒
    // 单项取消标志（点击行末 ✕ 触发 store(true)）
    pub(super) cancel_flag: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
}

/// 批量下载进度状态
#[derive(Default, Clone, Store)]
pub(crate) struct BatchState {
    pub(super) visible: bool,
    pub(super) progress: f32,         // 总体进度
    pub(super) status: String,
    pub(super) items: Vec<BatchItemState>,
    pub(super) is_downloading: bool,
}

/// 批量视频信息状态
#[derive(Default, Clone, Store)]
pub(crate) struct BatchVideoState {
    pub(super) visible: bool,
    pub(super) items: Vec<VideoInfoDisplay>,
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
