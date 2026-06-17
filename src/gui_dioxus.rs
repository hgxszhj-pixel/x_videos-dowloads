//! Dioxus GUI 入口
//!
//! 迁移自 gui.rs 的 Slint 实现

use dioxus::prelude::*;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use tracing::{error, info, warn};
use notify::{Watcher, RecursiveMode};

use crate::downloader::VideoDownloader;
use crate::types::DownloadProgress;
use crate::history::History;
use crate::platform::{ResolverRegistry, TikTokResolver, DouYinResolver, BilibiliResolver};
use crate::yt_dlp::YtDlp;
use crate::fxtwitter::FxTwitterResolver;
use crate::config::AppConfig;
use crate::types::{DownloadRequest};

/// 配置热重载状态
static CONFIG_VERSION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// 重新加载配置并触发版本递增
fn reload_config() {
    if let Ok(_config) = AppConfig::load() {
        // 更新全局配置版本，UI 会检测到变化
        CONFIG_VERSION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        info!("Configuration reloaded successfully");
    }
}

/// 发送系统通知 (使用 macOS osascript)
fn send_notification(title: &str, body: &str) {
    let script = format!(
        "display notification \"{}\" with title \"{}\"",
        body.replace("\"", "\\\""),
        title.replace("\"", "\\\"")
    );
    if let Err(e) = std::process::Command::new("osascript")
        .args(["-e", &script])
        .output()
    {
        warn!("Failed to send notification: {}", e);
    }
}

/// 运行 Dioxus GUI
pub fn run_gui() {
    info!("Starting Dioxus GUI...");

    // 检查 yt-dlp
    if !YtDlp::new().is_available() {
        error!("yt-dlp not found");
        eprintln!("Error: yt-dlp not found. Install with: brew install yt-dlp");
        return;
    }

    // 初始化 History
    let data_dir = directories::ProjectDirs::from("com", "x-video-downloader", "X Video Downloader")
        .map(|dirs| dirs.data_dir().to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let _history = match History::new(&data_dir) {
        Ok(h) => h,
        Err(e) => {
            error!("Failed to initialize history: {}", e);
            eprintln!("Failed to initialize history: {}", e);
            return;
        }
    };

    // 启动 Dioxus Desktop
    launch(AppComponent);

    // 注意：文件监控在 AppComponent 内部启动
    // 这里只是记录启动成功
    info!("Dioxus GUI started with config hot-reload support");
}

/// 主题配置
#[derive(Default, Clone, PartialEq)]
pub enum AppTheme {
    #[default]
    Purple,      // 紫色渐变
    Ocean,       // 海洋蓝
    Sunset,      // 日落橙
    Forest,      // 森林绿
}

#[allow(dead_code)]
impl AppTheme {
    pub fn gradient(&self) -> &'static str {
        match self {
            AppTheme::Purple => "linear-gradient(135deg, #667eea 0%, #764ba2 100%)",
            AppTheme::Ocean => "linear-gradient(135deg, #0066ff 0%, #00d4ff 100%)",
            AppTheme::Sunset => "linear-gradient(135deg, #ff6b6b 0%, #ffa502 100%)",
            AppTheme::Forest => "linear-gradient(135deg, #11998e 0%, #38ef7d 100%)",
        }
    }
    /// 主背景色
    pub fn background_color(&self) -> &'static str {
        match self {
            AppTheme::Purple => "#1a1a2e",
            AppTheme::Ocean => "#0a192f",
            AppTheme::Sunset => "#2d1f1f",
            AppTheme::Forest => "#0d261f",
        }
    }
    /// 卡片/容器背景色
    pub fn card_background(&self) -> &'static str {
        match self {
            AppTheme::Purple => "rgba(102, 126, 234, 0.15)",
            AppTheme::Ocean => "rgba(0, 102, 255, 0.15)",
            AppTheme::Sunset => "rgba(255, 107, 107, 0.15)",
            AppTheme::Forest => "rgba(17, 153, 142, 0.15)",
        }
    }
    /// 文字颜色
    pub fn text_color(&self) -> &'static str {
        match self {
            AppTheme::Purple => "#e8e8e8",
            AppTheme::Ocean => "#e0e0e0",
            AppTheme::Sunset => "#f0e0e0",
            AppTheme::Forest => "#e0f0ed",
        }
    }
}

/// 下载历史记录项
#[allow(dead_code)]
#[derive(Default, Clone)]
pub struct HistoryItem {
    pub title: String,
    pub uploader: String,
    pub duration: String,
    pub path: String,
    pub completed_at: String,
}

/// Video info for display
#[derive(Default, Clone)]
pub struct VideoInfoDisplay {
    pub url: String,
    pub title: String,
    pub uploader: String,
    pub duration: String,
    pub formats: Vec<VideoFormatDisplay>,  // 存储可选择的格式列表
}

#[derive(Default, Clone)]
pub struct VideoFormatDisplay {
    pub format_id: String,
    pub resolution: String,
    pub ext: String,
    pub filesize: String,
}

/// 格式化文件大小
fn format_file_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            url: String::new(),
            status_message: "Ready".to_string(),
            error_message: String::new(),
            is_fetching: false,
            is_downloading: false,
            has_video_info: false,
            download_progress: 0.0,
            download_speed: String::new(),
            is_batch_mode: false,
            batch_state: BatchState::default(),
            batch_video_state: BatchVideoState::default(),
            theme: AppTheme::default(),
            history: Vec::new(),
            show_history: false,
            selected_format_id: None,
        }
    }
}

#[allow(dead_code)]
pub struct AppState {
    pub url: String,
    pub status_message: String,
    pub error_message: String,
    pub is_fetching: bool,
    pub is_downloading: bool,
    pub has_video_info: bool,
    pub download_progress: f32,
    pub download_speed: String,
    pub is_batch_mode: bool,
    pub batch_state: BatchState,
    pub batch_video_state: BatchVideoState,
    pub theme: AppTheme,
    pub history: Vec<HistoryItem>,
    pub show_history: bool,
    pub selected_format_id: Option<String>,  // 用户选择的视频格式ID
}

/// 错误信息结构
#[allow(dead_code)]
#[derive(Default, Clone)]
pub struct ErrorDisplay {
    pub error_type: String,   // "network" | "auth" | "parse" | "download" | "unknown"
    pub message: String,       // 具体错误信息
    pub suggestion: String,    // 解决建议
}

/// 单个批量下载项的状态
#[allow(dead_code)]
#[derive(Clone)]
struct BatchItemState {
    url: String,
    title: String,
    status: String,        // "pending" | "downloading" | "completed" | "failed"
    progress: f32,         // 0.0 - 1.0
    speed: String,
    visible: bool,
    // 原子进度追踪（跨 task 共享）
    progress_arc: Option<Arc<AtomicU32>>, // 0-10000 表示 0.00%-100.00%
    speed_arc: Option<Arc<AtomicU64>>, // 字节/秒
}

/// 批量下载进度状态
#[allow(dead_code)]
#[derive(Default, Clone)]
pub(crate) struct BatchState {
    visible: bool,
    progress: f32,         // 总体进度
    status: String,
    items: Vec<BatchItemState>,
    is_downloading: bool,
}

/// 批量视频信息状态
#[allow(dead_code)]
#[derive(Default, Clone)]
pub(crate) struct BatchVideoState {
    visible: bool,
    items: Vec<VideoInfoDisplay>,
    status: String,
}

/// 主应用组件
#[component]
fn AppComponent() -> Element {
    let mut state = use_signal(AppState::default);
    let video_info = use_signal(VideoInfoDisplay::default);

    // 启动配置文件监控
    use_effect(move || {
        let config_path = AppConfig::config_path();
        if let Some(path) = config_path {
            if let Some(parent) = path.parent() {
                let parent = parent.to_path_buf();
                std::thread::spawn(move || {
                    let (tx, rx) = std::sync::mpsc::channel();
                    let mut watcher = notify::recommended_watcher(move |res: Result<notify::Event, _>| {
                        if res.is_ok() {
                            let _ = tx.send(());
                        }
                    }).ok();

                    if let Some(ref mut w) = watcher {
                        let _ = w.watch(&parent, RecursiveMode::NonRecursive);
                    }

                    for _ in rx {
                        reload_config();
                    }
                });
            }
        }
    });

    rsx! {
        div {
            style: "font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; padding: 24px; background: {state.read().theme.background_color()}; min-height: 100vh; transition: background-color 0.3s ease;",
            div {
                style: "display: flex; justify-content: space-between; align-items: center; margin-bottom: 20px; padding: 16px 20px; background: {state.read().theme.card_background()}; border-radius: 16px; box-shadow: 0 4px 20px rgba(0,0,0,0.15);",
                div {
                    style: "display: flex; align-items: center; gap: 12px;",
                    div {
                        style: "font-size: 22px; font-weight: 700; background: {state.read().theme.gradient()}; -webkit-background-clip: text; -webkit-text-fill-color: transparent;",
                        "🎬 X Video Downloader"
                    }
                }
                div {
                    style: "display: flex; gap: 8px; align-items: center;",
                    div {
                        style: "display: flex; gap: 4px;",
                        button {
                            onclick: move |_| { state.write().theme = AppTheme::Purple; },
                            style: "width: 28px; height: 28px; border-radius: 50%; border: 2px solid #667eea; background: linear-gradient(135deg, #667eea, #764ba2); cursor: pointer;",
                        }
                        button {
                            onclick: move |_| { state.write().theme = AppTheme::Ocean; },
                            style: "width: 28px; height: 28px; border-radius: 50%; border: 2px solid #0066ff; background: linear-gradient(135deg, #0066ff, #00d4ff); cursor: pointer;",
                        }
                        button {
                            onclick: move |_| { state.write().theme = AppTheme::Sunset; },
                            style: "width: 28px; height: 28px; border-radius: 50%; border: 2px solid #ff6b6b; background: linear-gradient(135deg, #ff6b6b, #ffa502); cursor: pointer;",
                        }
                        button {
                            onclick: move |_| { state.write().theme = AppTheme::Forest; },
                            style: "width: 28px; height: 28px; border-radius: 50%; border: 2px solid #11998e; background: linear-gradient(135deg, #11998e, #38ef7d); cursor: pointer;",
                        }
                    }
                    // 历史记录按钮
                    button {
                        onclick: move |_| {
                            let current = state.read().show_history;
                            state.write().show_history = !current;
                        },
                        style: "padding: 8px 14px; background: #f5f5f5; color: #666; border: none; border-radius: 20px; font-size: 12px; font-weight: 600; cursor: pointer; transition: all 0.3s ease;",
                        "📜 历史"
                    }
                    div {
                        style: "font-size: 12px; color: #888; padding: 6px 12px; background: #f0f0f0; border-radius: 20px;",
                        "Status: {state.read().status_message}"
                    }
                }
            }

            // URL 输入区
            div {
                style: "padding: 24px; background: {state.read().theme.card_background()}; border-radius: 16px; box-shadow: 0 8px 32px rgba(0,0,0,0.12); margin-bottom: 16px; transition: background-color 0.3s ease;",
                div {
                    style: "display: flex; align-items: center; height: 52px; border: 2px solid #e8e8e8; border-radius: 12px; padding-left: 16px; transition: all 0.3s ease;",
                    input {
                        r#type: "text",
                        placeholder: "Paste video URL here... (支持批量，多个用空格或逗号分隔)",
                        value: "{state.read().url}",
                        oninput: move |e| {
                            state.write().url = e.value().to_string();
                        },
                        style: "flex: 1; border: none; outline: none; font-size: 15px; background: transparent;"
                    }
                }
                div {
                    style: "font-size: 12px; color: #999; margin-top: 8px;",
                    "💡 支持 X.com, Twitter, TikTok, 抖音, Bilibili 等平台"
                }
                div {
                    style: "display: flex; align-items: center; height: 40px; border: 1px solid #ccc; border-radius: 6px; padding-left: 12px;",
                    input {
                        r#type: "text",
                        placeholder: "Enter X.com/Twitter video URL",
                        value: "{state.read().url}",
                        oninput: move |e| {
                            state.write().url = e.value().to_string();
                        },
                        style: "flex: 1; border: none; outline: none; font-size: 14px;"
                    }
                }
                div {
                    style: "font-size: 11px; color: #888; margin-top: 4px;",
                    "Enter URL(s), separate multiple URLs with spaces or commas"
                }
                div {
                    style: "display: flex; gap: 16px; height: 44px; margin-top: 12px;",
                    button {
                        onclick: move |_| {
                            let urls_text = state.read().url.clone();
                            // 解析多个 URL（空格/逗号分隔）
                            let url_list: Vec<String> = urls_text.split([' ', ','])
                                .map(|s| s.trim().to_string())
                                .filter(|s| !s.is_empty())
                                .collect();

                            if url_list.is_empty() {
                                state.write().error_message = "请输入有效的 URL".to_string();
                                return;
                            }

                            state.write().is_fetching = true;
                            state.write().status_message = "Fetching...".to_string();
                            state.write().error_message = String::new();
                            state.write().has_video_info = false;

                            let mut state = state;
                            let mut video_info_clone = video_info;
                            spawn(async move {
                                if url_list.len() == 1 {
                                    // 单个 URL - 显示视频信息
                                    match fetch_video_info_impl(&url_list[0]).await {
                                        Ok(info) => {
                                            state.write().has_video_info = true;
                                            state.write().is_fetching = false;
                                            state.write().status_message = "Video info fetched".to_string();
                                            state.write().error_message = String::new();
                                            video_info_clone.write().url = info.url.clone();
                                            video_info_clone.write().title = info.title.clone();
                                            video_info_clone.write().uploader = info.uploader.clone();
                                            video_info_clone.write().duration = info.duration.clone();
                                            video_info_clone.write().formats = info.formats.clone();
                                        }
                                        Err(err) => {
                                            state.write().is_fetching = false;
                                            state.write().error_message = err;
                                            state.write().status_message = "Failed to fetch video info".to_string();
                                        }
                                    }
                                } else {
                                    // 多个 URL - 批量获取视频信息
                                    let mut video_infos: Vec<VideoInfoDisplay> = Vec::new();
                                    let mut has_error = false;
                                    let mut error_msg = String::new();

                                    for url in &url_list {
                                        match fetch_video_info_impl(url).await {
                                            Ok(info) => video_infos.push(info),
                                            Err(e) => {
                                                has_error = true;
                                                error_msg = e;
                                                break;
                                            }
                                        }
                                    }

                                    if has_error {
                                        state.write().is_fetching = false;
                                        state.write().error_message = error_msg;
                                        state.write().status_message = "Failed to fetch video info".to_string();
                                    } else {
                                        state.write().batch_video_state = BatchVideoState {
                                            visible: true,
                                            items: video_infos,
                                            status: format!("获取了 {} 个视频信息", url_list.len()),
                                        };
                                        state.write().is_fetching = false;
                                        state.write().status_message = "Video info fetched".to_string();
                                        info!("批量视频信息获取完成: {} 个视频", url_list.len());
                                    }
                                }
                            });
                        },
                        style: "width: 140px; background: {state.read().theme.gradient()}; color: white; border: none; border-radius: 10px; font-size: 14px; font-weight: 600; box-shadow: 0 4px 15px rgba(0,0,0,0.2); transition: all 0.3s ease;",
                        if state.read().is_fetching { "加载中..." } else { "📥 获取信息" }
                    }
                    button {
                        onclick: move |_| {
                            state.write().url = String::new();
                            state.write().error_message = String::new();
                            state.write().status_message = "Enter video URL".to_string();
                            state.write().has_video_info = false;
                        },
                        style: "padding: 0 20px; background: #f5f5f5; border: none; border-radius: 10px; font-size: 14px; color: #666; transition: all 0.3s ease;",
                        "🗑️ 清空"
                    }
                }
            }

            // 错误提示 - 详细样式
            if !state.read().error_message.is_empty() {
                div {
                    style: "padding: 16px; background: linear-gradient(135deg, #fce8e8 0%, #ffe8e8 100%); border: 1px solid #f5c6cb; border-radius: 12px; margin-top: 12px; box-shadow: 0 4px 15px rgba(234, 67, 53, 0.15);",
                    div {
                        style: "display: flex; align-items: flex-start; gap: 12px;",
                        div {
                            style: "font-size: 24px; line-height: 1;",
                            "⚠️"
                        }
                        div {
                            style: "flex: 1;",
                            div {
                                style: "font-size: 14px; font-weight: 700; color: #ea4335; margin-bottom: 4px;",
                                "下载失败"
                            }
                            div {
                                style: "font-size: 13px; color: #721c24;",
                                "{state.read().error_message}"
                            }
                            // 根据错误类型显示建议
                            if state.read().error_message.contains("network") || state.read().error_message.contains("connection") {
                                div {
                                    style: "font-size: 12px; color: #856404; background: #fff3cd; padding: 8px; border-radius: 4px; margin-top: 8px;",
                                    "💡 Suggestion: Check your internet connection and try again"
                                }
                            } else if state.read().error_message.contains("auth") || state.read().error_message.contains("login") || state.read().error_message.contains("cookie") {
                                div {
                                    style: "font-size: 12px; color: #856404; background: #fff3cd; padding: 8px; border-radius: 4px; margin-top: 8px;",
                                    "💡 Suggestion: Update cookies in Settings (exit login → clear cookies → re-login → export)"
                                }
                            } else if state.read().error_message.contains("解析") || state.read().error_message.contains("parse") || state.read().error_message.contains("解析失败") {
                                div {
                                    style: "font-size: 12px; color: #856404; background: #fff3cd; padding: 8px; border-radius: 4px; margin-top: 8px;",
                                    "💡 Suggestion: Video format may not be supported or URL is incorrect"
                                }
                            }
                        }
                    }
                }
            }

            // 视频信息
            if state.read().has_video_info {
                div {
                    style: "padding: 20px; background: rgba(255,255,255,0.98); border-radius: 16px; box-shadow: 0 4px 20px rgba(0,0,0,0.08); margin-top: 12px;",
                    div {
                        style: "display: flex; align-items: center; gap: 10px; margin-bottom: 14px; padding-bottom: 14px; border-bottom: 1px solid #eee;",
                        div { style: "font-size: 24px;", "🎬" }
                        div {
                            style: "font-size: 16px; font-weight: 700; color: #333; flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;",
                            "{video_info.read().title}"
                        }
                    }
                    div {
                        style: "display: grid; grid-template-columns: repeat(3, 1fr); gap: 12px;",
                        div {
                            style: "text-align: center; padding: 14px 8px; background: linear-gradient(135deg, #f8f9fa 0%, #f0f0f0 100%); border-radius: 12px;",
                            div { style: "font-size: 11px; color: #888; margin-bottom: 6px;", "👤 上传者" }
                            div { style: "font-size: 12px; font-weight: 600; color: #333; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;", "{video_info.read().uploader}" }
                        }
                        div {
                            style: "text-align: center; padding: 14px 8px; background: linear-gradient(135deg, #f8f9fa 0%, #f0f0f0 100%); border-radius: 12px;",
                            div { style: "font-size: 11px; color: #888; margin-bottom: 6px;", "⏱️ 时长" }
                            div { style: "font-size: 12px; font-weight: 600; color: #333;", "{video_info.read().duration}" }
                        }
                        div {
                            style: "text-align: center; padding: 14px 8px; background: linear-gradient(135deg, #f8f9fa 0%, #f0f0f0 100%); border-radius: 12px;",
                            div { style: "font-size: 11px; color: #888; margin-bottom: 6px;", "📁 画质" }
                            div {
                                style: "font-size: 12px; font-weight: 600; color: #333;",
                                select {
                                    style: "width: 100%; padding: 6px 8px; border-radius: 8px; border: 1px solid #ddd; background: white; font-size: 12px; cursor: pointer;",
                                    onchange: move |evt| {
                                        let format_id = evt.value();
                                        if !format_id.is_empty() {
                                            state.write().selected_format_id = Some(format_id);
                                        }
                                    },
                                    option { value: "", "默认最佳" }
                                    for format in video_info.read().formats.iter() {
                                        option { value: "{format.format_id}", "{format.resolution} ({format.ext}) - {format.filesize}" }
                                    }
                                }
                            }
                        }
                    }
                }

                // 批量视频信息显示
                if state.read().batch_video_state.visible {
                    div {
                        style: "padding: 20px; background: rgba(255,255,255,0.98); border-radius: 16px; box-shadow: 0 4px 20px rgba(0,0,0,0.08); margin-top: 12px;",
                        div {
                            style: "display: flex; align-items: center; gap: 10px; margin-bottom: 14px;",
                            div { style: "font-size: 24px;", "📋" }
                            div {
                                style: "font-size: 16px; font-weight: 700; background: linear-gradient(90deg, #667eea, #764ba2); -webkit-background-clip: text; -webkit-text-fill-color: transparent;",
                                "批量视频列表 ({state.read().batch_video_state.items.len()})"
                            }
                        }
                        div {
                            style: "display: flex; flex-direction: column; gap: 10px; max-height: 320px; overflow-y: auto; padding-right: 4px;",
                            for (idx, item) in state.read().batch_video_state.items.iter().enumerate() {
                                div {
                                    style: "padding: 14px; background: linear-gradient(135deg, #f8f9fa 0%, #fff 100%); border-radius: 12px; border: 1px solid #e8e8e8; transition: all 0.3s ease;",
                                    div {
                                        style: "display: flex; align-items: center; gap: 10px; margin-bottom: 8px;",
                                        div {
                                            style: "width: 26px; height: 26px; background: linear-gradient(135deg, #667eea 0%, #764ba2 100%); border-radius: 50%; text-align: center; line-height: 26px; font-size: 12px; color: white; font-weight: 700;",
                                            "{idx + 1}"
                                        }
                                        div {
                                            style: "font-size: 14px; font-weight: 600; color: #333; flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;",
                                            "{item.title}"
                                        }
                                    }
                                    div {
                                        style: "font-size: 11px; color: #888; padding-left: 36px;",
                                        "👤 {item.uploader}  |  ⏱️ {item.duration}  |  📁 {item.formats.len()} 种画质"
                                    }
                                }
                            }
                        }
                    }

                    // 批量下载按钮
                    div {
                        style: "display: flex; gap: 12px; margin-top: 16px;",
                        button {
                            onclick: move |_| {
                                // 实现批量下载
                                let items = state.read().batch_video_state.items.clone();
                                if items.is_empty() {
                                    return;
                                }
                                state.write().batch_state = BatchState {
                                    visible: true,
                                    progress: 0.0,
                                    status: format!("准备下载 {} 个视频", items.len()),
                                    items: items.iter().map(|i| BatchItemState {
                                        url: i.url.clone(),
                                        title: i.title.clone(),
                                        status: "pending".to_string(),
                                        progress: 0.0,
                                        speed: String::new(),
                                        visible: true,
                                        progress_arc: None,
                                        speed_arc: None,
                                    }).collect(),
                                    is_downloading: true,
                                };
                                state.write().batch_video_state.visible = false;
                                let urls: Vec<String> = items.iter().map(|i| i.url.clone()).collect();
                                // 为每个下载项创建原子进度追踪
                                let progress_arcs: Vec<Option<Arc<AtomicU32>>> = items.iter().map(|_| Some(Arc::new(AtomicU32::new(0)))).collect();
                                let speed_arcs: Vec<Option<Arc<AtomicU64>>> = items.iter().map(|_| Some(Arc::new(AtomicU64::new(0)))).collect();
                                let mut state = state;
                                spawn(async move {
                                    batch_download_impl(urls, progress_arcs, speed_arcs).await;
                                    state.write().batch_state.visible = false;
                                    state.write().batch_state.is_downloading = false;
                                });
                            },
                            style: "flex: 1; padding: 14px; background: linear-gradient(135deg, #34a853 0%, #2e9651 100%); color: white; border: none; border-radius: 12px; font-size: 15px; font-weight: 600; cursor: pointer; box-shadow: 0 4px 15px rgba(52, 168, 83, 0.4); transition: all 0.3s ease;",
                            "⬇️ 全部下载"
                        }
                        button {
                            onclick: move |_| {
                                state.write().batch_video_state.visible = false;
                            },
                            style: "flex: 1; padding: 14px; background: linear-gradient(135deg, #ea4335 0%, #c62828 100%); color: white; border: none; border-radius: 12px; font-size: 15px; font-weight: 600; cursor: pointer; box-shadow: 0 4px 15px rgba(234, 67, 53, 0.4); transition: all 0.3s ease;",
                            "⏹️ 取消"
                        }
                    }
                }

                // 操作按钮
                div {
                    style: "display: flex; gap: 16px; margin-top: 12px;",
                    button {
                        onclick: move |_| {
                            let url = video_info.read().url.clone();
                            let title = video_info.read().title.clone();
                            if url.is_empty() {
                                return;
                            }
                            state.write().is_downloading = true;
                            state.write().status_message = "Downloading...".to_string();
                            state.write().download_progress = 0.0;

                            // 创建进度追踪原子变量
                            let progress_arc = Arc::new(AtomicU32::new(0));
                            let speed_arc = Arc::new(AtomicU64::new(0));
                            let progress_arc_clone = progress_arc.clone();
                            let speed_arc_clone = speed_arc.clone();

                            // 使用 spawn 在后台异步执行下载
                            let mut state = state;
                            spawn(async move {
                                match start_download_impl(&url, &title, Some(progress_arc_clone), Some(speed_arc_clone)).await {
                                    Ok(_) => {
                                        state.write().is_downloading = false;
                                        state.write().status_message = "Download completed".to_string();
                                        state.write().download_progress = 1.0;
                                        send_notification("Download Complete", &format!("{} has been downloaded successfully", title));
                                    }
                                    Err(e) => {
                                        state.write().is_downloading = false;
                                        state.write().error_message = e;
                                        state.write().status_message = "Download failed".to_string();
                                        send_notification("Download Failed", &title.to_string());
                                    }
                                }
                            });

                            // 轮询进度更新 UI（独立 task，500ms 间隔减少 UI 负载）
                            // 进度阈值：只有变化超过 1% 才更新 UI，减少不必要的渲染
                            let progress_arc_for_poll = progress_arc.clone();
                            let mut state_for_poll = state;
                            const PROGRESS_THRESHOLD: f32 = 0.01; // 1% 阈值
                            let mut last_updated_progress: f32 = 0.0;
                            spawn(async move {
                                loop {
                                    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                                    if !state_for_poll.read().is_downloading {
                                        break;
                                    }
                                    let percent = progress_arc_for_poll.load(Ordering::Relaxed) as f32 / 10000.0;
                                    // 只有进度变化超过阈值或达到完成状态才更新 UI
                                    if (percent - last_updated_progress).abs() >= PROGRESS_THRESHOLD || percent >= 1.0 {
                                        state_for_poll.write().download_progress = percent;
                                        last_updated_progress = percent;
                                    }
                                    if percent >= 1.0 {
                                        break;
                                    }
                                }
                            });
                        },
                        style: "flex: 1; padding: 14px; background: linear-gradient(135deg, #34a853 0%, #2e9651 100%); color: white; border: none; border-radius: 10px; font-size: 15px; font-weight: 600; box-shadow: 0 4px 15px rgba(52, 168, 83, 0.4); transition: all 0.3s ease;",
                        "⬇️ 开始下载"
                    }
                    button {
                        onclick: move |_| {
                            let urls = state.read().url.clone();
                            // 解析多个 URL（空格/逗号分隔）
                            let url_list: Vec<String> = urls.split([' ', ','])
                                .map(|s| s.trim().to_string())
                                .filter(|s| !s.is_empty())
                                .collect();

                            if url_list.is_empty() {
                                state.write().error_message = "请输入有效的 URL".to_string();
                                return;
                            }

                            // 创建进度追踪原子变量
                            let progress_arcs: Vec<Option<Arc<AtomicU32>>> = url_list.iter().map(|_| Some(Arc::new(AtomicU32::new(0)))).collect();
                            let speed_arcs: Vec<Option<Arc<AtomicU64>>> = url_list.iter().map(|_| Some(Arc::new(AtomicU64::new(0)))).collect();

                            // 初始化批量状态（关联 progress_arc）
                            let batch_items: Vec<BatchItemState> = url_list.iter().zip(progress_arcs.iter()).map(|(url, arc)| {
                                BatchItemState {
                                    url: url.clone(),
                                    title: "视频".to_string(),
                                    status: "pending".to_string(),
                                    progress: 0.0,
                                    speed: String::new(),
                                    visible: true,
                                    progress_arc: arc.clone(),
                                    speed_arc: None,
                                }
                            }).collect();

                            state.write().batch_state = BatchState {
                                visible: true,
                                progress: 0.0,
                                status: format!("准备下载 {} 个视频", url_list.len()),
                                items: batch_items,
                                is_downloading: true,
                            };

                            // 克隆 arc 供轮询使用
                            let progress_arcs_for_poll: Vec<Arc<AtomicU32>> = progress_arcs.iter().filter_map(|a| a.clone()).collect();
                            let mut state_for_poll = state;
                            let total_count = url_list.len();

                            // 轮询更新批量下载进度 UI
                            spawn(async move {
                                loop {
                                    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                                    if !state_for_poll.read().is_downloading {
                                        break;
                                    }
                                    // 读取每个视频的进度
                                    let mut total_progress = 0.0f32;
                                    let mut all_done = true;
                                    for (idx, arc) in progress_arcs_for_poll.iter().enumerate() {
                                        let percent = arc.load(Ordering::Relaxed) as f32 / 10000.0;
                                        total_progress += percent;
                                        if percent < 1.0 {
                                            all_done = false;
                                        }
                                        // 更新单个视频进度
                                        if idx < state_for_poll.read().batch_state.items.len() {
                                            state_for_poll.write().batch_state.items[idx].progress = percent;
                                        }
                                    }
                                    state_for_poll.write().batch_state.progress = total_progress / (total_count as f32);
                                    if all_done || total_progress >= (total_count as f32) {
                                        break;
                                    }
                                }
                            });

                            // 使用 spawn 在后台异步执行批量下载
                            let _state = state;
                            spawn(async move {
                                batch_download_impl(url_list, progress_arcs, speed_arcs).await;
                            });
                        },
                        style: "flex: 1; padding: 12px; background: #4285f4; color: white; border: none; border-radius: 6px; font-size: 14px; font-weight: 500;",
                        "Batch Download"
                    }
                }
            }

            // 进度显示
            if state.read().is_downloading && !state.read().batch_state.visible {
                div {
                    style: "padding: 20px; background: rgba(255,255,255,0.98); border-radius: 16px; box-shadow: 0 8px 32px rgba(0,0,0,0.12); margin-top: 12px;",
                    div {
                        style: "display: flex; justify-content: space-between; align-items: center; margin-bottom: 10px;",
                        div {
                            style: "font-size: 15px; font-weight: 600; color: #34a853;",
                            "📥 下载中... {(state.read().download_progress * 100.0).round() as i32}%"
                        }
                        div {
                            style: "font-size: 12px; color: #888;",
                            "{state.read().download_speed}"
                        }
                    }
                    div {
                        style: "height: 12px; background: #e8e8e8; border-radius: 6px; overflow: hidden;",
                        div {
                            style: "height: 100%; width: {state.read().download_progress * 100.0}%; background: linear-gradient(90deg, #34a853, #2ecc71); border-radius: 6px; transition: width 0.3s ease;",
                        }
                    }
                }
            }

            // 批量下载进度显示
            if state.read().batch_state.visible {
                div {
                    style: "padding: 20px; background: rgba(255,255,255,0.98); border-radius: 16px; box-shadow: 0 8px 32px rgba(0,0,0,0.12); margin-top: 12px;",
                    div {
                        style: "display: flex; justify-content: space-between; align-items: center; margin-bottom: 10px;",
                        div {
                            style: "font-size: 16px; font-weight: 700; background: linear-gradient(90deg, #667eea, #764ba2); -webkit-background-clip: text; -webkit-text-fill-color: transparent;",
                            "📚 批量下载 - {(state.read().batch_state.progress * 100.0).round() as i32}%"
                        }
                        button {
                            onclick: move |_| {
                                state.write().batch_state.visible = false;
                                state.write().batch_state.is_downloading = false;
                            },
                            style: "padding: 6px 14px; background: linear-gradient(135deg, #ea4335 0%, #c62828 100%); color: white; border: none; border-radius: 8px; font-size: 12px; cursor: pointer; box-shadow: 0 2px 8px rgba(234, 67, 53, 0.3);",
                            "⏹️ 取消"
                        }
                    }
                    div {
                        style: "height: 10px; background: #e8e8e8; border-radius: 5px; overflow: hidden; margin-top: 8px;",
                        div {
                            style: "height: 100%; width: {state.read().batch_state.progress * 100.0}%; background: linear-gradient(90deg, #667eea, #764ba2); border-radius: 5px; transition: width 0.3s ease;",
                        }
                    }
                    div {
                        style: "font-size: 12px; color: #888; margin-top: 6px;",
                        "{state.read().batch_state.status}"
                    }
                    // 各视频进度列表
                    div {
                        style: "display: flex; flex-direction: column; gap: 10px; margin-top: 16px;",
                        for (_idx, item) in state.read().batch_state.items.iter().enumerate() {
                            if item.visible {
                                div {
                                    style: "display: flex; align-items: center; gap: 12px; padding: 12px; background: #f8f9fa; border-radius: 10px; transition: all 0.3s ease;",
                                    div {
                                        style: "flex: 1; height: 10px; background: #e8e8e8; border-radius: 5px; overflow: hidden;",
                                        div {
                                            style: "height: 100%; width: {item.progress * 100.0}%; background: linear-gradient(90deg, #667eea, #764ba2); border-radius: 5px; transition: width 0.3s ease;",
                                        }
                                    }
                                    div {
                                        style: "width: 45px; font-size: 12px; font-weight: 600; color: #667eea; text-align: right;",
                                        "{(item.progress * 100.0).round() as i32}%"
                                    }
                                    div {
                                        style: "font-size: 11px; color: #888;",
                                        "{item.status}"
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // 历史记录面板
            if state.read().show_history {
                div {
                    style: "padding: 20px; background: rgba(255,255,255,0.98); border-radius: 16px; box-shadow: 0 4px 20px rgba(0,0,0,0.08); margin-top: 12px; max-height: 400px; overflow-y: auto;",
                    div {
                        style: "display: flex; justify-content: space-between; align-items: center; margin-bottom: 14px;",
                        div {
                            style: "display: flex; align-items: center; gap: 8px;",
                            div { style: "font-size: 24px;", "📜" }
                            div {
                                style: "font-size: 16px; font-weight: 700; background: {state.read().theme.gradient()}; -webkit-background-clip: text; -webkit-text-fill-color: transparent;",
                                "下载历史"
                            }
                        }
                        button {
                            onclick: move |_| { state.write().show_history = false; },
                            style: "width: 28px; height: 28px; border-radius: 50%; border: none; background: #f0f0f0; color: #888; font-size: 14px; cursor: pointer; display: flex; align-items: center; justify-content: center;",
                            "×"
                        }
                    }
                    if state.read().history.is_empty() {
                        div {
                            style: "text-align: center; padding: 40px; color: #888;",
                            div { style: "font-size: 48px; margin-bottom: 12px;", "📭" }
                            div { style: "font-size: 14px;", "暂无下载记录" }
                        }
                    } else {
                        div {
                            style: "display: flex; flex-direction: column; gap: 10px;",
                            for item in state.read().history.iter().rev() {
                                div {
                                    style: "display: flex; align-items: center; gap: 12px; padding: 12px; background: linear-gradient(135deg, #f8f9fa 0%, #fff 100%); border-radius: 12px; border: 1px solid #e8e8e8;",
                                    div {
                                        style: "width: 40px; height: 40px; background: linear-gradient(135deg, #34a853 0%, #2ecc71 100%); border-radius: 10px; display: flex; align-items: center; justify-content: center;",
                                        div { style: "font-size: 18px;", "✅" }
                                    }
                                    div {
                                        style: "flex: 1; min-width: 0;",
                                        div {
                                            style: "font-size: 13px; font-weight: 600; color: #333; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;",
                                            "{item.title}"
                                        }
                                        div {
                                            style: "font-size: 11px; color: #888; margin-top: 2px;",
                                            "👤 {item.uploader}  |  ⏱️ {item.duration}  |  🕐 {item.completed_at}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// 同步下载 - 使用 VideoDownloader
async fn start_download_impl(
    url: &str,
    title: &str,
    progress_arc: Option<Arc<AtomicU32>>,
    speed_arc: Option<Arc<AtomicU64>>,
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

    let request = DownloadRequest::new(url, &output_path);

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
async fn batch_download_impl(
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
                    match start_download_impl(&info.url, &info.title, progress_arc, speed_arc).await {
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

async fn fetch_video_info_impl(url: &str) -> Result<VideoInfoDisplay, String> {
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
    registry.register(tiktok_resolver);
    registry.register(douyin_resolver);
    registry.register(BilibiliResolver::new());

    let resolver = registry.select_resolver(url);
    let result = resolver.get_video_info(url).await;

    match result {
        Ok(info) => {
            let duration = info.duration.map(|d| format!("{}:{:02}", d / 60, d % 60)).unwrap_or_default();
            Ok(VideoInfoDisplay {
                url: info.url.clone(),
                title: info.title,
                uploader: info.uploader.unwrap_or_default(),
                duration,
                formats: info.formats.iter().map(|f| VideoFormatDisplay {
                    format_id: f.format_id.clone(),
                    resolution: f.resolution.clone().unwrap_or_default(),
                    ext: f.ext.clone(),
                    filesize: f.filesize.map(format_file_size).unwrap_or_default(),
                }).collect(),
            })
        }
        Err(e) => {
            warn!("Resolver failed: {}, trying direct YtDlp...", e);
            let ytdlp = YtDlp::new();
            match ytdlp.get_video_info(url) {
                Ok(info) => {
                    let duration = info.duration.map(|d| format!("{}:{:02}", d / 60, d % 60)).unwrap_or_default();
                    Ok(VideoInfoDisplay {
                        url: info.url.clone(),
                        title: info.title,
                        uploader: info.uploader.unwrap_or_default(),
                        duration,
                        formats: info.formats.iter().map(|f| VideoFormatDisplay {
                            format_id: f.format_id.clone(),
                            resolution: f.resolution.clone().unwrap_or_default(),
                            ext: f.ext.clone(),
                            filesize: f.filesize.map(format_file_size).unwrap_or_default(),
                        }).collect(),
                    })
                }
                Err(ytdlp_err) => {
                    warn!("YtDlp also failed: {}", ytdlp_err);
                    if url.contains("twitter.com") || url.contains("x.com") {
                        match FxTwitterResolver::new().get_video_info(url).await {
                            Ok(info) => {
                                let duration = info.duration.map(|d| format!("{}:{:02}", d / 60, d % 60)).unwrap_or_default();
                                Ok(VideoInfoDisplay {
                                    url: info.url.clone(),
                                    title: info.title,
                                    uploader: info.uploader.unwrap_or_default(),
                                    duration,
                                    formats: info.formats.iter().map(|f| VideoFormatDisplay {
                                        format_id: f.format_id.clone(),
                                        resolution: f.resolution.clone().unwrap_or_default(),
                                        ext: f.ext.clone(),
                                        filesize: f.filesize.map(format_file_size).unwrap_or_default(),
                                    }).collect(),
                                })
                            }
                            Err(_) => Err("Failed to fetch video info".to_string()),
                        }
                    } else {
                        Err(format!("Failed to fetch video info: {}", ytdlp_err))
                    }
                }
            }
        }
    }
}