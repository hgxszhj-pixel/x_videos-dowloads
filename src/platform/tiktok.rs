//! TikTok 解析器
//!
//! 使用 yt-dlp 作为主要方式,msToken/ttwid 用于认证
//! 支持 TokenManager 自动 token 生成和注入

use crate::history::Platform;
use crate::platform::{
    url_matcher, PlatformResolver, ResolverError, TokenManager, YtdlpBacked,
};
use crate::types::VideoInfo;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// TikTok 解析器
pub struct TikTokResolver {
    base: YtdlpBacked,
}

impl TikTokResolver {
    pub fn new() -> Self {
        Self {
            base: YtdlpBacked::new(Platform::TikTok),
        }
    }

    pub fn with_tokens(mut self, tokens: HashMap<String, String>) -> Self {
        self.base = self.base.with_tokens(tokens);
        self
    }

    #[expect(dead_code)]
    pub fn with_token_manager(mut self, tm: Arc<Mutex<TokenManager>>) -> Self {
        self.base = self.base.with_token_manager(tm);
        self
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
        self.base.fetch_via_ytdlp(url).await
    }

    fn requires_auth(&self) -> bool {
        self.base.has_token("msToken")
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
