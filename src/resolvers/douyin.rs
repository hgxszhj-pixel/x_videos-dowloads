//! 抖音解析器
//!
//! 使用 yt-dlp 作为主要方式，msToken/ttwid 用于认证
//! 支持信息流 URL (jingxuan/sports) 和 modal_id 解析

use crate::resolvers::{PlatformResolver, ResolverError, DouYinApiClient, extract_modal_id, CookieFileManager};
use crate::types::{VideoInfo, VideoFormat};
use crate::history::Platform;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// 抖音解析器
pub struct DouYinResolver {
    tokens: HashMap<String, String>,
    token_manager: Option<Arc<Mutex<crate::resolvers::TokenManager>>>,
    cookie_manager: CookieFileManager,
}

impl DouYinResolver {
    /// 创建新的 DouYinResolver
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
    pub fn with_token_manager(mut self, tm: Arc<Mutex<crate::resolvers::TokenManager>>) -> Self {
        self.token_manager = Some(tm);
        self
    }

    /// 获取 cookie 文件路径（从 tokens 创建临时 cookie 文件）
    fn get_cookie_file(&self) -> Option<std::path::PathBuf> {
        self.cookie_manager.get_cookie_file(&self.tokens, "douyin")
    }
}

impl Default for DouYinResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl PlatformResolver for DouYinResolver {
    fn platform(&self) -> Platform {
        Platform::DouYin
    }

    fn supported_url_patterns(&self) -> Vec<&'static str> {
        vec!["douyin.com", "v.douyin.com"]
    }

    fn get_video_info(&self, url: &str) -> anyhow::Result<VideoInfo, ResolverError> {
        // 检查是否是 feed URL (jingxuan/sports 等) 或包含 modal_id
        let modal_id = extract_modal_id(url);
        let is_feed_url = url.contains("jingxuan") || url.contains("sports") || url.contains("recommend");

        // 对于 feed URL 和包含 modal_id 的 URL，优先使用 API
        if modal_id.is_some() || is_feed_url {
            tracing::info!("DouYin feed/modal_id URL detected, using API directly");

            // 如果有 modal_id，尝试从 API 获取
            if let Some(modal_id) = &modal_id {
                let modal_id_clone = modal_id.clone();
                let ms_token = self.tokens.get("msToken").cloned();
                let api_result = std::thread::spawn(move || {
                    let rt = tokio::runtime::Runtime::new()?;
                    let client = if let Some(token) = ms_token {
                        DouYinApiClient::new().with_ms_token(token)
                    } else {
                        DouYinApiClient::new()
                    };
                    rt.block_on(client.get_video_by_modal_id(&modal_id_clone))
                }).join();

                match api_result {
                    Ok(Ok(resp)) => {
                        if let Some(video) = resp.aweme_list.and_then(|list| list.into_iter().next()) {
                            let title = video.get_title();
                            let video_id = video.get_video_id().unwrap_or(modal_id).to_string();
                            let uploader = video.get_author_name();
                            let duration = video.get_duration();
                            let thumbnail = video.video.as_ref()
                                .and_then(|v| v.cover.as_ref())
                                .and_then(|c| c.url_list.as_ref())
                                .and_then(|urls| urls.first())
                                .cloned();

                            tracing::info!("DouYin API success: {} by {}", title, uploader);

                            return Ok(VideoInfo {
                                title: if title.is_empty() { format!("DouYin Video {}", video_id) } else { title },
                                description: None,
                                uploader: Some(uploader),
                                duration: Some(duration),
                                url: format!("https://www.douyin.com/video/{}", video_id),
                                thumbnail,
                                formats: vec![VideoFormat {
                                    format_id: video_id.clone(),
                                    ext: "mp4".to_string(),
                                    resolution: Some(video.video.as_ref().map(|v| {
                                        let w = v.width.unwrap_or(0);
                                        let h = v.height.unwrap_or(0);
                                        if w > 0 && h > 0 {
                                            format!("{}x{}", w, h)
                                        } else {
                                            "1080p".to_string()
                                        }
                                    }).unwrap_or_else(|| "1080p".to_string())),
                                    filesize: None,
                                    bitrate: video.video.as_ref().and_then(|v| v.bitrate).map(|b| b as u64),
                                    codec: None,
                                    audio_only: false,
                                    format_note: Some("DouYin API".to_string()),
                                }],
                                created_at: None,
                            });
                        } else {
                            tracing::warn!("DouYin API returned empty aweme_list");
                            // 继续尝试 yt-dlp
                        }
                    }
                    Ok(Err(e)) => {
                        tracing::warn!("DouYin API call failed: {}, trying yt-dlp", e);
                    }
                    Err(e) => {
                        tracing::warn!("DouYin thread spawn failed: {:?}, trying yt-dlp", e);
                    }
                }
            }

            // API 失败或没有 modal_id，尝试 yt-dlp
            let mut ytdlp = crate::yt_dlp::YtDlp::new();
            if let Some(cookie_file) = self.get_cookie_file() {
                ytdlp = ytdlp.with_cookie_file(cookie_file);
            }

            // 如果是 feed URL 带有 modal_id，尝试构造直接视频 URL
            let download_url = if url.contains("/jingxuan/") || url.contains("/recommend/") {
                if let Some(ref m_id) = modal_id {
                    // 构造直接视频 URL
                    format!("https://www.douyin.com/video/{}", m_id)
                } else {
                    url.to_string()
                }
            } else {
                url.to_string()
            };

            match ytdlp.get_video_info(&download_url) {
                Ok(info) => {
                    tracing::info!("DouYin video info via yt-dlp: {}", info.title);
                    return Ok(info);
                }
                Err(e) => {
                    tracing::warn!("DouYin yt-dlp also failed: {}", e);
                    // 返回更友好的错误
                    return Err(ResolverError::ParseFailed("无法解析 DouYin feed URL，请尝试直接使用视频 URL (如 https://www.douyin.com/video/xxx)".to_string()));
                }
            }
        }

        // 普通视频 URL，使用 yt-dlp 优先策略
        if url.contains("/video/") {
            let mut ytdlp = crate::yt_dlp::YtDlp::new();
            if let Some(cookie_file) = self.get_cookie_file() {
                ytdlp = ytdlp.with_cookie_file(cookie_file);
            }

            match ytdlp.get_video_info(url) {
                Ok(info) => {
                    tracing::info!("DouYin video info via yt-dlp: {}", info.title);
                    return Ok(info);
                }
                Err(e) => {
                    tracing::warn!("DouYin yt-dlp failed: {}", e);
                }
            }
        }

        // 最后降级到 yt-dlp
        let mut ytdlp = crate::yt_dlp::YtDlp::new();
        if let Some(cookie_file) = self.get_cookie_file() {
            ytdlp = ytdlp.with_cookie_file(cookie_file);
        }
        ytdlp.get_video_info(url).map_err(ResolverError::from)
    }

    fn requires_auth(&self) -> bool {
        self.tokens.contains_key("msToken")
    }

    fn required_tokens(&self) -> Vec<&'static str> {
        vec!["msToken"]
    }
}

/// 从 URL 提取抖音视频 ID
pub fn extract_douyin_video_id(url: &str) -> anyhow::Result<String> {
    // 格式: https://www.douyin.com/video/7231679494828376333
    // 或: https://v.douyin.com/xxxx

    if url.contains("/video/") {
        let parts: Vec<&str> = url.split("/video/").collect();
        if parts.len() >= 2 {
            let id_part = parts[1].split('?').next().unwrap_or(parts[1]);
            if !id_part.is_empty() {
                return Ok(id_part.to_string());
            }
        }
    }

    if url.contains("v.douyin.com") {
        anyhow::bail!("抖音短链接需要先解析");
    }

    anyhow::bail!("无法从 URL 提取抖音视频 ID")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_supports_url() {
        let resolver = DouYinResolver::new();
        assert!(resolver.supports_url("https://www.douyin.com/video/7231679494828376333"));
        assert!(resolver.supports_url("https://v.douyin.com/xxxx"));
        assert!(!resolver.supports_url("https://tiktok.com/video"));
    }

    #[test]
    fn test_extract_video_id() {
        let id = extract_douyin_video_id("https://www.douyin.com/video/7231679494828376333");
        assert!(id.is_ok());
        assert_eq!(id.unwrap(), "7231679494828376333");
    }
}