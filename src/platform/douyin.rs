//! 抖音解析器
//!
//! 使用 yt-dlp 作为主要方式，msToken/ttwid 用于认证
//! 支持信息流 URL (jingxuan/sports) 和 modal_id 解析
//! API 分支（feed/modal_id）走 DouYinApiClient，yt-dlp 分支走 YtdlpBacked

use crate::history::Platform;
use crate::platform::{
    extract_modal_id, url_matcher, DouYinApiClient, PlatformResolver, ResolverError, YtdlpBacked,
};
use crate::types::{VideoFormat, VideoInfo};
use async_trait::async_trait;
use std::collections::HashMap;

/// 抖音解析器
pub struct DouYinResolver {
    base: YtdlpBacked,
}

impl DouYinResolver {
    pub fn new() -> Self {
        Self {
            base: YtdlpBacked::new(Platform::DouYin),
        }
    }

    pub fn with_tokens(mut self, tokens: HashMap<String, String>) -> Self {
        self.base = self.base.with_tokens(tokens);
        self
    }

    /// 构造直接视频 URL（jingxuan/recommend feed → modal_id）
    fn direct_video_url(&self, url: &str, modal_id: Option<&String>) -> String {
        if url.contains("/jingxuan/") || url.contains("/recommend/") {
            if let Some(m_id) = modal_id {
                return format!("https://www.douyin.com/video/{}", m_id);
            }
        }
        url.to_string()
    }
}

impl Default for DouYinResolver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PlatformResolver for DouYinResolver {
    fn platform(&self) -> Platform {
        Platform::DouYin
    }

    fn supported_url_patterns(&self) -> Vec<&'static str> {
        url_matcher::get_platform_patterns(Platform::DouYin).to_vec()
    }

    async fn get_video_info(&self, url: &str) -> Result<VideoInfo, ResolverError> {
        // 检查是否是 feed URL（jingxuan/sports 等）或包含 modal_id
        let modal_id = extract_modal_id(url);
        let is_feed_url =
            url.contains("jingxuan") || url.contains("sports") || url.contains("recommend");

        // 对于 feed URL 和包含 modal_id 的 URL，优先使用 API
        if modal_id.is_some() || is_feed_url {
            tracing::info!("DouYin feed/modal_id URL detected, using API directly");

            // 如果有 modal_id，尝试从 API 获取
            if let Some(modal_id) = &modal_id {
                let modal_id_clone = modal_id.clone();
                let ms_token = self.base.tokens_get("msToken").cloned();

                // get_video_by_modal_id 是 async fn，DouYinApiClient 实现了 Sync
                // 将整个 async 调用封装在 spawn_blocking 中，避免 nested Runtime::new().block_on()
                let api_result = tokio::task::spawn_blocking(move || {
                    let client = if let Some(token) = ms_token {
                        DouYinApiClient::new().with_ms_token(token)
                    } else {
                        DouYinApiClient::new()
                    };
                    // DouYinApiClient 内部使用 reqwest::Client（Sync），可以直接调用 async fn
                    let rt = tokio::runtime::Runtime::new()?;
                    rt.block_on(client.get_video_by_modal_id(&modal_id_clone))
                })
                .await;

                match api_result {
                    Ok(Ok(resp)) => {
                        if let Some(video) =
                            resp.aweme_list.and_then(|list| list.into_iter().next())
                        {
                            let title = video.get_title();
                            let video_id = video.get_video_id().unwrap_or(modal_id).to_string();
                            let uploader = video.get_author_name();
                            let duration = video.get_duration();
                            let thumbnail = video
                                .video
                                .as_ref()
                                .and_then(|v| v.cover.as_ref())
                                .and_then(|c| c.url_list.as_ref())
                                .and_then(|urls| urls.first())
                                .cloned();

                            tracing::info!("DouYin API success: {} by {}", title, uploader);

                            return Ok(VideoInfo {
                                title: if title.is_empty() {
                                    format!("DouYin Video {}", video_id)
                                } else {
                                    title
                                },
                                description: None,
                                uploader: Some(uploader),
                                duration: Some(duration),
                                url: format!("https://www.douyin.com/video/{}", video_id),
                                thumbnail,
                                formats: vec![VideoFormat {
                                    format_id: video_id.clone(),
                                    ext: "mp4".to_string(),
                                    resolution: Some(
                                        video
                                            .video
                                            .as_ref()
                                            .map(|v| {
                                                let w = v.width.unwrap_or(0);
                                                let h = v.height.unwrap_or(0);
                                                if w > 0 && h > 0 {
                                                    format!("{}x{}", w, h)
                                                } else {
                                                    "1080p".to_string()
                                                }
                                            })
                                            .unwrap_or_else(|| "1080p".to_string()),
                                    ),
                                    filesize: None,
                                    bitrate: video
                                        .video
                                        .as_ref()
                                        .and_then(|v| v.bitrate)
                                        .map(|b| b as u64),
                                    codec: None,
                                    audio_only: false,
                                    format_note: Some("DouYin API".to_string()),
                                }],
                                created_at: None,
                                download_strategy: crate::types::DownloadStrategy::YtDlp,
                                audio_url: None,
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

            // API 失败或没有 modal_id，尝试 yt-dlp（走 base）
            let download_url = self.direct_video_url(url, modal_id.as_ref());
            match self.base.fetch_via_ytdlp(&download_url).await {
                Ok(info) => {
                    tracing::info!("DouYin video info via yt-dlp: {}", info.title);
                    return Ok(info);
                }
                Err(e) => {
                    tracing::warn!("DouYin yt-dlp also failed: {}", e);
                    return Err(ResolverError::ParseFailed(
                        "无法解析 DouYin feed URL，请尝试直接使用视频 URL (如 https://www.douyin.com/video/xxx)".to_string()
                    ));
                }
            }
        }

        // 普通视频 URL，使用 yt-dlp（走 base）
        if url.contains("/video/") {
            return self.base.fetch_via_ytdlp(url).await;
        }

        // 最后降级到 yt-dlp（走 base）
        self.base.fetch_via_ytdlp(url).await
    }

    fn requires_auth(&self) -> bool {
        self.base.has_token("msToken")
    }

    fn required_tokens(&self) -> Vec<&'static str> {
        vec!["msToken"]
    }
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
}
