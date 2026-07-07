//! m3u8 播放列表解析器
//!
//! 支持 Master Playlist 和 Media Playlist，解析 EXTINF 分段和 AES-128 加密信息

use anyhow::Result;

/// 加密方法
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq)]
pub enum EncryptionMethod {
    None,
    AES128,
    SampleAes,
}

/// m3u8 分段
#[allow(dead_code)] // title field exposed in public API
#[derive(Debug, Clone)]
pub struct M3u8Segment {
    /// 分段序号
    pub sequence: u64,
    /// 分段 URL
    pub url: String,
    /// 分段时长（秒）
    pub duration: f64,
    /// 分段标题（可选）
    pub title: Option<String>,
    /// 加密密钥 URL（如果已加密）
    pub key_url: Option<String>,
    /// 加密 IV（hex 字符串，如果已加密）
    pub key_iv: Option<String>,
}

/// m3u8 播放列表
#[allow(dead_code)] // version/target_duration/is_live/base_url exposed in public API
#[derive(Debug, Clone)]
pub struct M3u8Playlist {
    /// 播放列表版本
    pub version: Option<u32>,
    /// 目标时长（Media Playlist）
    pub target_duration: f64,
    /// 分段列表
    pub segments: Vec<M3u8Segment>,
    /// 总体时长
    pub total_duration: f64,
    /// 是否为 Live 直播
    pub is_live: bool,
    /// 解析时的基础 URL（用于解析相对路径）
    pub base_url: String,
}

/// m3u8 解析器
pub struct M3u8Parser;

impl M3u8Parser {
    /// 解析 m3u8 内容
    pub fn parse(content: &str, base_url: &str) -> Result<M3u8Playlist> {
        let mut version: Option<u32> = None;
        let mut target_duration: f64 = 0.0;
        let mut segments: Vec<M3u8Segment> = Vec::new();
        let mut is_live = false;

        // 加密状态
        let mut current_key_url: Option<String> = None;
        let mut current_key_iv: Option<String> = None;

        let mut sequence: u64 = 0;
        let mut current_duration: f64 = 0.0;
        let mut current_title: Option<String> = None;

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                if line == "#EXTM3U" {
                    continue;
                }
                if line.starts_with("#EXT-X-VERSION:") {
                    version = line.trim_start_matches("#EXT-X-VERSION:")
                        .parse().ok();
                } else if line.starts_with("#EXT-X-TARGETDURATION:") {
                    target_duration = line.trim_start_matches("#EXT-X-TARGETDURATION:")
                        .parse().unwrap_or(0.0);
                } else if line.starts_with("#EXT-X-MEDIA-SEQUENCE:") {
                    sequence = line.trim_start_matches("#EXT-X-MEDIA-SEQUENCE:")
                        .parse().unwrap_or(0);
                } else if line.starts_with("#EXTINF:") {
                    // 格式: #EXTINF:<duration>[,<title>]
                    let parts: Vec<&str> = line.splitn(2, ',').collect();
                    current_duration = parts[0].trim_start_matches("#EXTINF:")
                        .parse().unwrap_or(0.0);
                    current_title = parts.get(1).map(|s| s.to_string());
                } else if line == "#EXT-X-KEY:METHOD=NONE" {
                    current_key_url = None;
                    current_key_iv = None;
                } else if line.starts_with("#EXT-X-KEY:METHOD=AES-128") {
                    current_key_url = Self::extract_attribute(line, "URI");
                    current_key_iv = Self::extract_iv(line);
                } else if line == "#EXT-X-ENDLIST" {
                    is_live = false;
                }
            } else {
                // 非注释行 = URL
                let segment = M3u8Segment {
                    sequence,
                    url: Self::resolve_url(base_url, line),
                    duration: current_duration,
                    title: current_title.clone(),
                    key_url: current_key_url.clone(),
                    key_iv: current_key_iv.clone(),
                };
                segments.push(segment);
                sequence += 1;
            }
        }

        let total_duration: f64 = segments.iter().map(|s| s.duration).sum();

        Ok(M3u8Playlist {
            version,
            target_duration,
            segments,
            total_duration,
            is_live,
            base_url: base_url.to_string(),
        })
    }

    /// 从 URL 加载并解析 m3u8（自动处理 Master Playlist）
    pub async fn fetch_and_parse(client: &reqwest::Client, url: &str) -> Result<M3u8Playlist> {
        let mut current_url = url.to_string();

        // 循环解析直到获得 Media Playlist（非 Master）
        loop {
            let response = client.get(&current_url).send().await?;
            let body = response.text().await?;
            let base_url = Self::extract_base_url(&current_url);
            let playlist = Self::parse(&body, &base_url)?;

            // 如果是 Master Playlist，提取子播放列表 URL 并继续循环
            if let Some(child_url) = Self::extract_master_child_url(&body, &base_url) {
                tracing::info!("检测到 Master Playlist，解析子播放列表: {}", child_url);
                current_url = child_url;
                continue;
            }

            return Ok(playlist);
        }
    }

    /// 从 Master Playlist 提取子播放列表 URL
    fn extract_master_child_url(content: &str, base_url: &str) -> Option<String> {
        // 验证 content 确实是 m3u8（应该包含 #EXTM3U 或 #EXT-X- 标签）
        if !content.contains("#EXTM3U") && !content.contains("#EXT-X-") {
            tracing::warn!("内容不是有效的 m3u8 播放列表（可能返回了 HTML 或其他内容）");
            return None;
        }

        let lines: Vec<&str> = content.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            if line.contains("EXT-X-STREAM-INF") && i + 1 < lines.len() {
                let next_line = lines[i + 1].trim();
                if !next_line.starts_with('#') && !next_line.is_empty() {
                    // 验证看起来像有效的 m3u8 URL
                    if next_line.ends_with(".m3u8") || next_line.contains("playlist") {
                        return Some(Self::resolve_url(base_url, next_line));
                    }
                }
            }
        }
        None
    }

    /// 提取属性值（带引号）
    fn extract_attribute(line: &str, attr: &str) -> Option<String> {
        let pattern = format!("{}=\"", attr);
        line.find(&pattern).and_then(|idx| {
            let start = idx + pattern.len();
            line[start..].find('"').map(|end| line[start..start + end].to_string())
        })
    }

    /// 提取 IV 值 (返回 hex 字符串)
    fn extract_iv(line: &str) -> Option<String> {
        line.find("IV=0x").map(|idx| {
            let hex = &line[idx + 5..];
            hex.chars().take_while(|c| c.is_ascii_hexdigit()).collect()
        })
    }

    /// 解析 base_url（去除文件名，保留到域名）
    fn extract_base_url(url: &str) -> String {
        if let Some(idx) = url.find("://") {
            let scheme_end = idx + 3;
            let after_scheme = &url[scheme_end..];
            // 找到域名后的第一个斜杠位置
            if let Some(slash_pos) = after_scheme.find('/') {
                url[..scheme_end + slash_pos].to_string()
            } else {
                url.to_string()
            }
        } else {
            url.to_string()
        }
    }

    /// 解析相对 URL
    fn resolve_url(base_url: &str, url: &str) -> String {
        if url.starts_with("http://") || url.starts_with("https://") {
            url.to_string()
        } else if url.starts_with('/') {
            // 绝对路径：提取 scheme + host
            if let Some(idx) = base_url.find("://") {
                let scheme_end = idx + 3;
                let host = &base_url[scheme_end..];
                let slash_pos = host.find('/').unwrap_or(host.len());
                format!("{}{}{}", &base_url[..scheme_end], &host[..slash_pos], url)
            } else {
                url.to_string()
            }
        } else {
            // 相对路径：确保 base_url 有斜杠
            if base_url.ends_with('/') {
                format!("{}{}", base_url, url)
            } else {
                format!("{}/{}", base_url, url)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_playlist() {
        let content = r#"#EXTM3U
#EXT-X-VERSION:3
#EXT-X-TARGETDURATION:10
#EXT-X-MEDIA-SEQUENCE:0
#EXTINF:9.9,
segment0.ts
#EXTINF:9.9,
segment1.ts
#EXT-X-ENDLIST
"#;
        let playlist = M3u8Parser::parse(content, "https://example.com/hls/").unwrap();
        assert_eq!(playlist.segments.len(), 2);
        assert_eq!(playlist.total_duration, 19.8);
    }

    #[test]
    fn test_resolve_url() {
        let base = "https://hn.bfvvs.com";
        assert_eq!(
            M3u8Parser::resolve_url(base, "/play/hls/x/index.m3u8"),
            "https://hn.bfvvs.com/play/hls/x/index.m3u8"
        );
    }
}
