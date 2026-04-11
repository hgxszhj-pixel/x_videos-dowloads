//! 类型模块单元测试

use x_video_downloader::types::{
    format_bytes, sanitize_filename, DownloadStatus, VideoInfo,
    DEFAULT_USER_AGENT,
};

#[test]
fn test_format_bytes() {
    assert_eq!(format_bytes(0), "0 B");
    assert_eq!(format_bytes(1024), "1.0 KB");
    assert_eq!(format_bytes(1024 * 1024), "1.0 MB");
    assert_eq!(format_bytes(1024 * 1024 * 1024), "1.0 GB");
    assert_eq!(format_bytes(1500), "1.5 KB");
    assert_eq!(format_bytes(1024 * 1024 + 512 * 1024), "1.5 MB");
}

#[test]
fn test_sanitize_filename() {
    // 正常文件名
    assert_eq!(sanitize_filename("video"), "video");
    assert_eq!(sanitize_filename("my_video"), "my_video");

    // 特殊字符替换为下划线
    assert_eq!(sanitize_filename("video:test"), "video_test");
    assert_eq!(sanitize_filename("video|test"), "video_test");
    assert_eq!(sanitize_filename("video\\test"), "video_test");
    assert_eq!(sanitize_filename("video*test"), "video_test");
    assert_eq!(sanitize_filename("video?test"), "video_test");
    assert_eq!(sanitize_filename("video\"test"), "video_test");
    assert_eq!(sanitize_filename("video<test>"), "video_test_");

    // 路径遍历攻击被阻止（返回 "video"）
    assert_eq!(sanitize_filename("../etc/passwd"), "video");  // 路径遍历
    assert_eq!(sanitize_filename("/etc/passwd"), "video");  // 绝对路径
    assert_eq!(sanitize_filename("~/secret"), "video");  // home 路径遍历

    // / 在中间变成下划线（不是路径遍历攻击）
    assert_eq!(sanitize_filename("video/test"), "video_test");

    // 中文保持
    assert_eq!(sanitize_filename("视频"), "视频");

    // 多个特殊字符（替换为下划线）
    assert_eq!(sanitize_filename("video:*?|test"), "video____test");
}

#[test]
fn test_download_status_is_terminal() {
    assert!(DownloadStatus::Completed.is_terminal());
    assert!(DownloadStatus::Failed.is_terminal());
    assert!(DownloadStatus::Cancelled.is_terminal());
    assert!(!DownloadStatus::Pending.is_terminal());
    assert!(!DownloadStatus::Downloading.is_terminal());
}

#[test]
fn test_download_status_is_active() {
    assert!(DownloadStatus::Downloading.is_active());
    assert!(DownloadStatus::Preparing.is_active());
    assert!(!DownloadStatus::Completed.is_active());
    assert!(!DownloadStatus::Failed.is_active());
}

#[test]
fn test_video_info_creation() {
    let info = VideoInfo::new("Test Video".to_string(), "https://x.com/test".to_string());
    assert_eq!(info.title, "Test Video");
    assert_eq!(info.url, "https://x.com/test");
    assert!(info.duration.is_none());
}

#[test]
fn test_default_user_agent() {
    assert!(DEFAULT_USER_AGENT.contains("Mozilla/5.0"));
    assert!(DEFAULT_USER_AGENT.contains("Chrome"));
}
