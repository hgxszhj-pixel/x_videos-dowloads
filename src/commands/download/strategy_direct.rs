//! Strategy: 直接 reqwest 下载（含 m3u8 自动检测）

use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use tracing::info;

use crate::cli::Cli;
use crate::commands::path;
use crate::downloader::{DownloaderConfig, VideoDownloader};
use crate::hls::HlsDownloader;
use crate::http_client;
use crate::types::{
    format_bytes, generate_unique_filename, sanitize_filename, DownloadProgress, DownloadRequest,
    DownloadResult, DEFAULT_USER_AGENT,
};

/// 直接下载（使用 reqwest 或 HLS）
///
/// 自动检测 `.m3u8` URL 走 HLS 下载器；否则走 `VideoDownloader`（reqwest + 分段）。
pub async fn download(url: &str, cli: &Cli, _start_time: Instant) -> Result<DownloadResult> {
    println!("直接下载模式: {}", url);

    // 确定输出路径
    let output_dir = if cli.output.as_os_str() == "." {
        path::get_default_output_dir()
    } else {
        cli.output.clone()
    };

    // 验证并规范化输出目录为绝对路径
    let output_dir = path::validate_output_dir(&output_dir)?;

    tokio::fs::create_dir_all(&output_dir).await?;

    // 提取文件名
    let base_filename = if let Some(ref name) = cli.filename {
        sanitize_filename(name)
    } else {
        // 从 URL 路径提取有意义的名称
        path::extract_filename_from_url(url).unwrap_or_else(|| "video".to_string())
    };

    // 检测 m3u8 URL，自动走 HLS 下载器
    if url.contains(".m3u8") {
        return download_hls(url, cli, &output_dir, &base_filename).await;
    }

    let config = DownloaderConfig {
        user_agent: cli.user_agent.clone(),
        proxy: cli.proxy.clone(),
        ..Default::default()
    };

    let downloader = VideoDownloader::with_config(config)?;

    let output_path = generate_unique_filename(&output_dir, &base_filename, "mp4");

    let request = DownloadRequest::new(url, &output_path)
        .with_user_agent(cli.user_agent.clone().unwrap_or_else(|| DEFAULT_USER_AGENT.to_string()));

    // 进度回调
    let progress_callback: Arc<dyn Fn(DownloadProgress) + Send + Sync> = Arc::new(|progress: DownloadProgress| {
        print!(
            "\r下载进度: {:.1}% | 速度: {} | 已下载: {}",
            progress.percent * 100.0,
            format_bytes(progress.speed as u64),
            format_bytes(progress.bytes_downloaded)
        );
    });

    let result = downloader.download(&request, Some(progress_callback)).await?;

    if result.success {
        println!("\n下载完成: {}", result.output_path.display());
    } else {
        println!(
            "\n下载失败: {}",
            result.error_message.as_deref().unwrap_or("未知错误")
        );
    }

    Ok(result)
}

/// HLS 下载（m3u8 URL 专用路径）
async fn download_hls(
    url: &str,
    cli: &Cli,
    output_dir: &std::path::Path,
    base_filename: &str,
) -> Result<DownloadResult> {
    println!("检测到 m3u8 URL，使用原生 HLS 下载器");
    let client = (*http_client::shared()).clone();
    let mut hls_downloader = HlsDownloader::new(client)
        .max_concurrent(4)
        .retry_count(3);
    if let Some(ref referer) = cli.referer {
        hls_downloader = hls_downloader.referer(referer);
        println!("使用自定义 Referer: {}", referer);
    }
    let output_path = generate_unique_filename(output_dir, base_filename, "mp4");
    match hls_downloader.download(url, &output_path, None).await {
        Ok(size) => {
            info!(
                "HLS 下载完成: {} ({} bytes)",
                output_path.display(),
                size
            );
            println!("\n下载完成: {} ({} bytes)", output_path.display(), size);
            Ok(DownloadResult {
                success: true,
                output_path,
                file_size: size,
                duration_secs: 0.0,
                error_message: None,
            })
        }
        Err(e) => {
            info!("HLS 下载失败: {}", e);
            println!("\nHLS 下载失败: {}", e);
            Ok(DownloadResult {
                success: false,
                output_path,
                file_size: 0,
                duration_secs: 0.0,
                error_message: Some(e.to_string()),
            })
        }
    }
}