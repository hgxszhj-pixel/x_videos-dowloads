//! GUI 显示用的数据结构与状态定义

use super::theme::AppTheme;

/// 下载历史记录项
#[derive(Default, Clone)]
pub(super) struct HistoryItem {
    pub(super) title: String,
    pub(super) uploader: String,
    pub(super) duration: String,
    pub(super) path: String,
    pub(super) completed_at: String,
}

/// Video info for display
#[derive(Default, Clone)]
pub(super) struct VideoInfoDisplay {
    pub(super) url: String,
    pub(super) original_url: Option<String>,  // 原始 URL（用于 yt-dlp 格式选择）
    pub(super) title: String,
    pub(super) uploader: String,
    pub(super) duration: String,
    pub(super) formats: Vec<VideoFormatDisplay>,  // 存储可选择的格式列表
    pub(super) download_strategy: crate::types::DownloadStrategy,
}

#[derive(Default, Clone)]
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

pub(super) struct AppState {
    pub(super) url: String,
    pub(super) status_message: String,
    pub(super) error_message: String,
    pub(super) is_fetching: bool,
    pub(super) is_downloading: bool,
    pub(super) has_video_info: bool,
    pub(super) download_progress: f32,
    pub(super) download_speed: String,
    pub(super) is_batch_mode: bool,
    pub(super) batch_state: BatchState,
    pub(super) batch_video_state: BatchVideoState,
    pub(super) theme: AppTheme,
    pub(super) history: Vec<HistoryItem>,
    pub(super) show_history: bool,
    pub(super) selected_format_id: Option<String>,  // 用户选择的视频格式ID
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
#[derive(Default, Clone)]
pub(super) struct ErrorDisplay {
    pub(super) error_type: String,   // "network" | "auth" | "parse" | "download" | "unknown"
    pub(super) message: String,       // 具体错误信息
    pub(super) suggestion: String,    // 解决建议
}

/// 单个批量下载项的状态
#[derive(Clone)]
pub(super) struct BatchItemState {
    pub(super) url: String,
    pub(super) title: String,
    pub(super) status: String,        // "pending" | "downloading" | "completed" | "failed"
    pub(super) progress: f32,         // 0.0 - 1.0
    pub(super) speed: String,
    pub(super) visible: bool,
    // 原子进度追踪（跨 task 共享）
    pub(super) progress_arc: Option<std::sync::Arc<std::sync::atomic::AtomicU32>>, // 0-10000 表示 0.00%-100.00%
    pub(super) speed_arc: Option<std::sync::Arc<std::sync::atomic::AtomicU64>>, // 字节/秒
}

/// 批量下载进度状态
#[derive(Default, Clone)]
pub(crate) struct BatchState {
    pub(super) visible: bool,
    pub(super) progress: f32,         // 总体进度
    pub(super) status: String,
    pub(super) items: Vec<BatchItemState>,
    pub(super) is_downloading: bool,
}

/// 批量视频信息状态
#[derive(Default, Clone)]
pub(crate) struct BatchVideoState {
    pub(super) visible: bool,
    pub(super) items: Vec<VideoInfoDisplay>,
    pub(super) status: String,
}
