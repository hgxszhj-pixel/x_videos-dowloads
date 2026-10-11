//! TikTok 解析器
//!
//! 使用 yt-dlp 作为主要方式,msToken/ttwid 用于认证
//!
//! 2026-10-11:TokenManager 自动注入链路 (YtdlpBacked.field + TikTokResolver.with_token_manager)
//! 自 Task 7 后无 reader,本次清理。后续若需重新接 token 自动生成,直接从
//! `crate::platform::TokenManager` 在 `get_video_info` 调用前手动 `.lock()`,无需
//! 再走 resolver 字段注入链。

use crate::history::Platform;
use crate::platform::{url_matcher, PlatformResolver, ResolverError, YtdlpBacked};
use crate::types::VideoInfo;
use async_trait::async_trait;
use std::collections::HashMap;

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
