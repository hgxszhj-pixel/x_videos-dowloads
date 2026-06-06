//! 弹幕处理模块
//!
//! 支持 Bilibili 弹幕下载和 ASS 转换

#![allow(dead_code)]

use anyhow::{Context, Result};
use std::path::Path;
use tracing::debug;

/// 弹幕格式
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DanmakuFormat {
    /// XML 格式 (Bilibili)
    _Xml,
    /// JSON 格式
    _Json,
    /// ASS 字幕格式
    _Ass,
}

/// 弹幕条目
#[derive(Debug, Clone)]
pub struct DanmakuEntry {
    /// 开始时间（秒）
    pub time: f64,
    /// 弹幕类型 (1=普通, 2=副屏幕, 3=顶部, 4=底部, 5=逆向, 6=高级, 7=BASSA, 8=代码)
    pub type_: u8,
    /// 弹幕颜色 (RGB)
    pub color: u32,
    /// 弹幕发送者 ID 哈希
    pub _uid_hash: String,
    /// 弹幕内容
    pub text: String,
}

/// 弹幕文件
#[derive(Debug, Clone)]
pub struct Danmaku {
    /// 视频 bvid
    pub bvid: String,
    /// 弹幕条目列表
    pub entries: Vec<DanmakuEntry>,
    /// 原始格式
    pub format: DanmakuFormat,
}

/// 下载弹幕 (XML 格式)
pub async fn download_danmaku(client: &reqwest::Client, bvid: &str) -> Result<String> {
    let url = format!("https://api.bilibili.com/x/v1/dm/list?pid={}&type=1", bvid);
    debug!("Downloading danmaku from: {}", url);

    let resp = client
        .get(&url)
        .header("User-Agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)")
        .header("Referer", "https://www.bilibili.com")
        .send()
        .await
        .context("failed to download danmaku")?;

    if !resp.status().is_success() {
        anyhow::bail!("danmaku API returned status: {}", resp.status());
    }

    let xml = resp
        .text()
        .await
        .context("failed to read danmaku response")?;

    Ok(xml)
}

/// 解析 XML 弹幕
pub fn parse_xml_danmaku(xml: &str) -> Result<Vec<DanmakuEntry>> {
    let mut entries = Vec::new();

    // 简单 XML 解析 — 提取 <d> 标签内容
    // 格式: <d p="time,type,color,uid_hash">text</d>
    for line in xml.lines() {
        let line = line.trim();

        // 跳过非弹幕行
        if !line.starts_with("<d ") {
            continue;
        }

        // 提取 p 属性
        if let Some(start) = line.find("p=\"") {
            let start = start + 3;
            if let Some(end) = line[start..].find('"') {
                let attrs = &line[start..start + end];
                let parts: Vec<&str> = attrs.split(',').collect();

                if parts.len() < 4 {
                    continue;
                }

                let time: f64 = parts[0].parse().unwrap_or(0.0);
                let type_: u8 = parts[1].parse().unwrap_or(1);
                let color: u32 = parts[2].parse().unwrap_or(0xFFFFFF);
                let uid_hash = parts[3].to_string();

                // 提取文本内容
                let text = if let Some(t_start) = line.find('>') {
                    let t_start = t_start + 1;
                    if let Some(t_end) = line.find("</d>") {
                        line[t_start..t_end].to_string()
                    } else {
                        continue;
                    }
                } else {
                    continue;
                };

                entries.push(DanmakuEntry {
                    time,
                    type_,
                    color,
                    _uid_hash: uid_hash,
                    text,
                });
            }
        }
    }

    Ok(entries)
}

/// 将弹幕转换为 ASS 格式
#[allow(unused_variables)]
pub fn danmaku_to_ass(danmaku: &[DanmakuEntry], width: u32, height: u32) -> String {
    let mut ass = String::new();

    // ASS 头
    ass.push_str("[Script Info]\n");
    ass.push_str("Title: Bilibili Danmaku\n");
    ass.push_str("ScriptType: v4.00+\n");
    ass.push_str("PlayDepth: 0\n");
    ass.push('\n');

    // V4+ Styles
    ass.push_str("[V4+ Styles]\n");
    ass.push_str("Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\n");

    // 普通弹幕样式 (底部)
    ass.push_str("Style: Normal, sans-serif, 28, &H00FFFFFF, &H00FFFFFF, &H00000000, &H00000000, 0, 0, 0, 0, 100, 100, 0, 0, 1, 2, 0, 2, 20, 20, 2, 1\n");
    // 顶部弹幕样式
    ass.push_str("Style: Top, sans-serif, 28, &H00FFFFFF, &H00FFFFFF, &H00000000, &H00000000, 0, 0, 0, 0, 100, 100, 0, 0, 1, 2, 0, 8, 20, 20, 2, 1\n");
    // 底部弹幕样式
    ass.push_str("Style: Bottom, sans-serif, 28, &H00FFFFFF, &H00FFFFFF, &H00000000, &H00000000, 0, 0, 0, 0, 100, 100, 0, 0, 1, 2, 0, 2, 20, 20, 2, 1\n");
    // 特殊弹幕样式 (副屏幕)
    ass.push_str("Style: Special, sans-serif, 28, &H00FFFFFF, &H00FFFFFF, &H00000000, &H00000000, 0, 0, 0, 0, 100, 100, 0, 0, 1, 2, 0, 5, 20, 20, 2, 1\n");

    ass.push_str("\n[Events]\n");
    ass.push_str("Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n");

    for entry in danmaku {
        let style = match entry.type_ {
            3 => "Top",
            4 => "Bottom",
            2 => "Special",
            _ => "Normal",
        };

        let color = format!("&H{:06X}", entry.color & 0xFFFFFF);
        let start = format_timestamp(entry.time);
        let end = format_timestamp(entry.time + 5.0); // 默认显示 5 秒

        // 过滤特殊字符
        let text = entry.text.replace('\\', "\\\\").replace('{', "\\{").replace('}', "\\}");

        ass.push_str(&format!(
            "Dialogue: 0,{},{},{},,20,20,2,,{{\\c{}}}{}\n",
            start, end, style, color, text
        ));
    }

    ass
}

/// 格式化时间戳为 ASS 格式 (H:MM:SS.cc)
fn format_timestamp(seconds: f64) -> String {
    let hours = (seconds / 3600.0).floor() as u32;
    let minutes = ((seconds % 3600.0) / 60.0).floor() as u32;
    let secs = (seconds % 60.0).floor() as u32;
    let centis = ((seconds % 1.0) * 100.0).floor() as u32;
    format!("{:01}:{:02}:{:02}.{:02}", hours, minutes, secs, centis)
}

/// 保存弹幕到文件
#[allow(dead_code)]
pub async fn save_danmaku(danmaku: &[DanmakuEntry], path: &Path, format: DanmakuFormat, width: u32, height: u32) -> Result<()> {
    let content = match format {
        DanmakuFormat::_Ass => danmaku_to_ass(danmaku, width, height),
        _ => anyhow::bail!("only ASS format supported for saving"),
    };

    tokio::fs::write(path, content)
        .await
        .context("failed to write danmaku file")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_xml_danmaku() {
        let xml = r#"
<弹幕>
<d p="1.5,1,16777215,abc123">测试弹幕</d>
<d p="3.0,3,16711680,def456">顶部弹幕</d>
<d p="5.5,4,255,xyz789">底部弹幕</d>
</弹幕>
"#;
        let entries = parse_xml_danmaku(xml).unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].text, "测试弹幕");
        assert!((entries[0].time - 1.5).abs() < 0.001);
        assert_eq!(entries[1].type_, 3); // 顶部
        assert_eq!(entries[2].type_, 4); // 底部
    }

    #[test]
    fn test_format_timestamp() {
        assert_eq!(format_timestamp(0.0), "0:00:00.00");
        assert_eq!(format_timestamp(1.5), "0:00:01.50");
        assert_eq!(format_timestamp(90.0), "0:01:30.00");
        assert_eq!(format_timestamp(3661.0), "1:01:01.00");
    }

    #[test]
    fn test_danmaku_to_ass() {
        let entries = vec![
                       DanmakuEntry {
                time: 1.0,
                type_: 1,
                color: 0xFFFFFF,
                _uid_hash: "abc".to_string(),
                text: "Hello".to_string(),
            },
            DanmakuEntry {
                time: 3.0,
                type_: 3,
                color: 0xFF0000,
                _uid_hash: "def".to_string(),
                text: "Top".to_string(),
            },
        ];

        let ass = danmaku_to_ass(&entries, 1920, 1080);
        assert!(ass.contains("[Script Info]"));
        assert!(ass.contains("[V4+ Styles]"));
        assert!(ass.contains("[Events]"));
        assert!(ass.contains("Hello"));
        assert!(ass.contains("Top"));
        // 顶部弹幕应该使用 Top style
        assert!(ass.contains("Style: Top"));
    }
}