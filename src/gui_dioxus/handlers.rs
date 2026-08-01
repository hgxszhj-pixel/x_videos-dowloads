//! 业务逻辑：视频信息解析、单/批量下载实现

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, AtomicU64, Ordering};
use std::sync::Arc;

use tracing::info;

use crate::config::AppConfig;
use crate::downloader::VideoDownloader;
use crate::fxtwitter::FxTwitterResolver;
use crate::types::sanitize_filename;
use crate::platform::{
    BilibiliResolver, DouYinResolver, NnyyResolver, PlatformResolver, ResolverRegistry,
    TikTokResolver, TulingPythonResolver, YouTubeResolver,
};
use crate::types::{DownloadProgress, DownloadRequest};

use super::types::{
    format_file_size, CourseCollectionState, CourseItemDisplay, ErrorDisplay, VideoFormatDisplay,
    VideoInfoDisplay,
};

/// 同步下载 - 使用 VideoDownloader
#[allow(clippy::too_many_arguments)]
pub(super) async fn start_download_impl(
    url: &str,
    original_url: Option<String>,
    title: &str,
    progress_arc: Option<Arc<AtomicU32>>,
    speed_arc: Option<Arc<AtomicU64>>,
    total_bytes_arc: Option<Arc<AtomicU64>>,
    cancel_flag: Option<Arc<AtomicBool>>,
    format_id: Option<String>,
    strategy: crate::types::DownloadStrategy,
) -> Result<(), ErrorDisplay> {
    let config = AppConfig::load().map_err(|e| ErrorDisplay::new("download", e.to_string(), "检查配置文件权限"))?;
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
    if let Some(flag) = cancel_flag {
        request = request.with_cancel_flag(flag);
    }

    // 创建进度回调
    type ProgressCallback = Arc<dyn Fn(DownloadProgress) + Send + Sync>;
    let callback: Option<ProgressCallback> = progress_arc.as_ref().map(|arc| {
        let progress_arc = arc.clone();
        let speed_arc = speed_arc.clone();
        let total_bytes_arc = total_bytes_arc.clone();
        Arc::new(move |p: DownloadProgress| {
            let percent = (p.percent * 10000.0) as u32;
            progress_arc.store(percent, Ordering::Relaxed);
            if let Some(speed_arc) = &speed_arc {
                speed_arc.store(p.speed as u64, Ordering::Relaxed);
            }
            if p.total_bytes > 0 {
                if let Some(tb_arc) = &total_bytes_arc {
                    tb_arc.store(p.total_bytes, Ordering::Relaxed);
                }
            }
        }) as ProgressCallback
    });

    let mut downloader = VideoDownloader::new().map_err(|e| ErrorDisplay::new("download", e.to_string(), "检查下载器初始化"))?;
    downloader.download(&request, callback).await
        .map_err(|e| {
            // 尝试 downcast 到结构化 DownloadError，否则 fallback
            if let Some(de) = e.downcast_ref::<crate::types::DownloadError>() {
                ErrorDisplay::from(de.clone())
            } else {
                ErrorDisplay::new("download", e.to_string(), "")
            }
        })?;
    Ok(())
}

/// 批量下载实现
pub(super) async fn batch_download_impl(
    urls: Vec<String>,
    progress_arcs: Vec<Option<Arc<AtomicU32>>>,
    speed_arcs: Vec<Option<Arc<AtomicU64>>>,
    cancel_flags: Vec<Option<Arc<AtomicBool>>>,
) {
    let concurrency = 3; // 最多同时下载3个
    let semaphore = Arc::new(tokio::sync::Semaphore::new(concurrency));

    // 并发下载所有 URL
    let handles: Vec<_> = urls.into_iter().enumerate().map(|(idx, url)| {
        let semaphore = semaphore.clone();
        let progress_arc = progress_arcs.get(idx).cloned().flatten();
        let speed_arc = speed_arcs.get(idx).cloned().flatten();
        let cancel_flag = cancel_flags.get(idx).cloned().flatten();
        tokio::spawn(async move {
            let _permit = semaphore.acquire_owned().await;
            // 获取视频信息
            match fetch_video_info_impl(&url).await {
                Ok(info) => {
                    // 下载视频
                    match start_download_impl(&info.url, info.original_url, &info.title, progress_arc, speed_arc, None, cancel_flag, None, info.download_strategy).await {
                        Ok(_) => (url, true, "completed".to_string(), 1.0),
                        Err(e) => (url, false, format!("failed: {}", e.message), 0.0),
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
        match handle.await {
            Ok(result) => {
                completed += 1;
                let (url, _success, status, _progress) = result;
                // 更新状态（这里简化处理，实际需要通过状态管理）
                info!("批量下载进度: {}/{} - {} - {}", completed, total, url, status);
            }
            Err(join_err) => {
                // spawn task panic/cancel 不再静默:记 warning,completed 计入但状态标 failed
                tracing::warn!(
                    "批量下载 task join 失败 (panic/cancel): {} - {}/{}",
                    join_err,
                    completed + 1,
                    total
                );
                completed += 1;
            }
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
    if let Ok(tp) = TulingPythonResolver::new() {
        registry.register(tp); // vip.tulingpyton.cn / tulingpyton.cn（含课程池 list_collection）
    }
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

/// 从课程页 URL 拉取课程目录。仅供具备 list_collection 的解析器使用
/// （目前只有 `TulingPythonResolver`），其他平台返回 `Err` 让上层走单视频流程。
pub(super) async fn fetch_course_collection_impl(url: &str) -> Result<CourseCollectionState, String> {
    use crate::platform::ResolverError;
    let mut registry = ResolverRegistry::new();
    if let Ok(tp) = TulingPythonResolver::new() {
        registry.register(tp);
    }
    let resolver = registry.select_resolver(url);
    if resolver.platform() != crate::history::Platform::TulingPython {
        return Err(format!("当前平台 ({:?}) 不支持课程池", resolver.platform()));
    }
    // 登录态预检:导航到用户实际想看的课程页(而非硬编码首页),
    // 课程页一定会触发 xe.micro_page.common.public.data.get 验证访问权限,
    // 首页可能不调该 API → 误判"未登录" → 用户报告"已用 ego-lite 登录却仍报需要登录"。
    // 2026-08-01: 区分 SessionExpired (Code:2506 当前用户不存在,cookie 过期需重扫码)
    // vs NotLoggedIn (从未登录)。后者提示"打开登录窗口"足够,前者必须显式说明过期。
    match crate::drission_client::is_logged_in(url).await {
        crate::drission_client::LoginStatus::LoggedIn => {}
        crate::drission_client::LoginStatus::SessionExpired => {
            // 前缀 🔑 让 actions.rs 的错误分类识别为 login 类型,而不是 course
            return Err("🔑 登录已过期 (Code:2506 当前用户不存在): 请点击\"打开登录窗口\"扫码重新登录".to_string());
        }
        crate::drission_client::LoginStatus::NotLoggedIn => {
            return Err(ResolverError::AuthenticationRequired.to_string());
        }
    }
    let entries = resolver
        .list_collection(url)
        .await
        .map_err(|e| match e {
            ResolverError::AuthenticationRequired => {
                "需要登录：请点击“打开登录窗口”扫码/账号登录".to_string()
            }
            other => format!("课程目录解析失败：{other}"),
        })?;
    if entries.is_empty() {
        return Err("未识别到课程条目，已回退到单视频流程".into());
    }
    let items: Vec<CourseItemDisplay> = entries
        .into_iter()
        .enumerate()
        .map(|(i, e)| CourseItemDisplay {
            id: e.id,
            index: (i + 1) as u32,
            title: e.name,
            page_url: url.to_string(),
            jump_url: e.jump_url,
            selected: true,
            status: "pending".into(),
            progress: 0.0,
            slot: None,
        })
        .collect();
    let total = items.len();
    Ok(CourseCollectionState {
        visible: true,
        items,
        status: format!("已识别 {total} 个课节"),
    })
}

/// 打开 headed 浏览器让用户登录小鹅通账号；180s 后自动关闭。
/// GUI 在捕获 "需要登录" 错误后,或用户主动点按钮时调用此函数。
/// `target_url` 是用户在主输入框填的视频/课程页 URL(SPA 会自动跳到登录页,
/// 登录后回跳),为空则回退到小鹅通首页。
pub(super) async fn open_tuling_login(target_url: &str) -> anyhow::Result<()> {
    let url = if target_url.trim().is_empty() {
        "https://vip.tulingpyton.cn"
    } else {
        target_url
    };
    crate::drission_client::open_login_window(url).await
}

/// 课程池串行解析 + 并发下载：
/// - 课节页 CDP 解析必须串行（共享一个浏览器 profile，避免锁冲突）；
/// - 解析完成或失败后，把 m3u8 交给现有 HLS 下载器，按 settings_concurrent 并发。
///
/// 返回每项最终状态 (id, status) 供调用方写回 UI。
/// 单条课节下载进度槽（跨 task 共享，GUI 端 use_effect 轮询拉取）
#[derive(Default)]
pub struct CourseItemSlot {
    pub progress: AtomicU32, // 0-10000 表示 0.00%-100.00%
    pub status: AtomicU8,    // 0=pending 1=resolving 2=downloading 3=completed 4=failed 5=skipped
}

impl CourseItemSlot {
    pub fn set_status(&self, s: &str) {
        let v = match s {
            "resolving" => 1,
            "downloading" => 2,
            "completed" => 3,
            "failed" => 4,
            "skipped" => 5,
            _ => 0,
        };
        self.status.store(v, Ordering::Relaxed);
    }
}

pub(super) async fn course_download_pool_impl(
    state: CourseCollectionState,
    slots: Vec<Arc<CourseItemSlot>>,
) -> Vec<(String, String)> {
    // 第一步：串行解析所有课节（CDP 同一 profile，必须串行）
    let (results, resolved) = resolve_items_serially(&state, &slots).await;
    // 第二步：并发下载已解析的 m3u8（HLS 独立任务，互不依赖）
    let dl_results = download_resolved_concurrently(&state, &slots, &resolved).await;
    let mut all = results;
    all.extend(dl_results);
    all
}

/// 串行解析每个课节：检查登录态 → 调 Drission → 翻译错误。
/// 返回：每项的 (id, status) 临时结果 + 已成功解析的 m3u8 列表。
async fn resolve_items_serially(
    state: &CourseCollectionState,
    slots: &[Arc<CourseItemSlot>],
) -> (Vec<(String, String)>, Vec<Option<String>>) {
    use crate::platform::TulingPythonResolver;
    let total = state.items.len();
    let mut results: Vec<(String, String)> = Vec::with_capacity(total);
    let mut resolved: Vec<Option<String>> = vec![None; total];

    for (i, item) in state.items.iter().enumerate() {
        let slot = slots.get(i).cloned();
        if !item.selected {
            results.push((item.id.clone(), "skipped".into()));
            if let Some(s) = slot { s.set_status("skipped"); }
            continue;
        }
        if let Some(s) = slot.as_ref() { s.set_status("resolving"); }
        // 登录态已在 fetch_course_collection_impl 入口预检；每条课节复用同一 session。
        // 单条课节页若 m3u8 未拦截到（CatCatch 提示），由 translate_resolve_error 归类为“需要登录”。
        // 优先用 item.jump_url(从 list_collection 透传),否则 fallback 到 page_url+resource_id
        let session_url = item
            .jump_url
            .clone()
            .unwrap_or_else(|| build_session_url(&item.page_url, &item.id));
        let resolver = match TulingPythonResolver::new() {
            Ok(r) => r,
            Err(e) => {
                let msg = format!("failed: TulingPythonResolver 初始化失败 ({e})");
                results.push((item.id.clone(), msg));
                if let Some(s) = slot.as_ref() { s.set_status("failed"); }
                continue;
            }
        };
        match resolver.get_video_info(&session_url).await {
            Ok(info) => {
                resolved[i] = Some(info.url);
                if let Some(s) = slot.as_ref() { s.set_status("downloading"); }
            }
            Err(e) => {
                let norm = translate_resolve_error(&e);
                results.push((item.id.clone(), norm));
                if let Some(s) = slot.as_ref() { s.set_status("failed"); }
            }
        }
    }
    (results, resolved)
}

/// 把课节页 URL 拼成课节播放页（追加 resource_id=xxx）。
fn build_session_url(page_url: &str, id: &str) -> String {
    if page_url.contains("resource_id=") {
        return page_url.to_string();
    }
    let sep = if page_url.contains('?') { '&' } else { '?' };
    format!("{page_url}{sep}resource_id={id}")
}

/// 把解析阶段错误归类（CatCatch 提示即登录缺失）。
fn translate_resolve_error(e: &crate::platform::ResolverError) -> String {
    let msg = e.to_string();
    if msg.contains("CatCatch") || msg.contains("需要用 CatCatch") {
        "failed: 需要登录（m3u8 未拦截到）".to_string()
    } else {
        format!("failed: {msg}")
    }
}

/// 并发下载已解析的 m3u8：受信号量控制；每项独立 task，原子进度回写。
async fn download_resolved_concurrently(
    state: &CourseCollectionState,
    slots: &[Arc<CourseItemSlot>],
    resolved: &[Option<String>],
) -> Vec<(String, String)> {
    let concurrency = AppConfig::load()
        .map(|c| c.concurrent_downloads.max(1))
        .unwrap_or(3);
    let semaphore = Arc::new(tokio::sync::Semaphore::new(concurrency));
    let mut handles = Vec::with_capacity(state.items.len());

    for (i, item) in state.items.iter().enumerate() {
        let Some(m3u8) = resolved[i].clone() else { continue };
        let title = item.title.clone();
        let id = item.id.clone();
        let slot = slots.get(i).cloned();
        let progress_cb: Arc<dyn Fn(crate::types::DownloadProgress) + Send + Sync> =
            Arc::new(move |dp: crate::types::DownloadProgress| {
                if let Some(s) = slot.as_ref() {
                    s.progress
                        .store((dp.percent * 10000.0) as u32, Ordering::Relaxed);
                }
            });
        let slot_for_done = slots.get(i).cloned();
        let semaphore = semaphore.clone();
        handles.push(tokio::spawn(async move {
            let _permit = semaphore.acquire_owned().await;
            match download_one(&title, &m3u8, progress_cb).await {
                Ok(()) => {
                    if let Some(s) = slot_for_done.as_ref() {
                        s.progress.store(10000, Ordering::Relaxed);
                        s.set_status("completed");
                    }
                    (id, "completed".into())
                }
                Err(msg) => {
                    if let Some(s) = slot_for_done.as_ref() {
                        s.set_status("failed");
                    }
                    (id, format!("failed: {msg}"))
                }
            }
        }));
    }
    let mut out = Vec::with_capacity(handles.len());
    for h in handles {
        match h.await {
            Ok((id, status)) => out.push((id, status)),
            Err(join_err) => {
                // 课程池 task panic/cancel 不再静默:记 warning 并把受影响项标 failed,
                // 上层 M7 聚合时能看见这条记录
                tracing::warn!("课程池下载 task join 失败 (panic/cancel): {}", join_err);
                out.push((
                    "<unknown>".to_string(),
                    format!("failed: task join error: {join_err}"),
                ));
            }
        }
    }
    out
}

/// 单条 HLS 下载：构造请求 + 调用 VideoDownloader，错误统一为 String。
async fn download_one(
    title: &str,
    m3u8: &str,
    progress_cb: Arc<dyn Fn(crate::types::DownloadProgress) + Send + Sync>,
) -> Result<(), String> {
    let output_dir = AppConfig::load()
        .map(|c| c.output_dir)
        .unwrap_or_else(|_| PathBuf::from("./Downloads"));
    let output_path = build_output_path(&output_dir, title);
    let req = DownloadRequest::new(m3u8, output_path)
        .with_strategy(crate::types::DownloadStrategy::Hls);
    let mut dl = VideoDownloader::new().map_err(|e| format!("VideoDownloader 初始化失败：{e}"))?;
    let result = dl
        .download(&req, Some(progress_cb))
        .await
        .map_err(|e| e.to_string())?;
    if result.success {
        Ok(())
    } else {
        Err(result.error_message.unwrap_or_default())
    }
}

/// 构造安全文件路径：复用 `crate::types::sanitize_filename` 防止路径遍历。
fn build_output_path(dir: &std::path::Path, title: &str) -> PathBuf {
    let safe = sanitize_filename(title);
    dir.join(format!("{safe}.mp4"))
}
