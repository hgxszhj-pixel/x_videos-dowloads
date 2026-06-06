//! 字幕处理模块
//!
//! 支持字幕下载和格式转换

use anyhow::{Context, Result};
use std::path::Path;
use tracing::debug;

/// 字幕信息
#[derive(Debug, Clone)]
pub struct Subtitle {
    /// 语言代码
    pub lang: String,
    /// 语言名称
    pub lang_name: String,
    /// 字幕内容
    pub content: String,
    /// 字幕格式
    pub format: SubtitleFormat,
}

/// 字幕格式
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SubtitleFormat {
    /// SRT 字幕
    Srt,
    /// VTT WebVTT 字幕
    Vtt,
    /// ASS/SSA 字幕
    Ass,
    /// 原始格式
    Raw,
}

impl SubtitleFormat {
    /// 从文件扩展名获取格式
    pub fn from_ext(ext: &str) -> Option<Self> {
        match ext.to_lowercase().as_str() {
            "srt" => Some(Self::Srt),
            "vtt" => Some(Self::Vtt),
            "ass" | "ssa" => Some(Self::Ass),
            _ => None,
        }
    }

    /// 获取文件扩展名
    pub fn extension(&self) -> &'static str {
        match self {
            Self::Srt => "srt",
            Self::Vtt => "vtt",
            Self::Ass => "ass",
            Self::Raw => "raw",
        }
    }
}

/// 字幕解析器 trait
pub trait SubtitleParser: Send + Sync {
    /// 解析字幕内容
    fn parse(&self, content: &str) -> Result<Vec<SubtitleEntry>>;
}

/// 字幕条目
#[derive(Debug, Clone)]
pub struct SubtitleEntry {
    /// 开始时间（秒）
    pub start: f64,
    /// 结束时间（秒）
    pub end: f64,
    /// 字幕文本
    pub text: String,
}

/// 将 VTT 转换为 SRT
pub fn vtt_to_srt(vtt_content: &str) -> String {
    let mut srt_output = String::new();
    let mut subtitle_index = 1;

    // 简单实现：跳过 WEBVTT 头，处理 timestamp 行
    for line in vtt_content.lines() {
        let line = line.trim();

        // 跳过空行和 WEBVTT 头
        if line.is_empty() || line == "WEBVTT" || line.starts_with("NOTE") || line.starts_with("STYLE") {
            continue;
        }

        // 检测时间戳行 (00:00:00.000 --> 00:00:01.000)
        if line.contains("-->") {
            let times = line.split("-->").collect::<Vec<_>>();
            if times.len() == 2 {
                let start = parse_vtt_time(times[0].trim());
                let end = parse_vtt_time(times[1].trim().split_whitespace().next().unwrap_or(""));

                srt_output.push_str(&format!("{}\n", subtitle_index));
                srt_output.push_str(&format!("{}\n", format_srt_time(start, end)));
                subtitle_index += 1;
            }
        } else if !line.starts_with('<') {
            // 文本行（跳过 HTML 标签）
            srt_output.push_str(line);
            srt_output.push('\n');
        }
    }

    srt_output
}

/// 解析 VTT 时间格式
fn parse_vtt_time(time: &str) -> f64 {
    let parts: Vec<&str> = time.split(':').collect();
    if parts.len() == 3 {
        let hours: f64 = parts[0].parse().unwrap_or(0.0);
        let minutes: f64 = parts[1].parse().unwrap_or(0.0);
        let seconds: f64 = parts[2].parse().unwrap_or(0.0);
        hours * 3600.0 + minutes * 60.0 + seconds
    } else if parts.len() == 2 {
        let minutes: f64 = parts[0].parse().unwrap_or(0.0);
        let seconds: f64 = parts[1].parse().unwrap_or(0.0);
        minutes * 60.0 + seconds
    } else {
        0.0
    }
}

/// 格式化 SRT 时间
fn format_srt_time(start: f64, end: f64) -> String {
    format!("{} --> {}",
        format_srt_timestamp(start),
        format_srt_timestamp(end)
    )
}

/// 格式化 SRT 时间戳
fn format_srt_timestamp(seconds: f64) -> String {
    let hours = (seconds / 3600.0).floor() as u32;
    let minutes = ((seconds % 3600.0) / 60.0).floor() as u32;
    let secs = (seconds % 60.0).floor() as u32;
    let millis = ((seconds % 1.0) * 1000.0).floor() as u32;
    format!("{:02}:{:02}:{:02},{:03}", hours, minutes, secs, millis)
}

/// 下载字幕文件
#[allow(dead_code)]
pub async fn download_subtitle(client: &reqwest::Client, url: &str) -> Result<Vec<u8>> {
    Ok(client
        .get(url)
        .send()
        .await
        .context("failed to download subtitle")?
        .bytes()
        .await
        .context("failed to read subtitle bytes")?
        .to_vec())
}

/// 保存字幕到文件
#[allow(dead_code)]
pub async fn save_subtitle(content: &str, path: &Path) -> Result<()> {
    tokio::fs::write(path, content)
        .await
        .context("failed to write subtitle file")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subtitle_format_from_ext() {
        assert_eq!(SubtitleFormat::from_ext("srt"), Some(SubtitleFormat::Srt));
        assert_eq!(SubtitleFormat::from_ext("vtt"), Some(SubtitleFormat::Vtt));
        assert_eq!(SubtitleFormat::from_ext("ass"), Some(SubtitleFormat::Ass));
        assert_eq!(SubtitleFormat::from_ext("ASS"), Some(SubtitleFormat::Ass));
        assert_eq!(SubtitleFormat::from_ext("unknown"), None);
    }

    #[test]
    fn test_parse_vtt_time() {
        assert!((parse_vtt_time("00:00:01.000") - 1.0).abs() < 0.001);
        assert!((parse_vtt_time("00:01:30.500") - 90.5).abs() < 0.001);
        assert!((parse_vtt_time("00:00:00.000") - 0.0).abs() < 0.001);
    }
}