//! Bilibili 解析器
//!
//! 使用 yt-dlp 作为主要方式，支持 cookies 认证

use crate::resolvers::{PlatformResolver, ResolverError, CookieFileManager};
use crate::types::VideoInfo;
use crate::history::Platform;
use std::collections::HashMap;

/// Bilibili 解析器
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

impl PlatformResolver for BilibiliResolver {
    fn platform(&self) -> Platform {
        Platform::Bilibili
    }

    fn supported_url_patterns(&self) -> Vec<&'static str> {
        vec!["bilibili.com", "bilivideo.com", "b23.tv"]
    }

    fn get_video_info(&self, url: &str) -> anyhow::Result<VideoInfo, ResolverError> {
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

/// 提取 Bilibili 视频 ID (BV号 或 av号)
pub fn extract_bilibili_video_id(url: &str) -> anyhow::Result<(String, bool)> {
    // BV 号: https://www.bilibili.com/video/BV1xx411c7XD
    // av 号: https://www.bilibili.com/video/av170001

    if url.contains("/video/BV") {
        let parts: Vec<&str> = url.split("/video/BV").collect();
        if parts.len() >= 2 {
            let bv = parts[1].split(['?', '/']).next().unwrap_or(parts[1]);
            return Ok((bv.to_string(), false));
        }
    }

    if url.contains("/video/av") {
        let parts: Vec<&str> = url.split("/video/av").collect();
        if parts.len() >= 2 {
            let av = parts[1].split(['?', '/']).next().unwrap_or(parts[1]);
            return Ok((av.to_string(), true));
        }
    }

    anyhow::bail!("无法从 URL 提取 Bilibili 视频 ID")
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

    #[test]
    fn test_extract_bv_id() {
        let id = extract_bilibili_video_id("https://www.bilibili.com/video/BV1xx411c7XD");
        assert!(id.is_ok());
        let (bv, is_av) = id.unwrap();
        assert_eq!(bv, "1xx411c7XD");
        assert!(!is_av);
    }

    #[test]
    fn test_extract_av_id() {
        let id = extract_bilibili_video_id("https://www.bilibili.com/video/av170001");
        assert!(id.is_ok());
        let (av, is_av) = id.unwrap();
        assert_eq!(av, "170001");
        assert!(is_av);
    }
}