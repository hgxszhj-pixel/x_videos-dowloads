//! X Video Downloader GUI
//!
//! 使用 Slint 框架的 GUI 界面

use crate::config::AppConfig;
use crate::downloader::{DownloaderConfig, VideoDownloader};
use crate::fxtwitter::FxTwitterResolver;
use crate::history::{History, Platform};
use crate::i18n;
use crate::resolvers::{ResolverRegistry, TikTokResolver, DouYinResolver, BilibiliResolver};
use crate::types::{format_bytes, sanitize_filename, DownloadProgress, DownloadRequest, VideoInfo, DEFAULT_USER_AGENT};
use crate::yt_dlp::YtDlp;
use futures::future;
use std::io::{BufRead, BufReader, Write};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use tokio::runtime::Runtime;
use tracing::{info, warn};

slint::include_modules!();

/// Per-URL download progress state
#[derive(Clone)]
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
    fn new(url: String, title: String) -> Self {
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

    fn with_atoms(url: String, title: String) -> Self {
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

    fn to_json(&self) -> String {
        format!(
            r#"{{"title":{},"percent":{},"speed":{},"status":{}}}"#,
            serde_json::to_string(&self.title).unwrap_or_default(),
            self.percent,
            serde_json::to_string(&self.speed).unwrap_or_default(),
            serde_json::to_string(&self.status).unwrap_or_default()
        )
    }
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
                // Batch download mode
                let total = pool.len();
                app.set_batch_total(total as i32);
                app.set_batch_current(0);
                app.set_batch_status(format!("Downloading 0/{}", total).into());
                app.set_batch_progress(0.0);
                app.set_is_downloading(true);

                let app_weak_for_progress = app_weak.clone();
                let pool_clone = pool.clone();
                let history_clone = history.clone();
                let output_dir_clone = output_dir.clone();
                let cookie_clone = cookie.clone();

                // Use the runtime's handle to spawn tasks without consuming it
                let rt_handle = rt.clone();

                rt_handle.spawn(async move {
                        let total = pool_clone.len();
                        let mut success_count = 0;
                        let mut fail_count = 0;
                        let completed = Arc::new(AtomicU32::new(0));
                        let completion_mutex = Arc::new(tokio::sync::Mutex::new(()));

                        // Initialize progress list for all URLs (show max 3, track remaining)
                        let max_display = 3;
                        let initial_progress: Vec<UrlProgress> = pool_clone.iter().map(|(_, vi)| UrlProgress::new(vi.url.clone(), vi.title.clone())).collect();
                        let display_progress = if initial_progress.len() > max_display {
                            initial_progress.iter().take(max_display).cloned().collect()
                        } else {
                            initial_progress.clone()
                        };
                        let remaining = pool_clone.len().saturating_sub(display_progress.len()) as i32;
                        let initial_json = format!(
                            "[{}]",
                            display_progress.iter().map(|p| p.to_json()).collect::<Vec<_>>().join(",")
                        );
                        let app_init = app_weak_for_progress.clone();
                        slint::invoke_from_event_loop(move || {
                            if let Some(app) = app_init.upgrade() {
                                app.set_download_list_progress(initial_json.into());
                                app.set_download_list_remaining(remaining);
                                app.set_batch_total(total as i32);
                                app.set_batch_current(0);
                                app.set_is_downloading(true);
                            }
                        }).ok();

                        // Create all futures first then join them
                        let mut futures = Vec::new();

                        for (_idx, (url, video_info)) in pool_clone.into_iter().enumerate() {
                            let video_url = video_info.url.clone();
                            let title = video_info.title.clone();
                            let _is_direct_url = video_url.contains("video.twimg.com");
                            let output_dir_clone = output_dir_clone.clone();
                            let cookie_clone = cookie_clone.clone();
                            let history_clone = history_clone.clone();
                            let app_update = app_weak_for_progress.clone();
                            let completed_clone = completed.clone();
                            let completion_mutex_clone = completion_mutex.clone();
                            let total_clone = total;
                            let _ = _idx; // suppress unused warning

                            // Create progress tracker with atomic cells
                            let progress = UrlProgress::with_atoms(url.clone(), title.clone());
                            let progress_arc = progress.progress_arc.clone().expect("progress_arc must be Some");
                            let speed_arc = progress.speed_arc.clone().expect("speed_arc must be Some");

                            // Clone for callback and yt-dlp stderr parsing
                            let progress_for_callback = progress_arc.clone();
                            let speed_for_callback = speed_arc.clone();
                            let progress_for_ytdlp = progress_arc.clone();
                            let speed_for_ytdlp = speed_arc.clone();

                            // Start UI update polling thread for this URL
                            let app_poll = app_weak_for_progress.clone();
                            let progress_poll = progress_arc.clone();
                            let speed_poll = speed_arc.clone();
                            let title_poll = title.clone();
                            std::thread::spawn(move || {
                                let mut last_update: i32 = -1;
                                let mut consecutive_skips = 0;
                                while progress_poll.load(Ordering::Relaxed) < 10000 {
                                    std::thread::sleep(std::time::Duration::from_millis(200));
                                    let current = progress_poll.load(Ordering::Relaxed) as i32;
                                    // Skip if change is too small but limit skips to 10
                                    if last_update >= 0 && (current - last_update).abs() < 2 && consecutive_skips < 10 {
                                        consecutive_skips += 1;
                                        continue;
                                    }
                                    consecutive_skips = 0;
                                    last_update = current;
                                    let pct = current as f32 / 100.0;
                                    let speed_val = speed_poll.load(Ordering::Relaxed);
                                    let speed_str = if speed_val > 0 {
                                        format_bytes(speed_val)
                                    } else {
                                        String::new()
                                    };
                                    let app_clone = app_poll.clone();
                                    let title_clone = title_poll.clone();
                                    let pct_for_status = current as f32;
                                    let speed_str_clone = speed_str.clone();
                                    slint::invoke_from_event_loop(move || {
                                        if let Some(app) = app_clone.upgrade() {
                                            // Use batch-progress to show real-time progress (store as 0-100 integer)
                                            app.set_batch_progress(pct_for_status);
                                            app.set_download_speed(speed_str_clone.clone().into());
                                            // Try to update JSON but don't rely on it for display
                                            let current_json = app.get_download_list_progress().to_string();
                                            if let Ok(mut items) = serde_json::from_str::<Vec<serde_json::Value>>(&format!("[{}]", current_json.trim_start_matches('[').trim_end_matches(']'))) {
                                                let mut updated = false;
                                                for item in items.iter_mut() {
                                                    if let Some(title_val) = item.get("title").and_then(|t| t.as_str()) {
                                                        if title_val.contains(&title_clone) || title_clone.contains(title_val) {
                                                            item["percent"] = serde_json::Value::from(pct);
                                                            item["speed"] = serde_json::Value::from(speed_str_clone.clone());
                                                            updated = true;
                                                            break;
                                                        }
                                                    }
                                                }
                                                if updated {
                                                    let new_json = items.iter()
                                                        .map(|i| serde_json::to_string(i).unwrap_or_default())
                                                        .collect::<Vec<_>>()
                                                        .join(",");
                                                    app.set_download_list_progress(format!("[{}]", new_json).into());
                                                }
                                            }
                                        }
                                    }).ok();
                                }
                            });

                            let progress_callback: Arc<dyn Fn(crate::types::DownloadProgress) + Send + Sync> =
                                Arc::new(move |p: crate::types::DownloadProgress| {
                                    progress_for_callback.store((p.percent * 100.0) as u32, Ordering::Relaxed);
                                    speed_for_callback.store(p.speed as u64, Ordering::Relaxed);
                                });
                            let _progress_callback = progress_callback;

                            futures.push(async move {
                            // Download based on URL type
                            let is_direct = video_url.contains("video.twimg.com");
                            let download_success = if is_direct {
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
                                    let filename = sanitize_filename(&title);
                                    let output_file = output_path.join(format!("{}.mp4", filename));
                                    let request = DownloadRequest::new(&video_url, &output_file)
                                        .with_user_agent(DEFAULT_USER_AGENT.to_string());
                                    match downloader.download(&request, Some(_progress_callback)).await {
                                        Ok(r) => r.success,
                                        _ => false,
                                    }
                                } else {
                                    false
                                }
                            } else {
                                let output_path = std::path::Path::new(&output_dir_clone);
                                if !output_path.exists() {
                                    let _ = std::fs::create_dir_all(output_path);
                                }
                                let filename = sanitize_filename(&title);
                                let output_file = output_path.join(format!("{}.%(ext)s", filename));
                                let mut ytdlp = YtDlp::new();
                                if !cookie_clone.is_empty() {
                                    let cookie_path = std::path::PathBuf::from(&cookie_clone);
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
                                // Use tokio async process to capture progress from stderr
                                let child = tokio::process::Command::new(&ytdlp.executable)
                                    .args(&args)
                                    .stdout(std::process::Stdio::piped())
                                    .stderr(std::process::Stdio::piped())
                                    .spawn()
                                    .map_err(|e| anyhow::anyhow!("Failed to spawn yt-dlp: {}", e)).ok();

                                let download_status = if let Some(mut c) = child {
                                    // Take stderr before spawning async task
                                    let child_stderr = c.stderr.take();
                                    let progress_store = progress_for_ytdlp.clone();
                                    let speed_store = speed_for_ytdlp.clone();
                                    let stderr_handle = tokio::spawn(async move {
                                        if let Some(stderr) = child_stderr {
                                            use tokio::io::AsyncBufReadExt;
                                            let mut reader = tokio::io::BufReader::new(stderr);
                                            let mut line = String::new();
                                            while reader.read_line(&mut line).await.ok() != Some(0) {
                                                // Parse yt-dlp progress: `[download]  45.0% of ~2.50MB at  2.50MiB/s ETA 00:15
                                                if line.contains("[download]") && line.contains('%') {
                                                    // Extract percentage
                                                    if let Some(pct_pos) = line.find('%') {
                                                        let before_pct = &line[..pct_pos];
                                                        if let Some(space_pos) = before_pct.rfind(' ') {
                                                            if let Ok(pct) = before_pct[space_pos+1..].parse::<f64>() {
                                                                progress_store.store((pct * 100.0) as u32, Ordering::Relaxed);
                                                            }
                                                        }
                                                    }
                                                    // Extract speed - look for "at " followed by a number
                                                    if let Some(at_pos) = line.find("at ") {
                                                        let after_at = &line[at_pos + 3..];
                                                        // Find 's' at the end of speed unit (use rfind to avoid "MiB" matching first)
                                                        if let Some(s_pos) = after_at.rfind('s') {
                                                            let speed_part = after_at[..s_pos].trim();
                                                            // Parse speed value - collect leading digits and decimal
                                                            let mut num_str = String::new();
                                                            for c in speed_part.chars() {
                                                                if c.is_numeric() || c == '.' {
                                                                    num_str.push(c);
                                                                } else {
                                                                    break;
                                                                }
                                                            }
                                                            if let Ok(speed_val) = num_str.parse::<f64>() {
                                                                // Determine multiplier based on what comes after the number
                                                                let after_num = &speed_part[num_str.len()..];
                                                                let speed_bytes = if after_num.starts_with("KiB") || after_num.starts_with("KB") {
                                                                    speed_val * 1024.0
                                                                } else if after_num.starts_with("MiB") || after_num.starts_with("MB") {
                                                                    speed_val * 1024.0 * 1024.0
                                                                } else if after_num.starts_with("GiB") || after_num.starts_with("GB") {
                                                                    speed_val * 1024.0 * 1024.0 * 1024.0
                                                                } else {
                                                                    speed_val * 1024.0 // default to KiB
                                                                };
                                                                speed_store.store(speed_bytes as u64, Ordering::Relaxed);
                                                            }
                                                        }
                                                    }
                                                }
                                                line.clear();
                                            }
                                        }
                                    });

                                    // Wait for the process to complete
                                    let status = c.wait().await.map(|s| s.success()).unwrap_or(false);
                                    let _ = stderr_handle.await;
                                    // Mark as complete
                                    progress_arc.store(10000, Ordering::Relaxed);
                                    status
                                } else {
                                    false
                                };
                                download_status
                            };

                            // Record to history
                            let status = if download_success {
                                crate::history::DownloadStatus::Completed
                            } else {
                                crate::history::DownloadStatus::Failed
                            };

                            if let Ok(mut h) = history_clone.write() {
                                let _ = h.add_entry_with_all(
                                    url.clone(),
                                    Some(title.clone()),
                                    None, None, None,
                                    status,
                                    None,
                                );
                            }

                            // Update completion count and UI
                            let count = completed_clone.fetch_add(1, Ordering::Relaxed) + 1;
                            let _guard = completion_mutex_clone.lock();
                            slint::invoke_from_event_loop(move || {
                                if let Some(app) = app_update.upgrade() {
                                    app.set_batch_current(count as i32);
                                    app.set_batch_status(format!("Downloading {}/{}...", count, total_clone).into());
                                    app.set_batch_progress((count as f32 / total_clone as f32) * 100.0);
                                    // Update download-list-progress with current URL status
                                    let _status_json = if download_success { "completed" } else { "failed" };
                                    let current_progress = app.get_download_list_progress().to_string();
                                    // Parse and update (simplified - just show completed/failed)
                                    let new_progress = format!("[{}]", current_progress.trim_start_matches('[').trim_end_matches(']'));
                                    app.set_download_list_progress(new_progress.into());
                                }
                            }).ok();

                            download_success
                            });
                        }

                        // Wait for all downloads to complete
                        let results = future::join_all(futures).await;

                        // Count successes and failures
                        for result in results {
                            if result {
                                success_count += 1;
                            } else {
                                fail_count += 1;
                            }
                        }

                        // Clear pool and update UI
                    if let Ok(mut s) = shared.write() {
                        s.download_pool = Vec::new();
                    }

                    let app_finish = app_weak.clone();
                    slint::invoke_from_event_loop(move || {
                        if let Some(app) = app_finish.upgrade() {
                            app.set_is_downloading(false);
                            app.set_batch_status(format!("Done: {}/{} success, {}/{} failed", success_count, total, fail_count, total).into());
                            app.set_batch_progress(100.0);
                        }
                    }).ok();
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
                            let output_file = output_path.join(format!("{}.mp4", filename));

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
                            let output_file = output_path.join(format!("{}.mp4", filename));
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
