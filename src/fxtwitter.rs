//! FxTwitter API 解析模块
//!
//! FxTwitter 是一个第三方 Twitter API，能够解析推文中的链接并返回视频信息
//! 当 yt-dlp 解析失败时，可以尝试使用此模块

use crate::types::{VideoFormat, VideoInfo};
use anyhow::{anyhow, Result};
use serde::Deserialize;

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
    client: reqwest::Client,
}

impl FxTwitterResolver {
    /// 创建新的解析器（自动使用环境变量代理）
    pub fn new() -> Self {
        let mut builder = reqwest::Client::builder()
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36")
            .tcp_keepalive(std::time::Duration::from_secs(60))
            .connect_timeout(std::time::Duration::from_secs(30))
            .timeout(std::time::Duration::from_secs(60));

        // 尝试从环境变量获取代理
        for env_var in ["https_proxy", "HTTPS_PROXY", "http_proxy", "HTTP_PROXY"] {
            if let Ok(proxy_url) = std::env::var(env_var) {
                if !proxy_url.is_empty() {
                    let proxy_with_scheme = if proxy_url.starts_with("http://") || proxy_url.starts_with("https://") {
                        proxy_url.to_string()
                    } else {
                        format!("http://{}", proxy_url)
                    };
                    if let Ok(proxy) = reqwest::Proxy::all(&proxy_with_scheme) {
                        builder = builder.proxy(proxy);
                        break;
                    }
                }
            }
        }

        Self {
            client: builder.build().unwrap_or_else(|_| reqwest::Client::new()),
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

        let client = reqwest::Client::builder()
            .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36")
            .proxy(reqwest::Proxy::all(&proxy_with_scheme)?)
            .build()
            .unwrap_or_default();

        Ok(Self { client })
    }

    /// 获取视频信息
    pub async fn get_video_info(&self, url: &str) -> Result<VideoInfo> {
        // 从 URL 提取推文 ID
        let tweet_id = extract_tweet_id(url)?;

        // 调用 FxTwitter API
        let api_url = format!("https://api.fxtwitter.com/status/{}", tweet_id);
        eprintln!("DEBUG FxTwitter: Calling API: {}", api_url);

        let response = self.client
            .get(&api_url)
            .header("Accept", "application/json")
            .send().await.map_err(|e| {
            eprintln!("DEBUG FxTwitter: request send error: {}", e);
            eprintln!("DEBUG FxTwitter: url was: {}", api_url);
            if e.is_timeout() {
                eprintln!("DEBUG FxTwitter: error is a timeout");
            } else if e.is_connect() {
                eprintln!("DEBUG FxTwitter: error is a connection error");
            }
            anyhow!("FxTwitter 请求失败: {}", e)
        })?;

        eprintln!("DEBUG FxTwitter: Response status: {}", response.status());

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
        eprintln!("DEBUG FxTwitter: author.name = '{}', screen_name = '{}'", tweet.author.name, tweet.author.screen_name);
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

                                    // 从 URL 路径推断扩展名（忽略 query string），避免 API 返回不准确的 container 值
                                    let ext = fmt_url.split('?').next()
                                        .and_then(|s| s.rsplit('/').next_back()?.split('.').next_back())
                                        .unwrap_or("mp4")
                                        .to_string();

                                    video_info.formats.push(VideoFormat {
                                        format_id: format!("fxtwitter_{}", idx),
                                        ext,
                                        resolution,
                                        filesize: None,
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
                    if item.media_type.as_deref() == Some("video") {
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
                        // 其次从 formats 获取
                        if let Some(ref formats) = item.formats {
                            // 优先选择 mp4 格式的高分辨率
                            for fmt in formats {
                                if let Some(fmt_url) = &fmt.url {
                                    if (fmt_url.contains("1280x720") || fmt_url.contains("1920x1080")) && !fmt_url.contains(".m3u8") {
                                        return Ok(fmt_url.clone());
                                    }
                                }
                            }
                            // 返回第一个非 m3u8 的 URL
                            for fmt in formats {
                                if let Some(ref fmt_url) = fmt.url {
                                    if !fmt_url.contains(".m3u8") {
                                        return Ok(fmt_url.clone());
                                    }
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
