//! YouTube 解析器
//!
//! 使用 yt-dlp 作为主要方式，支持 Cookie 认证
//! 用于处理 age-restricted 或需要登录的视频

use crate::history::Platform;
use crate::platform::{url_matcher, PlatformResolver, ResolverError};
use crate::types::VideoInfo;
use async_trait::async_trait;

/// YouTube 解析器
pub struct YouTubeResolver {
    cookie_path: Option<std::path::PathBuf>,
}

impl YouTubeResolver {
    /// 创建新的 YouTubeResolver
    pub fn new() -> Self {
        Self { cookie_path: None }
    }

    /// 从 cookie 文件创建解析器
    #[expect(dead_code)]
    pub fn with_cookie_file(mut self, path: std::path::PathBuf) -> Self {
        self.cookie_path = Some(path);
        self
    }
}

impl Default for YouTubeResolver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PlatformResolver for YouTubeResolver {
    fn platform(&self) -> Platform {
        Platform::YouTube
    }

    fn supported_url_patterns(&self) -> Vec<&'static str> {
        url_matcher::get_platform_patterns(Platform::YouTube).to_vec()
    }

    fn supports_url_by_host(&self, host_lower: &str) -> bool {
        host_lower == "youtube.com"
            || host_lower.ends_with(".youtube.com")
            || host_lower == "youtu.be"
            || host_lower.ends_with(".youtu.be")
    }

    async fn get_video_info(&self, url: &str) -> Result<VideoInfo, ResolverError> {
        let mut ytdlp = crate::yt_dlp::YtDlp::new();

        // 如果有 cookie 文件，注入到 yt-dlp
        if let Some(ref cookie_path) = self.cookie_path {
            if cookie_path.exists() {
                tracing::debug!("YouTube using cookie file for authentication");
                ytdlp = ytdlp.with_cookie_file(cookie_path.clone());
            }
        }

        // yt-dlp's get_video_info is now async and internally uses spawn_blocking
        match ytdlp.get_video_info(url).await {
            Ok(info) => {
                tracing::info!("YouTube video info via yt-dlp: {}", info.title);
                Ok(info)
            }
            Err(e) => {
                tracing::warn!("YouTube yt-dlp failed: {}", e);
                Err(ResolverError::ParseFailed(format!(
                    "YouTube 解析失败: {}",
                    e
                )))
            }
        }
    }

    fn requires_auth(&self) -> bool {
        false // Cookie 可选，不强制要求
    }

    fn download_strategy(&self) -> crate::types::DownloadStrategy {
        crate::types::DownloadStrategy::YtDlp
    }
}
