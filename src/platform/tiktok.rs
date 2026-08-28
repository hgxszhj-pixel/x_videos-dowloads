//! TikTok 解析器
//!
//! 使用 yt-dlp 作为主要方式，msToken/ttwid 用于认证
//! 支持 TokenManager 自动token生成和注入

use crate::history::Platform;
use crate::platform::{url_matcher, CookieFileManager, PlatformResolver, ResolverError};
use crate::types::VideoInfo;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

/// TikTok 解析器
#[expect(dead_code)]
pub struct TikTokResolver {
    tokens: HashMap<String, String>,
    token_manager: Option<Arc<Mutex<crate::platform::TokenManager>>>,
    cookie_manager: CookieFileManager,
}

impl TikTokResolver {
    /// 创建新的 TikTokResolver
    pub fn new() -> Self {
        Self {
            tokens: HashMap::new(),
            token_manager: None,
            cookie_manager: CookieFileManager::new(),
        }
    }

    /// 设置 tokens
    pub fn with_tokens(mut self, tokens: HashMap<String, String>) -> Self {
        self.tokens = tokens;
        self
    }

    /// 设置 TokenManager 用于自动token生成
    #[expect(dead_code)]
    pub fn with_token_manager(mut self, tm: Arc<Mutex<crate::platform::TokenManager>>) -> Self {
        self.token_manager = Some(tm);
        self
    }

    /// 获取 cookie 文件路径（从 tokens 创建临时 cookie 文件）
    fn get_cookie_file(&self) -> Option<std::path::PathBuf> {
        self.cookie_manager.get_cookie_file(&self.tokens, "tiktok")
    }
}

impl Default for TikTokResolver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PlatformResolver for TikTokResolver {
    fn platform(&self) -> Platform {
        Platform::TikTok
    }

    fn supported_url_patterns(&self) -> Vec<&'static str> {
        url_matcher::get_platform_patterns(Platform::TikTok).to_vec()
    }

    async fn get_video_info(&self, url: &str) -> Result<VideoInfo, ResolverError> {
        let mut ytdlp = crate::yt_dlp::YtDlp::new();

        // 如果有 tokens，转换为 cookie 文件注入 yt-dlp
        if let Some(cookie_file) = self.get_cookie_file() {
            tracing::debug!("TikTok using cookie file for authentication");
            ytdlp = ytdlp.with_cookie_file(cookie_file);
        }

        // 使用 yt-dlp 获取视频信息
        match ytdlp.get_video_info(url).await {
            Ok(info) => {
                tracing::info!("TikTok video info via yt-dlp: {}", info.title);
                Ok(info)
            }
            Err(e) => {
                tracing::warn!("TikTok yt-dlp failed: {}", e);
                Err(ResolverError::from(e))
            }
        }
    }

    fn requires_auth(&self) -> bool {
        self.tokens.contains_key("msToken")
    }

    fn required_tokens(&self) -> Vec<&'static str> {
        vec!["msToken", "ttwid"]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_supports_url() {
        let resolver = TikTokResolver::new();
        assert!(resolver.supports_url("https://www.tiktok.com/@user/video/123"));
        assert!(resolver.supports_url("https://vm.tiktok.com/abc"));
        assert!(!resolver.supports_url("https://youtube.com/watch"));
    }
}
