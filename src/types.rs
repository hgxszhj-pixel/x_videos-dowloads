//! X Video Downloader - 共享类型定义
//!
//! 项目中使用的核心数据类型定义

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

/// 默认 User-Agent 常量
pub const DEFAULT_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

/// 应用错误类型
#[derive(Debug, Error)]
#[allow(dead_code)]
pub enum AppError {
    #[error("网络错误: {0}")]
    Network(#[from] reqwest::Error),

    #[error("IO错误: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("yt-dlp错误: {0}")]
    YtDlp(String),
    
    #[error("URL验证失败: {0}")]
    InvalidUrl(String),
    
    #[error("文件路径无效: {0}")]
    InvalidPath(String),
    
    #[error("下载失败: {0}")]
    DownloadFailed(String),
    
    #[error("配置错误: {0}")]
    Config(String),
    
    #[error("未知错误: {0}")]
    Unknown(String),
}

impl From<anyhow::Error> for AppError {
    fn from(e: anyhow::Error) -> Self {
        AppError::Unknown(e.to_string())
    }
}

impl From<String> for AppError {
    fn from(s: String) -> Self {
        AppError::Unknown(s)
    }
}

impl From<&str> for AppError {
    fn from(s: &str) -> Self {
        AppError::Unknown(s.to_string())
    }
}

/// 视频信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoInfo {
    /// 视频标题
    pub title: String,
    /// 视频描述
    pub description: Option<String>,
    /// 视频作者/上传者
    pub uploader: Option<String>,
    /// 视频时长（秒）
    pub duration: Option<u64>,
    /// 视频URL
    pub url: String,
    /// 缩略图URL
    pub thumbnail: Option<String>,
    /// 可用格式列表
    pub formats: Vec<VideoFormat>,
    /// 创建时间
    pub created_at: Option<String>,
}

impl VideoInfo {
    /// 创建新的视频信息
    pub fn new(title: String, url: String) -> Self {
        Self {
            title,
            description: None,
            uploader: None,
            duration: None,
            url,
            thumbnail: None,
            formats: Vec::new(),
            created_at: Some(chrono::Utc::now().to_rfc3339()),
        }
    }
}

/// 视频格式
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoFormat {
    /// 格式ID
    pub format_id: String,
    /// 格式描述
    pub ext: String,
    /// 分辨率
    pub resolution: Option<String>,
    /// 文件大小（字节）
    pub filesize: Option<u64>,
    /// 比特率
    pub bitrate: Option<u64>,
    /// 编码格式
    pub codec: Option<String>,
    /// 是否是音频
    pub audio_only: bool,
    /// 格式备注
    pub format_note: Option<String>,
}

/// 下载请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadRequest {
    /// 视频 URL
    pub url: String,
    /// 输出文件路径
    pub output_path: PathBuf,
    /// 视频格式ID（可选）
    pub format_id: Option<String>,
    /// 仅音频
    pub audio_only: bool,
    /// 下载字幕
    pub download_subtitles: bool,
    /// 自定义User-Agent
    pub user_agent: Option<String>,
    /// Cookie文件路径
    pub cookie_file: Option<PathBuf>,
}

impl DownloadRequest {
    /// 创建新的下载请求
    pub fn new(url: impl Into<String>, output_path: impl Into<PathBuf>) -> Self {
        Self {
            url: url.into(),
            output_path: output_path.into(),
            format_id: None,
            audio_only: false,
            download_subtitles: false,
            user_agent: None,
            cookie_file: None,
        }
    }

    /// 设置格式ID
    #[allow(dead_code)]
    pub fn with_format_id(mut self, format_id: String) -> Self {
        self.format_id = Some(format_id);
        self
    }

    /// 仅下载音频
    #[allow(dead_code)]
    pub fn audio_only(mut self) -> Self {
        self.audio_only = true;
        self
    }

    /// 下载字幕
    #[allow(dead_code)]
    pub fn with_subtitles(mut self) -> Self {
        self.download_subtitles = true;
        self
    }

    /// 设置User-Agent
    pub fn with_user_agent(mut self, user_agent: String) -> Self {
        self.user_agent = Some(user_agent);
        self
    }

    /// 设置Cookie文件
    #[allow(dead_code)]
    pub fn with_cookie_file(mut self, cookie_file: PathBuf) -> Self {
        self.cookie_file = Some(cookie_file);
        self
    }
}

/// 下载结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadResult {
    /// 是否成功
    pub success: bool,
    /// 输出文件路径
    pub output_path: PathBuf,
    /// 文件大小（字节）
    pub file_size: u64,
    /// 下载时长（秒）
    pub duration_secs: f64,
    /// 错误信息（如果失败）
    pub error_message: Option<String>,
}

impl DownloadResult {
    /// 创建成功结果
    pub fn success(output_path: PathBuf, file_size: u64, duration_secs: f64) -> Self {
        Self {
            success: true,
            output_path,
            file_size,
            duration_secs,
            error_message: None,
        }
    }

    /// 创建失败结果
    #[allow(dead_code)]
    pub fn failure(output_path: PathBuf, error: String) -> Self {
        Self {
            success: false,
            output_path,
            file_size: 0,
            duration_secs: 0.0,
            error_message: Some(error),
        }
    }
}

/// 下载进度
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadProgress {
    /// 任务ID
    pub task_id: String,
    /// 已下载字节数
    pub bytes_downloaded: u64,
    /// 总字节数
    pub total_bytes: u64,
    /// 下载进度 (0.0 - 1.0)
    pub percent: f64,
    /// 下载速度 (bytes/s)
    pub speed: f64,
    /// 预计剩余时间 (秒)
    pub eta_secs: Option<f64>,
    /// 当前状态
    pub status: DownloadStatus,
}

impl DownloadProgress {
    /// 创建新的进度
    pub fn new(task_id: String) -> Self {
        Self {
            task_id,
            bytes_downloaded: 0,
            total_bytes: 0,
            percent: 0.0,
            speed: 0.0,
            eta_secs: None,
            status: DownloadStatus::Pending,
        }
    }

    /// 设置总字节数
    pub fn with_total_bytes(mut self, total: u64) -> Self {
        self.total_bytes = total;
        self
    }

    /// 设置状态
    pub fn with_status(mut self, status: DownloadStatus) -> Self {
        self.status = status;
        self
    }

    /// 更新进度
    pub fn update(&mut self, bytes_downloaded: u64, speed: f64) {
        self.bytes_downloaded = bytes_downloaded;
        self.speed = speed;

        if self.total_bytes > 0 {
            self.percent = (bytes_downloaded as f64 / self.total_bytes as f64).min(1.0);
        }

        if speed > 0.0 {
            let remaining = self.total_bytes.saturating_sub(bytes_downloaded);
            self.eta_secs = Some(remaining as f64 / speed);
        } else {
            self.eta_secs = None;
        }
    }
}

/// 下载状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DownloadStatus {
    /// 等待中
    Pending,
    /// 准备中
    Preparing,
    /// 下载中
    Downloading,
    /// 已完成
    Completed,
    /// 已暂停
    Paused,
    /// 失败
    Failed,
    /// 已取消
    Cancelled,
}

impl DownloadStatus {
    /// 是否是终态（测试用）
    #[allow(dead_code)]
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            DownloadStatus::Completed | DownloadStatus::Failed | DownloadStatus::Cancelled
        )
    }

    /// 是否是活跃状态
    #[allow(dead_code)]
    pub fn is_active(&self) -> bool {
        matches!(self, DownloadStatus::Downloading | DownloadStatus::Preparing)
    }
}

impl std::fmt::Display for DownloadStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DownloadStatus::Pending => write!(f, "等待中"),
            DownloadStatus::Preparing => write!(f, "准备中"),
            DownloadStatus::Downloading => write!(f, "下载中"),
            DownloadStatus::Completed => write!(f, "已完成"),
            DownloadStatus::Paused => write!(f, "已暂停"),
            DownloadStatus::Failed => write!(f, "失败"),
            DownloadStatus::Cancelled => write!(f, "已取消"),
        }
    }
}

/// 格式化字节数
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_index = 0;

    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }

    if unit_index == 0 {
        format!("{} {}", bytes, UNITS[unit_index])
    } else {
        format!("{:.1} {}", size, UNITS[unit_index])
    }
}

/// 清理文件名，移除非法字符，防止路径遍历
pub fn sanitize_filename(name: &str) -> String {
    // 先检查原始名称是否包含路径遍历特征
    let has_path_traversal = name.contains("..")
        || name.starts_with('/')
        || name.starts_with('\\')
        || name.starts_with('~');

    if has_path_traversal {
        return "video".to_string();
    }

    // 替换非法字符为下划线
    let sanitized: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .take(200)  // 限制长度防止过长文件名
        .collect();

    // 防止空文件名
    if sanitized.is_empty() {
        return "video".to_string();
    }

    sanitized
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(1024 * 1024), "1.0 MB");
        assert_eq!(format_bytes(1536), "1.5 KB");
    }

    #[test]
    fn test_download_request_builder() {
        let request = DownloadRequest::new("https://example.com/video", "/tmp/video.mp4")
            .with_format_id("1080p".to_string())
            .audio_only();

        assert_eq!(request.url, "https://example.com/video");
        assert_eq!(request.format_id, Some("1080p".to_string()));
        assert!(request.audio_only);
    }

    #[test]
    fn test_download_status() {
        assert!(DownloadStatus::Completed.is_terminal());
        assert!(DownloadStatus::Failed.is_terminal());
        assert!(!DownloadStatus::Downloading.is_terminal());
        assert!(DownloadStatus::Downloading.is_active());
    }
}
