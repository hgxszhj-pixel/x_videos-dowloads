//! download 子命令：视频信息解析与下载执行（orchestrator）
//!
//! 4 模块拆分：
//! - [`dispatch`]       — 纯函数：cli + video_info → DownloadPath
//! - [`history_writer`] — 隔离 9 参数 add_entry_with_all 调用
//! - [`strategy_direct`]— reqwest / m3u8 直接下载
//! - [`strategy_native`]— 智学云 reqwest + Authorization header
//! - [`strategy_ytdlp`] — yt-dlp 子进程（含直接 twimg URL + HLS 分支）

use std::time::Instant;

use anyhow::Result;
use tracing::info;

use crate::cli::Cli;
use crate::config::AppConfig;
use crate::history::{DownloadStatus, History};
use crate::resolver_chain::get_video_info;
use crate::types::VideoInfo;
use crate::yt_dlp::YtDlp;

pub mod dispatch;
pub mod history_writer;
pub mod strategy_direct;
pub mod strategy_native;
pub mod strategy_ytdlp;

// Re-export 保持 `use crate::commands::download::dispatch::DownloadPath` 等路径不变
pub use dispatch::DownloadPath;

/// 主入口：CLI 子命令 `download` 的执行
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

    // 处理 URL（CLI 单一 URL 或批量文件）
    let urls = if let Some(ref url) = cli.url {
        vec![url.clone()]
    } else if let Some(batch_file) = &cli.batch {
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

        // 获取视频信息（direct 模式跳过）
        let video_info = if cli.direct {
            None
        } else {
            Some(get_video_info(&ytdlp, url, &cli, &config).await?)
        };

        // 显示格式列表（不下载）
        if cli.list_formats {
            if let Some(ref info) = video_info {
                print_video_formats(info);
            }
            continue;
        }

        // 决定下载路径
        let path = match dispatch::decide_path(cli.direct, video_info.as_ref(), Some(url)) {
            Some(p) => p,
            None => {
                tracing::warn!("无法决定下载路径，跳过: {}", url);
                continue;
            }
        };

        // 分发到对应 strategy + 写历史
        dispatch_strategy(
            &path,
            video_info.as_ref(),
            &cli,
            &config,
            &ytdlp,
            start_time,
            &mut history,
        )
        .await;
    }

    let elapsed = start_time.elapsed().as_secs_f64();
    println!("\n完成! 总耗时: {:.2}s", elapsed);

    Ok(())
}

/// 分发到 strategy 执行下载，结果写入历史
async fn dispatch_strategy(
    path: &DownloadPath,
    video_info: Option<&VideoInfo>,
    cli: &Cli,
    config: &AppConfig,
    ytdlp: &YtDlp,
    start_time: Instant,
    history: &mut History,
) {
    let url = path.url();
    match path {
        DownloadPath::Native { .. } => {
            let info = match video_info {
                Some(i) => i,
                None => {
                    tracing::warn!("Native path 需要 video_info，但为 None: {}", url);
                    return;
                }
            };
            match strategy_native::download(info, cli, start_time).await {
                Ok(result) => write_history(history, url, Some(&info.title), Some(info), &result),
                Err(e) => tracing::error!("Native 下载错误: {}", e),
            }
        }
        DownloadPath::YtDlp { .. } => {
            let info = match video_info {
                Some(i) => i,
                None => {
                    tracing::warn!("YtDlp path 需要 video_info，但为 None: {}", url);
                    return;
                }
            };
            match strategy_ytdlp::download(ytdlp, info, cli, config, start_time).await {
                Ok(result) => {
                    let status = decide_ytdlp_history_status(result.success);
                    write_history_with_status(
                        history,
                        url,
                        Some(&info.title),
                        Some(info),
                        &result,
                        status,
                    );
                }
                Err(e) => tracing::error!("yt-dlp 下载错误: {}", e),
            }
        }
        DownloadPath::Direct { .. } => {
            match strategy_direct::download(url, cli, start_time).await {
                Ok(result) => write_history(history, url, None, None, &result),
                Err(e) => tracing::error!("直接下载错误: {}", e),
            }
        }
        DownloadPath::Hls { .. } => {
            // HLS URL：direct mode 用 strategy_direct；parser-decided Hls 用 strategy_ytdlp
            if let Some(info) = video_info {
                match strategy_ytdlp::download(ytdlp, info, cli, config, start_time).await {
                    Ok(result) => {
                        write_history(history, url, Some(&info.title), Some(info), &result)
                    }
                    Err(e) => tracing::error!("HLS 下载错误: {}", e),
                }
            } else {
                match strategy_direct::download(url, cli, start_time).await {
                    Ok(result) => write_history(history, url, None, None, &result),
                    Err(e) => tracing::error!("HLS 下载错误: {}", e),
                }
            }
        }
    }
}

/// 写入历史（成功/失败都记录，失败时 status 由 decide_ytdlp_history_status 决定）
fn write_history(
    history: &mut History,
    url: &str,
    title: Option<&str>,
    video_info: Option<&VideoInfo>,
    result: &crate::types::DownloadResult,
) {
    let status = decide_ytdlp_history_status(result.success);
    write_history_with_status(history, url, title, video_info, result, status);
}

fn write_history_with_status(
    history: &mut History,
    url: &str,
    title: Option<&str>,
    video_info: Option<&VideoInfo>,
    result: &crate::types::DownloadResult,
    status: DownloadStatus,
) {
    if let Err(e) = history_writer::record(
        history,
        history_writer::HistoryCtx {
            url,
            title,
            video_info,
            result,
        },
        status,
    ) {
        tracing::warn!("写入历史失败: {}", e);
    }
}

/// 打印可用格式
fn print_video_formats(info: &VideoInfo) {
    use crate::types::format_bytes;
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
}