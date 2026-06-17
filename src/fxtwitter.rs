//! FxTwitter API 解析模块
//!
//! FxTwitter 是一个第三方 Twitter API，能够解析推文中的链接并返回视频信息
//! 当 yt-dlp 解析失败时，可以尝试使用此模块

use crate::types::{VideoFormat, VideoInfo};
use anyhow::{anyhow, Result};
use serde::Deserialize;
use std::sync::Arc;

/// FxTwitter API 响应
#[derive(Debug, Deserialize)]
struct FxTwitterResponse {
    code: u16,
    message: String,
    #[serde(rename = "tweet")]
    tweet: Option<FxTweet>,
}

/// FxTweet 数据结构
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct FxTweet {
    url: String,
    id: String,
    text: String,
    #[serde(rename = "raw_text")]
    raw_text: Option<FxRawText>,
    author: FxAuthor,
    media: Option<FxMedia>,
}

/// FxRawText
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct FxRawText {
    #[serde(rename = "display_text_range")]
    display_text_range: Option<Vec<u64>>,
    facets: Option<Vec<FxFacet>>,
    #[serde(default)]
    text: String,
}

/// FxFacet（推文中的链接/媒体标记）
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct FxFacet {
    #[serde(rename = "type")]
    facet_type: Option<String>,
    indices: Option<Vec<u64>>,
    id: Option<String>,
    display: Option<String>,
    original: Option<String>,
    replacement: Option<String>,
}

/// FxAuthor
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct FxAuthor {
    #[serde(rename = "screen_name")]
    screen_name: String,
    name: String,
    #[serde(default)]
    avatar: Option<String>,
}

/// FxMedia
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct FxMedia {
    all: Option<Vec<FxMediaItem>>,
    videos: Option<Vec<FxMediaItem>>,
}

/// FxMediaItem
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct FxMediaItem {
    id: Option<String>,
    url: Option<String>,
    #[serde(rename = "thumbnail_url")]
    thumbnail_url: Option<String>,
    duration: Option<f64>,
    width: Option<u64>,
    height: Option<u64>,
    #[serde(rename = "type")]
    media_type: Option<String>,
    formats: Option<Vec<FxFormat>>,
    variants: Option<Vec<FxVariant>>,
}

/// FxFormat
#[derive(Debug, Deserialize)]
struct FxFormat {
    url: Option<String>,
    #[allow(dead_code)]
    container: Option<String>,
    codec: Option<String>,
    bitrate: Option<u64>,
}

/// FxVariant
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct FxVariant {
    url: Option<String>,
    bitrate: Option<u64>,
    #[serde(rename = "content_type")]
    content_type: Option<String>,
}

/// FxTwitter API 解析器
pub struct FxTwitterResolver {
    client: Arc<reqwest::Client>,
}

impl FxTwitterResolver {
    /// 创建新的解析器（使用共享客户端，自动支持环境变量代理）
    pub fn new() -> Self {
        // 使用共享客户端，自动继承环境变量代理支持
        Self {
            client: crate::http_client::shared(),
        }
    }

    /// 使用指定代理创建解析器
    pub fn with_proxy(proxy_url: &str) -> Result<Self> {
        // 确保代理 URL 有协议前缀
        let proxy_with_scheme = if proxy_url.starts_with("http://") || proxy_url.starts_with("https://") {
            proxy_url.to_string()
        } else {
            format!("http://{}", proxy_url)
        };

        // 创建带有代理的新客户端，基于共享客户端配置
        let client = reqwest::Client::builder()
            .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36")
            .tcp_keepalive(std::time::Duration::from_secs(120))
            .connect_timeout(std::time::Duration::from_secs(30))
            .timeout(std::time::Duration::from_secs(600))
            .pool_max_idle_per_host(32)
            .proxy(reqwest::Proxy::all(&proxy_with_scheme)?)
            .build()
            .map_err(|e| anyhow!("Failed to create client with proxy: {}", e))?;

        Ok(Self { client: Arc::new(client) })
    }

    /// 获取视频信息
    pub async fn get_video_info(&self, url: &str) -> Result<VideoInfo> {
        // 从 URL 提取推文 ID
        let tweet_id = extract_tweet_id(url)?;

        // 调用 FxTwitter API
        let api_url = format!("https://api.fxtwitter.com/status/{}", tweet_id);

        let response = self.client
            .get(&api_url)
            .header("Accept", "application/json")
            .send().await.map_err(|e: reqwest::Error| -> anyhow::Error {
                if e.is_timeout() || e.is_connect() {
                    anyhow!("FxTwitter connection failed: {}", e)
                } else {
                    anyhow!("FxTwitter 请求失败: {}", e)
                }
            })?;


        if !response.status().is_success() {
            return Err(anyhow!("FxTwitter API 请求失败: HTTP {}", response.status()));
        }

        let fx_response: FxTwitterResponse = response.json().await?;

        if fx_response.code != 200 {
            return Err(anyhow!("FxTwitter API 错误: {} - {}", fx_response.code, fx_response.message));
        }

        let tweet = fx_response.tweet
            .ok_or_else(|| anyhow!("推文数据为空"))?;

        // 解析推文中的链接，找到真正的视频源
        let video_url = self.find_video_url(&tweet)?;

        // 构建 VideoInfo - 使用推文文本作为标题
        let title = tweet.text.clone();
        let mut video_info = VideoInfo::new(title, video_url.clone());

        video_info.uploader = Some(tweet.author.name.clone());
        video_info.description = Some(tweet.text.clone());

        // 解析媒体信息
        if let Some(ref media) = tweet.media {
            // 直接迭代媒体项，避免创建中间 Vec
            if let Some(ref all_media) = media.all {
                for item in all_media {
                    if item.media_type.as_deref() == Some("video") {
                        // 设置缩略图
                        if let Some(ref thumb_url) = item.thumbnail_url {
                            video_info.thumbnail = Some(thumb_url.clone());
                        }

                        // 设置时长
                        if let Some(dur) = item.duration {
                            video_info.duration = Some(dur as u64);
                        }

                        // 解析格式
                        if let Some(ref formats) = item.formats {
                            for (idx, fmt) in formats.iter().enumerate() {
                                if let Some(ref fmt_url) = fmt.url {
                                    let resolution = Self::extract_resolution_from_url(fmt_url);

                                    // 优先使用 container 字段，否则从 URL 推断或默认 mp4
                                    let ext = fmt.container.clone()
                                        .or_else(|| {
                                            fmt_url.split('?').next()
                                                .and_then(|s| s.rsplit('/').next_back()?.split('.').next_back())
                                                .map(|e| e.to_string())
                                                .filter(|e| !e.is_empty() && e.len() <= 5)
                                        })
                                        .unwrap_or_else(|| "mp4".to_string());

                                    // 估算文件大小: bitrate(bps) × duration(s) / 8 = bytes
                                    let filesize = fmt.bitrate.and_then(|br| {
                                        video_info.duration.map(|dur| (br * dur) / 8)
                                    });

                                    video_info.formats.push(VideoFormat {
                                        format_id: format!("fxtwitter_{}", idx),
                                        ext,
                                        resolution,
                                        filesize,
                                        bitrate: fmt.bitrate,
                                        codec: fmt.codec.clone(),
                                        audio_only: false,
                                        format_note: Some(format!("{} kbps", fmt.bitrate.unwrap_or(0) / 1000)),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        // 如果没有解析到格式，至少添加一个默认格式
        if video_info.formats.is_empty() {
            video_info.formats.push(VideoFormat {
                format_id: "default".to_string(),
                ext: "mp4".to_string(),
                resolution: None,
                filesize: None,
                bitrate: None,
                codec: Some("h264".to_string()),
                audio_only: false,
                format_note: None,
            });
        }

        Ok(video_info)
    }

    /// 从推文中找到视频 URL
    fn find_video_url(&self, tweet: &FxTweet) -> Result<String> {
        // 首先检查直接嵌入的视频
        if let Some(ref media) = tweet.media {
            if let Some(ref all) = media.all {
                for item in all {
                    // 优先检查 media_type === "video"
                    // 但也要处理 media_type 为 undefined 但有 variants 的情况
                    let has_video_type = item.media_type.as_deref() == Some("video");
                    let has_variants = item.variants.as_ref().map(|v| !v.is_empty()).unwrap_or(false);

                    if has_video_type || has_variants {
                        // 优先从 variants 获取最高质量的视频 URL
                        if let Some(ref variants) = item.variants {
                            // 找最高的 bitrate 的 mp4 视频
                            let mut best_variant: Option<&FxVariant> = None;
                            let mut best_bitrate: u64 = 0;
                            for variant in variants {
                                if variant.content_type.as_deref() == Some("video/mp4") {
                                    if let Some(bitrate) = variant.bitrate {
                                        if bitrate > best_bitrate {
                                            best_bitrate = bitrate;
                                            best_variant = Some(variant);
                                        }
                                    }
                                }
                            }
                            // 如果没找到 mp4，找第一个有 URL 的 variant
                            if best_variant.is_none() {
                                for variant in variants {
                                    if let Some(ref url) = variant.url {
                                        if !url.contains(".m3u8") {
                                            return Ok(url.clone());
                                        }
                                    }
                                }
                            }
                            if let Some(variant) = best_variant {
                                if let Some(url) = &variant.url {
                                    return Ok(url.clone());
                                }
                            }
                        }
                        // 其次从 formats 获取 - 选择最佳质量（最高比特率 + 分辨率）
                        if let Some(ref formats) = item.formats {
                            let mut best_format: Option<&FxFormat> = None;
                            let mut best_quality: u64 = 0;

                            for fmt in formats {
                                if let Some(fmt_url) = &fmt.url {
                                    if fmt_url.contains(".m3u8") {
                                        continue;
                                    }
                                    // 基于比特率 + 分辨率评估质量
                                    let quality = fmt.bitrate.unwrap_or(0);
                                    let resolution_bonus = if fmt_url.contains("1920x1080") {
                                        100_000_000
                                    } else if fmt_url.contains("1280x720") {
                                        50_000_000
                                    } else if fmt_url.contains("720x") || fmt_url.contains("x720") {
                                        25_000_000
                                    } else {
                                        0
                                    };
                                    let total_quality = quality + resolution_bonus;
                                    if total_quality > best_quality {
                                        best_quality = total_quality;
                                        best_format = Some(fmt);
                                    }
                                }
                            }

                            if let Some(fmt) = best_format {
                                if let Some(ref fmt_url) = fmt.url {
                                    return Ok(fmt_url.clone());
                                }
                            }
                        }
                    }
                }
            }
        }

        Err(anyhow!("未在推文中找到视频内容"))
    }

    /// 从 URL 提取分辨率
    fn extract_resolution_from_url(url: &str) -> Option<String> {
        // 尝试从 URL 中提取分辨率，如 1280x720
        if let Some(res) = url.split("/vid/").nth(1) {
            let parts: Vec<&str> = res.split('/').collect();
            if !parts.is_empty() {
                let dims: Vec<&str> = parts[0].split('x').collect();
                if dims.len() == 2 {
                    return Some(format!("{}x{}", dims[0], dims[1]));
                }
            }
        }
        None
    }
}

impl Default for FxTwitterResolver {
    fn default() -> Self {
        Self::new()
    }
}

/// 从推文 URL 提取推文 ID
pub fn extract_tweet_id(url: &str) -> Result<String> {
    // 支持的格式:
    // https://x.com/user/status/123456789
    // https://twitter.com/user/status/123456789
    // https://x.com/user/status/123456789?s=20

    let url = url.split('?').next().unwrap_or(url);

    if let Some(status_idx) = url.find("/status/") {
        let after_status = &url[status_idx + 8..];
        // 提取数字 ID
        let id: String = after_status.chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();

        if id.is_empty() {
            return Err(anyhow!("无法从 URL 中提取推文 ID: {}", url));
        }

        return Ok(id);
    }

    Err(anyhow!("无效的推文 URL 格式: {}", url))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_tweet_id() {
        assert_eq!(
            extract_tweet_id("https://x.com/HopePJ/status/2043180830642807037").unwrap(),
            "2043180830642807037"
        );

        assert_eq!(
            extract_tweet_id("https://x.com/HopePJ/status/2043180830642807037?s=20").unwrap(),
            "2043180830642807037"
        );

        assert_eq!(
            extract_tweet_id("https://twitter.com/user/status/123456789").unwrap(),
            "123456789"
        );
    }
}
