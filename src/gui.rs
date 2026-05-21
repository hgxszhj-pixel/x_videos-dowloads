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
use std::io::{BufRead, BufReader, Write};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use tokio::runtime::Runtime;
use tracing::{info, warn};

slint::include_modules!();

/// App 状态（跨线程共享）
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
        }
    }
}

pub fn run_gui() {
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

    let app = AppWindow::new().unwrap();

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

    // 创建 Weak 引用给后台线程使用
    let app_weak_fetch = app.as_weak();
    let app_weak_download = app.as_weak();
    let app_weak_browse = app.as_weak();
    let app_weak_clear = app.as_weak();

    // 克隆 Weak 引用给各个回调使用
    let app_weak_fetch_clone = app_weak_fetch.clone();
    let _app_weak_download_clone = app_weak_download.clone();
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
    let shared_save = shared.clone();
    let shared_tick = shared.clone();
    let shared_cancel = shared.clone();
    let shared_remove = shared.clone();
    let history_fetch = history.clone();
    let history_download = history.clone();
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
            let url = app.get_url().to_string();
            let cookie = app.get_cookie_file().to_string();
            let output_dir = app.get_output_dir().to_string();


            if validate_url(&url).is_err() {
                app.set_error_message("Please enter a valid URL".into());
                app.set_status_message("Failed to fetch video info".into());
                return;
            }

            // 更新 UI
            app.set_is_fetching(true);
            app.set_status_message("Fetching video info...".into());
            app.set_error_message("".into());
            app.set_has_video_info(false);

            // 更新 shared state
            {
                let mut s = match shared.write() {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("Lock poisoned: {}", e);
                        return;
                    }
                };
                s.url = url.clone();
                s.cookie_file = cookie.clone();
                s.output_dir = output_dir;
                s.is_fetching = true;
                s.status_message = "Fetching video info...".into();
                s.error_message = String::new();
            }

            let url_clone = url.clone();
            let cookie_clone = cookie.clone();

            // 直接使用 tokio::spawn 在现有 runtime 中执行，避免 std::thread 开销
            rt.spawn(async move {

                // 使用 ResolverRegistry 选择合适的解析器
                let mut registry = ResolverRegistry::new();
                registry.register(TikTokResolver::new());
                registry.register(BilibiliResolver::new());

                // 创建 DouYinResolver 并注入 msToken
                let mut douyin_resolver = DouYinResolver::new();
                if let Ok(config) = AppConfig::load() {
                    if let Some(ref ms_token) = config.douyin_ms_token {
                        let mut tokens = std::collections::HashMap::new();
                        tokens.insert("msToken".to_string(), ms_token.clone());
                        douyin_resolver = douyin_resolver.with_tokens(tokens);
                    }
                }
                registry.register(douyin_resolver);

                let resolver = registry.select_resolver(&url_clone);

                // 同步调用 get_video_info
                let result = resolver.get_video_info(&url_clone);

                let final_result = match result {
                    Ok(info) => Ok(info),
                    Err(e) => {
                        warn!("Resolver failed: {}, trying direct YtDlp...", e);
                        let mut ytdlp = YtDlp::new();
                        if !cookie_clone.is_empty() {
                            ytdlp = ytdlp.with_cookie_file(std::path::PathBuf::from(&cookie_clone));
                        }
                        let ytdlp_result = ytdlp.get_video_info(&url_clone);

                        match ytdlp_result {
                            Ok(info) => {
                                warn!("YtDlp fallback succeeded");
                                Ok(info)
                            }
                            Err(ytdlp_err) => {
                                warn!("YtDlp also failed: {}", ytdlp_err);
                                // 最后尝试 FxTwitter (仅对 Twitter/X 有效)
                                if url_clone.contains("twitter.com") || url_clone.contains("x.com") {
                                    let url_fx = url_clone.clone();
                                    let fx_result = FxTwitterResolver::new().get_video_info(&url_fx).await;
                                    match fx_result {
                                        Ok(info) => {
                                            warn!("FxTwitter fallback succeeded");
                                            Ok(info)
                                        }
                                        Err(fx_err) => {
                                            Err(format!("{} | YtDlp: {} | FxTwitter: {}", e, ytdlp_err, fx_err))
                                        }
                                    }
                                } else {
                                    Err(format!("{} | YtDlp: {}", e, ytdlp_err))
                                }
                            }
                        }
                    }
                };

                // 使用 invoke_from_event_loop 更新 UI
                let app_weak_clone = app_weak.clone();
                let _final_result_clone = final_result.clone();
                let _shared_clone = shared.clone();
                let _invoke_result = slint::invoke_from_event_loop(move || {
                    if let Some(app) = app_weak_clone.upgrade() {
                        match &final_result {
                            Ok(info) => {
                                app.set_video_title(info.title.clone().into());
                                app.set_video_uploader(info.uploader.clone().unwrap_or_default().into());
                                if let Some(dur) = info.duration {
                                    app.set_video_duration(format!("{}:{:02}", dur / 60, dur % 60).into());
                                }
                                app.set_video_formats(format!("{}", info.formats.len()).into());
                                app.set_has_video_info(true);
                                app.set_status_message(format!("Fetched: {}", info.title).into());
                            }
                            Err(e) => {
                                app.set_error_message(e.clone().into());
                                app.set_status_message("Failed to fetch video info".into());
                            }
                        }
                        app.set_is_fetching(false);

                        // 更新共享状态
                        let mut s = match shared.write() {
                            Ok(s) => s,
                            Err(e) => {
                                eprintln!("Lock poisoned: {}", e);
                                return;
                            }
                        };
                        s.is_fetching = false;
                        if let Ok(ref info) = final_result {
                            s.video_info = Some(info.clone());
                        } else {
                            s.video_info = None;
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
        let cookie = {
            let s = match shared.read() {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Lock poisoned: {}", e);
                    return;
                }
            };
            s.cookie_file.clone()
        };

        if let Some(app) = app_weak.upgrade() {
            let url = app.get_url().to_string();
            let output_dir = app.get_output_dir().to_string();

            if validate_url(&url).is_err() {
                app.set_error_message("Please enter a valid URL".into());
                return;
            }

            let video_info = {
                let s = match shared.read() {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("Lock poisoned: {}", e);
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
                            eprintln!("Lock poisoned: {}", e);
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

                // 启动进度更新线程 (使用 std 线程，无需 tokio)
                std::thread::spawn(move || {
                    let mut last_update: i32 = -1;  // 初始化为 -1，确保第一次更新总是执行
                    while progress_arc_for_update.load(Ordering::Relaxed) < 100 {  // 使用 100 表示 100%
                        std::thread::sleep(std::time::Duration::from_millis(100));  // 100ms 间隔，平衡实时性和性能
                        let current_progress = progress_arc_for_update.load(Ordering::Relaxed) as i32;
                        // 进度变化小于 1% 时跳过 UI 更新，减少重绘（但第一次更新总是执行）
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

                            // 使用预创建的 runtime 执行下载，避免每次创建新 runtime
                            let result = downloader.download(&request, Some(progress_callback)).await;

                            match result {
                                Ok(r) if r.success => {
                                    progress_arc.store(100, Ordering::Relaxed);
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
                        // 传递 cookie 文件到 yt-dlp
                        if !cookie.is_empty() {
                            ytdlp = ytdlp.with_cookie_file(std::path::PathBuf::from(&cookie));
                        }
                        let args = ytdlp.build_download_args(
                            &url_clone,
                            &output_file.to_string_lossy(),
                            Some("best"),
                            false,
                            false,
                        );

                        // 直接调用 yt-dlp，不使用 script 命令包装（避免 shell 注入风险）
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

                        // 异步读取 stderr 以实时获取进度
                        let progress_arc_clone = progress_arc.clone();
                        let stderr_handle = child.stderr.take();
                        let stdout_handle = child.stdout.take();
                        std::thread::spawn(move || {
                            // yt-dlp 在非 TTY 模式下可能输出到 stdout 或 stderr
                            if let Some(stderr) = stderr_handle {
                                let reader = BufReader::new(stderr);
                                for line in reader.lines().map_while(Result::ok) {
                                    if let Some(percent) = parse_ytdlp_progress(&line) {
                                        progress_arc_clone.store(percent as u32, Ordering::Relaxed);
                                    }
                                }
                            }
                            // 同时读取 stdout（某些情况下进度会输出到 stdout）
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
                                progress_arc.store(100, Ordering::Relaxed);
                                download_result = Ok(format!("Download completed: {}", title));
                            }
                            Ok(status) => {
                                download_result = Err(format!("yt-dlp exited with: {}", status));
                            }
                            Err(e) => download_result = Err(format!("yt-dlp error: {}", e)),
                        }
                    }

                    // 使用 invoke_from_event_loop 更新 UI
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
                                    // 记录到历史
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
                                        // 更新 UI 历史列表
                                        drop(h);
                                        let h = history_clone.write().unwrap();
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
                                Err(e) => {
                                    app.set_error_message(e.clone().into());
                                    app.set_status_message("Download failed".into());
                                    // 记录失败到历史
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
                                    eprintln!("Lock poisoned: {}", e);
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
    });

    // Cancel Download 回调
    app.on_cancel_download(move || {
        let shared = shared_cancel.clone();
        let mut s = match shared.write() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Lock poisoned: {}", e);
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
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Cookie Files", &["txt", "cookies", "json"])
                .set_directory("/Users/hgxszhj/Downloads")
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
                    eprintln!("Lock poisoned: {}", e);
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
                    eprintln!("Lock poisoned: {}", e);
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
                                    eprintln!("Lock poisoned: {}", e);
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
                            eprintln!("Lock poisoned: {}", e);
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

    app.run().unwrap();
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

fn validate_url(url: &str) -> Result<String, String> {
    let url = url.trim();
    if url.is_empty() {
        return Err("URL cannot be empty".to_string());
    }

    // Must start with http:// or https://
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err("URL must start with http:// or https://".to_string());
    }

    // Parse and validate URL components
    if let Ok(parsed) = url::Url::parse(url) {
        // Check for dangerous schemes
        if parsed.scheme() != "http" && parsed.scheme() != "https" {
            return Err("Only http and https schemes are allowed".to_string());
        }

        // Check host is not empty
        if parsed.host_str().is_none() {
            return Err("URL must have a valid host".to_string());
        }
    } else {
        return Err("Invalid URL format".to_string());
    }

    Ok(url.to_string())
}

/// 更新历史列表到 UI
fn update_history_list(app: &AppWindow, history: &Arc<RwLock<History>>) {
    let entries = {
        let h = match history.write() {
            Ok(h) => h,
            Err(e) => {
                eprintln!("Lock poisoned: {}", e);
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

fn parse_ytdlp_progress(line: &str) -> Option<f32> {
    // 尝试两种格式：
    // 1. 标准格式: [download]  45.0% of ~2.50MB at  1.20MiB/s ETA 00:01
    // 2. progress-template 格式: download:12345/67890 45.0%
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
