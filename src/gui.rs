//! X Video Downloader GUI
//!
//! 使用 Slint 框架的 GUI 界面

#![allow(dead_code)]

use crate::config::AppConfig;
use crate::downloader::{DownloaderConfig, VideoDownloader};
use crate::fxtwitter::FxTwitterResolver;
use crate::history::{History, Platform};
use crate::i18n;
use crate::platform::{ResolverRegistry, TikTokResolver, DouYinResolver, BilibiliResolver};
use crate::types::{format_bytes, sanitize_filename, generate_unique_filename, DownloadProgress, DownloadRequest, VideoInfo, DEFAULT_USER_AGENT};
use crate::yt_dlp::YtDlp;
use tokio::sync::Semaphore;
use futures::future::join_all;
use std::io::{BufRead, BufReader, Write};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use tokio::runtime::Runtime;
use tracing::{info, warn};

slint::include_modules!();

/// Per-URL download progress state
#[derive(Clone)]
#[allow(dead_code)]
struct UrlProgress {
    #[allow(dead_code)]
    url: String,
    title: String,
    percent: f32,
    speed: String,
    status: String,
    progress_arc: Option<Arc<AtomicU32>>,
    speed_arc: Option<Arc<AtomicU64>>,
}

impl UrlProgress {
    fn _new(url: String, title: String) -> Self {
        Self {
            url,
            title,
            percent: 0.0,
            speed: String::new(),
            status: "pending".to_string(),
            progress_arc: None,
            speed_arc: None,
        }
    }

    fn _with_atoms(url: String, title: String) -> Self {
        Self {
            url,
            title,
            percent: 0.0,
            speed: String::new(),
            status: "downloading".to_string(),
            progress_arc: Some(Arc::new(AtomicU32::new(0))),
            speed_arc: Some(Arc::new(AtomicU64::new(0))),
        }
    }

    fn _to_json(&self) -> String {
        format!(
            r#"{{"title":{},"percent":{},"speed":{},"status":{}}}"#,
            serde_json::to_string(&self.title).unwrap_or_default(),
            self.percent,
            serde_json::to_string(&self.speed).unwrap_or_default(),
            serde_json::to_string(&self.status).unwrap_or_default()
        )
    }
}

/// 单个批量下载项的状态
#[derive(Debug, Clone, PartialEq)]
pub enum BatchItemStatus {
    Pending,
    Downloading,
    Paused,
    Completed,
    Failed(String),
    Cancelled,
}

/// 单个视频的批量下载状态项
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub(crate) struct BatchItem {
    url: String,
    title: String,
    status: BatchItemStatus,
    progress: f32,          // 0.0 - 100.0
    speed: u64,             // bytes/s
    #[allow(dead_code)]
    bytes_downloaded: u64,
    #[allow(dead_code)]
    total_bytes: u64,
    error_message: Option<String>,
    progress_arc: Arc<AtomicU32>,   // 存储 0-10000，整数
    speed_arc: Arc<AtomicU64>,
}

impl BatchItem {
    fn new(url: String, title: String) -> Self {
        Self {
            url,
            title,
            status: BatchItemStatus::Pending,
            progress: 0.0,
            speed: 0,
            bytes_downloaded: 0,
            total_bytes: 0,
            error_message: None,
            progress_arc: Arc::new(AtomicU32::new(0)),
            speed_arc: Arc::new(AtomicU64::new(0)),
        }
    }

    fn update_from_atoms(&mut self) {
        // progress_arc 存储 0-10000，需要转换回 0-1 范围 (ProgressIndicator 期望)
        let raw = self.progress_arc.load(Ordering::Relaxed) as f32;
        // raw 是 0-10000 范围 (例如 3500 表示 35%)
        // 转换为 0-1 范围
        self.progress = raw / 10000.0; // 3500/10000 = 0.35
        self.speed = self.speed_arc.load(Ordering::Relaxed);
    }

    fn status_label(&self) -> String {
        match &self.status {
            BatchItemStatus::Pending => "pending".into(),
            BatchItemStatus::Downloading => format!("{:.1}%", self.progress),
            BatchItemStatus::Paused => "paused".into(),
            BatchItemStatus::Completed => "done".into(),
            BatchItemStatus::Failed(e) => format!("failed: {}", e),
            BatchItemStatus::Cancelled => "cancelled".into(),
        }
    }
}

///批量并行下载管理器
#[derive(Debug)]
pub struct BatchManager {
    items: RwLock<Vec<BatchItem>>,
    #[allow(dead_code)]
    concurrency: usize,
    sem: Arc<Semaphore>,
}

impl BatchManager {
    pub fn new(concurrency: usize) -> Self {
        Self {
            items: RwLock::new(Vec::new()),
            concurrency,
            sem: Arc::new(Semaphore::new(concurrency)),
        }
    }

    pub fn add(&self, url: String, title: String) {
        let item = BatchItem::new(url, title);
        self.items.write().unwrap_or_else(|e| e.into_inner()).push(item);
    }

    pub(crate) fn items(&self) -> Vec<BatchItem> {
        self.items.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn update_all_from_atoms(&self) {
        let mut items = self.items.write().unwrap_or_else(|e| e.into_inner());
        for item in items.iter_mut() {
            item.update_from_atoms();
        }
    }

    pub fn stats(&self) -> BatchStats {
        let items = self.items.read().unwrap_or_else(|e| e.into_inner());
        BatchStats {
            total: items.len(),
            pending: items.iter().filter(|i| i.status == BatchItemStatus::Pending).count(),
            downloading: items.iter().filter(|i| i.status == BatchItemStatus::Downloading).count(),
            completed: items.iter().filter(|i| i.status == BatchItemStatus::Completed).count(),
            failed: items.iter().filter(|i| matches!(i.status, BatchItemStatus::Failed(_))).count(),
        }
    }

    /// 重试所有失败项
    pub fn retry_failed(&self) {
        let mut items = self.items.write().unwrap_or_else(|e| e.into_inner());
        for item in items.iter_mut() {
            if matches!(item.status, BatchItemStatus::Failed(_)) {
                item.status = BatchItemStatus::Pending;
                item.progress = 0.0;
                item.error_message = None;
                item.progress_arc.store(0, Ordering::Relaxed);
                item.speed_arc.store(0, Ordering::Relaxed);
            }
        }
    }

 /// 下载单个视频（由 start 调用）
    async fn download_single(
        item: BatchItem,
        output_dir: String,
        cookie: String,
        sem: Arc<Semaphore>,
        _app_weak: slint::Weak<AppWindow>,
    ) -> bool {

        // 获取并发许可
        let _permit = sem.acquire_owned().await;

        let video_url = item.url.clone();
        let title = item.title.clone();
        let progress_arc = item.progress_arc.clone();
        let speed_arc = item.speed_arc.clone();
        let is_direct = video_url.contains("video.twimg.com");

        let output_path = std::path::Path::new(&output_dir);
        if !output_path.exists() {
            let _ = std::fs::create_dir_all(output_path);
        }

        let filename = sanitize_filename(&title);
        let output_file = generate_unique_filename(output_path, &filename, "mp4");

        let download_success = if is_direct {
            let config = DownloaderConfig {
                user_agent: Some(DEFAULT_USER_AGENT.to_string()),
                proxy: None,
                ..Default::default()
            };
            if let Ok(downloader) = VideoDownloader::with_config(config) {
                let request = DownloadRequest::new(&video_url, &output_file)
                    .with_user_agent(DEFAULT_USER_AGENT.to_string());
                // Direct URL 下载：用 progress callback
                let progress_for_cb = progress_arc.clone();
                let speed_for_cb = speed_arc.clone();
                let progress_callback: Arc<dyn Fn(DownloadProgress) + Send + Sync> =
                    Arc::new(move |p: DownloadProgress| {
                        progress_for_cb.store((p.percent * 100.0) as u32, Ordering::Relaxed);
                        speed_for_cb.store(p.speed as u64, Ordering::Relaxed);
                    });
                match downloader.download(&request, Some(progress_callback)).await {
                    Ok(r) => r.success,
                    _ => false,
                }
            } else {
                false
            }
        } else {
            // yt-dlp 下载
            let mut ytdlp = YtDlp::new();
            if !cookie.is_empty() {
                let cookie_path = std::path::PathBuf::from(&cookie);
                if cookie_path.exists() {
                    ytdlp = ytdlp.with_cookie_file(cookie_path);
                }
            }
            let args = ytdlp.build_download_args(
                &video_url,
                &output_file.to_string_lossy(),
                Some("best"),
                false,
                false,
            );
            let child = tokio::process::Command::new(&ytdlp.executable)
                .args(&args)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn();

            match child {
                Ok(mut child) => {
                    let child_stderr = child.stderr.take();
                    let progress_store = progress_arc.clone();
                    let speed_store = speed_arc.clone();

                    let stderr_handle = tokio::spawn(async move {
                        if let Some(stderr) = child_stderr {
                            use tokio::io::AsyncBufReadExt;
                            let mut reader = tokio::io::BufReader::new(stderr);
                            let mut line = String::new();
                            while reader.read_line(&mut line).await.ok() != Some(0) {
                                if line.contains("[download]") && line.contains('%') {
                                    if let Some(pct_pos) = line.find('%') {
                                        let before_pct = &line[..pct_pos];
                                        if let Some(space_pos) = before_pct.rfind(' ') {
                                            if let Ok(pct) = before_pct[space_pos+1..].parse::<f64>() {
                                                // 确保值有效 (0-100 范围)
                                                if pct.is_finite() && pct >= 0.0 && pct <= 100.0 {
                                                    progress_store.store((pct * 100.0) as u32, Ordering::Relaxed);
                                                }
                                            }
                                        }
                                    }
                                    if let Some(at_pos) = line.find("at ") {
                                        let after_at = &line[at_pos + 3..];
                                        if let Some(s_pos) = after_at.rfind('s') {
                                            let speed_part = &after_at[..s_pos].trim();
                                            let mut num_str = String::new();
                                            for c in speed_part.chars() {
                                                if c.is_numeric() || c == '.' { num_str.push(c); }
                                            }
                                            if let Ok(speed_val) = num_str.parse::<f64>() {
                                                let mbps = speed_val * 1_000_000.0;
                                                speed_store.store(mbps as u64, Ordering::Relaxed);
                                            }
                                        }
                                    }
                                }
                                line.clear();
                            }
                        }
                    });

                    let status = child.wait().await;
                    let _ = stderr_handle.await;
                    status.map(|s| s.success()).unwrap_or(false)
                }
                Err(_) => false,
            }
        };

        download_success
    }

 /// 启动批量下载（并发控制）
    pub async fn start(
        &self,
        output_dir: String,
        cookie: String,
        app_weak: slint::Weak<AppWindow>,
    ) {
        let items = self.items.read().unwrap_or_else(|e| e.into_inner()).clone();
        let sem = self.sem.clone();

        let futures: Vec<_> = items.iter().map(|item| {
            let sem = sem.clone();
            let item = item.clone();
            let output_dir = output_dir.clone();
            let cookie = cookie.clone();
            let app_weak = app_weak.clone();
            async move {
                Self::download_single(item, output_dir, cookie, sem, app_weak).await
            }
        }).collect();

        let results = join_all(futures).await;

        // 更新完成状态
        {
            let mut items = self.items.write().unwrap_or_else(|e| e.into_inner());
            for (i, result) in results.iter().enumerate() {
                if i < items.len() {
                    items[i].status = if *result {
                        BatchItemStatus::Completed
                    } else {
                        BatchItemStatus::Failed("download failed".into())
                    };
                    items[i].progress = if *result { 100.0 } else { items[i].progress };
                }
            }
        }

        //通知 UI
        let stats = self.stats();
        let app_update = app_weak.clone();
        let failed_count = stats.failed;
        let failed_items: Vec<String> = if failed_count > 0 {
            let items = self.items.read().unwrap_or_else(|e| e.into_inner());
            items.iter()
                .filter(|i| matches!(i.status, BatchItemStatus::Failed(_)))
                .map(|i| format!("[{}] {}", i.title, i.error_message.clone().unwrap_or_default()))
                .collect()
        } else {
            Vec::new()
        };
        slint::invoke_from_event_loop(move || {
            if let Some(app) = app_update.upgrade() {
                app.set_batch_status(format!("Done: {}/{} success, {}/{} failed",
                    stats.completed, stats.total, stats.failed, stats.total).into());
                app.set_batch_progress(100.0);
                app.set_is_downloading(false);
                if failed_count > 0 {
                    let msg = format!("{} video(s) failed: {}", failed_count, failed_items.join("; "));
                    app.set_error_message(msg.into());
                }
            }
        }).ok();
    }
}

#[derive(Debug, Clone)]
pub struct BatchStats {
    pub total: usize,
    pub pending: usize,
    pub downloading: usize,
    pub completed: usize,
    pub failed: usize,
}

struct SharedState {
    url: String,
    cookie_file: String,
    output_dir: String,
    video_info: Option<VideoInfo>,
    is_fetching: bool,
    is_downloading: bool,
    download_progress: f32,
    download_speed: String,
    error_message: String,
    error_action: String,
    status_message: String,
    selected_history_id: String,
    download_progress_arc: Option<Arc<AtomicU32>>,
    cancel_flag: Arc<AtomicU32>,
    // Batch download state
    #[allow(dead_code)]
    is_batch_mode: bool,
    #[allow(dead_code)]
    batch_urls: Vec<String>,
    #[allow(dead_code)]
    batch_current: usize,
    #[allow(dead_code)]
    batch_total: usize,
    #[allow(dead_code)]
    batch_status: String,
    download_pool: Vec<(String, VideoInfo)>,
    // Per-URL progress list for multi-progress display
    #[allow(dead_code)]
    download_list_progress: Vec<UrlProgress>,
}

impl Default for SharedState {
    fn default() -> Self {
        Self {
            url: String::new(),
            cookie_file: String::new(),
            output_dir: String::new(),
            video_info: None,
            is_fetching: false,
            is_downloading: false,
            download_progress: 0.0,
            download_speed: String::new(),
            error_message: String::new(),
            error_action: String::new(),
            status_message: "Enter video URL".to_string(),
            selected_history_id: String::new(),
            download_progress_arc: None,
            cancel_flag: Arc::new(AtomicU32::new(0)),
            is_batch_mode: false,
            batch_urls: Vec::new(),
            batch_current: 0,
            batch_total: 0,
            batch_status: String::new(),
            download_pool: Vec::new(),
            download_list_progress: Vec::new(),
        }
    }
}

pub fn run_gui() {
    // 禁用 Slint i18n 警告
    std::env::set_var("SLINT_DISABLE_I18N", "1");
    // 设置默认 locale 避免 ICU4X 错误
    std::env::set_var("LANG", "en_US.UTF-8");

    // 创建日志文件以捕获调试输出
    let log_dir = directories::ProjectDirs::from("com", "x-video-downloader", "X Video Downloader")
        .map(|dirs| dirs.data_dir().to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let _ = std::fs::create_dir_all(&log_dir);
    let log_file_path = log_dir.join("gui_debug.log");

    // 写入日志的辅助函数
    fn log_to_file(file: &std::path::Path, msg: &str) {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(file)
            .and_then(|mut f| {
                writeln!(f, "[{}] {}", timestamp, msg)
            });
    }

    log_to_file(&log_file_path, "=== GUI Started ===");

    if !YtDlp::new().is_available() {
        log_to_file(&log_file_path, "ERROR: yt-dlp not found");
        eprintln!("Error: yt-dlp not found. Install with: brew install yt-dlp");
        return;
    }

    let (saved_cookie, saved_output) = load_config();
    let default_output = directories::UserDirs::new()
        .and_then(|dirs| dirs.video_dir().map(|p| p.to_string_lossy().to_string()))
        .unwrap_or_else(|| "./Downloads".to_string());

    let output_dir_value = if saved_output.is_empty() {
        default_output.clone()
    } else {
        saved_output.clone()
    };

    let app = AppWindow::new().expect("Failed to create AppWindow");

    // 初始化 i18n 并设置字体
    i18n::init();
    let _font = i18n::get_font_family();

    // 初始化 History
    let data_dir = directories::ProjectDirs::from("com", "x-video-downloader", "X Video Downloader")
        .map(|dirs| dirs.data_dir().to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let history = match History::new(&data_dir) {
        Ok(h) => h,
        Err(e) => {
            eprintln!("Failed to initialize history: {}", e);
            return;
        }
    };
    let history = Arc::new(RwLock::new(history));

    // 创建 tokio runtime handle (wrapped in Arc for cheap cloning)
    let rt = Arc::new(Runtime::new().expect("Failed to create tokio runtime"));
    let rt_handle = rt.handle().clone();
    let rt_for_fetch = Arc::new(rt_handle.clone());
    let rt_for_download = Arc::new(rt_handle.clone());
    let rt_for_batch = Arc::new(rt_handle.clone());

    // 创建 Weak 引用给后台线程使用
    let app_weak_fetch = app.as_weak();
    let app_weak_download = app.as_weak();
    let app_weak_browse = app.as_weak();
    let app_weak_clear = app.as_weak();

    // 克隆 Weak 引用给各个回调使用
    let app_weak_fetch_clone = app_weak_fetch.clone();
    let app_weak_download_clone = app_weak_download.clone();
    let app_weak_download_batch = app_weak_download.clone();
    let _ = app_weak_download_batch;
    let app_weak_browse_clone = app_weak_browse.clone();
    let app_weak_browse_bookmark = app_weak_browse.clone();
    let app_weak_browse_remove = app_weak_browse.clone();
    let app_weak_clear_clone = app_weak_clear.clone();
    let app_weak_select = app_weak_clear.clone();
    let app_weak_select_clone = app_weak_select.clone();
    let app_weak_tick = app.as_weak();
    let app_weak_tick_clone = app_weak_tick.clone();

    // 初始化共享状态
    let shared = Arc::new(RwLock::new(SharedState {
        url: String::new(),
        cookie_file: saved_cookie.clone(),
        output_dir: output_dir_value.clone(),
        ..Default::default()
    }));

    // 设置初始 UI 值
    app.set_url("".into());
    app.set_output_dir(output_dir_value.clone().into());
    app.set_cookie_file(saved_cookie.into());
    app.set_status_message("Enter video URL".into());
    app.set_error_message("".into());
    app.set_error_action("".into());
    app.set_is_fetching(false);
    app.set_is_downloading(false);
    app.set_has_video_info(false);
    app.set_download_progress(0.0);
    app.set_download_speed("".into());
    app.set_download_eta("".into());
    app.set_network_status("Checking".into());
    app.set_download_threads("".into());

    // 初始化历史列表
    app.set_history_list("".into());
    app.set_selected_tab("history".into());
    app.set_filter_status("all".into());
    app.set_filter_platform("all".into());

    // 克隆给各个回调
    let shared_fetch = shared.clone();
    let shared_download = shared.clone();
    let shared_batch = shared.clone();
    let shared_save = shared.clone();
    let shared_tick = shared.clone();
    let shared_cancel = shared.clone();
    let shared_remove = shared.clone();
    let history_fetch = history.clone();
    let history_download = history.clone();
    let history_batch = history.clone();
    let history_gui = history.clone();
    let history_select = history.clone();
    let history_bookmark = history.clone();
    let history_remove = history.clone();

    // 更新历史列表
    update_history_list(&app, &history_gui);

    // Fetch Info 回调
    app.on_fetch_info(move || {
        let rt = rt_for_fetch.clone();
        let app_weak = app_weak_fetch_clone.clone();
        let shared = shared_fetch.clone();
        let _history = history_fetch.clone();

        // 从 UI 获取 URL
        if let Some(app) = app_weak.upgrade() {
            let url_text = app.get_url().to_string();
            let cookie = app.get_cookie_file().to_string();
            let output_dir = app.get_output_dir().to_string();

            // 解析多个 URL
            let urls = split_urls(&url_text);
            if urls.is_empty() {
                app.set_error_message("Please enter a valid URL".into());
                app.set_status_message("Failed to fetch video info".into());
                return;
            }

            app.set_is_fetching(true);
            app.set_error_message("".into());
            app.set_status_message(format!("Fetching {} video(s)...", urls.len()).into());
            app.set_has_video_info(false);
            app.set_batch_total(urls.len() as i32);
            app.set_batch_current(0);

            {
                let mut s = match shared.write() {
                    Ok(s) => s,
                    Err(e) => {
                        warn!("Lock poisoned: {}", e);
                        app.set_is_fetching(false);
                        return;
                    }
                };
                s.url = url_text.clone();
                s.cookie_file = cookie.clone();
                s.output_dir = output_dir;
                s.is_fetching = true;
                s.status_message = format!("Fetching {} video(s)...", urls.len());
                s.error_message = String::new();
                s.download_pool = Vec::new();
            }

            let urls_clone = urls.clone();
            let urls_len = urls_clone.len();
            let cookie_clone = cookie.clone();
            let app_weak_for_progress = app_weak.clone();

            rt.spawn(async move {
                let mut pool: Vec<(String, VideoInfo)> = Vec::new();
                let mut failed_count = 0;

                for (idx, single_url) in urls_clone.iter().enumerate() {
                    let single_url = single_url.trim();
                    if single_url.is_empty() {
                        continue;
                    }

                    // Update progress
                    let app_update = app_weak_for_progress.clone();
                    slint::invoke_from_event_loop(move || {
                        if let Some(app) = app_update.upgrade() {
                            app.set_batch_current((idx + 1) as i32);
                            app.set_batch_status(format!("Fetching {}/{}...", idx + 1, urls_len).into());
                        }
                    }).ok();

                    // Validate URL
                    if validate_url(single_url).is_err() {
                        failed_count += 1;
                        continue;
                    }

                    // Fetch video info for this URL
                    let mut registry = ResolverRegistry::new();
                    registry.register(TikTokResolver::new());
                    registry.register(BilibiliResolver::new());

                    let mut douyin_resolver = DouYinResolver::new();
                    if let Ok(config) = AppConfig::load() {
                        if let Some(ref ms_token) = config.douyin_ms_token {
                            let mut tokens = std::collections::HashMap::new();
                            tokens.insert("msToken".to_string(), ms_token.clone());
                            douyin_resolver = douyin_resolver.with_tokens(tokens);
                        }
                    }
                    registry.register(douyin_resolver);

                    let resolver = registry.select_resolver(single_url);
                    let result = resolver.get_video_info(single_url).await;

                    let final_info = match result {
                        Ok(info) => Some(info),
                        Err(e) => {
                            warn!("Resolver failed: {}, trying direct YtDlp...", e);
                            let mut ytdlp = YtDlp::new();
                            if !cookie_clone.is_empty() {
                                ytdlp = ytdlp.with_cookie_file(std::path::PathBuf::from(&cookie_clone));
                            }
                            match ytdlp.get_video_info(single_url) {
                                Ok(info) => Some(info),
                                Err(ytdlp_err) => {
                                    warn!("YtDlp also failed: {}", ytdlp_err);
                                    if single_url.contains("twitter.com") || single_url.contains("x.com") {
                                        match FxTwitterResolver::new().get_video_info(single_url).await {
                                            Ok(info) => Some(info),
                                            Err(_) => {
                                                failed_count += 1;
                                                None
                                            }
                                        }
                                    } else {
                                        failed_count += 1;
                                        None
                                    }
                                }
                            }
                        }
                    };

                    if let Some(info) = final_info {
                        pool.push((single_url.to_string(), info));
                    }
                }

                // Update UI with final results
                let app_weak_clone = app_weak.clone();
                let pool_clone = pool.clone();
                slint::invoke_from_event_loop(move || {
                    if let Some(app) = app_weak_clone.upgrade() {
                        app.set_is_fetching(false);

                        if pool_clone.is_empty() {
                            app.set_error_message(format!("Failed to fetch any videos ({}/{} failed)", failed_count, urls_len).into());
                            app.set_status_message("Failed to fetch video info".into());
                        } else {
                            app.set_status_message(format!("Fetched: {}/{} videos", pool_clone.len(), urls_len).into());
                            app.set_has_video_info(true);
                            // Show first video info in UI
                            if let Some((_, info)) = pool_clone.first() {
                                app.set_video_title(info.title.clone().into());
                                app.set_video_uploader(info.uploader.clone().unwrap_or_default().into());
                                if let Some(dur) = info.duration {
                                    app.set_video_duration(format!("{}:{:02}", dur / 60, dur % 60).into());
                                }
                                app.set_video_formats(format!("{}", info.formats.len()).into());
                            }
                            app.set_batch_status(format!("Ready: {} videos", pool_clone.len()).into());
                        }
                        app.set_batch_progress(100.0);

                        // Store pool in shared state
                        if let Ok(mut s) = shared.write() {
                            s.download_pool = pool_clone;
                        }
                    }
                }).ok();
            });
        }
    });

    // Start Download 回调
    app.on_start_download(move || {
        let rt = rt_for_download.clone();
        let app_weak = app_weak_download.clone();
        let shared = shared_download.clone();
        let history = history_download.clone();

        if let Some(app) = app_weak.upgrade() {
            let output_dir = app.get_output_dir().to_string();

            // Check if there's a download pool
            let pool = {
                let s = match shared.read() {
                    Ok(s) => s,
                    Err(e) => {
                        warn!("Lock poisoned: {}", e);
                        return;
                    }
                };
                s.download_pool.clone()
            };

            let cookie = {
                let s = match shared.read() {
                    Ok(s) => s,
                    Err(e) => {
                        warn!("Lock poisoned: {}", e);
                        return;
                    }
                };
                s.cookie_file.clone()
            };

            if !pool.is_empty() {
                // Batch download mode (parallel via BatchManager)
                let total = pool.len();
                app.set_batch_total(total as i32);
                app.set_batch_current(0);
                app.set_batch_status(format!("Downloading 0/{}", total).into());
                app.set_batch_progress(0.0);
                app.set_is_downloading(true);

                let pool_clone = pool.clone();
                let output_dir_clone = output_dir.clone();
                let cookie_clone = cookie.clone();

                // 并发数从 SharedState 或默认值读取
                let concurrency = 3;

                // 创建 BatchManager
                let manager = Arc::new(BatchManager::new(concurrency));

                // 添加所有视频到管理器
                for (_, video_info) in pool_clone.iter().cloned() {
                    manager.add(video_info.url.clone(), video_info.title.clone());
                }

                // UI 轮询线程：每 200ms 读取所有原子量并更新 UI
                let manager_for_poll = manager.clone();
                let app_poll = app_weak.clone();
                std::thread::spawn(move || {
                    let mut last_batch_progress: i32 = -1;
                    let mut consecutive_skips = 0;
                    loop {
                        std::thread::sleep(std::time::Duration::from_millis(200));

                        manager_for_poll.update_all_from_atoms();
                        // 临时获取 items 和 stats 避免 RwLock 冲突
                        let items_for_ui = manager_for_poll.items();
                        let stats_for_ui = manager_for_poll.stats();
                        let items = items_for_ui;
                        let stats = stats_for_ui;

                        // 检查是否全部结束
                        if stats.total > 0 && stats.downloading == 0 && stats.pending == 0 {
                            break;
                        }

                        // 聚合总进度
                        let total_progress: f32 = items.iter().map(|i| i.progress).sum::<f32>() / stats.total as f32;
                        let batch_pct = (total_progress * 100.0) as i32;

                        // 节流：跳过微小变化
                        if last_batch_progress >= 0 && (batch_pct - last_batch_progress).abs() < 2 && consecutive_skips < 10 {
                            consecutive_skips += 1;
                            continue;
                        }
                        consecutive_skips = 0;
                        last_batch_progress = batch_pct;

                        // 构造 JSON 列表
                        let json_items: Vec<String> = items.iter().map(|item| {
                            let speed_str = if item.speed > 0 { format_bytes(item.speed) } else { String::new() };
                            format!(
                                r#"{{"title":{},"percent":{},"speed":{},"status":{},"error":{},"url":{}"}}"#,
                                serde_json::to_string(&item.title).unwrap_or_default(),
                                item.progress,
                                serde_json::to_string(&speed_str).unwrap_or_default(),
                                serde_json::to_string(&item.status_label()).unwrap_or_default(),
                                serde_json::to_string(&item.error_message.clone().unwrap_or_default()).unwrap_or_default(),
                                serde_json::to_string(&item.url).unwrap_or_default(),
                            )
                        }).collect();
                        let _json = format!("[{}]", json_items.join(","));

                        let app_update = app_poll.clone();
                        slint::invoke_from_event_loop(move || {
                            if let Some(app) = app_update.upgrade() {
                                app.set_batch_progress(batch_pct as f32);
                                app.set_batch_status(format!("{}/{}/{} done", stats.completed, stats.failed, stats.total).into());
                                // 填充 5 个槽
                                for (i, item) in items.iter().take(5).enumerate() {
                                    let speed_str = if item.speed > 0 { format_bytes(item.speed) } else { String::new() };
                                    let status_str = item.status_label();
                                    match i {
                                        0 => {
                                            app.set_per1_title(item.title.clone().into());
                                            app.set_per1_progress(item.progress);
                                            app.set_per1_speed(speed_str.into());
                                            app.set_per1_status(status_str.into());
                                            app.set_per1_visible(true);
                                        }
                                        1 => {
                                            app.set_per2_title(item.title.clone().into());
                                            app.set_per2_progress(item.progress);
                                            app.set_per2_speed(speed_str.into());
                                            app.set_per2_status(status_str.into());
                                            app.set_per2_visible(true);
                                        }
                                        2 => {
                                            app.set_per3_title(item.title.clone().into());
                                            app.set_per3_progress(item.progress);
                                            app.set_per3_speed(speed_str.into());
                                            app.set_per3_status(status_str.into());
                                            app.set_per3_visible(true);
                                        }
                                        3 => {
                                            app.set_per4_title(item.title.clone().into());
                                            app.set_per4_progress(item.progress);
                                            app.set_per4_speed(speed_str.into());
                                            app.set_per4_status(status_str.into());
                                            app.set_per4_visible(true);
                                        }
                                        4 => {
                                            app.set_per5_title(item.title.clone().into());
                                            app.set_per5_progress(item.progress);
                                            app.set_per5_speed(speed_str.into());
                                            app.set_per5_status(status_str.into());
                                            app.set_per5_visible(true);
                                        }
                                        _ => {}
                                    }
                                }
                                // 隐藏多余的槽
                                for i in items.len()..5 {
                                    match i {
                                        0 => app.set_per1_visible(false),
                                        1 => app.set_per2_visible(false),
                                        2 => app.set_per3_visible(false),
                                        3 => app.set_per4_visible(false),
                                        4 => app.set_per5_visible(false),
                                        _ => {}
                                    }
                                }
                            }
                        }).ok();
                    }
                });

                // 启动下载
                let manager_start = manager.clone();
                let output_dir_clone = output_dir_clone.clone();
                let cookie_clone = cookie_clone.clone();
                let _history_clone = history.clone();
                let app_start = app_weak.clone();

                rt_handle.spawn(async move {
                    manager_start.start(output_dir_clone, cookie_clone, app_start).await;
                });
            } else {
                // Single download mode (original behavior)
                let url = app.get_url().to_string();

                if validate_url(&url).is_err() {
                    app.set_error_message("Please enter a valid URL".into());
                    return;
                }

                let video_info = {
                    let s = match shared.read() {
                        Ok(s) => s,
                        Err(e) => {
                            warn!("Lock poisoned: {}", e);
                            return;
                        }
                    };
                    s.video_info.clone()
                };

                if let Some(info) = video_info {
                    let progress_arc = Arc::new(AtomicU32::new(0));
                    let speed_arc = Arc::new(AtomicU64::new(0));
                    let threads_arc = Arc::new(AtomicU32::new(0));

                    app.set_is_downloading(true);
                    app.set_download_progress(0.0);
                    app.set_status_message("Connecting to server...".into());
                    app.set_error_message("".into());
                    app.set_download_threads("4".into());

                    {
                        let mut s = match shared.write() {
                            Ok(s) => s,
                            Err(e) => {
                                warn!("Lock poisoned: {}", e);
                                return;
                            }
                        };
                        s.is_downloading = true;
                        s.download_progress = 0.0;
                        s.download_speed = String::new();
                        s.status_message = "Connecting to server...".into();
                        s.error_message = String::new();
                        s.download_progress_arc = Some(progress_arc.clone());
                    }

                    let url_clone = url.clone();
                    let video_url = info.url.clone();
                    let output_dir_clone = output_dir.clone();
                    let title = info.title.clone();
                    let uploader = info.uploader.clone().unwrap_or_default();
                    let is_direct_url = video_url.contains("video.twimg.com");
                    let progress_arc_for_update = progress_arc.clone();
                    let speed_arc_for_update = speed_arc.clone();
                    let threads_arc_for_update = threads_arc.clone();
                    let app_weak_for_update = app_weak_download.clone();

                    // 启动进度更新线程
                    std::thread::spawn(move || {
                        let mut last_update: i32 = -1;
                        while progress_arc_for_update.load(Ordering::Relaxed) < 10000 {
                            std::thread::sleep(std::time::Duration::from_millis(100));
                            let current_progress = progress_arc_for_update.load(Ordering::Relaxed) as i32;
                            if last_update >= 0 && (current_progress - last_update).abs() < 1 && current_progress < 100 {
                                continue;
                            }
                            last_update = current_progress;
                            let progress = current_progress as f32 / 100.0;
                            let current_speed = speed_arc_for_update.load(Ordering::Relaxed);
                            let current_threads = threads_arc_for_update.load(Ordering::Relaxed);
                            let speed_str = if current_speed > 0 {
                                format_bytes(current_speed)
                            } else {
                                String::new()
                            };
                            let threads_str = if current_threads > 0 {
                                current_threads.to_string()
                            } else {
                                String::new()
                            };
                            let app_weak_clone = app_weak_for_update.clone();
                            slint::invoke_from_event_loop(move || {
                                if let Some(app) = app_weak_clone.upgrade() {
                                    app.set_download_progress(progress);
                                    app.set_download_speed(speed_str.into());
                                    app.set_download_threads(threads_str.into());
                                }
                            }).ok();
                        }
                    });

                    // 使用 tokio::spawn 在现有 runtime 中执行下载
                    rt.spawn(async move {
                        let output_path = std::path::Path::new(&output_dir_clone);
                        if !output_path.exists() {
                            let _ = std::fs::create_dir_all(output_path);
                        }

                        let filename = sanitize_filename(&format!("{}_{}", uploader, title));
                        let download_result: Result<String, String>;

                        if is_direct_url {
                            let output_file = generate_unique_filename(output_path, &filename, "mp4");

                            let config = DownloaderConfig {
                                user_agent: Some(DEFAULT_USER_AGENT.to_string()),
                                proxy: None,
                                ..Default::default()
                            };

                            if let Ok(downloader) = VideoDownloader::with_config(config) {
                                let request = DownloadRequest::new(&video_url, &output_file)
                                    .with_user_agent(DEFAULT_USER_AGENT.to_string());

                                let progress_for_thread = progress_arc.clone();
                                let speed_for_thread = speed_arc.clone();
                                let threads_for_thread = threads_arc.clone();
                                let progress_callback: Arc<dyn Fn(DownloadProgress) + Send + Sync> =
                                    Arc::new(move |p: DownloadProgress| {
                                        progress_for_thread.store((p.percent * 100.0) as u32, Ordering::Relaxed);
                                        speed_for_thread.store(p.speed as u64, Ordering::Relaxed);
                                        threads_for_thread.store(p.active_threads, Ordering::Relaxed);
                                    });

                                let result = downloader.download(&request, Some(progress_callback)).await;

                                match result {
                                    Ok(r) if r.success => {
                                        progress_arc.store(10000, Ordering::Relaxed);
                                        let size_str = format_bytes(r.file_size);
                                        download_result = Ok(format!("Download completed: {} ({})", title, size_str));
                                    }
                                    Ok(r) => download_result = Err(r.error_message.unwrap_or_else(|| "Download failed".to_string())),
                                    Err(e) => download_result = Err(format!("Download error: {}", e)),
                                }
                            } else {
                                download_result = Err("Failed to create downloader".to_string());
                            }
                        } else {
                            let output_file = output_path.join(format!("{}.%(ext)s", filename));
                            let mut ytdlp = YtDlp::new();
                            if !cookie.is_empty() {
                                let cookie_path = std::path::PathBuf::from(&cookie);
                                if cookie_path.exists() {
                                    ytdlp = ytdlp.with_cookie_file(cookie_path);
                                } else {
                                    warn!("Cookie file not found: {}", cookie);
                                }
                            }
                            let args = ytdlp.build_download_args(
                                &url_clone,
                                &output_file.to_string_lossy(),
                                Some("best"),
                                false,
                                false,
                            );

                            let mut child = match std::process::Command::new(&ytdlp.executable)
                                .args(&args)
                                .stdout(std::process::Stdio::piped())
                                .stderr(std::process::Stdio::piped())
                                .spawn()
                            {
                                Ok(c) => c,
                                Err(e) => {
                                    eprintln!("ERROR: Failed to spawn yt-dlp: {}", e);
                                    return;
                                }
                            };

                            let progress_arc_clone = progress_arc.clone();
                            let stderr_handle = child.stderr.take();
                            let stdout_handle = child.stdout.take();
                            std::thread::spawn(move || {
                                if let Some(stderr) = stderr_handle {
                                    let reader = BufReader::new(stderr);
                                    for line in reader.lines().map_while(Result::ok) {
                                        if let Some(percent) = parse_ytdlp_progress(&line) {
                                            progress_arc_clone.store(percent as u32, Ordering::Relaxed);
                                        }
                                    }
                                }
                                if let Some(stdout) = stdout_handle {
                                    let reader = BufReader::new(stdout);
                                    for line in reader.lines().map_while(Result::ok) {
                                        if let Some(percent) = parse_ytdlp_progress(&line) {
                                            progress_arc_clone.store(percent as u32, Ordering::Relaxed);
                                        }
                                    }
                                }
                            });

                            match child.wait() {
                                Ok(status) if status.success() => {
                                    progress_arc.store(10000, Ordering::Relaxed);
                                    download_result = Ok(format!("Download completed: {}", title));
                                }
                                Ok(status) => {
                                    download_result = Err(format!("yt-dlp exited with: {}", status));
                                }
                                Err(e) => download_result = Err(format!("yt-dlp error: {}", e)),
                            }
                        }

                        let app_weak_clone = app_weak.clone();
                        let result_clone = download_result.clone();
                        let history_clone = history.clone();
                        let url_for_history = url_clone.clone();
                        let title_for_history = title.clone();
                        slint::invoke_from_event_loop(move || {
                            if let Some(app) = app_weak_clone.upgrade() {
                                match &result_clone {
                                    Ok(msg) => {
                                        app.set_status_message(msg.clone().into());
                                        app.set_download_progress(100.0);
                                        if let Ok(mut h) = history_clone.write() {
                                            let _ = h.add_entry_with_all(
                                                url_for_history.clone(),
                                                Some(title_for_history.clone()),
                                                None,
                                                None,
                                                None,
                                                crate::history::DownloadStatus::Completed,
                                                None,
                                            );
                                            let entries = h.get_all().to_vec();
                                            let display: Vec<String> = entries.iter().take(20).map(|e| {
                                                let title = e.title.as_deref().unwrap_or("Untitled").trim();
                                                let platform = match e.platform {
                                                    crate::history::Platform::X => "[X]",
                                                    crate::history::Platform::YouTube => "[YT]",
                                                    crate::history::Platform::TikTok => "[TT]",
                                                    crate::history::Platform::DouYin => "[DY]",
                                                    crate::history::Platform::Bilibili => "[B]",
                                                    crate::history::Platform::Instagram => "[IG]",
                                                    crate::history::Platform::Facebook => "[FB]",
                                                    crate::history::Platform::Weibo => "[WB]",
                                                    Platform::ZhiXueYun => "[ZX]",
                                                    crate::history::Platform::Other => "[*]",
                                                };
                                                format!("{} {} - {}", platform, title, e.downloaded_at.format("%m-%d %H:%M"))
                                            }).collect();
                                            app.set_history_list(display.join("\n").into());
                                        }
                                    }
                                    Err(e) => {
                                        app.set_error_message(e.clone().into());
                                        app.set_status_message("Download failed".into());
                                        if let Ok(mut h) = history_clone.write() {
                                            let _ = h.add_entry_with_all(
                                                url_for_history.clone(),
                                                Some(title_for_history.clone()),
                                                None,
                                                None,
                                                None,
                                                crate::history::DownloadStatus::Failed,
                                                Some(e.clone()),
                                            );
                                        }
                                    }
                                }
                                app.set_is_downloading(false);

                                let mut s = match shared.write() {
                                    Ok(s) => s,
                                    Err(e) => {
                                        warn!("Lock poisoned: {}", e);
                                        return;
                                    }
                                };
                                s.is_downloading = false;
                                s.download_progress = 0.0;
                            }
                        }).ok();

                        info!("Download result: {:?}", download_result);
                    });
                }
            }
        }
    });

    // Start Batch Download 回调
    app.on_start_batch_download(move || {
        let rt = rt_for_batch.clone();
        let app_weak = app_weak_download_clone.clone();
        let _shared = shared_batch.clone();
        let history = history_batch.clone();

        if let Some(app) = app_weak.upgrade() {
            let pool_json = app.get_download_pool().to_string();
            let output_dir = app.get_output_dir().to_string();

            // Parse video info from download pool
            let pool: Vec<VideoInfo> = match serde_json::from_str(&pool_json) {
                Ok(pool) => pool,
                Err(_) => {
                    app.set_error_message("No download pool. Please fetch info first.".into());
                    return;
                }
            };

            if pool.is_empty() {
                app.set_error_message("Download pool is empty. Please fetch info first.".into());
                return;
            }

            let total = pool.len();
            app.set_batch_total(total as i32);
            app.set_batch_current(0);
            app.set_batch_status(format!("Processing 0/{}", total).into());
            app.set_batch_progress(0.0);
            app.set_is_downloading(true);
            drop(app);

            let app_weak_for_update = app_weak.clone();
            rt.spawn(async move {
                let mut success_count = 0;
                let mut fail_count = 0;

                for (idx, video_info) in pool.into_iter().enumerate() {
                    // Update current progress
                    let current = idx + 1;
                    let app_weak_clone = app_weak_for_update.clone();
                    let total_clone = total;
                    slint::invoke_from_event_loop(move || {
                        if let Some(app) = app_weak_clone.upgrade() {
                            app.set_batch_current(current as i32);
                            app.set_batch_status(format!("Processing {}/{}", current, total_clone).into());
                            app.set_batch_progress((current as f32 / total_clone as f32) * 100.0);
                        }
                    }).ok();

                    // Execute download using pre-fetched video info
                    let output_dir_clone = output_dir.clone();
                    let url_for_history = video_info.url.clone();
                    let title_for_history = video_info.title.clone();
                    let video_url = video_info.url.clone();
                    let is_direct_url = video_url.contains("video.twimg.com");

                    // Download based on URL type
                    let download_success = if is_direct_url {
                        let config = DownloaderConfig {
                            user_agent: Some(DEFAULT_USER_AGENT.to_string()),
                            proxy: None,
                            ..Default::default()
                        };
                        if let Ok(downloader) = VideoDownloader::with_config(config) {
                            let output_path = std::path::Path::new(&output_dir_clone);
                            if !output_path.exists() {
                                let _ = std::fs::create_dir_all(output_path);
                            }
                            let filename = sanitize_filename(&title_for_history);
                            let output_file = generate_unique_filename(output_path, &filename, "mp4");
                            let request = DownloadRequest::new(&video_url, &output_file)
                                .with_user_agent(DEFAULT_USER_AGENT.to_string());
                            match downloader.download(&request, None).await {
                                Ok(r) => r.success,
                                _ => false,
                            }
                        } else {
                            false
                        }
                    } else {
                        // yt-dlp fallback
                        let output_path = std::path::Path::new(&output_dir_clone);
                        if !output_path.exists() {
                            let _ = std::fs::create_dir_all(output_path);
                        }
                        let filename = sanitize_filename(&title_for_history);
                        let output_file = output_path.join(format!("{}.%(ext)s", filename));
                        let ytdlp = YtDlp::new();
                        let args = ytdlp.build_download_args(
                            &video_url,
                            &output_file.to_string_lossy(),
                            Some("best"),
                            false,
                            false,
                        );
                        std::process::Command::new(&ytdlp.executable)
                            .args(&args)
                            .output()
                            .map(|o| o.status.success())
                            .unwrap_or(false)
                    };

                    // Record to history
                    let status = if download_success {
                        success_count += 1;
                        crate::history::DownloadStatus::Completed
                    } else {
                        fail_count += 1;
                        crate::history::DownloadStatus::Failed
                    };

                    if let Ok(mut h) = history.write() {
                        let _ = h.add_entry_with_all(
                            url_for_history,
                            Some(title_for_history),
                            None, None, None,
                            status,
                            None,
                        );
                    }
                }

                // Finish, update UI
                let app_weak_finish = app_weak.clone();
                let total_finish = total;
                slint::invoke_from_event_loop(move || {
                    if let Some(app) = app_weak_finish.upgrade() {
                        app.set_is_downloading(false);
                        app.set_batch_status(format!("Done: {}/{} success, {}/{} failed", success_count, total_finish, fail_count, total_finish).into());
                        app.set_batch_progress(100.0);
                    }
                }).ok();
            });
        }
    });

    // Cancel Download 回调
    app.on_cancel_download(move || {
        let shared = shared_cancel.clone();
        let mut s = match shared.write() {
            Ok(s) => s,
            Err(e) => {
                warn!("Lock poisoned: {}", e);
                return;
            }
        };
        if s.is_downloading {
            s.cancel_flag.store(1, Ordering::Relaxed);
            s.is_downloading = false;
            s.status_message = "Download cancelled".to_string();
            s.error_message = "Download was cancelled by user".to_string();
            s.error_action = "".to_string();
        }
    });

    // Browse Cookie File 回调
    app.on_browse_cookie(move || {
        if let Some(app) = app_weak_browse_clone.upgrade() {
            let default_dir = directories::UserDirs::new()
                .and_then(|d| d.download_dir().map(|p| p.to_path_buf()))
                .unwrap_or_else(|| std::path::PathBuf::from("."));
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Cookie Files", &["txt", "cookies", "json"])
                .set_directory(default_dir)
                .pick_file()
            {
                let path_str = path.to_string_lossy().to_string();
                app.set_cookie_file(slint::SharedString::from(path_str.as_str()));
            }
        }
    });

    // Save Config 回调
    app.on_save_config(move || {
        let (cookie, output) = {
            let s = match shared_save.read() {
                Ok(s) => s,
                Err(e) => {
                    warn!("Lock poisoned: {}", e);
                    return;
                }
            };
            (s.cookie_file.clone(), s.output_dir.clone())
        };
        if let Err(e) = save_config_to_file(&cookie, &output) {
            eprintln!("Save config error: {}", e);
        }
    });

    // Clear URL 回调
    app.on_clear_url(move || {
        if let Some(app) = app_weak_clear_clone.upgrade() {
            app.set_url("".into());
            app.set_status_message("Enter video URL".into());
            app.set_has_video_info(false);
            app.set_error_message("".into());
            app.set_error_action("".into());
        }
    });

    // Select History Item 回调
    app.on_select_history(move |id| {
        let app_weak = app_weak_select_clone.clone();
        let history = history_select.clone();
        if let Some(app) = app_weak.upgrade() {
            // 记录选中的 ID
            app.set_selected_history_id(id.clone());
            if let Ok(h) = history.write() {
                if let Ok(uuid) = uuid::Uuid::parse_str(&id) {
                    if let Some(entry) = h.get_all().iter().find(|e| e.id == uuid) {
                        app.set_url(entry.url.clone().into());
                        app.set_status_message("Loaded from history".into());
                        app.set_has_video_info(false);
                    }
                }
            }
        }
    });

    // Add Bookmark 回调
    app.on_add_bookmark(move || {
        let app_weak = app_weak_browse_bookmark.clone();
        let history = history_bookmark.clone();
        if let Some(app) = app_weak.upgrade() {
            let url = app.get_url().to_string();
            if !url.is_empty() {
                if let Ok(mut h) = history.write() {
                    let title = app.get_video_title().to_string();
                    if h.add_bookmark(url, Some(title), None).is_ok() {
                        app.set_status_message("Bookmark added".into());
                    } else {
                        app.set_error_message("URL already bookmarked".into());
                    }
                }
            }
        }
    });

    // Remove History 回调
    app.on_remove_history(move |_id| {
        let app_weak = app_weak_browse_remove.clone();
        let history = history_remove.clone();
        let shared = shared_remove.clone();
        let selected_id = {
            let s = match shared.read() {
                Ok(s) => s,
                Err(e) => {
                    warn!("Lock poisoned: {}", e);
                    return;
                }
            };
            s.selected_history_id.clone()
        };
        if !selected_id.is_empty() {
            if let Ok(uuid) = uuid::Uuid::parse_str(&selected_id) {
                if let Ok(mut h) = history.write() {
                    if h.remove_entry(uuid) {
                        if let Some(app) = app_weak.upgrade() {
                            app.set_status_message("Item removed".into());
                            // 清空选中
                            let mut s = match shared.write() {
                                Ok(s) => s,
                                Err(e) => {
                                    warn!("Lock poisoned: {}", e);
                                    return;
                                }
                            };
                            s.selected_history_id = String::new();
                            // 刷新列表
                            let entries = h.get_all().to_vec();
                            drop(h);
                            let display: Vec<String> = entries.iter().take(20).map(|e| {
                                let title = e.title.as_deref().unwrap_or("Untitled").trim();
                                let platform = match e.platform {
                                    crate::history::Platform::X => "[X]",
                                    crate::history::Platform::YouTube => "[YT]",
                                    crate::history::Platform::TikTok => "[TT]",
                                    crate::history::Platform::DouYin => "[DY]",
                                    crate::history::Platform::Bilibili => "[B]",
                                    crate::history::Platform::Instagram => "[IG]",
                                    crate::history::Platform::Facebook => "[FB]",
                                    crate::history::Platform::Weibo => "[WB]",
                                                Platform::ZhiXueYun => "[ZX]",
                                    crate::history::Platform::Other => "[*]",
                                };
                                format!("{} {} - {}", platform, title, e.downloaded_at.format("%m-%d %H:%M"))
                            }).collect();
                            app.set_history_list(display.join("\n").into());
                        }
                    }
                }
            }
        }
    });

    // Tick 回调 - 定期更新下载进度和网络状态
    app.on_tick(move || {
        let app_weak = app_weak_tick_clone.clone();
        let shared = shared_tick.clone();

        if let Some(app) = app_weak.upgrade() {
            // 检查网络状态 - 使用百度作为检测点(国内可访问)
            let baidu_ip = std::net::SocketAddr::from(([120, 46, 59, 242], 443));
            let is_online = std::net::TcpStream::connect_timeout(&baidu_ip, std::time::Duration::from_secs(2)).is_ok();
            let network_status = if is_online { "Online" } else { "Offline" };
            app.set_network_status(network_status.into());

            let is_downloading = app.get_is_downloading();
            if is_downloading {
                let progress = {
                    let s = match shared.read() {
                        Ok(s) => s,
                        Err(e) => {
                            warn!("Lock poisoned: {}", e);
                            return;
                        }
                    };
                    s.download_progress_arc
                        .as_ref()
                        .map(|arc| arc.load(Ordering::Relaxed) as f32 / 100.0)
                        .unwrap_or(0.0)
                };
                app.set_download_progress(progress);
            }
        }
    });

    // 启动定时器定期更新网络状态
    let app_weak_for_timer = app.as_weak();
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(std::time::Duration::from_secs(30));
            let app_weak_clone = app_weak_for_timer.clone();
            slint::invoke_from_event_loop(move || {
                if let Some(app) = app_weak_clone.upgrade() {
                    app.invoke_tick();
                }
            }).ok();
        }
    });

    app.run().expect("Failed to run GUI application")
}

fn load_config() -> (String, String) {
    let config_file = config_path();
    if config_file.exists() {
        if let Ok(content) = std::fs::read_to_string(&config_file) {
            if let Ok(config) = serde_json::from_str::<serde_json::Value>(&content) {
                let cookie = config
                    .get("cookie_file")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_default();
                let output_dir = config
                    .get("output_dir")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_default();
                return (cookie, output_dir);
            }
        }
    }
    (String::new(), String::new())
}

fn config_path() -> std::path::PathBuf {
    directories::ProjectDirs::from("com", "x-video-downloader", "X Video Downloader")
        .map(|dirs| dirs.config_dir().to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("gui_config.json")
}

fn save_config_to_file(cookie_file: &str, output_dir: &str) -> Result<(), String> {
    let config_file = config_path();
    if let Some(parent) = config_file.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let config = serde_json::json!({
        "cookie_file": cookie_file,
        "output_dir": output_dir
    });
    std::fs::write(
        &config_file,
        serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// 验证单个 URL
fn validate_url(url: &str) -> Result<String, String> {
    let url = url.trim();
    if url.is_empty() {
        return Err("URL cannot be empty".to_string());
    }

    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err("URL must start with http:// or https://".to_string());
    }

    if let Ok(parsed) = url::Url::parse(url) {
        if parsed.scheme() != "http" && parsed.scheme() != "https" {
            return Err("Only http and https schemes are allowed".to_string());
        }
        if parsed.host_str().is_none() {
            return Err("URL must have a valid host".to_string());
        }
    } else {
        return Err("Invalid URL format".to_string());
    }

    Ok(url.to_string())
}

fn split_urls(url_text: &str) -> Vec<String> {
    url_text
        .split([' ', ','])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}

fn parse_ytdlp_progress(line: &str) -> Option<f32> {
    if line.contains("[download]") || line.starts_with("download:") {
        for part in line.split_whitespace() {
            let part = part.trim();
            if part.ends_with('%') && !part.starts_with('%') {
                if let Ok(percent) = part.trim_end_matches('%').parse::<f32>() {
                    if (0.0..=100.0).contains(&percent) {
                        return Some(percent);
                    }
                }
            }
        }
    }
    None
}

/// 更新历史列表到 UI
fn update_history_list(app: &AppWindow, history: &Arc<RwLock<History>>) {
    let entries = {
        let h = match history.write() {
            Ok(h) => h,
            Err(e) => {
                warn!("Lock poisoned: {}", e);
                return;
            }
        };
        h.get_all().to_vec()
    };

    if entries.is_empty() {
        app.set_history_list("No items".into());
        return;
    }

    // 简化为文本显示（实际项目可用 ListView）
    let display: Vec<String> = entries.iter().take(20).map(|e| {
        let title = e.title.as_deref().unwrap_or("Untitled").trim();
        let platform = match e.platform {
            Platform::X => "[X]",
            Platform::YouTube => "[YT]",
            Platform::TikTok => "[TT]",
            Platform::DouYin => "[DY]",
            Platform::Bilibili => "[B]",
            Platform::Instagram => "[IG]",
            Platform::Facebook => "[FB]",
            Platform::Weibo => "[WB]",
                                                Platform::ZhiXueYun => "[ZX]",
            Platform::Other => "[*]",
        };
        format!("{} {} - {}", platform, title, e.downloaded_at.format("%m-%d %H:%M"))
    }).collect();

    // 使用双换行符分隔，每个条目占一行
    app.set_history_list(display.join("\n").into());
}
