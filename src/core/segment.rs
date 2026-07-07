//! 分段下载实现
//!
//! 支持并行分段下载、重试机制和暂停功能

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;
use anyhow::{anyhow, Context, Result};
use futures::StreamExt;
use reqwest::Client;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tracing::{info, warn};

use crate::types::DownloadProgress;
use super::constants::*;

/// 下载单个分段（用于并行下载）
#[allow(clippy::too_many_arguments)]
pub async fn download_segment(
    client: &Client,
    url: &str,
    output_path: &PathBuf,
    start: u64,
    end: u64,
    _segment_size: u64,
    _progress_callback: Option<Arc<dyn Fn(DownloadProgress) + Send + Sync>>,
    total_downloaded: Option<Arc<AtomicU64>>,
) -> Result<u64> {
    let response = client
        .get(url)
        .header("Range", format!("bytes={}-{}", start, end))
        .send()
        .await?;

    if !response.status().is_success() && response.status() != reqwest::StatusCode::PARTIAL_CONTENT {
        return Err(anyhow!("分段下载失败: HTTP {}", response.status()))
            .context("segment download request failed");
    }

    let mut file = File::create(output_path).await?;
    let mut stream = response.bytes_stream();
    let mut downloaded: u64 = 0;
    let start_time = std::time::Instant::now();

    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result?;
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;

        let elapsed = start_time.elapsed().as_secs_f64();
        let _speed = if elapsed > 0.0 {
            downloaded as f64 / elapsed
        } else {
            0.0
        };

        // 更新 total_downloaded（供后台任务统一计算进度）
        if let Some(ref td) = total_downloaded {
            td.fetch_add(chunk.len() as u64, Ordering::Relaxed);
        }
    }

    file.flush().await?;
    Ok(downloaded)
}

/// 带暂停标志的下载状态
pub struct PausableDownloadState {
    pub cancel_flag: Arc<AtomicBool>,
}

impl Default for PausableDownloadState {
    fn default() -> Self {
        Self {
            cancel_flag: Arc::new(AtomicBool::new(false)),
        }
    }
}

/// 带重试的分段下载（支持暂停）
#[allow(clippy::too_many_arguments)]
pub async fn download_segment_with_retry(
    client: &Client,
    url: &str,
    output_path: &PathBuf,
    start: u64,
    end: u64,
    expected_size: u64,
    retry_count: u32,
    total_downloaded: Option<Arc<AtomicU64>>,
    progress_callback: Option<Arc<dyn Fn(DownloadProgress) + Send + Sync>>,
    pause_state: Option<&PausableDownloadState>,
) -> Result<u64> {
    let segment_timeout = Duration::from_secs(SEGMENT_TIMEOUT_SECS);
    let segment_size = end - start + 1;

    for attempt in 0..retry_count {
        // 检查暂停标志
        if let Some(state) = pause_state {
            while state.cancel_flag.load(Ordering::Relaxed) {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }

        if attempt > 0 {
            let delay = Duration::from_secs(2u64.pow(attempt - 1).min(30));
            info!("分段 [{}-{}] 重试 {} / {}，等待 {}s", start, end, attempt + 1, retry_count, delay.as_secs());
            tokio::time::sleep(delay).await;
        }

        match tokio::time::timeout(segment_timeout, download_segment(client, url, output_path, start, end, segment_size, progress_callback.clone(), total_downloaded.clone())).await {
            Ok(Ok(size)) => {
                // 验证下载大小是否合理
                if size >= expected_size * SEGMENT_VALIDATION_RATIO / 100 {
                    return Ok(size);
                } else {
                    warn!("分段 [{}-{}] 大小异常: {} (期望 {})", start, end, size, expected_size);
                    if attempt < retry_count - 1 {
                        continue;
                    }
                }
            }
            Ok(Err(e)) => {
                warn!("分段 [{}-{}] 下载错误: {:?}", start, end, e);
            }
            Err(_) => {
                warn!("分段 [{}-{}] 超时 ({}s)", start, end, segment_timeout.as_secs());
            }
        }
    }

    Err(anyhow!("分段 [{}-{}] 下载失败，已重试 {} 次", start, end, retry_count))
        .context("segment download failed after all retry attempts")
}

/// 解析 yt-dlp 进度输出
pub fn parse_ytdlp_progress(line: &str) -> Option<DownloadProgress> {
    use crate::types::DownloadStatus;
    
    // 格式: [download]  45.2% of 100.00MiB at  2.50MiB/s ETA 00:15
    if !line.contains("[download]") || !line.contains('%') {
        return None;
    }

    // 提取百分比
    let percent_str = line.split('%').next()?.split_whitespace().last()?;
    let percent: f64 = percent_str.parse::<f64>().ok()? / 100.0;

    // 提取速度
    let speed = if let Some(speed_part) = line.split("at ").nth(1) {
        let speed_str = speed_part.split('s').next()?.split_whitespace().next()?;
        parse_speed(speed_str).unwrap_or(0.0)
    } else {
        0.0
    };

    // 提取文件大小
    let total_bytes = if let Some(size_part) = line.split("of ").nth(1) {
        let size_str = size_part.split(' ').next()?;
        parse_size(size_str).unwrap_or(0)
    } else {
        0
    };

    let bytes_downloaded = (total_bytes as f64 * percent) as u64;

    Some(DownloadProgress {
        task_id: "ytdlp".to_string(),
        bytes_downloaded,
        total_bytes,
        percent,
        speed,
        eta_secs: None,
        active_threads: 0,
        status: DownloadStatus::Downloading,
    })
}

/// 解析速度字符串 (如 "2.50MiB")
pub fn parse_speed(s: &str) -> Option<f64> {
    // 提取数字部分
    let numeric: String = s.chars().take_while(|c| c.is_numeric() || *c == '.').collect();
    let value: f64 = numeric.parse().ok()?;

    if s.contains("KiB") {
        Some(value * 1024.0)
    } else if s.contains("MiB") {
        Some(value * 1024.0 * 1024.0)
    } else if s.contains("GiB") {
        Some(value * 1024.0 * 1024.0 * 1024.0)
    } else {
        Some(value)
    }
}

/// 解析大小字符串 (如 "100.00MiB")
pub fn parse_size(s: &str) -> Option<u64> {
    // 提取数字部分
    let numeric: String = s.chars().take_while(|c| c.is_numeric() || *c == '.').collect();
    let value: f64 = numeric.parse().ok()?;

    if s.contains("KiB") {
        Some((value * 1024.0) as u64)
    } else if s.contains("MiB") {
        Some((value * 1024.0 * 1024.0) as u64)
    } else if s.contains("GiB") {
        Some((value * 1024.0 * 1024.0 * 1024.0) as u64)
    } else {
        Some(value as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_speed() {
        assert_eq!(parse_speed("2.50MiB"), Some(2.50 * 1024.0 * 1024.0));
        assert_eq!(parse_speed("1.00KiB"), Some(1024.0));
    }

    #[test]
    fn test_parse_size() {
        assert_eq!(parse_size("100.00MiB"), Some((100.0 * 1024.0 * 1024.0) as u64));
    }

    #[test]
    fn test_parse_speed_boundary() {
        assert_eq!(parse_speed("0.00KiB"), Some(0.0));
        assert_eq!(parse_speed("1024.00KiB"), Some(1024.0 * 1024.0));
        assert_eq!(parse_speed("0.00MiB"), Some(0.0));
        assert_eq!(parse_speed("1.00MiB"), Some(1024.0 * 1024.0));
        assert_eq!(parse_speed("1.00GiB"), Some(1024.0 * 1024.0 * 1024.0));
        assert_eq!(parse_speed("123.45"), Some(123.45));
        assert_eq!(parse_speed(""), None);
        assert_eq!(parse_speed("abc"), None);
    }

    #[test]
    fn test_parse_size_boundary() {
        assert_eq!(parse_size("0.00KiB"), Some(0));
        assert_eq!(parse_size("1.00MiB"), Some(1024 * 1024));
        assert_eq!(parse_size("1.00GiB"), Some(1024 * 1024 * 1024));
        assert_eq!(parse_size("123.45"), Some(123));
        assert_eq!(parse_size(""), None);
        assert_eq!(parse_size("abc"), None);
    }
}
