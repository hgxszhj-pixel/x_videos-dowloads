//! Strategy: yt-dlp 下载（含 3 个分支：直接 twimg URL、HLS、yt-dlp 子进程）

use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use tracing::info;

use crate::cli::Cli;
use crate::commands::path;
use crate::config::AppConfig;
use crate::downloader::VideoDownloader;
use crate::types::{
    format_bytes, generate_unique_filename, sanitize_filename, DownloadProgress, DownloadRequest,
    DownloadResult, DownloadStrategy, VideoInfo, DEFAULT_USER_AGENT,
};
use crate::website_profile;
use crate::yt_dlp::YtDlp;

/// 入口：根据 strategy 分发到 3 个分支
pub async fn download(
    ytdlp: &YtDlp,
    info: &VideoInfo,
    cli: &Cli,
    config: &AppConfig,
    _start_time: Instant,
) -> Result<DownloadResult> {
    let output_dir = resolve_output_dir(cli, config);
    tokio::fs::create_dir_all(&output_dir).await?;
    let base_filename = resolve_base_filename(cli, info);

    let is_direct_url = info.url.contains("video.twimg.com");

    // 分支 1：video.twimg.com 直接视频 URL → Rust VideoDownloader
    if is_direct_url && !cli.audio_only && !cli.subtitles {
        return download_direct_twimg_url(info, cli, &output_dir, &base_filename).await;
    }

    // 分支 2：parser 决定走 HLS → 原生 HLS 下载
    if info.download_strategy == DownloadStrategy::Hls {
        return download_hls(info, cli, &output_dir, &base_filename).await;
    }

    // 分支 3：默认 yt-dlp 子进程
    download_ytdlp_subprocess(ytdlp, info, cli, &output_dir, &base_filename).await
}

/// 分支 1：video.twimg.com 直接视频 URL → Rust VideoDownloader
async fn download_direct_twimg_url(
    info: &VideoInfo,
    cli: &Cli,
    output_dir: &std::path::Path,
    base_filename: &str,
) -> Result<DownloadResult> {
    info!("检测到直接视频URL，使用 Rust 下载器: {}", info.title);

    // 从 URL 本身提取扩展名（最可靠，因为 API 返回的 container 字段可能不准确）
    let ext = info
        .url
        .split('?')
        .next()
        .and_then(|s| s.rsplit('/').next()?.split('.').next_back())
        .unwrap_or("mp4");
    let final_output_path = generate_unique_filename(output_dir, base_filename, ext);

    let downloader = VideoDownloader::new()?.with_website_profile(&info.url);
    let request = DownloadRequest::new(&info.url, &final_output_path)
        .with_user_agent(
            cli.user_agent
                .clone()
                .unwrap_or_else(|| DEFAULT_USER_AGENT.to_string()),
        )
        .with_strategy(info.download_strategy);
    info!("DEBUG: download_strategy = {:?}", info.download_strategy);

    let progress_callback: Arc<dyn Fn(DownloadProgress) + Send + Sync> =
        Arc::new(|progress: DownloadProgress| {
            print!(
                "\r下载进度: {:.1}% | 速度: {} | 已下载: {}",
                progress.percent * 100.0,
                format_bytes(progress.speed as u64),
                format_bytes(progress.bytes_downloaded)
            );
        });

    let result = downloader
        .download(&request, Some(progress_callback))
        .await?;
    println!();

    // 记录下载结果（用于网站特征学习）
    let speed_bps = if result.duration_secs > 0.0 {
        result.file_size as f64 / result.duration_secs as f64
    } else {
        0.0
    };
    website_profile::record_download_result(&info.url, result.success, speed_bps as u64);

    if result.success {
        println!("下载完成: {} ({} bytes)", info.title, result.file_size);
    } else {
        println!(
            "下载失败: {}",
            result.error_message.as_deref().unwrap_or("未知错误")
        );
    }

    Ok(result)
}

/// 分支 2：HLS 策略 → 原生 HLS 下载（通过 VideoDownloader）
async fn download_hls(
    info: &VideoInfo,
    cli: &Cli,
    output_dir: &std::path::Path,
    base_filename: &str,
) -> Result<DownloadResult> {
    info!("使用原生 HLS 下载: {}", info.title);
    let hls_output = output_dir.join(format!("{}.mp4", base_filename));
    let downloader = VideoDownloader::new()?.with_website_profile(&info.url);
    let request = DownloadRequest::new(&info.url, &hls_output)
        .with_user_agent(
            cli.user_agent
                .clone()
                .unwrap_or_else(|| DEFAULT_USER_AGENT.to_string()),
        )
        .with_strategy(DownloadStrategy::Hls);
    downloader.download(&request, None).await
}

/// 分支 3：默认 yt-dlp 子进程
async fn download_ytdlp_subprocess(
    ytdlp: &YtDlp,
    info: &VideoInfo,
    cli: &Cli,
    output_dir: &std::path::Path,
    base_filename: &str,
) -> Result<DownloadResult> {
    let mut yt = ytdlp.clone();

    if let Some(ref ua) = cli.user_agent {
        yt = yt.with_user_agent(ua.clone());
    }
    if let Some(ref cookie) = cli.cookies {
        yt = yt.with_cookie_file(cookie.clone());
    }
    if let Some(ref proxy) = cli.proxy {
        yt = yt.with_proxy(proxy.clone());
    }

    // 确定格式
    let format = cli.format.clone().or_else(|| {
        if cli.audio_only {
            Some("bestaudio".to_string())
        } else {
            None
        }
    });

    // 构建 yt-dlp 命令
    let output_path = output_dir.join(format!("{}.%(ext)s", base_filename));

    info!("开始下载: {}", info.title);

    // 使用统一的参数构建函数
    let args = yt.build_download_args(
        &info.url,
        &output_path.to_string_lossy(),
        format.as_deref(),
        cli.audio_only,
        cli.subtitles,
    );

    // 执行命令
    println!("执行: yt-dlp {}", args.join(" "));

    use tokio::process::Command;

    let output = Command::new(&yt.executable).args(&args).output().await?;
    println!(); // 换行

    let success = output.status.success();
    let final_path = std::path::PathBuf::from(output_path.to_string_lossy().to_string());

    if success {
        println!("下载完成: {}", info.title);
        Ok(DownloadResult::success(final_path, 0, 0.0))
    } else {
        let err = String::from_utf8_lossy(&output.stderr).to_string();
        let msg = if err.trim().is_empty() {
            format!("yt-dlp 退出码: {:?}", output.status.code())
        } else {
            err
        };
        println!("下载失败: {} ({})", info.title, msg);
        Ok(DownloadResult::failure(final_path, msg))
    }
}

/// 解析输出目录（CLI 参数 > 默认值）
fn resolve_output_dir(cli: &Cli, config: &AppConfig) -> std::path::PathBuf {
    let raw = if cli.output.as_os_str() == "." {
        config.output_dir.clone()
    } else {
        cli.output.clone()
    };
    path::validate_output_dir(&raw).expect("output dir validation")
}

/// 解析基础文件名（无扩展名）
fn resolve_base_filename(cli: &Cli, info: &VideoInfo) -> String {
    if let Some(ref name) = cli.filename {
        sanitize_filename(name)
    } else {
        sanitize_filename(&info.title)
    }
}