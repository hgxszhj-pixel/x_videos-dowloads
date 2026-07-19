//! download 子命令：视频信息解析与下载执行

use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use tracing::info;

use crate::cli::Cli;
use crate::config::AppConfig;
use super::path;
use crate::downloader::{DownloaderConfig, VideoDownloader};
use crate::hls::HlsDownloader;
use crate::history::{DownloadStatus, History};
use crate::resolver_chain::get_video_info;
use crate::types::{
    self, format_bytes, generate_unique_filename, sanitize_filename, DownloadRequest,
    DownloadResult, DownloadStrategy, VideoInfo, DEFAULT_USER_AGENT,
};
use crate::website_profile;
use crate::yt_dlp::YtDlp;

pub async fn run(cli: Cli, config: AppConfig) -> Result<()> {
    let start_time = Instant::now();

    // 初始化历史管理器
    let mut history = History::new(&config.output_dir)?;

    // 检查 yt-dlp 是否可用
    let ytdlp = YtDlp::new();

    if !ytdlp.is_available() {
        anyhow::bail!("错误: 未找到 yt-dlp。请安装 yt-dlp: https://github.com/yt-dlp/yt-dlp#installation");
    }

    info!("X Video Downloader v{}", env!("CARGO_PKG_VERSION"));
    info!(
        "yt-dlp 版本: {}",
        ytdlp.version().unwrap_or_else(|| "未知".to_string())
    );

    // 处理 URL
    let urls = if let Some(ref url) = cli.url {
        vec![url.clone()]
    } else if let Some(batch_file) = &cli.batch {
        // 从文件读取 URL 列表
        let content = tokio::fs::read_to_string(&batch_file).await?;
        content
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty() && !s.starts_with('#'))
            .collect()
    } else {
        anyhow::bail!("请提供视频 URL 或使用 -b/--batch 指定批量文件");
    };

    info!("待下载视频数: {}", urls.len());

    // 下载每个视频
    for (index, url) in urls.iter().enumerate() {
        if urls.len() > 1 {
            println!("\n[{}/{}] 处理: {}", index + 1, urls.len(), url);
        }

        // 获取视频信息
        let video_info = if cli.direct {
            // 直接下载模式
            None
        } else {
            Some(get_video_info(&ytdlp, url, &cli, &config).await?)
        };

        // 显示格式列表
        if cli.list_formats {
            if let Some(ref info) = video_info {
                print_video_formats(info);
            }
            continue;
        }

        // 下载视频
        if cli.direct {
            // 直接下载（使用 reqwest）
            let result = download_direct(url, &cli, start_time).await?;
            // 记录到历史
            if result.success {
                history.add_entry_with_all(
                    url.clone(),
                    None,
                    Some(result.output_path.clone()),
                    Some(result.file_size),
                    None,
                    DownloadStatus::Completed,
                    None,
                )?;
            }
        } else if let Some(ref info) = video_info {
            // 根据解析器决定的下载策略选择下载方式
            if info.download_strategy == DownloadStrategy::Native {
                info!("使用原生下载...");
                let result = download_with_native(info, &cli, start_time).await?;
                if result.success {
                    history.add_entry_with_all(
                        url.clone(),
                        Some(info.title.clone()),
                        Some(result.output_path.clone()),
                        Some(result.file_size),
                        None,
                        DownloadStatus::Completed,
                        None,
                    )?;
                }
            } else {
                // 使用 yt-dlp 下载
                let result = download_with_ytdlp(&ytdlp, info, &cli, &config, start_time).await?;
                // 记录到历史（duration 转换为 f64）
                let duration_secs = info.duration.map(|d| d as f64);
                let status = decide_ytdlp_history_status(result.success);
                history.add_entry_with_all(
                    url.clone(),
                    Some(info.title.clone()),
                    None,
                    None,
                    duration_secs,
                    status,
                    result.error_message,
                )?;
            }
        }
    }

    let elapsed = start_time.elapsed().as_secs_f64();
    println!("\n完成! 总耗时: {:.2}s", elapsed);

    Ok(())
}

/// 打印可用格式
fn print_video_formats(info: &VideoInfo) {
    println!("\n视频: {}", info.title);
    println!("时长: {}s", info.duration.unwrap_or(0));
    println!("\n可用格式:");
    println!(
        "{:<10} {:<8} {:<15} {:<12}",
        "ID", "扩展名", "分辨率", "大小"
    );
    println!("{}", "-".repeat(50));

    for fmt in &info.formats {
        let size = fmt
            .filesize
            .map(format_bytes)
            .unwrap_or_else(|| "N/A".to_string());

        let resolution = fmt
            .resolution
            .clone()
            .unwrap_or_else(|| "N/A".to_string());

        println!(
            "{:<10} {:<8} {:<15} {:<12}",
            fmt.format_id, fmt.ext, resolution, size
        );
    }
}

/// 使用原生下载（wget）用于智学云等平台
async fn download_with_native(info: &VideoInfo, cli: &Cli, _start_time: Instant) -> Result<DownloadResult> {
    // 确定输出路径
    let output_dir = if cli.output.as_os_str() == "." {
        path::get_default_output_dir()
    } else {
        cli.output.clone()
    };
    let output_dir = path::validate_output_dir(&output_dir)?;
    tokio::fs::create_dir_all(&output_dir).await?;

    // 确定文件名
    let filename = if let Some(ref name) = cli.filename {
        sanitize_filename(name)
    } else {
        sanitize_filename(&info.title)
    };

    let output_path = output_dir.join(format!("{}.mp4", filename));

    // 获取 token
    let token = std::env::var("ZHIXUEYUN_TOKEN")
        .or_else(|_| {
            Ok::<_, ()>(
                crate::platform::zhixueyun_token::load_zhixueyun_cookies().unwrap_or_default(),
            )
        })
        .unwrap_or_default();

    // P0-2 修复：通过 reqwest header 传递 token，不再 shell out 到 wget
    // （之前 wget --header 会把 token 写入 argv，`ps aux` 可见）
    let url = &info.url;
    let user_agent = cli
        .user_agent
        .clone()
        .unwrap_or_else(|| DEFAULT_USER_AGENT.to_string());
    let auth_header = format_zhixueyun_auth_header(&token);

    let client = (*crate::http_client::shared()).clone();
    let response = client
        .get(url)
        .header(reqwest::header::USER_AGENT, user_agent)
        .header(reqwest::header::AUTHORIZATION, auth_header)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let err = format!("HTTP {}", status);
        println!("\n下载失败: {}", err);
        return Ok(DownloadResult::failure(output_path, err));
    }

    let bytes = response.bytes().await?;
    tokio::fs::write(&output_path, &bytes).await?;
    let file_size = bytes.len() as u64;

    println!("\n下载完成: {} ({} bytes)", output_path.display(), file_size);
    Ok(DownloadResult::success(output_path, file_size, 0.0))
}

/// 使用 yt-dlp 下载
///
/// 返回 [`DownloadResult`] 而非 `()`，让调用方知道子进程是否真正成功
/// （P0-1 修复：之前 `Result<()>` 永远返回 `Ok`，导致 history 全部被标 Completed）。
async fn download_with_ytdlp(
    ytdlp: &YtDlp,
    info: &VideoInfo,
    cli: &Cli,
    config: &AppConfig,
    _start_time: Instant,
) -> Result<DownloadResult> {
    // 确定输出路径（CLI参数 > 配置文件 > 默认值）
    let output_dir = if cli.output.as_os_str() == "." {
        config.output_dir.clone()
    } else {
        cli.output.clone()
    };

    // 验证并规范化输出目录为绝对路径
    let output_dir = path::validate_output_dir(&output_dir)?;

    // 确保输出目录存在
    tokio::fs::create_dir_all(&output_dir).await?;

    // 确定文件名（无扩展名，作为基础文件名）
    let base_filename = if let Some(ref name) = cli.filename {
        sanitize_filename(name)
    } else {
        sanitize_filename(&info.title)
    };

    // 确定格式
    // 检测是否为直接视频URL（video.twimg.com）
    let is_direct_url = info.url.contains("video.twimg.com");
    let format = cli.format.clone().or_else(|| {
        if cli.audio_only {
            Some("bestaudio".to_string())
        } else if is_direct_url {
            // 直接视频URL不支持合并格式，使用best
            Some("best".to_string())
        } else {
            // 设为 None，让 build_download_args 使用智能默认格式选择
            None
        }
    });

    // 构建 yt-dlp 命令
    let output_path = output_dir.join(format!("{}.%(ext)s", base_filename));

    // 对于 video.twimg.com 的直接 URL，使用 Rust 下载器更可靠
    if is_direct_url && !cli.audio_only && !cli.subtitles {
        info!("检测到直接视频URL，使用 Rust 下载器: {}", info.title);

        // 从 URL 本身提取扩展名（最可靠，因为 API 返回的 container 字段可能不准确）
        let ext = info
            .url
            .split('?')
            .next()
            .and_then(|s| s.rsplit('/').next()?.split('.').next_back())
            .unwrap_or("mp4");
        let final_output_path = generate_unique_filename(&output_dir, &base_filename, ext);

        // 使用 Rust VideoDownloader 直接下载（应用网站特征配置）
        let downloader = VideoDownloader::new()?.with_website_profile(&info.url);
        let request = DownloadRequest::new(&info.url, &final_output_path)
            .with_user_agent(cli.user_agent.clone().unwrap_or_else(|| DEFAULT_USER_AGENT.to_string()))
            .with_strategy(info.download_strategy); // DEBUG
        info!("DEBUG: download_strategy = {:?}", info.download_strategy);

        let progress_callback: Arc<dyn Fn(types::DownloadProgress) + Send + Sync> = Arc::new(|progress: types::DownloadProgress| {
            print!(
                "\r下载进度: {:.1}% | 速度: {} | 已下载: {}",
                progress.percent * 100.0,
                format_bytes(progress.speed as u64),
                format_bytes(progress.bytes_downloaded)
            );
        });

        let result = downloader.download(&request, Some(progress_callback)).await?;
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
            println!("下载失败: {}", result.error_message.as_deref().unwrap_or("未知错误"));
        }

        return Ok(result);
    }

    // 使用 yt-dlp 下载
    let mut yt = ytdlp.clone();

    // 应用配置
    if let Some(ref ua) = cli.user_agent {
        yt = yt.with_user_agent(ua.clone());
    }

    if let Some(ref cookie) = cli.cookies {
        yt = yt.with_cookie_file(cookie.clone());
    }

    if let Some(ref proxy) = cli.proxy {
        yt = yt.with_proxy(proxy.clone());
    }

    // 检查是否使用原生 HLS 下载
    if info.download_strategy == DownloadStrategy::Hls {
        info!("使用原生 HLS 下载: {}", info.title);
        let hls_output = output_dir.join(format!("{}.mp4", base_filename));
        let downloader = VideoDownloader::new()?.with_website_profile(&info.url);
        let request = DownloadRequest::new(&info.url, &hls_output)
            .with_user_agent(cli.user_agent.clone().unwrap_or_else(|| DEFAULT_USER_AGENT.to_string()))
            .with_strategy(DownloadStrategy::Hls);
        return downloader.download(&request, None).await;
    }

    // 执行下载
    info!("开始下载: {}", info.title);

    // 使用统一的参数构建函数
    let args = yt.build_download_args(
        &info.url,
        &output_path.to_string_lossy(),
        format.as_deref(),
        cli.audio_only,
        cli.subtitles,
    );

    // 执行命令（带实时进度）
    println!("执行: yt-dlp {}", args.join(" "));

    use tokio::process::Command;

    // 使用 wait_with_output 避免管道死锁，同时捕获输出
    let output = Command::new(&yt.executable).args(&args).output().await?;

    println!(); // 换行

    let success = output.status.success();
    let output_path = std::path::PathBuf::from(output_path.to_string_lossy().to_string());

    if success {
        println!("下载完成: {}", info.title);
        Ok(DownloadResult::success(output_path, 0, 0.0))
    } else {
        let err = String::from_utf8_lossy(&output.stderr).to_string();
        let msg = if err.trim().is_empty() {
            format!("yt-dlp 退出码: {:?}", output.status.code())
        } else {
            err
        };
        println!("下载失败: {} ({})", info.title, msg);
        Ok(DownloadResult::failure(output_path, msg))
    }
}

/// 直接下载（使用 reqwest 或 HLS）
pub async fn download_direct(url: &str, cli: &Cli, _start_time: Instant) -> Result<DownloadResult> {
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
        println!("检测到 m3u8 URL，使用原生 HLS 下载器");
        let client = (*crate::http_client::shared()).clone();
        let mut hls_downloader = HlsDownloader::new(client)
            .max_concurrent(4)
            .retry_count(3);
        if let Some(ref referer) = cli.referer {
            hls_downloader = hls_downloader.referer(referer);
            println!("使用自定义 Referer: {}", referer);
        }
        let output_path = generate_unique_filename(&output_dir, &base_filename, "mp4");
        match hls_downloader.download(url, &output_path, None).await {
            Ok(size) => {
                println!("\n下载完成: {} ({} bytes)", output_path.display(), size);
                return Ok(DownloadResult {
                    success: true,
                    output_path,
                    file_size: size,
                    duration_secs: 0.0,
                    error_message: None,
                });
            }
            Err(e) => {
                println!("\nHLS 下载失败: {}", e);
                return Ok(DownloadResult {
                    success: false,
                    output_path,
                    file_size: 0,
                    duration_secs: 0.0,
                    error_message: Some(e.to_string()),
                });
            }
        }
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
    let progress_callback: Arc<dyn Fn(types::DownloadProgress) + Send + Sync> = Arc::new(|progress: types::DownloadProgress| {
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

/// 根据 yt-dlp 子进程的退出结果决定历史记录状态
///
/// P0-1 修复契约：失败下载不能被记录为 Completed。
/// - `success == true`  → Completed
/// - `success == false` → Failed
pub(crate) fn decide_ytdlp_history_status(success: bool) -> DownloadStatus {
    if success {
        DownloadStatus::Completed
    } else {
        DownloadStatus::Failed
    }
}

/// 格式化智学云平台的 Authorization 请求头
///
/// P0-2 修复契约：token 必须通过 HTTP header 传递，绝不能进入
/// `tokio::process::Command` 的 argv（否则 `ps aux` 会泄漏）。
/// 格式：`Authorization: Bearer__{token}`
pub(crate) fn format_zhixueyun_auth_header(token: &str) -> String {
    format!("Authorization: Bearer__{token}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ytdlp_success_records_completed() {
        assert_eq!(
            decide_ytdlp_history_status(true),
            DownloadStatus::Completed
        );
    }

    #[test]
    fn ytdlp_failure_records_failed() {
        // P0-1 regression: 失败的下载不应被记为 Completed
        assert_eq!(
            decide_ytdlp_history_status(false),
            DownloadStatus::Failed
        );
    }

    #[test]
    fn zhixueyun_auth_header_contains_token() {
        // P0-2 regression: token 必须出现在 Authorization header 中，
        // 并且绝不进入 Command::args（避免 ps 可见）
        let header = format_zhixueyun_auth_header("SECRET_TOKEN_123");
        assert!(
            header.contains("SECRET_TOKEN_123"),
            "token must be in header: {header}"
        );
        assert!(
            header.starts_with("Authorization:"),
            "header must be well-formed: {header}"
        );
    }
}
