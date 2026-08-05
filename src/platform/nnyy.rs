//! nnyy.in 解析器
//!
//! 支持 nnyy.in 电视剧/电影页面的 m3u8 链接提取

use crate::history::Platform;
use crate::hls::M3u8Parser;
use crate::platform::{PlatformResolver, ResolverError};
use crate::types::{DownloadStrategy, VideoFormat, VideoInfo};
use async_trait::async_trait;
use reqwest::Client;

/// nnyy.in 解析器
pub struct NnyyResolver {
    client: Client,
}

impl NnyyResolver {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36")
                .build()
                .unwrap_or_else(|_| Client::new()),
        }
    }

    /// 从页面 HTML 中提取 m3u8 URL
    async fn extract_m3u8_url(&self, url: &str) -> Result<String, ResolverError> {
        tracing::info!("NnyyResolver: 正在获取页面: {}", url);
        let response = self.client.get(url).send().await.map_err(|e| {
            tracing::error!("NnyyResolver: 请求失败: {}", e);
            ResolverError::Network(format!("请求失败: {}", e))
        })?;

        let html = response.text().await.map_err(|e| {
            tracing::error!("NnyyResolver: 读取响应失败: {}", e);
            ResolverError::Network(format!("读取响应失败: {}", e))
        })?;

        tracing::info!("NnyyResolver: 页面内容长度: {}", html.len());

        // 多种模式匹配 m3u8 URL
        let patterns = [
            r#"player\.src\s*\(\s*["']([^"']+\.m3u8[^"']*)["']"#,
            r#"src\s*=\s*["']([^"']+\.m3u8[^"']*)["']"#,
            r#"url\s*:\s*["']([^"']+\.m3u8[^"']*)["']"#,
            r#"https?://[^"'\s]+\.m3u8[^"'\s]*"#,
        ];

        for pattern in &patterns {
            if let Ok(re) = regex::Regex::new(pattern) {
                if let Some(caps) = re.captures(&html) {
                    if let Some(m3u8_url) = caps.get(1).or_else(|| caps.get(0)) {
                        let url = m3u8_url.as_str().to_string();
                        tracing::info!("NnyyResolver: 找到 m3u8 URL: {}", url);
                        return Ok(url);
                    }
                }
            }
        }

        // 输出部分页面内容用于调试
        tracing::warn!(
            "NnyyResolver: 未找到 m3u8 URL，页面前500字符: {}",
            &html[..html.len().min(500)]
        );
        Err(ResolverError::ParseFailed("未找到 m3u8 URL".to_string()))
    }

    /// 获取视频信息
    async fn get_video_info_impl(&self, url: &str) -> Result<VideoInfo, ResolverError> {
        // 提取 m3u8 URL
        let m3u8_url = self.extract_m3u8_url(url).await?;

        // 解析 m3u8 获取时长和分段信息
        let playlist = M3u8Parser::fetch_and_parse(&self.client, &m3u8_url)
            .await
            .map_err(|e| ResolverError::ParseFailed(format!("m3u8 解析失败: {}", e)))?;

        // 生成标题
        let title = format!(
            "nnyy video ({} segments, {:.1}s)",
            playlist.segments.len(),
            playlist.total_duration
        );

        Ok(VideoInfo {
            title,
            description: Some(format!("来源: {}", url)),
            uploader: None,
            duration: Some(playlist.total_duration as u64),
            url: m3u8_url,
            thumbnail: None,
            formats: vec![VideoFormat {
                format_id: "hls".to_string(),
                ext: "mp4".to_string(),
                resolution: None,
                filesize: None,
                bitrate: None,
                codec: None,
                audio_only: false,
                format_note: None,
            }],
            created_at: None,
            download_strategy: DownloadStrategy::Native,
            audio_url: None,
        })
    }
}

impl Default for NnyyResolver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PlatformResolver for NnyyResolver {
    fn platform(&self) -> Platform {
        Platform::Other
    }

    fn supported_url_patterns(&self) -> Vec<&'static str> {
        vec!["https://nnyy.in/*"]
    }

    fn supports_url_by_host(&self, host_lower: &str) -> bool {
        host_lower == "nnyy.in" || host_lower.ends_with(".nnyy.in")
    }

    async fn get_video_info(&self, url: &str) -> Result<VideoInfo, ResolverError> {
        self.get_video_info_impl(url).await
    }

    fn requires_auth(&self) -> bool {
        false
    }

    fn download_strategy(&self) -> DownloadStrategy {
        DownloadStrategy::Hls
    }
}
