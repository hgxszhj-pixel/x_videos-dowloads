//! Bilibili 解析器
//!
//! 使用 yt-dlp 作为主要方式，支持 cookies 认证

use crate::resolvers::{PlatformResolver, ResolverError, CookieFileManager, url_matcher};
use crate::types::VideoInfo;
use crate::history::Platform;
use async_trait::async_trait;
use std::collections::HashMap;

/// Bilibili 解析器
#[allow(dead_code)]
pub struct BilibiliResolver {
    cookies: HashMap<String, String>,
    cookie_manager: CookieFileManager,
}

impl BilibiliResolver {
    /// 创建新的 BilibiliResolver
    pub fn new() -> Self {
        Self {
            cookies: HashMap::new(),
            cookie_manager: CookieFileManager::new(),
        }
    }

    /// 设置 cookies
    #[allow(dead_code)]
    pub fn with_cookies(mut self, cookies: HashMap<String, String>) -> Self {
        self.cookies = cookies;
        self
    }

    /// 获取 cookie 文件路径
    fn get_cookie_file(&self) -> Option<std::path::PathBuf> {
        self.cookie_manager.get_cookie_file(&self.cookies, "bilibili")
    }
}

impl Default for BilibiliResolver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PlatformResolver for BilibiliResolver {
    fn platform(&self) -> Platform {
        Platform::Bilibili
    }

    fn supported_url_patterns(&self) -> Vec<&'static str> {
        url_matcher::get_platform_patterns(Platform::Bilibili).to_vec()
    }

    async fn get_video_info(&self, url: &str) -> Result<VideoInfo, ResolverError> {
        let mut ytdlp = crate::yt_dlp::YtDlp::new();

        // 如果有 cookies，设置 cookie 文件
        if let Some(cookie_file) = self.get_cookie_file() {
            ytdlp = ytdlp.with_cookie_file(cookie_file);
        }

        // 使用 yt-dlp 获取视频信息
        match ytdlp.get_video_info(url) {
            Ok(info) => {
                tracing::info!("Bilibili video info via yt-dlp: {}", info.title);
                Ok(info)
            }
            Err(e) => {
                tracing::warn!("Bilibili yt-dlp failed: {}", e);
                Err(ResolverError::from(e))
            }
        }
    }

    fn requires_auth(&self) -> bool {
        !self.cookies.is_empty()
    }

    fn required_tokens(&self) -> Vec<&'static str> {
        vec!["SESSDATA", "bili_jct", "buvid3"]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_supports_url() {
        let resolver = BilibiliResolver::new();
        assert!(resolver.supports_url("https://www.bilibili.com/video/BV1xx411c7XD"));
        assert!(resolver.supports_url("https://b23.tv/abc"));
        assert!(!resolver.supports_url("https://youtube.com/watch"));
    }

}