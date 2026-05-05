//! X Video Downloader GUI
//!
//! 使用 Slint 框架的 GUI 界面

use crate::downloader::{DownloaderConfig, VideoDownloader};
use crate::fxtwitter::FxTwitterResolver;
use crate::types::{format_bytes, sanitize_filename, DownloadProgress, DownloadRequest, VideoInfo, DEFAULT_USER_AGENT};
use crate::yt_dlp::YtDlp;
use std::io::{BufRead, BufReader};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
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
            download_progress_arc: None,
            cancel_flag: Arc::new(AtomicU32::new(0)),
        }
    }
}

pub fn run_gui() {
    if !YtDlp::new().is_available() {
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

    // 创建 Weak 引用给后台线程使用
    let app_weak_fetch = app.as_weak();
    let app_weak_download = app.as_weak();
    let app_weak_browse = app.as_weak();
    let app_weak_clear = app.as_weak();

    // 克隆 Weak 引用给各个回调使用
    let app_weak_fetch_clone = app_weak_fetch.clone();
    let _app_weak_download_clone = app_weak_download.clone();
    let app_weak_browse_clone = app_weak_browse.clone();
    let app_weak_clear_clone = app_weak_clear.clone();
    let app_weak_tick = app.as_weak();
    let app_weak_tick_clone = app_weak_tick.clone();

    // 初始化共享状态
    let shared = Arc::new(Mutex::new(SharedState {
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

    // 克隆给各个回调
    let shared_fetch = shared.clone();
    let shared_download = shared.clone();
    let shared_save = shared.clone();
    let shared_tick = shared.clone();
    let shared_cancel = shared.clone();

    // 创建单一的 tokio runtime (避免每次回调创建新 runtime 的开销)
    let rt = Runtime::new().expect("Failed to create tokio runtime");
    let rt = Arc::new(rt);
    // 使用 Mutex 包装以支持 FnMut 回调 (回调可能被调用多次)
    let rt_fetch = Arc::clone(&rt);
    let rt_download = Arc::clone(&rt);

    // Fetch Info 回调
    app.on_fetch_info(move || {
        let app_weak = app_weak_fetch_clone.clone();
        let shared = shared_fetch.clone();

        // 从 UI 获取 URL
        if let Some(app) = app_weak.upgrade() {
            let url = app.get_url().to_string();
            let cookie = app.get_cookie_file().to_string();
            let output_dir = app.get_output_dir().to_string();

            eprintln!("DEBUG: fetch-info - url='{}'", url);

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
                let mut s = shared.lock().unwrap();
                s.url = url.clone();
                s.cookie_file = cookie;
                s.output_dir = output_dir;
                s.is_fetching = true;
                s.status_message = "Fetching video info...".into();
                s.error_message = String::new();
            }

            let url_clone = url.clone();
            // 克隆 runtime 以在后台线程中使用
            let rt = rt_fetch.clone();

            // 在后台线程执行
            std::thread::spawn(move || {
                eprintln!("DEBUG: background thread started");
                let ytdlp = YtDlp::new();
                let result = ytdlp.get_video_info(&url_clone);
                eprintln!("DEBUG: yt-dlp result: {:?}", result.is_ok());

                let final_result = match result {
                    Ok(info) => Ok(info),
                    Err(e) => {
                        warn!("yt-dlp failed: {}, trying FxTwitter...", e);
                        let url_fx = url_clone.clone();
                        // 使用预创建的 runtime 执行 async 代码，避免每次创建新 runtime
                        let fx_result = rt.block_on(async {
                            FxTwitterResolver::new().get_video_info(&url_fx).await
                        });

                        match fx_result {
                            Ok(info) => {
                                warn!("FxTwitter fallback succeeded");
                                Ok(info)
                            }
                            Err(fx_err) => {
                                warn!("FxTwitter also failed: {}", fx_err);
                                Err(format!("{} | FxTwitter: {}", e, fx_err))
                            }
                        }
                    }
                };

                // 使用 invoke_from_event_loop 更新 UI
                eprintln!("DEBUG: about to call invoke_from_event_loop");
                let app_weak_clone = app_weak.clone();
                let _final_result_clone = final_result.clone();
                let _shared_clone = shared.clone();
                let _invoke_result = slint::invoke_from_event_loop(move || {
                    eprintln!("DEBUG: inside invoke_from_event_loop");
                    if let Some(app) = app_weak_clone.upgrade() {
                        eprintln!("DEBUG: app upgraded successfully");
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
                                eprintln!("DEBUG: setting error message: {}", e);
                                app.set_error_message(e.clone().into());
                                app.set_status_message("Failed to fetch video info".into());
                            }
                        }
                        app.set_is_fetching(false);

                        // 更新共享状态
                        let mut s = shared.lock().unwrap();
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
        let app_weak = app_weak_download.clone();
        let shared = shared_download.clone();

        if let Some(app) = app_weak.upgrade() {
            let url = app.get_url().to_string();
            let output_dir = app.get_output_dir().to_string();

            if validate_url(&url).is_err() {
                app.set_error_message("Please enter a valid URL".into());
                return;
            }

            let video_info = {
                let s = shared.lock().unwrap();
                s.video_info.clone()
            };

            if let Some(info) = video_info {
                let progress_arc = Arc::new(AtomicU32::new(0));
                let speed_arc = Arc::new(AtomicU64::new(0));

                app.set_is_downloading(true);
                app.set_download_progress(0.0);
                app.set_status_message("Connecting to server...".into());
                app.set_error_message("".into());
                app.set_download_threads("4".into());

                {
                    let mut s = shared.lock().unwrap();
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
                let app_weak_for_update = app_weak_download.clone();

                // 启动进度更新线程 (使用 std 线程，无需 tokio)
                std::thread::spawn(move || {
                    while progress_arc_for_update.load(Ordering::Relaxed) < 100 {  // 使用 100 表示 100%
                        std::thread::sleep(std::time::Duration::from_millis(1));  // 1ms 间隔，极速更新
                        let current_progress = progress_arc_for_update.load(Ordering::Relaxed) as f32 / 100.0;
                        let current_speed = speed_arc_for_update.load(Ordering::Relaxed);
                        let speed_str = if current_speed > 0 {
                            format_bytes(current_speed)
                        } else {
                            String::new()
                        };
                        let app_weak_clone = app_weak_for_update.clone();
                        slint::invoke_from_event_loop(move || {
                            if let Some(app) = app_weak_clone.upgrade() {
                                app.set_download_progress(current_progress);
                                app.set_download_speed(speed_str.into());
                            }
                        }).ok();
                    }
                });

                // 克隆 runtime 以在下载线程中使用
                let rt_for_download = rt_download.clone();
                std::thread::spawn(move || {
                    let rt = rt_for_download;

                    let output_path = std::path::Path::new(&output_dir_clone);
                    if !output_path.exists() {
                        let _ = std::fs::create_dir_all(output_path);
                    }

                    let filename = sanitize_filename(&format!("{}_{}", uploader, title));
                    let download_result: Result<String, String>;

                    if is_direct_url {
                        eprintln!("DEBUG: Using direct URL download path: {}", video_url);
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
                            let progress_callback: Arc<dyn Fn(DownloadProgress) + Send + Sync> =
                                Arc::new(move |p: DownloadProgress| {
                                    progress_for_thread.store((p.percent * 100.0) as u32, Ordering::Relaxed);
                                    speed_for_thread.store(p.speed as u64, Ordering::Relaxed);
                                });

                            // 使用预创建的 runtime 执行下载，避免每次创建新 runtime
                            let result = rt.block_on(async {
                                downloader.download(&request, Some(progress_callback)).await
                            });

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
                        let ytdlp = YtDlp::new();
                        let args = ytdlp.build_download_args(
                            &url_clone,
                            &output_file.to_string_lossy(),
                            Some("best"),
                            false,
                            false,
                        );

                        let progress_for_ytdlp = progress_arc.clone();
                        let mut child = match std::process::Command::new(&ytdlp.executable)
                            .args(&args)
                            .spawn()
                        {
                            Ok(c) => c,
                            Err(e) => {
                                eprintln!("ERROR: Failed to spawn yt-dlp: {}", e);
                                return;
                            }
                        };

                        // 异步读取 stderr 以实时获取进度（仅在 stderr 可用时）
                        if let Some(stderr) = child.stderr.take() {
                            let progress_for_ytdlp_clone = progress_for_ytdlp.clone();
                            std::thread::spawn(move || {
                                let reader = BufReader::new(stderr);
                                #[allow(clippy::unnecessary_filter_map)]
                                for line in reader.lines().filter_map(|l| l.ok()) {
                                    eprintln!("DEBUG yt-dlp stderr: {}", line);
                                    if let Some(percent) = parse_ytdlp_progress(&line) {
                                        eprintln!("DEBUG: yt-dlp parsed progress: {}%", percent);
                                        progress_for_ytdlp_clone.store(percent as u32, Ordering::Relaxed);
                                    }
                                }
                            });
                        }

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
                    slint::invoke_from_event_loop(move || {
                        if let Some(app) = app_weak_clone.upgrade() {
                            match &result_clone {
                                Ok(msg) => {
                                    app.set_status_message(msg.clone().into());
                                    app.set_download_progress(100.0);
                                }
                                Err(e) => {
                                    app.set_error_message(e.clone().into());
                                    app.set_status_message("Download failed".into());
                                }
                            }
                            app.set_is_downloading(false);

                            let mut s = shared.lock().unwrap();
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
        let mut s = shared.lock().unwrap();
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
        let cookie = shared_save.lock().unwrap().cookie_file.clone();
        let output = shared_save.lock().unwrap().output_dir.clone();
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

    // Tick 回调 - 定期更新下载进度和网络状态
    app.on_tick(move || {
        let app_weak = app_weak_tick_clone.clone();
        let shared = shared_tick.clone();

        if let Some(app) = app_weak.upgrade() {
            // 检查网络状态 - 使用百度作为检测点(国内可访问)
            let baidu_ip = std::net::SocketAddr::from(([120, 46, 59, 242], 443));
            let is_online = std::net::TcpStream::connect_timeout(&baidu_ip, std::time::Duration::from_secs(2)).is_ok();
            eprintln!("DEBUG: network check to {} = {}", baidu_ip, is_online);
            let network_status = if is_online { "Online" } else { "Offline" };
            app.set_network_status(network_status.into());

            let is_downloading = app.get_is_downloading();
            if is_downloading {
                let progress = {
                    let s = shared.lock().unwrap();
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
            std::thread::sleep(std::time::Duration::from_secs(5));
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
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err("URL must start with http:// or https://".to_string());
    }
    Ok(url.to_string())
}

fn parse_ytdlp_progress(line: &str) -> Option<f32> {
    // 解析 yt-dlp 进度输出，如: [download]  45.0% of ~2.50MB at  1.20MiB/s ETA 00:01
    if !line.contains("[download]") {
        return None;
    }
    // 查找 "XX.X%" 格式
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
    None
}
