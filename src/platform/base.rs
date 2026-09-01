//! yt-dlp 包装型 resolver 的共享基类
//!
//! 收纳 YouTube / TikTok / Bilibili / DouYin(yt-dlp 分支) 4 个 resolver
//! 的字段 / 构造器 / 调用样板。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::history::Platform;
use crate::platform::{CookieFileManager, TokenManager};

pub struct YtdlpBacked {
    #[allow(dead_code)]
    tokens: HashMap<String, String>,
    #[allow(dead_code)]
    cookie_manager: CookieFileManager,
    #[allow(dead_code)]
    platform_tag: Platform,
    #[allow(dead_code)]
    token_manager: Option<Arc<Mutex<TokenManager>>>,
}

impl YtdlpBacked {
    #[allow(dead_code)]
    pub fn new(platform: Platform) -> Self {
        Self {
            tokens: HashMap::new(),
            cookie_manager: CookieFileManager::new(),
            platform_tag: platform,
            token_manager: None,
        }
    }

    #[allow(dead_code)]
    pub fn with_tokens(mut self, tokens: HashMap<String, String>) -> Self {
        self.tokens = tokens;
        self
    }

    #[allow(dead_code)]
    pub fn with_token_manager(mut self, tm: Arc<Mutex<TokenManager>>) -> Self {
        self.token_manager = Some(tm);
        self
    }

    #[allow(dead_code)]
    pub fn has_token(&self, key: &str) -> bool {
        self.tokens.contains_key(key)
    }

    #[allow(dead_code)]
    pub fn cookie_file(&self) -> Option<PathBuf> {
        let ns = match self.platform_tag {
            Platform::YouTube => "youtube",
            Platform::TikTok => "tiktok",
            Platform::Bilibili => "bilibili",
            Platform::DouYin => "douyin",
            _ => "default",
        };
        self.cookie_manager.get_cookie_file(&self.tokens, ns)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ytdlp_backed_new_initializes_empty() {
        let base = YtdlpBacked::new(Platform::TikTok);
        assert!(!base.has_token("msToken"));
        assert!(!base.has_token("any"));
        assert!(base.cookie_file().is_none());
    }

    #[test]
    fn test_ytdlp_backed_with_tokens_sets_tokens() {
        let base = YtdlpBacked::new(Platform::TikTok)
            .with_tokens(HashMap::from([("msToken".to_string(), "abc".to_string())]));
        assert!(base.has_token("msToken"));
        assert!(!base.has_token("ttwid"));
    }

    #[test]
    fn test_ytdlp_backed_cookie_file_none_when_empty() {
        let base = YtdlpBacked::new(Platform::TikTok);
        assert!(base.cookie_file().is_none());
    }

    #[test]
    fn test_ytdlp_backed_cookie_file_namespace_per_platform() {
        let tiktok = YtdlpBacked::new(Platform::TikTok)
            .with_tokens(HashMap::from([("msToken".to_string(), "x".to_string())]));
        let path = tiktok.cookie_file();
        assert!(path.is_some(), "TikTok with msToken must produce cookie file");
        let path_str = path.unwrap().to_string_lossy().into_owned();
        assert!(
            path_str.contains("tiktok"),
            "TikTok cookie file path must contain 'tiktok', got: {path_str}"
        );
    }
}
