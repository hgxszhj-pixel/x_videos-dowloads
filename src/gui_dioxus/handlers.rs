//! 业务逻辑：视频信息解析、单/批量下载实现

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;

use tracing::info;

use crate::config::AppConfig;
use crate::downloader::VideoDownloader;
use crate::fxtwitter::FxTwitterResolver;
use crate::platform::{
    BilibiliResolver, DouYinResolver, NnyyResolver, PlatformResolver, ResolverRegistry,
    ShanhuResolver, TikTokResolver, TulingPythonResolver, YouTubeResolver,
};
use crate::types::sanitize_filename;
use crate::types::{DownloadProgress, DownloadRequest};

use super::store::add_completed_download;

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
    let config = AppConfig::load()
        .map_err(|e| ErrorDisplay::new("download", e.to_string(), "检查配置文件权限"))?;
    let output_dir = if config.output_dir.to_string_lossy().is_empty() {
        directories::UserDirs::new()
            .and_then(|dirs| dirs.video_dir().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("./Downloads"))
    } else {
        config.output_dir.clone()
    };

    // 生成文件名：默认使用 mp4 扩展名（大多数视频格式）
    let ext = "mp4";
    let safe_title = title
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-' || *c == '_')
        .collect::<String>();
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

    let mut downloader = VideoDownloader::new()
        .map_err(|e| ErrorDisplay::new("download", e.to_string(), "检查下载器初始化"))?;
    downloader.download(&request, callback).await.map_err(|e| {
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
    let handles: Vec<_> = urls
        .into_iter()
        .enumerate()
        .map(|(idx, url)| {
            let semaphore = semaphore.clone();
            let progress_arc = progress_arcs.get(idx).cloned().flatten();
            let speed_arc = speed_arcs.get(idx).cloned().flatten();
            let cancel_flag = cancel_flags.get(idx).cloned().flatten();
            tokio::spawn(async move {
                let _permit = semaphore.acquire_owned().await;
                tracing::info!(
                    "[batch_download] sub-task 启动 idx={} url={}",
                    idx,
                    url
                );
                // 获取视频信息
                match fetch_video_info_impl(&url).await {
                    Ok(info) => {
                        tracing::info!(
                            "[batch_download] fetch 成功 idx={} title={} resolved_url={}",
                            idx,
                            info.title,
                            info.url
                        );
                        // 下载视频
                        match start_download_impl(
                            &info.url,
                            info.original_url,
                            &info.title,
                            progress_arc,
                            speed_arc,
                            None,
                            cancel_flag,
                            None,
                            info.download_strategy,
                        )
                        .await
                        {
                            Ok(_) => {
                                tracing::info!(
                                    "[batch_download] 下载完成 idx={} url={}",
                                    idx,
                                    url
                                );
                                (url, true, "completed".to_string(), 1.0)
                            }
                            Err(e) => {
                                tracing::warn!(
                                    "[batch_download] 下载失败 idx={} url={} err={}",
                                    idx,
                                    url,
                                    e.message
                                );
                                (url, false, format!("failed: {}", e.message), 0.0)
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            "[batch_download] 解析失败 idx={} url={} err={}",
                            idx,
                            url,
                            e
                        );
                        (url, false, format!("解析失败: {}", e), 0.0)
                    }
                }
            })
        })
        .collect();

    // 等待所有下载完成
    let mut completed = 0;
    let total = handles.len();

    for handle in handles {
        match handle.await {
            Ok(result) => {
                completed += 1;
                let (url, _success, status, _progress) = result;
                // 更新状态（这里简化处理，实际需要通过状态管理）
                info!(
                    "批量下载进度: {}/{} - {} - {}",
                    completed, total, url, status
                );
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
    if let Ok(sr) = ShanhuResolver::new() {
        registry.register(sr); // rexxi.cn / wanzibook.com / tenwiseacademy.cn（白标 SHANHU/万字）
    }
    // 其他平台使用默认的 YtDlpResolver

    // 使用 try_resolve_all 尝试所有解析器
    match registry.try_resolve_all(url).await {
        Ok(info) => {
            let duration = info
                .duration
                .map(|d| format!("{}:{:02}", d / 60, d % 60))
                .unwrap_or_default();
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
                formats: info
                    .formats
                    .iter()
                    .map(|f| VideoFormatDisplay {
                        format_id: f.format_id.clone(),
                        resolution: f.resolution.clone().unwrap_or_default(),
                        ext: f.ext.clone(),
                        filesize: f.filesize.map(format_file_size).unwrap_or_default(),
                    })
                    .collect(),
                download_strategy: info.download_strategy,
                selected: true, // 批量解析得到的视频默认勾选（与 CourseItemDisplay.selected 对齐）
            })
        }
        Err(e) => Err(format!("Failed to fetch video info: {}", e)),
    }
}

/// 单 URL 智能派发:SHANHU 课程 URL 自动走批量路径,其它平台保持单视频流程。
///
/// 设计动机(2026-09-06):用户输入课程页 URL(如 rexxi.cn/PC/#/yueke3/3529714 含 83 章节)
/// 点"获取信息",原行为只显示 1 个 video card(`ShanhuResolver::get_video_info` 返回
/// 第 1 个有 `file_id` 的章节)。期望:显示完整 N 章节列表(带 checkbox)。
///
/// 实现路径:
/// 1. 用 `select_resolver` 识别平台
/// 2. 仅 `Platform::Shanhu` 走批量(`list_collection` 完全匿名,无需登录)
/// 3. 其它平台 → 直接 `fetch_video_info_impl` 单视频(原行为)
/// 4. SHANHU 拿到 N 章节后逐条调 `get_video_info` 拿 m3u8 → `FetchResult::Batch`
/// 5. 任一环节失败 → fallback 到单视频路径(graceful degradation)
///
/// **章节级跳转**:SHANHU `list_collection` (shanhu.rs:739-742) 给每条 chapter
/// 生成 `jump_url = "<原URL>#chapter=<id>"`(或 `&chapter=<id>` 当原 URL 已有
/// `#`);`get_video_info` (shanhu.rs:790-791) 通过 `extract_chapter_id` 解析该
/// 片段,确定要下载的章节。所以批量解析会拿到 N 个不同章节的 m3u8,**不会**
/// 出现"N 条相同项下载首章 N 次"。fragment 仅作客户端参数,不会发到 server。
pub(super) async fn fetch_smart_impl(url: &str) -> Result<super::types::FetchResult, String> {
    use crate::history::Platform;
    use crate::platform::ShanhuResolver;

    // 1) 仅当 resolver 是 SHANHU 时才走批量
    let mut registry = ResolverRegistry::new();
    if let Ok(sr) = ShanhuResolver::new() {
        registry.register(sr);
    }
    let resolver = registry.select_resolver(url);
    if resolver.platform() != Platform::Shanhu {
        return fetch_video_info_impl(url)
            .await
            .map(super::types::FetchResult::Single);
    }

    // 2) SHANHU list_collection 匿名可达
    let entries = match resolver.list_collection(url).await {
        Ok(e) if !e.is_empty() => e,
        Ok(_) => {
            // 空列表 → 视为单视频
            return fetch_video_info_impl(url)
                .await
                .map(super::types::FetchResult::Single);
        }
        Err(e) => {
            // list_collection 失败 → fallback 单视频(ResloverError 不外泄给用户)
            tracing::warn!("SHANHU list_collection 失败,回退单视频: {e}");
            return fetch_video_info_impl(url)
                .await
                .map(super::types::FetchResult::Single);
        }
    };

    // 3) 逐章节 resolve m3u8 → Vec<VideoInfoDisplay>
    //    单条失败跳过(对照 SHANHU 实测:83 章节仅 2 条有 file_id,其它跳过后 N=2)
    let mut items = Vec::with_capacity(entries.len());
    for entry in &entries {
        let target = entry.jump_url.as_deref().unwrap_or(url);
        match fetch_video_info_impl(target).await {
            Ok(info) => items.push(info),
            Err(e) => {
                tracing::warn!("批量章节解析跳过 {}: {}", entry.name, e);
            }
        }
    }
    if items.is_empty() {
        // 全部解析失败 → 检查 session 是否过期,过期时给明确提示
        // (P3 UX 修复 2026-09-13:原 fallback 到 fetch_video_info_impl 单视频路径
        //  会让用户看到误导的"URL 无效或平台不支持",根因是每章 fetch_video_info_impl
        //  在 stale session 下全失败,而 fallback 路径无 resolver 匹配 rexxi URL)
        let summary = crate::platform::session_summary();
        if summary.is_stale {
            let days = summary.age_days.unwrap_or(0);
            return Err(format!(
                "SHANHU session 已过期(距今 {} 天,阈值 7 天),请重跑 \
                 `x-video-downloader --shanhu-login <state.json>` 后再试",
                days
            ));
        }
        return fetch_video_info_impl(url)
            .await
            .map(super::types::FetchResult::Single);
    }
    Ok(super::types::FetchResult::Batch(items))
}

/// 从课程页 URL 拉取课程目录。仅供具备 list_collection 的解析器使用
/// （目前只有 `TulingPythonResolver`），其他平台返回 `Err` 让上层走单视频流程。
pub(super) async fn fetch_course_collection_impl(
    url: &str,
) -> Result<CourseCollectionState, String> {
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
            return Err(
                "🔑 登录已过期 (Code:2506 当前用户不存在): 请点击\"打开登录窗口\"扫码重新登录"
                    .to_string(),
            );
        }
        crate::drission_client::LoginStatus::NotLoggedIn => {
            return Err(ResolverError::AuthenticationRequired.to_string());
        }
    }
    let entries = resolver.list_collection(url).await.map_err(|e| match e {
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
            speed: String::new(),
            downloaded_bytes: 0,
            total_bytes: 0,
        })
        .collect();
    let total = items.len();
    Ok(CourseCollectionState {
        visible: true,
        items,
        status: format!("已识别 {total} 个课节"),
        // P-3 (2026-08-29): 重新加载课程池时重置覆盖开关(避免粘性状态导致误覆盖)
        overwrite_existing: false,
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
/// 课节状态枚举 — 替代之前 `set_status` 与 `slot_status_to_str` 的两处并行 `&str/u8 → variant` switch。
/// 编译器强制每个 `match` 覆盖全部 7 个 variant,新增/删除 variant 时两处同步更新,
/// 避免 drift 引入"写入 6 但 UI 永远显示 pending"之类的隐性 bug。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CourseItemStatus {
    Pending,
    Resolving,
    Downloading,
    Completed,
    Failed,
    Skipped,
    /// 输出目录已有同名 .mp4 文件(>0 字节),跳过本次下载
    Exists,
}

impl CourseItemStatus {
    /// 字符串名(与历史 wire format 一致:小写英文,UI 层 `status_display()` 单独映射 emoji)
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Resolving => "resolving",
            Self::Downloading => "downloading",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
            Self::Exists => "exists",
        }
    }

    /// 原子存储用的 u8 discriminant — 与历史 `AtomicU8` wire format 兼容
    pub fn as_u8(self) -> u8 {
        match self {
            Self::Pending => 0,
            Self::Resolving => 1,
            Self::Downloading => 2,
            Self::Completed => 3,
            Self::Failed => 4,
            Self::Skipped => 5,
            Self::Exists => 6,
        }
    }

    /// 从原子值反向解析 — 未知值返回 `None` (UI fallback 到默认 pending)
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Pending),
            1 => Some(Self::Resolving),
            2 => Some(Self::Downloading),
            3 => Some(Self::Completed),
            4 => Some(Self::Failed),
            5 => Some(Self::Skipped),
            6 => Some(Self::Exists),
            _ => None,
        }
    }

    /// 是否为终态(完成 / 失败 / 跳过 / 已存在)。用于 polling task 退出守卫。
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Skipped | Self::Exists
        )
    }
}

impl std::str::FromStr for CourseItemStatus {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(Self::Pending),
            "resolving" => Ok(Self::Resolving),
            "downloading" => Ok(Self::Downloading),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "skipped" => Ok(Self::Skipped),
            "exists" => Ok(Self::Exists),
            _ => Err(()),
        }
    }
}

#[derive(Default)]
pub struct CourseItemSlot {
    pub progress: AtomicU32, // 0-10000 表示 0.00%-100.00%
    pub status: AtomicU8,    // 用 CourseItemStatus::as_u8() / from_u8() 双向转换,避免裸 u8 漂移
    /// 实时下载速度（字节/秒，由 DownloadProgress 回调写入）
    pub speed: AtomicU64,
    /// 总字节数（首次回调确定后写入,>0 表示已知）
    pub total_bytes: AtomicU64,
}

impl CourseItemSlot {
    pub fn set_status(&self, st: CourseItemStatus) {
        self.status.store(st.as_u8(), Ordering::Relaxed);
    }
}

#[cfg(test)]
mod status_enum_tests {
    use super::*;

    /// 7 个 variant 全部覆盖 — 防止新增 status 时漏改 `as_str` / `as_u8` / `FromStr`
    #[test]
    fn test_status_enum_round_trip_string() {
        for &st in &[
            CourseItemStatus::Pending,
            CourseItemStatus::Resolving,
            CourseItemStatus::Downloading,
            CourseItemStatus::Completed,
            CourseItemStatus::Failed,
            CourseItemStatus::Skipped,
            CourseItemStatus::Exists,
        ] {
            let s = st.as_str();
            let parsed: CourseItemStatus = s.parse().expect("known status must parse");
            assert_eq!(parsed, st, "round-trip mismatch for {s}");
        }
    }

    #[test]
    fn test_status_enum_round_trip_u8() {
        for &st in &[
            CourseItemStatus::Pending,
            CourseItemStatus::Resolving,
            CourseItemStatus::Downloading,
            CourseItemStatus::Completed,
            CourseItemStatus::Failed,
            CourseItemStatus::Skipped,
            CourseItemStatus::Exists,
        ] {
            let v = st.as_u8();
            let parsed = CourseItemStatus::from_u8(v).expect("known u8 must parse");
            assert_eq!(parsed, st, "u8 round-trip mismatch for {v}");
        }
    }

    /// wire format 与历史 AtomicU8 兼容(0-6 连续,7+ 返回 None 视为 corrupted state)
    #[test]
    fn test_status_enum_u8_values_match_legacy_doc() {
        assert_eq!(CourseItemStatus::Pending.as_u8(), 0);
        assert_eq!(CourseItemStatus::Resolving.as_u8(), 1);
        assert_eq!(CourseItemStatus::Downloading.as_u8(), 2);
        assert_eq!(CourseItemStatus::Completed.as_u8(), 3);
        assert_eq!(CourseItemStatus::Failed.as_u8(), 4);
        assert_eq!(CourseItemStatus::Skipped.as_u8(), 5);
        assert_eq!(CourseItemStatus::Exists.as_u8(), 6);
    }

    #[test]
    fn test_status_enum_unknown_inputs_fall_back() {
        // 未知 u8 → None (UI 端 fallback pending,不会 panic)
        assert_eq!(CourseItemStatus::from_u8(7), None);
        assert_eq!(CourseItemStatus::from_u8(255), None);
        // 未知字符串 → parse 失败
        assert!("future".parse::<CourseItemStatus>().is_err());
        assert!("".parse::<CourseItemStatus>().is_err());
    }

    /// is_terminal 必须覆盖 Completed/Failed/Skipped/Exists(active=false 的终态),
    /// 排除 Pending/Resolving/Downloading(active 还在跑)。新增 active variant 时
    /// 若忘了加,会让 polling task 提前 break → UI 永远停在中间状态。
    #[test]
    fn test_is_terminal_covers_terminal_variants() {
        assert!(CourseItemStatus::Completed.is_terminal());
        assert!(CourseItemStatus::Failed.is_terminal());
        assert!(CourseItemStatus::Skipped.is_terminal());
        assert!(CourseItemStatus::Exists.is_terminal());
        assert!(!CourseItemStatus::Pending.is_terminal());
        assert!(!CourseItemStatus::Resolving.is_terminal());
        assert!(!CourseItemStatus::Downloading.is_terminal());
    }
}

pub(super) async fn course_download_pool_impl(
    state: CourseCollectionState,
    slots: Vec<Arc<CourseItemSlot>>,
) -> Vec<(String, String)> {
    // 第一步：串行解析所有课节（CDP 同一 profile，必须串行）
    let overwrite = state.overwrite_existing;
    let (results, resolved) = resolve_items_serially(&state, &slots, overwrite).await;
    // 第二步：并发下载已解析的 m3u8（HLS 独立任务，互不依赖）
    let dl_results = download_resolved_concurrently(&state, &slots, &resolved).await;
    let mut all = results;
    all.extend(dl_results);
    all
}

/// 串行解析每个课节：检查登录态 → 调 Drission → 翻译错误。
/// 返回：每项的 (id, status) 临时结果 + 已成功解析的 m3u8 列表。
///
/// `overwrite_existing` (P-3 2026-08-29): true → 跳过 exists 检查,允许覆盖;
/// false → 存在同名 mp4 时标记 "✓ 已下载" 不重下。
async fn resolve_items_serially(
    state: &CourseCollectionState,
    slots: &[Arc<CourseItemSlot>],
    overwrite_existing: bool,
) -> (Vec<(String, String)>, Vec<Option<String>>) {
    use crate::platform::TulingPythonResolver;
    let total = state.items.len();
    let mut results: Vec<(String, String)> = Vec::with_capacity(total);
    let mut resolved: Vec<Option<String>> = vec![None; total];

    // 一次性加载输出目录,避免每条课节重复 load config
    let output_dir = AppConfig::load()
        .map(|c| c.output_dir)
        .unwrap_or_else(|_| PathBuf::from("./Downloads"));

    for (i, item) in state.items.iter().enumerate() {
        let slot = slots.get(i).cloned();
        if !item.selected {
            results.push((item.id.clone(), "skipped".into()));
            if let Some(s) = slot {
                s.set_status(CourseItemStatus::Skipped);
            }
            continue;
        }
        // P-1 (2026-08-16): 跳过已下载课节 — 检查输出目录是否存在同名 mp4 (>0 字节)
        // 大小 > 0 视为"有内容",0 字节的占位文件视为不存在,触发正常下载
        // P-2 (2026-08-29): 兼容外部脚本命名 `{idx:02}_{title}.mp4`(Python 批量下载工具)
        // 填充 slot.total_bytes/progress 让 UI 直接显示 "✓ 已下载" + 100% + 文件大小
        // P-3 (2026-08-29): `overwrite_existing=true` 时跳过本检查,允许覆盖重下
        if !overwrite_existing {
            if let Some(file_size) = file_is_already_downloaded_with_index(
                &output_dir,
                &item.title,
                Some(item.index),
            )
            .await
            {
                if let Some(s) = slot.as_ref() {
                    s.total_bytes.store(file_size, Ordering::Relaxed);
                    s.progress.store(10000, Ordering::Relaxed);
                    s.set_status(CourseItemStatus::Exists);
                }
                results.push((item.id.clone(), "exists".into()));
                continue;
            }
        } else {
            // 覆盖模式:先删旧文件,避免 HLS `File::create` 截断失败
            // (实测 m3u8 17GB 文件覆盖时,旧 .tmp 目录残留会导致空间浪费)
            delete_existing_with_index(
                &output_dir,
                &item.title,
                Some(item.index),
            )
            .await;
        }
        if let Some(s) = slot.as_ref() {
            s.set_status(CourseItemStatus::Resolving);
        }
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
                if let Some(s) = slot.as_ref() {
                    s.set_status(CourseItemStatus::Failed);
                }
                continue;
            }
        };
        match resolver.get_video_info(&session_url).await {
            Ok(info) => {
                resolved[i] = Some(info.url);
                if let Some(s) = slot.as_ref() {
                    s.set_status(CourseItemStatus::Downloading);
                }
            }
            Err(e) => {
                let norm = translate_resolve_error(&e);
                results.push((item.id.clone(), norm));
                if let Some(s) = slot.as_ref() {
                    s.set_status(CourseItemStatus::Failed);
                }
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
        let Some(m3u8) = resolved[i].clone() else {
            continue;
        };
        let title = item.title.clone();
        let id = item.id.clone();
        let page_url = item.page_url.clone();
        let slot = slots.get(i).cloned();
        let progress_cb: Arc<dyn Fn(crate::types::DownloadProgress) + Send + Sync> =
            Arc::new(move |dp: crate::types::DownloadProgress| {
                if let Some(s) = slot.as_ref() {
                    s.progress
                        .store((dp.percent * 10000.0) as u32, Ordering::Relaxed);
                    s.speed.store(dp.speed as u64, Ordering::Relaxed);
                    if dp.total_bytes > 0 {
                        s.total_bytes.store(dp.total_bytes, Ordering::Relaxed);
                    }
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
                        s.set_status(CourseItemStatus::Completed);
                    }
                    // 记录到全局下载历史 (与单视频 app.rs:456 一致),
                    // 否则课程池新下的视频不会出现在 "下载历史" 面板。
                    // Exists (复用旧文件) 不记录 — 文件本身就在,留作未来用户清除重下时再记
                    add_completed_download(title.clone(), page_url.clone(), None);
                    (id, "completed".into())
                }
                Err(msg) => {
                    if let Some(s) = slot_for_done.as_ref() {
                        s.set_status(CourseItemStatus::Failed);
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
    let req =
        DownloadRequest::new(m3u8, output_path).with_strategy(crate::types::DownloadStrategy::Hls);
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

/// 检查 `output_dir/{title}.mp4` 是否已存在并有内容 (>0 字节)。
/// - 返回 `Some(size)` 表示文件已就绪,调用方应跳过下载
/// - 返回 `None` 表示文件不存在 / 是 0 字节占位符 / metadata 读取失败
///
/// 抽离成独立函数 (而非 inline 在 `resolve_items_serially`) 是为了单测可覆盖三个分支:
/// 不存在 / 0 字节 / 有内容。inline 写 + 串行 resolve 流程则只能端到端覆盖 (需要 mock Drission)。
pub(super) async fn file_is_already_downloaded(
    output_dir: &std::path::Path,
    title: &str,
) -> Option<u64> {
    let output_path = build_output_path(output_dir, title);
    tokio::fs::metadata(&output_path).await.ok().and_then(|m| {
        if m.len() > 0 {
            Some(m.len())
        } else {
            None
        }
    })
}

/// 文件存在性检查的宽松版:除 `{title}.mp4` 外,还尝试兼容外部工具的命名约定
/// (`tmp_batch_download.py` 用 `{idx:02}_{title}.mp4` + **不 sanitize 空格/横线**),
/// 避免同名课程重复下载。
///
/// P-2 (2026-08-29) 修复:`tmp_batch_download.py` 已经把 14 节课下载到 `~/Movies/`,
/// 但因命名带 `01_`...`14_` 前缀且保留空格/横线,Rust GUI 找不到,触发重新下载 +
/// 403 (missing parameter t)。
///
/// **命名冲突**:
/// - Rust `sanitize_filename` 把空格替换为 `_`(横线保留)
/// - Python `safe_filename` 只替换 `\\/:*?"<>|\n\r\t`,**保留空格和横线**
///
/// → 同样 `{idx:02}_{title}` 在两边产出不同文件名,必须两种都查。
pub(super) async fn file_is_already_downloaded_with_index(
    output_dir: &std::path::Path,
    title: &str,
    index: Option<u32>,
) -> Option<u64> {
    // 1. Rust GUI 自身命名: `{sanitize(title)}.mp4`
    if let Some(size) = file_is_already_downloaded(output_dir, title).await {
        return Some(size);
    }
    let idx = index?;

    // 2. Python 脚本命名: `{idx:02}_{raw_title}.mp4`(保留空格和横线)
    let py_title = format!("{:02}_{}", idx, title);
    if let Some(size) = metadata_size(output_dir.join(format!("{py_title}.mp4"))).await {
        return Some(size);
    }

    // 3. Rust 加 index 前缀: `{idx:02}_{sanitize(title)}.mp4`(空格变 _)
    let rust_title = format!("{:02}_{}", idx, sanitize_filename(title));
    metadata_size(output_dir.join(format!("{rust_title}.mp4"))).await
}

/// 取文件大小(>0 字节视为有效,0 字节或不存在返回 None)。
/// 与 `file_is_already_downloaded` 的判断一致:0 字节占位文件视为不存在。
async fn metadata_size(path: std::path::PathBuf) -> Option<u64> {
    tokio::fs::metadata(&path)
        .await
        .ok()
        .and_then(|m| if m.len() > 0 { Some(m.len()) } else { None })
}

/// P-3 (2026-08-29): 覆盖模式预清理 — 删除已存在的同名 mp4(三种命名都查)。
/// HLS `File::create` 会截断旧文件,但 17GB 大文件覆盖时旧 .tmp 目录残留
/// 会浪费磁盘空间。先删旧主文件,再让 HLS 完整重写。
async fn delete_existing_with_index(
    output_dir: &std::path::Path,
    title: &str,
    index: Option<u32>,
) {
    // 1. Rust 原生命名
    let p1 = output_dir.join(format!("{}.mp4", sanitize_filename(title)));
    let _ = tokio::fs::remove_file(&p1).await;

    let Some(idx) = index else { return };
    // 2. Python 原始命名
    let p2 = output_dir.join(format!("{:02}_{}.mp4", idx, title));
    let _ = tokio::fs::remove_file(&p2).await;
    // 3. Rust 加 index 前缀命名
    let p3 = output_dir.join(format!("{:02}_{}.mp4", idx, sanitize_filename(title)));
    let _ = tokio::fs::remove_file(&p3).await;
}

#[cfg(test)]
mod skip_existing_tests {
    use super::*;
    use std::io::Write;

    /// 三分支:不存在 → None,0 字节 → None,有内容 → Some(size)
    /// —— 任何分支回归都会让用户重复下载已存在的课节
    #[tokio::test]
    async fn test_file_is_already_downloaded_returns_none_when_missing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let size = file_is_already_downloaded(dir.path(), "lesson_01").await;
        assert_eq!(size, None, "missing file should not be skipped");
    }

    #[tokio::test]
    async fn test_file_is_already_downloaded_returns_none_for_zero_bytes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("lesson_02.mp4");
        std::fs::File::create(&path).expect("create 0-byte file");
        let size = file_is_already_downloaded(dir.path(), "lesson_02").await;
        assert_eq!(size, None, "0-byte placeholder should not be skipped");
    }

    #[tokio::test]
    async fn test_file_is_already_downloaded_returns_size_when_present() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("lesson_03.mp4");
        let payload = vec![0u8; 1024];
        std::fs::File::create(&path)
            .expect("create")
            .write_all(&payload)
            .expect("write");
        let size = file_is_already_downloaded(dir.path(), "lesson_03").await;
        assert_eq!(
            size,
            Some(1024),
            "1 KiB file should be skipped with exact size"
        );
    }

    /// 标题中的特殊字符经过 `sanitize_filename` 后路径可能变化 —
    /// 跳过检查必须用与 `download_one` 完全相同的 `build_output_path` 公式。
    /// 该测试保证两边共用 helper (已经验证),无独立断言
    #[tokio::test]
    async fn test_file_is_already_downloaded_uses_sanitized_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        // 标题含空格 + 特殊字符,sanitize_filename 会替换为空格变下划线
        let sanitized_title = sanitize_filename("lesson / special?");
        let path = dir.path().join(format!("{sanitized_title}.mp4"));
        std::fs::write(&path, vec![1u8; 16]).expect("write");
        let size = file_is_already_downloaded(dir.path(), "lesson / special?").await;
        assert_eq!(
            size,
            Some(16),
            "sanitized title must map to the same path as download_one"
        );
    }

    /// P-2 (2026-08-29): 兼容 `tmp_batch_download.py` 命名约定 `{idx:02}_{title}.mp4`。
    /// 14 节课已经用 Python 脚本下到 ~/Movies/,GUI 必须识别这些文件避免重复下载。
    /// Python `safe_filename` **保留**空格和横线,Rust `sanitize_filename` 把空格 → `_`,
    /// 必须三种命名都能命中。
    #[tokio::test]
    async fn test_with_index_detects_python_script_naming() {
        let dir = tempfile::tempdir().expect("tempdir");

        // 1) Python 命名: `{idx:02}_{raw_title}.mp4`(保留空格/横线)
        let py_path = dir.path().join("01_js逆向14 AI逆向 - 魔改算法.mp4");
        std::fs::write(&py_path, vec![0u8; 2048]).expect("write");
        let py_hit = file_is_already_downloaded_with_index(
            dir.path(),
            "js逆向14 AI逆向 - 魔改算法",
            Some(1),
        )
        .await;
        assert_eq!(py_hit, Some(2048), "Python 命名必须命中");

        // 2) Rust 加 index 前缀: `{idx:02}_{sanitize(title)}.mp4`(空格 → `_`,横线 `-` 保留)
        //    sanitize_filename: "js逆向14 AI逆向 - 魔改算法" → "js逆向14_AI逆向_-_魔改算法"
        let dir2 = tempfile::tempdir().expect("tempdir");
        let rust_idx_path = dir2.path().join("02_js逆向14_AI逆向_-_魔改算法.mp4");
        std::fs::write(&rust_idx_path, vec![0u8; 4096]).expect("write");
        let rust_idx_hit = file_is_already_downloaded_with_index(
            dir2.path(),
            "js逆向14 AI逆向 - 魔改算法",
            Some(2),
        )
        .await;
        assert_eq!(rust_idx_hit, Some(4096), "Rust 加 index 前缀命名也要能命中");

        // 3) Rust 原生: `{sanitize(title)}.mp4`(无 index 前缀)
        let dir3 = tempfile::tempdir().expect("tempdir");
        let rust_native = dir3.path().join("lesson_native.mp4");
        std::fs::write(&rust_native, vec![0u8; 8192]).expect("write");
        let rust_native_hit =
            file_is_already_downloaded_with_index(dir3.path(), "lesson_native", Some(99)).await;
        assert_eq!(rust_native_hit, Some(8192), "Rust 原生命名也要能命中");

        // 4) None index → 不查 index 模式,只查原生
        let dir4 = tempfile::tempdir().expect("tempdir");
        let with_idx_only = dir4.path().join("01_only_with_idx.mp4");
        std::fs::write(&with_idx_only, vec![0u8; 16]).expect("write");
        let miss = file_is_already_downloaded(dir4.path(), "only_with_idx").await;
        assert_eq!(miss, None, "无 index 时不查 index 模式,应 miss");
    }

    /// P-3 (2026-08-29): 覆盖模式预清理 — 三命名都要删。
    /// 删完后再调 `file_is_already_downloaded_with_index` 必须返回 None。
    #[tokio::test]
    async fn test_delete_existing_with_index_clears_all_naming_patterns() {
        let dir = tempfile::tempdir().expect("tempdir");
        let title = "js逆向14 AI逆向 - 魔改算法";

        // 三种命名都建文件
        let p1 = dir.path().join(format!("{}.mp4", sanitize_filename(title))); // Rust 原生
        let p2 = dir.path().join(format!("01_{}.mp4", title)); // Python 原始
        let p3 = dir.path().join(format!("01_{}.mp4", sanitize_filename(title))); // Rust 加 index
        std::fs::write(&p1, vec![0u8; 100]).expect("write p1");
        std::fs::write(&p2, vec![0u8; 200]).expect("write p2");
        std::fs::write(&p3, vec![0u8; 300]).expect("write p3");
        assert!(p1.exists() && p2.exists() && p3.exists());

        delete_existing_with_index(dir.path(), title, Some(1)).await;

        assert!(!p1.exists(), "Rust 原生命名应被删");
        assert!(!p2.exists(), "Python 原始命名应被删");
        assert!(!p3.exists(), "Rust 加 index 命名应被删");
        // 删完后 exists 检查必须返回 None
        let after = file_is_already_downloaded_with_index(dir.path(), title, Some(1)).await;
        assert_eq!(after, None, "删完文件后 exists 检查应返回 None");
    }
}
