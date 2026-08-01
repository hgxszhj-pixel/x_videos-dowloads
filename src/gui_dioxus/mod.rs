//! Dioxus GUI 入口
//!
//! 迁移自 gui.rs 的 Slint 实现。
//!
//! 结构:
//! - [`app`]       - 主 `AppComponent`（RSX 视图）
//! - [`theme`]     - 主题配置
//! - [`types`]     - 显示用数据结构与状态
//! - [`store`]     - 全局 History / 配置热重载 / 通知
//! - [`handlers`]  - 视频信息解析与下载实现

mod actions;
mod app;
mod app_batch_progress;
mod app_batch_video;
mod app_course_pool;
mod app_history;
mod app_settings;
mod app_video_card;
mod bridge;
mod components;
mod handlers;
mod poller;
mod store;
mod theme;
mod types;

use dioxus::desktop::launch::launch;
use dioxus::desktop::{tao, Config};
use tracing::{error, info};

use crate::history::History;
use crate::yt_dlp::YtDlp;

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
    // 初始化全局 History 实例（带缓存）
    let data_dir = directories::ProjectDirs::from("com", "x-video-downloader", "X Video Downloader")
        .map(|dirs| dirs.data_dir().to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let history = match History::new(&data_dir) {
        Ok(h) => h,
        Err(e) => {
            error!("Failed to initialize history: {}", e);
            eprintln!("Failed to initialize history: {}", e);
            return;
        }
    };
    store::init_global_history(history);

    // 启动 Dioxus Desktop (设置窗口标题为 "X Video Downloader", 覆盖默认 "Dioxus App")
    let window_config = Config::default()
        .with_window(tao::window::WindowBuilder::new().with_title("X Video Downloader"));
    launch(app::AppComponent, vec![], vec![Box::new(window_config)]);
}
