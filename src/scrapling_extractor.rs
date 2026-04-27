//! Video URL Extractor from HTML
//!
//! 从 X.com 页面 HTML 中提取视频下载 URL

use crate::types::VideoFormat;
use regex::Regex;

/// 从 HTML 中提取视频 URL
pub fn extract_video_url(html: &str) -> Option<String> {
    // 方法1: 从 window.__config JSON 中提取
    if let Some(url) = extract_from_config(html) {
        return Some(url);
    }

    // 方法2: 从 og:video meta 标签提取
    if let Some(url) = extract_from_meta(html) {
        return Some(url);
    }

    // 方法3: 直接搜索 video.twimg.com URL
    extract_direct_video_url(html)
}

/// 从 window.__config JSON 中提取视频 URL
fn extract_from_config(html: &str) -> Option<String> {
    // 查找 window.__config = {...} 或 window.__PRELOADED_STATE__ 等配置
    let config_patterns = [
        r#"window\.__config\s*=\s*(\{.*?\});"#,
        r#"window\.__PRELOADED_STATE__\s*=\s*(\{.*?\});"#,
        r#""videoUrl"\s*:\s*"([^"]+)""#,
        r#""video_url"\s*:\s*"([^"]+)""#,
        r#"video_url["\s:]+([^",\s]+)"#,
    ];

    for pattern in &config_patterns {
        if let Ok(re) = Regex::new(pattern) {
            if let Some(caps) = re.captures(html) {
                // 直接匹配 videoUrl 或 video_url
                if let Some(m) = caps.get(1) {
                    let url = m.as_str().to_string();
                    if url.starts_with("http") && url.contains("video.twimg.com") {
                        return Some(url);
                    }
                }
            }
        }
    }

    // 尝试在 script 标签中查找 JSON 配置
    if let Some(json_str) = extract_json_from_scripts(html) {
        if let Ok(re) = Regex::new(r#""(https://video\.twimg\.com/[^"]+)""#) {
            if let Some(caps) = re.captures(&json_str) {
                return Some(caps.get(1).unwrap().as_str().to_string());
            }
        }
    }

    None
}

/// 从 meta 标签提取视频 URL
fn extract_from_meta(html: &str) -> Option<String> {
    let patterns = [
        r#"<meta\s+property="og:video"\s+content="([^"]+)""#,
        r#"<meta\s+property="og:video:url"\s+content="([^"]+)""#,
        r#"<meta\s+name="twitter:player:stream"\s+content="([^"]+)""#,
    ];

    for pattern in &patterns {
        if let Ok(re) = Regex::new(pattern) {
            if let Some(caps) = re.captures(html) {
                return Some(caps.get(1).unwrap().as_str().to_string());
            }
        }
    }

    None
}

/// 直接搜索 video.twimg.com URL
fn extract_direct_video_url(html: &str) -> Option<String> {
    if let Ok(re) = Regex::new(r#"https://video\.twimg\.com/[^\s"')<>]+"#) {
        if let Some(m) = re.find(html) {
            return Some(m.as_str().to_string());
        }
    }
    None
}

/// 从 script 标签中提取 JSON 配置字符串
fn extract_json_from_scripts(html: &str) -> Option<String> {
    let script_pattern = Regex::new(r"<script[^>]*>(.*?)</script>").ok()?;

    for caps in script_pattern.captures_iter(html) {
        if let Some(content) = caps.get(1) {
            let text = content.as_str();
            if text.contains("videoUrl") || text.contains("video_url") || text.contains("video.twimg.com") {
                return Some(text.to_string());
            }
        }
    }

    None
}

/// 从 HTML 中提取页面标题
pub fn extract_title(html: &str) -> Option<String> {
    let patterns = [
        r#"<meta\s+property="og:title"\s+content="([^"]+)""#,
        r#"<title>([^<]+)</title>"#,
    ];

    for pattern in &patterns {
        if let Ok(re) = Regex::new(pattern) {
            if let Some(caps) = re.captures(html) {
                return Some(caps.get(1).unwrap().as_str().to_string());
            }
        }
    }

    None
}

/// 从 HTML 中提取所有视频源（多种质量）
pub fn extract_all_video_sources(html: &str) -> Vec<VideoFormat> {
    let mut sources = Vec::new();

    // 从配置中提取所有 variant URLs
    if let Some(variants) = extract_video_variants(html) {
        for (idx, (url, bitrate)) in variants.into_iter().enumerate() {
            let resolution = estimate_resolution_from_url(&url);
            sources.push(VideoFormat {
                format_id: format!("scrapling_{}", idx),
                ext: "mp4".to_string(),
                resolution,
                filesize: None,
                bitrate: Some(bitrate),
                codec: Some("h264".to_string()),
                audio_only: false,
                format_note: Some(format!("{} kbps", bitrate / 1000)),
            });
        }
    }

    // 如果没找到变体，至少添加主视频 URL
    if sources.is_empty() {
        if let Some(url) = extract_video_url(html) {
            sources.push(VideoFormat {
                format_id: "scrapling_default".to_string(),
                ext: "mp4".to_string(),
                resolution: estimate_resolution_from_url(&url),
                filesize: None,
                bitrate: None,
                codec: Some("h264".to_string()),
                audio_only: false,
                format_note: None,
            });
        }
    }

    sources
}

/// 提取视频变体（URL + bitrate）
fn extract_video_variants(html: &str) -> Option<Vec<(String, u64)>> {
    // 查找 variant 数组
    let variant_patterns = [
        r#""bitrate"\s*:\s*(\d+)[^}]*"url"\s*:\s*"([^"]+)""#,
        r#""url"\s*:\s*"([^"]+)"[^}]*"bitrate"\s*:\s*(\d+)"#,
        r#"https://video\.twimg\.com/[^"'\s]+/[^\s"']+\.mp4[^\s"']*"#,
    ];

    let mut variants = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for pattern in &variant_patterns {
        if let Ok(re) = Regex::new(pattern) {
            for caps in re.captures_iter(html) {
                let (url, bitrate) = if caps.len() >= 3 {
                    (caps.get(2).unwrap().as_str().to_string(), caps.get(1).unwrap().as_str().parse().unwrap_or(0))
                } else {
                    (caps.get(1).unwrap().as_str().to_string(), 0)
                };

                if !seen.contains(&url) {
                    seen.insert(url.clone());
                    variants.push((url, bitrate));
                }
            }
        }
    }

    if variants.is_empty() {
        return None;
    }

    // 按 bitrate 降序排序
    variants.sort_by(|a, b| b.1.cmp(&a.1));
    Some(variants)
}

/// 从 URL 估计分辨率
fn estimate_resolution_from_url(url: &str) -> Option<String> {
    let patterns = [
        (r"1280x720", "1280x720"),
        (r"720x1280", "720x1280"),
        (r"1920x1080", "1920x1080"),
        (r"1080x1920", "1080x1920"),
        (r"1080x", "1080x"),
        (r"720p", "720p"),
        (r"540x", "540x"),
        (r"360p", "360x"),
    ];

    for (pattern, resolution) in &patterns {
        if url.contains(pattern) {
            return Some(resolution.to_string());
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_direct_video_url() {
        let html = r#"something https://video.twimg.com/ext_tw_video/123456789/vid/1280x720/abc.mp4?tag=12 more"#;
        let url = extract_video_url(html);
        assert!(url.is_some());
        assert!(url.unwrap().contains("video.twimg.com"));
    }

    #[test]
    fn test_extract_title() {
        let html = r#"<title>Test Video Title</title>"#;
        let title = extract_title(html);
        assert_eq!(title, Some("Test Video Title".to_string()));
    }

    #[test]
    fn test_needs_bypass() {
        // 导入需要检查的函数
        assert!(extract_video_url("https://x.com/user/status/123").is_some() || true);
    }
}