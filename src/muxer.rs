//! 音视频混流模块
//!
//! 使用 FFmpeg 将视频和音频流合并

#![allow(dead_code)]

use anyhow::{Context, Result};
use std::path::Path;
use tokio::process::Command;
use tracing::{debug, info};

/// 混流配置
#[derive(Debug, Clone)]
pub struct MuxerConfig {
    /// FFmpeg 路径
    pub ffmpeg_path: String,
    /// FFprobe 路径
    pub ffprobe_path: String,
    /// 是否保留中间文件
    pub keep_temp_files: bool,
    /// 默认比特率
    pub default_bitrate: Option<String>,
}

impl Default for MuxerConfig {
    fn default() -> Self {
        Self {
            ffmpeg_path: "ffmpeg".to_string(),
            ffprobe_path: "ffprobe".to_string(),
            keep_temp_files: false,
            default_bitrate: None,
        }
    }
}

/// 媒体流信息
#[derive(Debug, Clone)]
pub struct MediaStream {
    /// 流类型 (video/audio/subtitle)
    pub stream_type: String,
    /// 编解码器
    pub codec: String,
    /// 比特率
    pub bitrate: Option<String>,
    /// 分辨率 (视频)
    pub resolution: Option<String>,
    /// 声道数 (音频)
    pub channels: Option<u32>,
}

/// 媒体文件信息
#[derive(Debug, Clone)]
pub struct MediaInfo {
    /// 时长（秒）
    pub duration: f64,
    /// 流列表
    pub streams: Vec<MediaStream>,
}

/// 检查 FFmpeg 是否可用
pub async fn check_ffmpeg(path: &str) -> Result<String> {
    let output = Command::new(path)
        .arg("-version")
        .output()
        .await
        .context("failed to run ffmpeg")?;

    if !output.status.success() {
        anyhow::bail!("ffmpeg not found at {}", path);
    }

    // 解析版本
    let version = String::from_utf8_lossy(&output.stdout);
    let version_line = version.lines().next().unwrap_or("unknown");
    Ok(version_line.to_string())
}

/// 获取媒体文件信息
pub async fn get_media_info(path: &Path, config: &MuxerConfig) -> Result<MediaInfo> {
    let output = Command::new(&config.ffprobe_path)
        .args([
            "-v",
            "quiet",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
            path.to_str().unwrap_or(""),
        ])
        .output()
        .await
        .context("failed to run ffprobe")?;

    if !output.status.success() {
        anyhow::bail!("ffprobe failed for {:?}", path);
    }

    let json_str = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&json_str).context("failed to parse ffprobe output")?;

    // 解析 duration
    let duration = json["format"]["duration"]
        .as_str()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0);

    // 解析 streams
    let mut streams = Vec::new();
    if let Some(arr) = json["streams"].as_array() {
        for stream in arr {
            let stream_type = stream["codec_type"]
                .as_str()
                .unwrap_or("unknown")
                .to_string();
            let codec = stream["codec_name"]
                .as_str()
                .unwrap_or("unknown")
                .to_string();
            let bitrate = stream["bit_rate"].as_str().map(|s| s.to_string());
            let resolution = if stream_type == "video" {
                Some(format!(
                    "{}x{}",
                    stream["width"].as_i64().unwrap_or(0),
                    stream["height"].as_i64().unwrap_or(0)
                ))
            } else {
                None
            };
            let channels = if stream_type == "audio" {
                stream["channels"].as_i64().map(|c| c as u32)
            } else {
                None
            };

            streams.push(MediaStream {
                stream_type,
                codec,
                bitrate,
                resolution,
                channels,
            });
        }
    }

    Ok(MediaInfo { duration, streams })
}

/// 混流：将视频和音频合并
pub async fn mux(
    video_path: &Path,
    audio_path: &Path,
    output_path: &Path,
    config: &MuxerConfig,
) -> Result<()> {
    info!(
        "Starting mux: {:?} + {:?} -> {:?}",
        video_path, audio_path, output_path
    );

    let mut cmd = Command::new(&config.ffmpeg_path);
    cmd.args([
        "-y", // 覆盖输出文件
        "-i", // 输入文件
        video_path.to_str().unwrap_or(""),
        "-i", // 第二个输入
        audio_path.to_str().unwrap_or(""),
        "-c:v",
        "copy", // 复制视频流
        "-c:a",
        "aac", // 编码音频为 AAC
        "-strict",
        "-2", // 允许实验性格式
    ])
    .arg(output_path.to_str().unwrap_or(""));

    debug!("Running: {:?}", cmd);

    let output = cmd.output().await.context("failed to run ffmpeg")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("ffmpeg failed: {}", stderr);
    }

    info!("Mux completed: {:?}", output_path);
    Ok(())
}

/// 混流并指定质量
#[allow(clippy::too_many_arguments)]
pub async fn mux_with_quality(
    video_path: &Path,
    audio_path: &Path,
    output_path: &Path,
    video_codec: &str,
    audio_codec: &str,
    crf: Option<u32>,
    bitrate: Option<&str>,
    config: &MuxerConfig,
) -> Result<()> {
    info!(
        "Starting quality mux: {:?} + {:?} -> {:?}",
        video_path, audio_path, output_path
    );

    let mut cmd_args = vec![
        "-y".to_string(),
        "-i".to_string(),
        video_path.to_string_lossy().into_owned(),
        "-i".to_string(),
        audio_path.to_string_lossy().into_owned(),
        "-c:v".to_string(),
        video_codec.to_string(),
        "-c:a".to_string(),
        audio_codec.to_string(),
    ];

    if let Some(crf_val) = crf {
        cmd_args.push("-crf".to_string());
        cmd_args.push(crf_val.to_string());
    }

    if let Some(br) = bitrate {
        cmd_args.push("-b:a".to_string());
        cmd_args.push(br.to_string());
    }

    cmd_args.push(output_path.to_string_lossy().into_owned());

    let output = Command::new(&config.ffmpeg_path)
        .args(&cmd_args)
        .output()
        .await
        .context("failed to run ffmpeg")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("ffmpeg failed: {}", stderr);
    }

    Ok(())
}

/// 下载并混流 (一行命令完成)
pub async fn download_and_mux(
    video_url: &str,
    audio_url: &str,
    output_path: &Path,
    config: &MuxerConfig,
) -> Result<()> {
    info!("Downloading and muxing from URLs...");

    let mut cmd = Command::new(&config.ffmpeg_path);
    cmd.args([
        "-y", "-i", video_url, "-i", audio_url, "-c:v", "copy", "-c:a", "aac", "-strict", "-2",
    ])
    .arg(output_path.to_str().unwrap_or(""));

    let output = cmd.output().await.context("failed to run ffmpeg")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("ffmpeg failed: {}", stderr);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_check_ffmpeg() {
        // 假设系统有 ffmpeg
        match check_ffmpeg("ffmpeg").await {
            Ok(version) => info!("FFmpeg version: {}", version),
            Err(e) => info!("FFmpeg not available: {}", e),
        }
    }
}
