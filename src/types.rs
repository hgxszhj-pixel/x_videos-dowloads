//! X Video Downloader - 共享类型定义
//!
//! 项目中使用的核心数据类型定义

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// 平台来源
///
/// 历史模块使用，定义下载源平台。从 `history` 迁移到 `types` 以打破
/// `platform ↔ history` 模块间的循环依赖（platform/url_matcher 需要 Platform）。
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
    TulingPython,
    /// SHANHU/珊瑚 SaaS 平台（白标域 rexxi.cn + 后端 wanzibook.com + CDN tenwiseacademy.cn）。
    /// 2026-09-06 v2 实测:整条 m3u8 链路完全匿名,无需 token。
    Shanhu,
    Other,
}

impl Platform {
    /// 平台显示名称（用于 UI 提示）
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
            Platform::TulingPython => "图灵Python",
            Platform::Shanhu => "SHANHU/珊瑚",
            Platform::Other => "Other",
        }
    }
}

/// 默认 User-Agent 常量
pub const DEFAULT_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

/// 下载错误类型（用户友好）
#[derive(Debug, Clone)]
/// 预留 API：用户友好的下载错误分类（当前由 DownloadError 内部使用）
pub enum DownloadErrorType {
    /// 网络连接问题（超时、断开）
    Network,
    /// HTTP 错误（403/404/500 等）
    Http(u16),
    /// 内容不可用（视频不存在、私密、地区限制）
    ContentUnavailable,
    /// 需要认证（登录、Cookie 过期）
    Authentication,
    /// 速度限制/封 IP
    RateLimited,
    /// 磁盘空间不足
    DiskFull,
    /// 磁盘写入错误（权限、路径无效）
    DiskIo,
    /// 下载被取消
    Cancelled,
    /// yt-dlp 内部错误
    YtDlp(String),
    /// 未知错误
    Unknown(String),
}

/// 下载错误（带用户友好的错误描述和操作建议）
#[derive(Debug, Clone)]
pub struct DownloadError {
    /// 错误类型
    pub error_type: DownloadErrorType,
    /// 简短错误信息（供 UI 显示）
    pub message: String,
    /// 操作建议（告诉用户怎么解决）
    pub action: Option<String>,
}

impl DownloadError {
    /// 创建网络错误
    pub fn network(_e: impl std::fmt::Display) -> Self {
        Self {
            error_type: DownloadErrorType::Network,
            message: "网络连接失败，请检查网络".to_string(),
            action: Some("尝试更换网络环境或使用代理".to_string()),
        }
    }

    /// 创建 HTTP 错误
    pub fn http(status: u16) -> Self {
        let (message, action) = match status {
            403 => (
                "服务器拒绝访问".to_string(),
                Some("可能需要登录，请设置 Cookie".to_string()),
            ),
            404 => ("视频不存在或已被删除".to_string(), None),
            429 => (
                "请求过于频繁，已被限速".to_string(),
                Some("请稍后再试或使用代理".to_string()),
            ),
            450..=499 => (
                "无法访问该内容".to_string(),
                Some("可能需要登录或内容不可用".to_string()),
            ),
            500..=599 => ("服务器异常".to_string(), Some("稍后重试".to_string())),
            _ => ("下载请求失败".to_string(), None),
        };
        Self {
            error_type: DownloadErrorType::Http(status),
            message,
            action,
        }
    }

    /// 创建内容不可用错误
    pub fn content_unavailable(reason: &str) -> Self {
        Self {
            error_type: DownloadErrorType::ContentUnavailable,
            message: format!("无法下载: {}", reason),
            action: Some("检查视频是否需要登录或地区限制".to_string()),
        }
    }

    /// 创建认证错误
    pub fn authentication() -> Self {
        Self {
            error_type: DownloadErrorType::Authentication,
            message: "需要登录才能下载此视频".to_string(),
            action: Some("点击「设置 Cookie」提供登录凭证".to_string()),
        }
    }

    /// 创建限速错误
    pub fn rate_limited() -> Self {
        Self {
            error_type: DownloadErrorType::RateLimited,
            message: "下载被限速".to_string(),
            action: Some("等待几分钟后重试，或使用代理".to_string()),
        }
    }

    /// 创建磁盘错误
    pub fn disk_io(reason: &str) -> Self {
        Self {
            error_type: DownloadErrorType::DiskIo,
            message: format!("磁盘写入失败: {}", reason),
            action: Some("检查磁盘空间和写入权限".to_string()),
        }
    }

    /// 创建 yt-dlp 错误
    pub fn ytdlp(msg: &str) -> Self {
        Self {
            error_type: DownloadErrorType::YtDlp(msg.to_string()),
            message: format!("下载工具错误: {}", msg),
            action: None,
        }
    }

    /// 创建取消错误
    pub fn cancelled() -> Self {
        Self {
            error_type: DownloadErrorType::Cancelled,
            message: "下载已取消".to_string(),
            action: None,
        }
    }

    /// 从 reqwest 错误转换
    pub fn from_reqwest(e: reqwest::Error) -> Self {
        if e.is_timeout() {
            return Self::network("连接超时");
        }
        if e.is_connect() {
            return Self::network("无法连接到服务器");
        }
        if let Some(status) = e.status() {
            return Self::http(status.as_u16());
        }
        Self::network(e.to_string())
    }

    /// 获取简短消息
    pub fn short_message(&self) -> &str {
        &self.message
    }
}

impl std::fmt::Display for DownloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)?;
        if let Some(ref action) = self.action {
            write!(f, " ({})", action)?;
        }
        Ok(())
    }
}

impl From<DownloadError> for String {
    fn from(e: DownloadError) -> String {
        e.to_string()
    }
}

impl From<DownloadError> for anyhow::Error {
    fn from(e: DownloadError) -> Self {
        anyhow::anyhow!("{}", e)
    }
}

/// ResolverError conversion for unified error handling.
///
/// Usage guidelines:
/// - **DownloadError** / **DownloadErrorType**: For download operations (file I/O, HTTP, yt-dlp)
/// - **ResolverError**: For platform resolution logic (parsing URLs, extracting video metadata)
impl From<crate::platform::ResolverError> for DownloadError {
    fn from(e: crate::platform::ResolverError) -> Self {
        match e {
            crate::platform::ResolverError::AuthenticationRequired => {
                DownloadError::authentication()
            }
            crate::platform::ResolverError::ContentUnavailable(msg) => {
                DownloadError::content_unavailable(&msg)
            }
            crate::platform::ResolverError::Network(msg) => DownloadError::network(msg.as_str()),
            crate::platform::ResolverError::ParseFailed(msg) => DownloadError {
                error_type: DownloadErrorType::Unknown(msg.clone()),
                message: format!("解析失败: {}", msg),
                action: Some("检查URL是否支持".to_string()),
            },
            crate::platform::ResolverError::Detailed { message, .. } => DownloadError {
                error_type: DownloadErrorType::Unknown(message.clone()),
                message: message.clone(),
                action: None,
            },
        }
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
    /// 下载策略（由解析器决定）
    pub download_strategy: DownloadStrategy,
    /// 音频流 URL（用于 Merged 策略）
    pub audio_url: Option<String>,
}

impl VideoInfo {
    /// 创建新的视频信息
    pub fn new(title: String, url: String, audio_url: Option<String>) -> Self {
        Self {
            title,
            description: None,
            uploader: None,
            duration: None,
            url,
            thumbnail: None,
            formats: Vec::new(),
            created_at: Some(chrono::Utc::now().to_rfc3339()),
            download_strategy: DownloadStrategy::YtDlp,
            audio_url,
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
    /// 视频 URL（最终下载用的 URL）
    pub url: String,
    /// 原始 URL（用于 yt-dlp 格式选择，如 YouTube 原始页面 URL）
    pub original_url: Option<String>,
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
    /// 定时下载时间（可选）
    pub schedule_time: Option<DateTime<Utc>>,
    /// 下载策略
    pub download_strategy: DownloadStrategy,
    /// 音频流 URL（用于 Merged 策略）
    pub audio_url: Option<String>,
    /// 取消信号（外部 toggle 可中断下载）
    #[serde(skip, default)]
    pub cancel_flag: Option<Arc<AtomicBool>>,
}

impl DownloadRequest {
    /// 创建新的下载请求
    pub fn new(url: impl Into<String>, output_path: impl Into<PathBuf>) -> Self {
        Self {
            url: url.into(),
            original_url: None,
            output_path: output_path.into(),
            format_id: None,
            audio_only: false,
            download_subtitles: false,
            user_agent: None,
            cookie_file: None,
            schedule_time: None,
            download_strategy: DownloadStrategy::YtDlp,
            audio_url: None,
            cancel_flag: None,
        }
    }

    /// 设置原始 URL（用于 yt-dlp 下载）
    /// 预留 API：备用入口，当用户提供的 URL 已被改写时记录原 URL
    pub fn with_original_url(mut self, original_url: String) -> Self {
        self.original_url = Some(original_url);
        self
    }

    /// 设置下载策略
    /// 预留 API：未来支持自定义下载策略（如 Direct/HLS/YtDlp 切换）
    pub fn with_strategy(mut self, strategy: DownloadStrategy) -> Self {
        self.download_strategy = strategy;
        self
    }

    /// 设置格式ID
    /// 预留 API：GUI 格式选择器使用（当前未完全透传到 yt-dlp）
    pub fn with_format_id(mut self, format_id: String) -> Self {
        self.format_id = Some(format_id);
        self
    }

    /// 仅下载音频
    /// 预留 API：未来支持仅音频下载模式（提取音频流）
    #[allow(dead_code)]
    pub fn audio_only(mut self) -> Self {
        self.audio_only = true;
        self
    }

    /// 下载字幕
    /// 预留 API：未来支持字幕下载（与 yt-dlp --write-subs 配合）
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
    /// 预留 API：未来支持浏览器 Cookie 认证下载（X/抖音需要）
    #[expect(dead_code)]
    pub fn with_cookie_file(mut self, cookie_file: PathBuf) -> Self {
        self.cookie_file = Some(cookie_file);
        self
    }

    /// 设置音频流 URL（用于 Merged 策略）
    /// 预留 API：未来支持 Merged 策略（视频/音频分离流合并）
    #[expect(dead_code)]
    pub fn with_audio_url(mut self, audio_url: String) -> Self {
        self.audio_url = Some(audio_url);
        self
    }

    /// 设置取消信号
    pub fn with_cancel_flag(mut self, flag: Arc<AtomicBool>) -> Self {
        self.cancel_flag = Some(flag);
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
    /// 预留 API：失败结果构造函数（当前主要由测试使用）
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
    /// 当前活动线程数
    pub active_threads: u32,
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
            active_threads: 0,
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
    pub fn update(&mut self, bytes_downloaded: u64, speed: f64, active_threads: u32) {
        self.bytes_downloaded = bytes_downloaded;
        self.speed = speed;
        self.active_threads = active_threads;

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

/// 下载策略
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DownloadStrategy {
    /// 使用原生下载（单流）
    Native,
    /// 使用原生下载（双流并行下载后合并）
    Merged,
    /// 使用原生 HLS 下载（分段并发 + 合并）
    Hls,
    /// 使用 yt-dlp 下载
    #[default]
    YtDlp,
}

impl DownloadStatus {
    /// 是否是终态（测试用）
    /// 预留 API：状态机判断，当前仅 mod tests 引用
    #[allow(dead_code)]
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            DownloadStatus::Completed | DownloadStatus::Failed | DownloadStatus::Cancelled
        )
    }

    /// 是否是活跃状态
    /// 预留 API：状态机判断，当前仅 mod tests 引用
    #[allow(dead_code)]
    pub fn is_active(&self) -> bool {
        matches!(
            self,
            DownloadStatus::Downloading | DownloadStatus::Preparing
        )
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

/// 验证输出文件路径是否在允许的目录内
///
/// 当前 downloader 内部 pipeline 已不再调用(merge_segments 走 trust-boundary-sanitized output_path),
/// 保留给外部代码 (CLI/GUI) 在 user-input 信任边界可选调用。
#[expect(dead_code)]
pub fn validate_output_path(
    output_path: &std::path::Path,
    allowed_dir: &std::path::Path,
) -> Result<(), String> {
    let canonical_output = output_path
        .canonicalize()
        .map_err(|_| format!("Invalid output path: {:?}", output_path))?;
    let canonical_allowed = allowed_dir
        .canonicalize()
        .map_err(|e| format!("Invalid allowed directory: {}", e))?;

    if !canonical_output.starts_with(&canonical_allowed) {
        return Err("Output path escapes allowed directory".to_string());
    }

    Ok(())
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
        .take(200) // 限制长度防止过长文件名
        .collect();

    // 防止空文件名
    if sanitized.is_empty() {
        return "video".to_string();
    }

    sanitized
}

/// 生成唯一的文件名，避免覆盖已存在的文件
/// 如果 `base_name.ext` 已存在，尝试 `base_name (1).ext`、`base_name (2).ext` 等
pub fn generate_unique_filename(
    output_dir: &std::path::Path,
    base_name: &str,
    ext: &str,
) -> std::path::PathBuf {
    let mut counter = 0;
    loop {
        let filename = if counter == 0 {
            format!("{}.{}", base_name, ext)
        } else {
            format!("{} ({}).{}", base_name, counter, ext)
        };
        let path = output_dir.join(&filename);
        if !path.exists() {
            return path;
        }
        counter += 1;
    }
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

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("normal_video.mp4"), "normal_video.mp4");
        assert_eq!(
            sanitize_filename("video with spaces.mp4"),
            "video_with_spaces.mp4"
        );
        assert_eq!(sanitize_filename("../../../etc/passwd"), "video");
        assert_eq!(sanitize_filename("/absolute/path"), "video");
        assert_eq!(sanitize_filename(""), "video");
        assert_eq!(sanitize_filename("a".repeat(300).as_str()).len(), 200);
    }

    #[test]
    fn test_download_error() {
        let err = DownloadError::http(403);
        assert!(matches!(err.error_type, DownloadErrorType::Http(403)));
        assert!(err.message.contains("拒绝访问"));

        let err = DownloadError::network("timeout");
        assert!(matches!(err.error_type, DownloadErrorType::Network));

        let err = DownloadError::rate_limited();
        assert!(matches!(err.error_type, DownloadErrorType::RateLimited));
    }

    #[test]
    fn test_download_progress_update() {
        let mut progress = DownloadProgress::new("task1".to_string())
            .with_total_bytes(1000)
            .with_status(DownloadStatus::Downloading);

        progress.update(500, 100.0, 4);
        assert_eq!(progress.bytes_downloaded, 500);
        assert!((progress.percent - 0.5).abs() < 0.001);
        assert!(progress.eta_secs.is_some());

        progress.update(1000, 100.0, 4);
        assert!((progress.percent - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_video_info_new() {
        let info = VideoInfo::new(
            "Test Video".to_string(),
            "https://example.com/video.mp4".to_string(),
            None,
        );
        assert_eq!(info.title, "Test Video");
        assert_eq!(info.url, "https://example.com/video.mp4");
        assert!(info.formats.is_empty());
        assert!(info.created_at.is_some());
    }

    #[test]
    fn test_download_result() {
        let result = DownloadResult::success(
            std::path::PathBuf::from("/tmp/video.mp4"),
            1024 * 1024,
            10.5,
        );
        assert!(result.success);
        assert_eq!(result.file_size, 1024 * 1024);
        assert_eq!(result.duration_secs, 10.5);
        assert!(result.error_message.is_none());

        let result = DownloadResult::failure(
            std::path::PathBuf::from("/tmp/video.mp4"),
            "Network error".to_string(),
        );
        assert!(!result.success);
        assert!(result.error_message.is_some());
    }

    #[test]
    fn test_download_request_options() {
        let request = DownloadRequest::new("https://example.com/video", "/tmp")
            .with_user_agent("test-ua".to_string())
            .with_subtitles();

        assert!(request.user_agent.is_some());
        assert!(request.download_subtitles);
    }

    #[test]
    fn test_format_bytes_large() {
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.0 GB");
        assert_eq!(format_bytes(1024 * 1024 * 1024 * 1024), "1.0 TB");
        assert_eq!(format_bytes(500), "500 B");
    }

    #[test]
    fn test_sanitize_filename_edge_cases() {
        assert_eq!(sanitize_filename("normal_video.mp4"), "normal_video.mp4");
        assert_eq!(
            sanitize_filename("video with spaces.mp4"),
            "video_with_spaces.mp4"
        );
        assert_eq!(sanitize_filename("../../../etc/passwd"), "video");
        assert_eq!(sanitize_filename("/absolute/path"), "video");
    }

    #[test]
    fn test_generate_unique_filename_first_available() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = generate_unique_filename(temp_dir.path(), "video", "mp4");
        assert_eq!(path.file_name().unwrap().to_str().unwrap(), "video.mp4");
    }

    #[test]
    fn test_generate_unique_filename_avoids_conflict() {
        let temp_dir = tempfile::tempdir().unwrap();
        // 创建 "video.mp4"
        std::fs::write(temp_dir.path().join("video.mp4"), b"existing").unwrap();
        let path = generate_unique_filename(temp_dir.path(), "video", "mp4");
        assert_eq!(path.file_name().unwrap().to_str().unwrap(), "video (1).mp4");
    }

    #[test]
    fn test_generate_unique_filename_multiple_conflicts() {
        let temp_dir = tempfile::tempdir().unwrap();
        // 创建 "video.mp4", "video (1).mp4"
        std::fs::write(temp_dir.path().join("video.mp4"), b"1").unwrap();
        std::fs::write(temp_dir.path().join("video (1).mp4"), b"2").unwrap();
        let path = generate_unique_filename(temp_dir.path(), "video", "mp4");
        assert_eq!(path.file_name().unwrap().to_str().unwrap(), "video (2).mp4");
    }

    #[test]
    fn test_generate_unique_filename_gap_reuse() {
        let temp_dir = tempfile::tempdir().unwrap();
        // 创建 "video.mp4", "video (1).mp4", "video (2).mp4"
        std::fs::write(temp_dir.path().join("video.mp4"), b"1").unwrap();
        std::fs::write(temp_dir.path().join("video (1).mp4"), b"2").unwrap();
        std::fs::write(temp_dir.path().join("video (2).mp4"), b"3").unwrap();
        // 删除 "video (1).mp4"，下次应该生成 "video (1).mp4"（填坑）
        std::fs::remove_file(temp_dir.path().join("video (1).mp4")).unwrap();
        let path = generate_unique_filename(temp_dir.path(), "video", "mp4");
        assert_eq!(path.file_name().unwrap().to_str().unwrap(), "video (1).mp4");
    }

    #[test]
    fn test_download_error_factory() {
        let err = DownloadError::http(403);
        assert!(matches!(err.error_type, DownloadErrorType::Http(403)));
        let err = DownloadError::rate_limited();
        assert!(matches!(err.error_type, DownloadErrorType::RateLimited));
    }

    #[test]
    fn test_download_progress() {
        let mut p = DownloadProgress::new("t1".to_string()).with_total_bytes(1000);
        p.update(500, 100.0, 1);
        assert!((p.percent - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_default_user_agent() {
        assert!(DEFAULT_USER_AGENT.contains("Mozilla/5.0"));
        assert!(DEFAULT_USER_AGENT.contains("Chrome"));
    }
}
