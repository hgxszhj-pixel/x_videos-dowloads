//! 抖音 API 客户端
//!
//! 用于直接调用抖音 API 获取视频信息，绕过 yt-dlp 对某些 URL 类型的限制
//! 参考 F2 的 DouYinAPI 实现

use crate::resolvers::xbogus::XBogus;
use serde::Deserialize;

/// 抖音 API 客户端
pub struct DouYinApiClient {
    client: reqwest::Client,
    xbogus: XBogus,
    user_agent: String,
    ms_token: Option<String>,
}

impl DouYinApiClient {
    /// 创建新的 API 客户端
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_default(),
            xbogus: XBogus::new(),
            user_agent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36".to_string(),
            ms_token: None,
        }
    }

    /// 设置 msToken
    pub fn with_ms_token(mut self, token: String) -> Self {
        self.ms_token = Some(token);
        self
    }

    /// 生成带 A-Bogus 签名的请求 URL
    fn sign_url(&self, url: &str) -> String {
        let parts: Vec<&str> = url.split('?').collect();
        if parts.len() < 2 {
            return url.to_string();
        }

        let query = parts[1];
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        let signature = self.xbogus.sign(query, &self.user_agent, timestamp);
        format!("{}?{}&X-Bogus={}", parts[0], query, signature)
    }

    /// 从 modal_id 获取视频详情
    pub async fn get_video_by_modal_id(&self, modal_id: &str) -> anyhow::Result<VideoDetailResponse> {
        // 抖音视频详情 API - 参考 F2 实现
        // 使用更多参数提高成功率
        let url = format!(
            "https://www.douyin.com/aweme/v1/web/aweme/detail/?modal_id={}&device_platform=web&beta_bit_list=1&cookie_enabled=1&screen_size=1920&downlink=10&css=1&js=1&platform=pc&channel=pc&webid=&extension_pcgid=&search_channel=aweme_video_web&keyword_session_id=&search_source=normal_search&queryKeyword=&scene=search_result&sponsor_block_config=&user_is_login=1&aweme_id=&search_key=",
            modal_id
        );

        let signed_url = self.sign_url(&url);
        tracing::debug!("DouYin API URL: {}", signed_url);

        let mut request = self.client.get(&signed_url);

        request = request
            .header("User-Agent", &self.user_agent)
            .header("Referer", "https://www.douyin.com/video/".to_string() + modal_id)
            .header("Accept", "application/json, text/plain, */*")
            .header("Accept-Language", "zh-CN,zh;q=0.9,en;q=0.8")
            .header("Accept-Encoding", "gzip, deflate, br")
            .header("Connection", "keep-alive")
            .header("sec-ch-ua", "\"Not_A Brand\";v=\"8\", \"Chromium\";v=\"120\"")
            .header("sec-ch-ua-mobile", "?0")
            .header("sec-ch-ua-platform", "\"macOS\"")
            .header("sec-fetch-dest", "empty")
            .header("sec-fetch-mode", "cors")
            .header("sec-fetch-site", "same-origin");

        // 添加 msToken cookie
        if let Some(token) = &self.ms_token {
            // 直接构建字符串，避免 format! 的格式化开销
            let cookie_len = 8 + token.len(); // "msToken=" (8) + token
            let mut cookie = String::with_capacity(cookie_len);
            cookie.push_str("msToken=");
            cookie.push_str(token);
            request = request.header("Cookie", cookie);
        }

        let response = request.send().await?;

        let status = response.status();
        tracing::debug!("DouYin API response status: {}", status);

        let text = response.text().await?;
        tracing::debug!("DouYin API response body length: {}", text.len());

        // 调试：打印实际响应内容的前500字符
        let preview = text.chars().take(500).collect::<String>();
        tracing::debug!("DouYin API response preview: {}", preview);

        if text.is_empty() {
            anyhow::bail!("Empty response from DouYin API");
        }

        // 检查是否返回了 HTML（重定向或其他）
        if text.starts_with("<!DOCTYPE") || text.starts_with("<html") {
            tracing::warn!("DouYin API returned HTML instead of JSON");
            anyhow::bail!("API returned HTML, likely need full login cookies");
        }

        let json: VideoDetailResponse = serde_json::from_str(&text)
            .map_err(|e| anyhow::anyhow!("Failed to parse response: {} - body: {}", e, &text.chars().take(200).collect::<String>()))?;

        Ok(json)
    }
}

impl Default for DouYinApiClient {
    fn default() -> Self {
        Self::new()
    }
}

/// 视频详情响应
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct VideoDetailResponse {
    #[serde(rename = "status_code")]
    pub status_code: i32,
    #[serde(rename = "aweme_list")]
    pub aweme_list: Option<Vec<AwemeItem>>,
}

/// 作品项
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct AwemeItem {
    #[serde(rename = "aweme_id")]
    pub aweme_id: Option<String>,
    #[serde(rename = "desc")]
    pub desc: Option<String>,
    #[serde(rename = "create_time")]
    pub create_time: Option<i64>,
    #[serde(rename = "author")]
    pub author: Option<Author>,
    #[serde(rename = "video")]
    pub video: Option<Video>,
    #[serde(rename = "statistics")]
    pub statistics: Option<Statistics>,
}

impl AwemeItem {
    /// 获取视频 ID
    pub fn get_video_id(&self) -> Option<&str> {
        self.aweme_id.as_deref()
    }

    /// 获取视频标题
    pub fn get_title(&self) -> String {
        self.desc.clone().unwrap_or_default()
    }

    /// 获取作者昵称
    pub fn get_author_name(&self) -> String {
        self.author.as_ref()
            .and_then(|a| a.nickname.clone())
            .unwrap_or_default()
    }

    /// 获取视频时长(秒)
    pub fn get_duration(&self) -> u64 {
        self.video.as_ref()
            .and_then(|v| v.duration)
            .unwrap_or(0) as u64 / 1000
    }
}

impl VideoDetailResponse {
    /// 获取第一个视频（如果存在）
    #[allow(dead_code)]
    pub fn get_first_video(&self) -> Option<&AwemeItem> {
        self.aweme_list.as_ref()?.first()
    }
}

/// 作者信息
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct Author {
    #[serde(rename = "nickname")]
    pub nickname: Option<String>,
    #[serde(rename = "unique_id")]
    pub unique_id: Option<String>,
    #[serde(rename = "uid")]
    pub uid: Option<String>,
}

/// 视频信息
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct Video {
    #[serde(rename = "duration")]
    pub duration: Option<i64>,
    #[serde(rename = "play_addr")]
    pub play_addr: Option<PlayAddr>,
    #[serde(rename = "cover")]
    pub cover: Option<Cover>,
    #[serde(rename = "bitrate")]
    pub bitrate: Option<i64>,
    #[serde(rename = "width")]
    pub width: Option<i64>,
    #[serde(rename = "height")]
    pub height: Option<i64>,
}

/// 播放地址
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct PlayAddr {
    #[serde(rename = "uri")]
    pub uri: Option<String>,
    #[serde(rename = "url_list")]
    pub url_list: Option<Vec<String>>,
}

/// 封面
#[derive(Debug, Deserialize)]
pub struct Cover {
    #[serde(rename = "url_list")]
    pub url_list: Option<Vec<String>>,
}

/// 统计数据
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct Statistics {
    #[serde(rename = "play_count")]
    pub play_count: Option<u64>,
    #[serde(rename = "digg_count")]
    pub digg_count: Option<u64>,
    #[serde(rename = "comment_count")]
    pub comment_count: Option<u64>,
    #[serde(rename = "share_count")]
    pub share_count: Option<u64>,
}

/// 从 URL 解析 modal_id
pub fn extract_modal_id(url: &str) -> Option<String> {
    // URL 格式: https://www.douyin.com/jingxuan/sports?modal_id=7616615936887459108
    if let Some(query) = url.split('?').nth(1) {
        for pair in query.split('&') {
            let mut parts = pair.split('=');
            if let Some(key) = parts.next() {
                if key == "modal_id" {
                    return parts.next().map(|s| s.to_string());
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_modal_id() {
        let url = "https://www.douyin.com/jingxuan/sports?modal_id=7616615936887459108";
        let modal_id = extract_modal_id(url);
        assert_eq!(modal_id, Some("7616615936887459108".to_string()));
    }

    #[test]
    fn test_extract_modal_id_from_video_url() {
        let url = "https://www.douyin.com/video/7231679494828376333?modal_id=7616615936887459108";
        // modal_id 优先
        let modal_id = extract_modal_id(url);
        assert_eq!(modal_id, Some("7616615936887459108".to_string()));
    }
}
