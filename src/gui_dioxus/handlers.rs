//! 业务逻辑：视频信息解析、单/批量下载实现

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

use tracing::info;

use crate::config::AppConfig;
use crate::downloader::VideoDownloader;
use crate::fxtwitter::FxTwitterResolver;
use crate::platform::{
    BilibiliResolver, DouYinResolver, NnyyResolver, ResolverRegistry, TikTokResolver,
    YouTubeResolver,
};
use crate::types::{DownloadProgress, DownloadRequest};

use super::types::{format_file_size, VideoFormatDisplay, VideoInfoDisplay};

/// 同步下载 - 使用 VideoDownloader
pub(super) async fn start_download_impl(
    url: &str,
    original_url: Option<String>,
    title: &str,
    progress_arc: Option<Arc<AtomicU32>>,
    speed_arc: Option<Arc<AtomicU64>>,
    format_id: Option<String>,
    strategy: crate::types::DownloadStrategy,
) -> Result<(), String> {
    let config = AppConfig::load().map_err(|e| e.to_string())?;
    let output_dir = if config.output_dir.to_string_lossy().is_empty() {
        directories::UserDirs::new()
            .and_then(|dirs| dirs.video_dir().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("./Downloads"))
    } else {
        config.output_dir.clone()
    };

    // 生成文件名：默认使用 mp4 扩展名（大多数视频格式）
    let ext = "mp4";
    let safe_title = title.chars().filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-' || *c == '_').collect::<String>();
    let filename = if safe_title.is_empty() {
        format!("video.{}", ext)
    } else {
        format!("{}.{}", safe_title, ext)
    };
    let output_path = output_dir.join(&filename);

    // 构建 DownloadRequest
    let mut request = DownloadRequest::new(url, &output_path);
    if let Some(fid) = format_id {
        request = request.with_format_id(fid);
    }
    if let Some(orig_url) = original_url {
        request = request.with_original_url(orig_url);
    }
    request = request.with_strategy(strategy);

    // 创建进度回调
    type ProgressCallback = Arc<dyn Fn(DownloadProgress) + Send + Sync>;
    let callback: Option<ProgressCallback> = progress_arc.as_ref().map(|arc| {
        let progress_arc = arc.clone();
        let speed_arc = speed_arc.clone();
        Arc::new(move |p: DownloadProgress| {
            let percent = (p.percent * 10000.0) as u32;
            progress_arc.store(percent, Ordering::Relaxed);
            if let Some(speed_arc) = &speed_arc {
                speed_arc.store(p.speed as u64, Ordering::Relaxed);
            }
        }) as ProgressCallback
    });

    let downloader = VideoDownloader::new().map_err(|e| e.to_string())?;
    downloader.download(&request, callback).await
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// 批量下载实现
pub(super) async fn batch_download_impl(
    urls: Vec<String>,
    progress_arcs: Vec<Option<Arc<AtomicU32>>>,
    speed_arcs: Vec<Option<Arc<AtomicU64>>>,
) {
    let concurrency = 3; // 最多同时下载3个
    let semaphore = Arc::new(tokio::sync::Semaphore::new(concurrency));

    // 并发下载所有 URL
    let handles: Vec<_> = urls.into_iter().enumerate().map(|(idx, url)| {
        let semaphore = semaphore.clone();
        let progress_arc = progress_arcs.get(idx).cloned().flatten();
        let speed_arc = speed_arcs.get(idx).cloned().flatten();
        tokio::spawn(async move {
            let _permit = semaphore.acquire_owned().await;
            // 获取视频信息
            match fetch_video_info_impl(&url).await {
                Ok(info) => {
                    // 下载视频
                    match start_download_impl(&info.url, info.original_url, &info.title, progress_arc, speed_arc, None, info.download_strategy).await {
                        Ok(_) => (url, true, "completed".to_string(), 1.0),
                        Err(e) => (url, false, format!("failed: {}", e), 0.0),
                    }
                }
                Err(e) => (url, false, format!("解析失败: {}", e), 0.0),
            }
        })
    }).collect();

    // 等待所有下载完成
    let mut completed = 0;
    let total = handles.len();

    for handle in handles {
        if let Ok(result) = handle.await {
            completed += 1;
            let (url, _success, status, _progress) = result;
            // 更新状态（这里简化处理，实际需要通过状态管理）
            info!("批量下载进度: {}/{} - {} - {}", completed, total, url, status);
        }
    }

    info!("批量下载完成: {}/{} 成功", completed, total);
}

pub(super) async fn fetch_video_info_impl(url: &str) -> Result<VideoInfoDisplay, String> {
    let mut registry = ResolverRegistry::new();

    // 为 TikTok 和 DouYin 配置 msToken
    let mut douyin_resolver = DouYinResolver::new();
    let mut tiktok_resolver = TikTokResolver::new();

    if let Ok(config) = AppConfig::load() {
        if let Some(ref ms_token) = config.douyin_ms_token {
            let mut tokens = std::collections::HashMap::new();
            tokens.insert("msToken".to_string(), ms_token.clone());
            douyin_resolver = douyin_resolver.with_tokens(tokens.clone());
            tiktok_resolver = tiktok_resolver.with_tokens(tokens);
        }
    }

    // 按优先级注册解析器（第一个匹配的会被选中）
    registry.register(FxTwitterResolver::new()); // x.com/twitter.com
    registry.register(tiktok_resolver);
    registry.register(douyin_resolver);
    registry.register(BilibiliResolver::new());
    registry.register(YouTubeResolver::new()); // youtube.com
    registry.register(NnyyResolver::new()); // nnyy.in
    // 其他平台使用默认的 YtDlpResolver

    // 使用 try_resolve_all 尝试所有解析器
    match registry.try_resolve_all(url).await {
        Ok(info) => {
            let duration = info.duration.map(|d| format!("{}:{:02}", d / 60, d % 60)).unwrap_or_default();
            // 如果 url 和输入 url 不同，说明 url 被修改了，保存原始 url
            let original_url = if info.url != url {
                Some(url.to_string())
            } else {
                None
            };
            Ok(VideoInfoDisplay {
                url: info.url.clone(),
                original_url,
                title: info.title,
                uploader: info.uploader.unwrap_or_default(),
                duration,
                formats: info.formats.iter().map(|f| VideoFormatDisplay {
                    format_id: f.format_id.clone(),
                    resolution: f.resolution.clone().unwrap_or_default(),
                    ext: f.ext.clone(),
                    filesize: f.filesize.map(format_file_size).unwrap_or_default(),
                }).collect(),
                download_strategy: info.download_strategy,
            })
        }
        Err(e) => Err(format!("Failed to fetch video info: {}", e)),
    }
}
