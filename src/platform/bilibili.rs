//! Bilibili 解析器
//!
//! 使用 yt-dlp 作为主要方式，支持 cookies 认证

use crate::history::Platform;
use crate::platform::{url_matcher, PlatformResolver, ResolverError, YtdlpBacked};
use crate::types::VideoInfo;
use async_trait::async_trait;
use std::collections::HashMap;

/// Bilibili 解析器
pub struct BilibiliResolver {
    base: YtdlpBacked,
}

impl BilibiliResolver {
    pub fn new() -> Self {
        Self {
            base: YtdlpBacked::new(Platform::Bilibili),
        }
    }

    #[expect(dead_code)]
    pub fn with_cookies(mut self, cookies: HashMap<String, String>) -> Self {
        self.base = self.base.with_tokens(cookies);
        self
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
        self.base.fetch_via_ytdlp(url).await
    }

    fn requires_auth(&self) -> bool {
        // Bilibili 旧逻辑：cookies 非空 → 需要 auth
        !self.base.tokens_is_empty()
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
        assert!(!resolver.supports_url("https://tiktok.com/video"));
    }
}