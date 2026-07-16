//! 主应用组件（Dioxus RSX 视图）

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::path::PathBuf;
use std::sync::Arc;

use dioxus::prelude::*;
use notify::{RecursiveMode, Watcher};
use tracing::info;

use crate::config::AppConfig;

use super::handlers::{batch_download_impl, fetch_video_info_impl, start_download_impl};
use super::store::{
    add_completed_download, clear_global_history, get_history_items, reload_config,
    send_notification,
};
use super::theme::AppTheme;
use super::types::{format_file_size, format_speed, AppState, BatchItemState, BatchState, BatchVideoState, ErrorDisplay, VideoInfoDisplay};
use super::components::{BrowseButton, CardShadow, CloseButton, DestructivePillButton, SettingsNumberInput, SettingsTextInput, ThemeSwatch, UrlInput, WhiteCard};
use super::theme::{STYLE_BTN_ICON, STYLE_PROGRESS_TRACK, BG_SUBTLE, BORDER_DEFAULT, TEXT_MUTED, TEXT_PRIMARY, TEXT_SECONDARY};

/// 主应用组件
#[component]
pub(super) fn AppComponent() -> Element {
    let mut state = use_signal(AppState::default);
    let video_info = use_signal(VideoInfoDisplay::default);

    // 拖放 URL bridge — 解锁 HANDOFF §明日继续 #5（macOS WKWebView 监听外部 drop）
    use_effect(move || {
        crate::gui_dioxus::bridge::install_drag_drop_bridge(state);
    });

    // 启动配置文件监控
    // 加载历史记录到 UI state
    use_effect(move || {
        let history_items = get_history_items();
        state.write().history = history_items;
    });

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

    // 启动时从 AppConfig 加载主题
    use_effect(move || {
        if let Ok(cfg) = AppConfig::load() {
            let theme = match cfg.theme.as_str() {
                "Ocean" => AppTheme::Ocean,
                "Sunset" => AppTheme::Sunset,
                "Forest" => AppTheme::Forest,
                _ => AppTheme::Purple,
            };
            state.write().theme = theme;
        }
    });

    // Enter 键 + button 点击共享的 URL 提交逻辑（抽到外层 closure 让 onkeydown/onclick 复用）
    let mut submit_urls = move || {
        let urls_text = state.read().url.clone();
        let url_list: Vec<String> = urls_text.split([' ', ','])
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        if url_list.is_empty() {
            state.write().error_display = Some(ErrorDisplay::new("parse", "请输入有效的 URL", "请输入视频链接后重试"));
            return;
        }

        state.write().is_fetching = true;
        state.write().status_message = "Fetching...".to_string();
        state.write().error_display = None;
        state.write().has_video_info = false;

        let mut state_clone = state;
        let mut video_info_clone = video_info;
        spawn(async move {
            if url_list.len() == 1 {
                match fetch_video_info_impl(&url_list[0]).await {
                    Ok(info) => {
                        state_clone.write().has_video_info = true;
                        state_clone.write().is_fetching = false;
                        state_clone.write().status_message = "Video info fetched".to_string();
                        state_clone.write().error_display = None;
                        video_info_clone.write().url = info.url.clone();
                        video_info_clone.write().title = info.title.clone();
                        video_info_clone.write().uploader = info.uploader.clone();
                        video_info_clone.write().duration = info.duration.clone();
                        video_info_clone.write().formats = info.formats.clone();
                    }
                    Err(err) => {
                        state_clone.write().is_fetching = false;
                        state_clone.write().error_display = Some(ErrorDisplay::new("parse", err, "URL 无效或平台不支持"));
                        state_clone.write().status_message = "Failed to fetch video info".to_string();
                    }
                }
            } else {
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
                    state_clone.write().is_fetching = false;
                    state_clone.write().error_display = Some(ErrorDisplay::new("parse", error_msg, "检查 URL 列表"));
                    state_clone.write().status_message = "Failed to fetch video info".to_string();
                } else {
                    state_clone.write().batch_video_state = BatchVideoState {
                        visible: true,
                        items: video_infos,
                        status: format!("获取了 {} 个视频信息", url_list.len()),
                    };
                    state_clone.write().is_fetching = false;
                    state_clone.write().status_message = "Video info fetched".to_string();
                    info!("批量视频信息获取完成: {} 个视频", url_list.len());
                }
            }
        });
    };

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
                        ThemeSwatch {
                            theme: AppTheme::Purple,
                            onclick: move |_| {
                                state.write().theme = AppTheme::Purple;
                                // 持久化
                                if let Ok(mut cfg) = AppConfig::load() {
                                    cfg.theme = "Purple".to_string();
                                    let _ = cfg.save();
                                }
                            },
                        }
                        ThemeSwatch {
                            theme: AppTheme::Ocean,
                            onclick: move |_| {
                                state.write().theme = AppTheme::Ocean;
                                if let Ok(mut cfg) = AppConfig::load() {
                                    cfg.theme = "Ocean".to_string();
                                    let _ = cfg.save();
                                }
                            },
                        }
                        ThemeSwatch {
                            theme: AppTheme::Sunset,
                            onclick: move |_| {
                                state.write().theme = AppTheme::Sunset;
                                if let Ok(mut cfg) = AppConfig::load() {
                                    cfg.theme = "Sunset".to_string();
                                    let _ = cfg.save();
                                }
                            },
                        }
                        ThemeSwatch {
                            theme: AppTheme::Forest,
                            onclick: move |_| {
                                state.write().theme = AppTheme::Forest;
                                if let Ok(mut cfg) = AppConfig::load() {
                                    cfg.theme = "Forest".to_string();
                                    let _ = cfg.save();
                                }
                            },
                        }
                    }
                    // 设置齿轮按钮
                    button {
                        onclick: move |_| {
                            let mut s = state.write();
                            s.show_settings = !s.show_settings;
                            if s.show_settings {
                                // 打开时从 AppConfig 加载当前值
                                if let Ok(cfg) = AppConfig::load() {
                                    s.settings_output_dir = cfg.output_dir.to_string_lossy().to_string();
                                    s.settings_ms_token = cfg.douyin_ms_token.clone().unwrap_or_default();
                                    s.settings_cookie_file = cfg.cookie_file.as_ref()
                                        .map(|p| p.to_string_lossy().to_string())
                                        .unwrap_or_default();
                                    s.settings_concurrent = cfg.concurrent_downloads as u32;
                                }
                            }
                        },
                        style: "padding: 6px 10px; background: transparent; border: 1px solid #ccc; border-radius: 8px; font-size: 16px; cursor: pointer;",
                        "⚙️"
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
                        style: "font-size: 12px; color: {TEXT_MUTED}; padding: 6px 12px; background: {BG_SUBTLE}; border-radius: 20px;",
                        "Status: {state.read().status_message}"
                    }
                }
            }

            // URL 输入区
            div {
                style: "padding: 24px; background: {state.read().theme.card_background()}; border-radius: 16px; box-shadow: 0 8px 32px rgba(0,0,0,0.12); margin-bottom: 16px; transition: background-color 0.3s ease;",
                div {
                    style: "display: flex; align-items: center; height: 52px; border: 2px solid #e8e8e8; border-radius: 12px; padding-left: 16px; transition: all 0.3s ease;",
                    UrlInput {
                        placeholder: "Paste video URL here... (支持批量，多个用空格或逗号分隔)".to_string(),
                        value: state.read().url.clone(),
                        on_input: move |e: dioxus::events::FormEvent| { state.write().url = e.value().to_string(); },
                        on_submit: move |_: dioxus::events::KeyboardEvent| { submit_urls(); },
                        font_size: "15px".to_string(),
                    }
                }
                div {
                    style: "font-size: 12px; color: #999; margin-top: 8px;",
                    "💡 支持 X.com, Twitter, TikTok, 抖音, Bilibili 等平台"
                }
                div {
                    style: "display: flex; align-items: center; height: 40px; border: 1px solid #ccc; border-radius: 6px; padding-left: 12px;",
                    UrlInput {
                        placeholder: "Enter X.com/Twitter video URL".to_string(),
                        value: state.read().url.clone(),
                        on_input: move |e: dioxus::events::FormEvent| { state.write().url = e.value().to_string(); },
                        on_submit: move |_: dioxus::events::KeyboardEvent| { submit_urls(); },
                        font_size: "14px".to_string(),
                    }
                }
                div {
                    style: "font-size: 11px; color: {TEXT_MUTED}; margin-top: 4px;",
                    "Enter URL(s), separate multiple URLs with spaces or commas"
                }
                div {
                    style: "display: flex; gap: 16px; height: 44px; margin-top: 12px;",
                    button {
                        onclick: move |_| submit_urls(),
                        style: "width: 140px; background: {state.read().theme.gradient()}; color: white; border: none; border-radius: 10px; font-size: 14px; font-weight: 600; box-shadow: 0 4px 15px rgba(0,0,0,0.2); transition: all 0.3s ease;",
                        if state.read().is_fetching { "加载中..." } else { "📥 获取信息" }
                    }
                    button {
                        onclick: move |_| {
                            state.write().url = String::new();
                            state.write().error_display = None;
                            state.write().status_message = "Enter video URL".to_string();
                            state.write().has_video_info = false;
                        },
                        style: "padding: 0 20px; background: #f5f5f5; border: none; border-radius: 10px; font-size: 14px; color: #666; transition: all 0.3s ease;",
                        "🗑️ 清空"
                    }
                }
            }

            // 错误提示 - 结构化（ErrorDisplay 类型驱动）
            if let Some(err) = state.read().error_display.clone() {
                div {
                    style: "{state.read().theme.bg_error()} margin-top: 12px;",
                    div {
                        style: "display: flex; align-items: flex-start; gap: 12px;",
                        div {
                            style: "font-size: 24px; line-height: 1;",
                            "{err.icon()}"
                        }
                        div {
                            style: "flex: 1;",
                            div {
                                style: "font-size: 14px; font-weight: 700; color: {state.read().theme.color_danger()}; margin-bottom: 4px;",
                                "下载失败 · {err.error_type}"
                            }
                            div {
                                style: "font-size: 13px; color: {state.read().theme.color_text_danger()};",
                                "{err.message}"
                            }
                            // 错误建议（结构化字段，不再字符串匹配）
                            if !err.suggestion.is_empty() {
                                div {
                                    style: "font-size: 12px; color: #856404; background: #fff3cd; padding: 8px; border-radius: 4px; margin-top: 8px;",
                                    "💡 {err.suggestion}"
                                }
                            }
                            // 重试按钮（清除错误，回到初始状态）
                            button {
                                onclick: move |_| {
                                    state.write().error_display = None;
                                    state.write().status_message = "Ready".to_string();
                                },
                                style: "{state.read().theme.btn_solid(state.read().theme.color_danger(), \"6px 14px\", \"16px\", \"12px\")} margin-top: 10px;",
                                "🔄 重试 / 清除"
                            }
                        }
                    }
                }
            }

            // 设置面板
            if state.read().show_settings {
                WhiteCard {
                    shadow: CardShadow::Elevated,
                    div {
                        style: "display: flex; justify-content: space-between; align-items: center; margin-bottom: 14px;",
                        div {
                            style: "font-size: 18px; font-weight: 700; color: {TEXT_PRIMARY};",
                            "⚙️ 设置"
                        }
                        CloseButton {
                            onclick: move |_| { state.write().show_settings = false; },
                        }
                    }
                    // 输出目录
                    div { style: "margin-bottom: 14px;",
                        div { style: "font-size: 13px; color: {TEXT_SECONDARY}; margin-bottom: 4px; font-weight: 600;", "📁 输出目录" }
                        div { style: "display: flex; gap: 6px;",
                            SettingsTextInput {
                                value: state.read().settings_output_dir.clone(),
                                on_input: move |evt: dioxus::events::FormEvent| { state.write().settings_output_dir = evt.value(); },
                            }
                            BrowseButton {
                                label: "浏览".to_string(),
                                theme: state.read().theme.clone(),
                                onclick: move |_| {
                                    if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                                        state.write().settings_output_dir = dir.to_string_lossy().to_string();
                                    }
                                },
                            }
                        }
                    }
                    // ms_token
                    div { style: "margin-bottom: 14px;",
                        div { style: "font-size: 13px; color: {TEXT_SECONDARY}; margin-bottom: 4px; font-weight: 600;", "🔑 抖音/TikTok msToken" }
                        SettingsTextInput {
                            value: state.read().settings_ms_token.clone(),
                            on_input: move |evt: dioxus::events::FormEvent| { state.write().settings_ms_token = evt.value(); },
                            width: "width: 100%".to_string(),
                        }
                    }
                    // Cookie 文件
                    div { style: "margin-bottom: 14px;",
                        div { style: "font-size: 13px; color: {TEXT_SECONDARY}; margin-bottom: 4px; font-weight: 600;", "🍪 Cookie 文件（可选）" }
                        div { style: "display: flex; gap: 6px;",
                            SettingsTextInput {
                                value: state.read().settings_cookie_file.clone(),
                                on_input: move |evt: dioxus::events::FormEvent| { state.write().settings_cookie_file = evt.value(); },
                            }
                            BrowseButton {
                                label: "浏览".to_string(),
                                theme: state.read().theme.clone(),
                                onclick: move |_| {
                                    if let Some(f) = rfd::FileDialog::new().pick_file() {
                                        state.write().settings_cookie_file = f.to_string_lossy().to_string();
                                    }
                                },
                            }
                        }
                    }
                    // 并发数
                    div { style: "margin-bottom: 18px;",
                        div { style: "font-size: 13px; color: {TEXT_SECONDARY}; margin-bottom: 4px; font-weight: 600;", "⚡ 并发下载数（1-16）" }
                        SettingsNumberInput {
                            value: state.read().settings_concurrent,
                            min: 1,
                            max: 16,
                            on_change: move |n: u32| { state.write().settings_concurrent = n; },
                        }
                    }
                    // 按钮行
                    div { style: "display: flex; justify-content: flex-end; gap: 8px;",
                        button {
                            onclick: move |_| { state.write().show_settings = false; },
                            style: "padding: 8px 16px; background: {BG_SUBTLE}; color: {TEXT_SECONDARY}; border: none; border-radius: 8px; font-size: 13px; cursor: pointer;",
                            "取消"
                        }
                        button {
                            onclick: move |_| {
                                // 收集当前 settings_* 字段，写入 AppConfig
                                let out_dir = state.read().settings_output_dir.clone();
                                let ms_token = {
                                    let v = state.read().settings_ms_token.clone();
                                    if v.is_empty() { None } else { Some(v) }
                                };
                                let cookie = {
                                    let v = state.read().settings_cookie_file.clone();
                                    if v.is_empty() { None } else { Some(PathBuf::from(v)) }
                                };
                                let concurrent = state.read().settings_concurrent as usize;
                                match AppConfig::load() {
                                    Ok(mut cfg) => {
                                        cfg.output_dir = PathBuf::from(out_dir);
                                        cfg.douyin_ms_token = ms_token;
                                        cfg.cookie_file = cookie;
                                        cfg.concurrent_downloads = concurrent;
                                        match cfg.save() {
                                            Ok(_) => {
                                                state.write().status_message = "✅ 设置已保存".to_string();
                                                state.write().show_settings = false;
                                            }
                                            Err(e) => {
                                                state.write().error_display = Some(ErrorDisplay::new(
                                                    "download",
                                                    format!("保存设置失败: {}", e),
                                                    "检查配置文件权限和路径"
                                                ));
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        state.write().error_display = Some(ErrorDisplay::new(
                                            "download",
                                            format!("加载配置失败: {}", e),
                                            "检查配置文件存在性"
                                        ));
                                    }
                                }
                            },
                            style: "{state.read().theme.btn_solid(state.read().theme.color_success(), \"8px 16px\", \"8px\", \"13px\")}",
                            "💾 保存"
                        }
                    }
                }
            }

            // 视频信息
            if state.read().has_video_info {
                WhiteCard {
                    shadow: CardShadow::Soft,
                    div {
                        style: "display: flex; align-items: center; gap: 10px; margin-bottom: 14px; padding-bottom: 14px; border-bottom: 1px solid #eee;",
                        div { style: "font-size: 24px;", "🎬" }
                        div {
                            style: "font-size: 16px; font-weight: 700; color: {TEXT_PRIMARY}; flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;",
                            "{video_info.read().title}"
                        }
                    }
                    div {
                        style: "display: grid; grid-template-columns: repeat(3, 1fr); gap: 12px;",
                        div {
                            style: "{state.read().theme.bg_card_subtle()} text-align: center; padding: 14px 8px;",
                            div { style: "font-size: 11px; color: {TEXT_MUTED}; margin-bottom: 6px;", "👤 上传者" }
                            div { style: "font-size: 12px; font-weight: 600; color: {TEXT_PRIMARY}; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;", "{video_info.read().uploader}" }
                        }
                        div {
                            style: "{state.read().theme.bg_card_subtle()} text-align: center; padding: 14px 8px;",
                            div { style: "font-size: 11px; color: {TEXT_MUTED}; margin-bottom: 6px;", "⏱️ 时长" }
                            div { style: "font-size: 12px; font-weight: 600; color: {TEXT_PRIMARY};", "{video_info.read().duration}" }
                        }
                        div {
                            style: "{state.read().theme.bg_card_subtle()} text-align: center; padding: 14px 8px;",
                            div { style: "font-size: 11px; color: {TEXT_MUTED}; margin-bottom: 6px;", "📁 画质" }
                            div {
                                style: "font-size: 12px; font-weight: 600; color: {TEXT_PRIMARY};",
                                select {
                                    style: "width: 100%; padding: 6px 8px; border-radius: 8px; border: 1px solid {BORDER_DEFAULT}; background: white; font-size: 12px; cursor: pointer;",
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
                    WhiteCard {
                        shadow: CardShadow::Soft,
                        div {
                            style: "display: flex; align-items: center; gap: 10px; margin-bottom: 14px;",
                            div { style: "font-size: 24px;", "📋" }
                            div {
                                style: "font-size: 16px; font-weight: 700; background: {state.read().theme.gradient()}; -webkit-background-clip: text; -webkit-text-fill-color: transparent;",
                                "批量视频列表 ({state.read().batch_video_state.items.len()})"
                            }
                        }
                        div {
                            style: "display: flex; flex-direction: column; gap: 10px; max-height: 320px; overflow-y: auto; padding-right: 4px;",
                            for (idx, item) in state.read().batch_video_state.items.iter().enumerate() {
                                div {
                                    style: "{state.read().theme.bg_card_white()} padding: 14px; transition: all 0.3s ease;",
                                    div {
                                        style: "display: flex; align-items: center; gap: 10px; margin-bottom: 8px;",
                                        div {
                                            style: "width: 26px; height: 26px; background: linear-gradient(135deg, #667eea 0%, #764ba2 100%); border-radius: 50%; text-align: center; line-height: 26px; font-size: 12px; color: white; font-weight: 700;",
                                            "{idx + 1}"
                                        }
                                        div {
                                            style: "font-size: 14px; font-weight: 600; color: {TEXT_PRIMARY}; flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;",
                                            "{item.title}"
                                        }
                                    }
                                    div {
                                        style: "font-size: 11px; color: {TEXT_MUTED}; padding-left: 36px;",
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
                                // 为每个下载项创建原子进度追踪 + 取消标志
                                let urls: Vec<String> = items.iter().map(|i| i.url.clone()).collect();
                                let progress_arcs: Vec<Option<Arc<AtomicU32>>> = items.iter().map(|_| Some(Arc::new(AtomicU32::new(0)))).collect();
                                let speed_arcs: Vec<Option<Arc<AtomicU64>>> = items.iter().map(|_| Some(Arc::new(AtomicU64::new(0)))).collect();
                                let cancel_flags: Vec<Option<Arc<AtomicBool>>> = items.iter().map(|_| Some(Arc::new(AtomicBool::new(false)))).collect();
                                let initial_items: Vec<BatchItemState> = items.iter().enumerate().map(|(idx, i)| BatchItemState {
                                    url: i.url.clone(),
                                    title: i.title.clone(),
                                    status: "pending".to_string(),
                                    progress: 0.0,
                                    speed: String::new(),
                                    visible: true,
                                    progress_arc: progress_arcs[idx].clone(),
                                    speed_arc: speed_arcs[idx].clone(),
                                    cancel_flag: cancel_flags[idx].clone(),
                                }).collect();
                                state.write().batch_state = BatchState {
                                    visible: true,
                                    progress: 0.0,
                                    status: format!("准备下载 {} 个视频", initial_items.len()),
                                    items: initial_items,
                                    is_downloading: true,
                                };
                                state.write().batch_video_state.visible = false;
                                let mut state = state;
                                spawn(async move {
                                    batch_download_impl(urls, progress_arcs, speed_arcs, cancel_flags).await;
                                    state.write().batch_state.visible = false;
                                    state.write().batch_state.is_downloading = false;
                                });
                            },
                            style: "{state.read().theme.btn_success(\"14px\", \"12px\", \"15px\")} flex: 1;",
                            "⬇️ 全部下载"
                        }
                        button {
                            onclick: move |_| {
                                state.write().batch_video_state.visible = false;
                            },
                            style: "{state.read().theme.btn_danger(\"14px\", \"12px\", \"15px\")} flex: 1;",
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
                            let total_bytes_arc = Arc::new(AtomicU64::new(0));
                            let cancel_flag = Arc::new(AtomicBool::new(false));
                            // 把 cancel_flag 存到 state 让 progress UI 能访问
                            state.write().cancel_flag = Some(cancel_flag.clone());
                            let progress_arc_clone = progress_arc.clone();
                            let speed_arc_clone = speed_arc.clone();
                            let total_bytes_arc_clone = total_bytes_arc.clone();
                            let cancel_flag_clone = cancel_flag.clone();

                            // 提前获取 format_id，避免 borrow 问题
                            let format_id = state.read().selected_format_id.clone();

                            // 使用 spawn 在后台异步执行下载
                            let mut state = state;
                            spawn(async move {
                                match start_download_impl(&url, None, &title, Some(progress_arc_clone), Some(speed_arc_clone), Some(total_bytes_arc_clone), Some(cancel_flag_clone), format_id, crate::types::DownloadStrategy::Native).await {
                                    Ok(_) => {
                                        state.write().is_downloading = false;
                                        state.write().cancel_flag = None;
                                        // Stream copy 合并很快，短暂显示合并状态
                                        state.write().status_message = "🔄 合并中...".to_string();
                                        // 等待 1.5 秒让 ffmpeg 完成 stream copy 合并
                                        tokio::time::sleep(tokio::time::Duration::from_millis(1500)).await;
                                        state.write().status_message = "Download completed".to_string();
                                        state.write().download_progress = 1.0;
                                        // 添加到历史记录
                                        add_completed_download(title.clone(), url.clone(), None);
                                        send_notification("Download Complete", &format!("{} has been downloaded successfully", title));
                                    }
                                    Err(e) => {
                                        state.write().is_downloading = false;
                                        state.write().cancel_flag = None;
                                        state.write().error_display = Some(e);
                                        state.write().status_message = "Download failed".to_string();
                                        send_notification("Download Failed", &title.to_string());
                                    }
                                }
                            });

                            // 轮询进度更新 UI（独立 task，500ms 间隔减少 UI 负载）
                            // 进度阈值：只有变化超过 1% 才更新 UI，减少不必要的渲染
                            let progress_arc_for_poll = progress_arc.clone();
                            let speed_arc_for_poll = speed_arc.clone();
                            let total_bytes_arc_for_poll = total_bytes_arc.clone();
                            let mut state_for_poll = state;
                            const PROGRESS_THRESHOLD: f32 = 0.01; // 1% 阈值
                            let mut last_updated_progress: f32 = 0.0;
                            let mut last_updated_size: u64 = 0;
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
                                    // 轮询下载速度（每 500ms 更新）
                                    let speed = speed_arc_for_poll.load(Ordering::Relaxed);
                                    state_for_poll.write().download_speed = format_speed(speed);
                                    // 轮询文件大小（仅在变化时更新，避免重复格式化）
                                    let total = total_bytes_arc_for_poll.load(Ordering::Relaxed);
                                    if total != last_updated_size && total > 0 {
                                        state_for_poll.write().download_file_size = format_file_size(total);
                                        last_updated_size = total;
                                    }
                                    if percent >= 1.0 {
                                        break;
                                    }
                                }
                            });
                        },
                        style: "{state.read().theme.btn_success(\"14px\", \"10px\", \"15px\")} flex: 1;",
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
                                state.write().error_display = Some(ErrorDisplay::new("parse", "请输入有效的 URL", "请输入视频链接后重试"));
                                return;
                            }

                            // 创建进度追踪原子变量 + 取消标志
                            let progress_arcs: Vec<Option<Arc<AtomicU32>>> = url_list.iter().map(|_| Some(Arc::new(AtomicU32::new(0)))).collect();
                            let speed_arcs: Vec<Option<Arc<AtomicU64>>> = url_list.iter().map(|_| Some(Arc::new(AtomicU64::new(0)))).collect();
                            let cancel_flags: Vec<Option<Arc<AtomicBool>>> = url_list.iter().map(|_| Some(Arc::new(AtomicBool::new(false)))).collect();

                            // 初始化批量状态（关联 progress_arc + cancel_flag）
                            let batch_items: Vec<BatchItemState> = url_list.iter().enumerate().map(|(idx, url)| {
                                BatchItemState {
                                    url: url.clone(),
                                    title: "视频".to_string(),
                                    status: "pending".to_string(),
                                    progress: 0.0,
                                    speed: String::new(),
                                    visible: true,
                                    progress_arc: progress_arcs[idx].clone(),
                                    speed_arc: speed_arcs[idx].clone(),
                                    cancel_flag: cancel_flags[idx].clone(),
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
                                        // 更新单个视频进度 + 速度
                                        if idx < state_for_poll.read().batch_state.items.len() {
                                            let speed_arc_clone = state_for_poll.read().batch_state.items[idx].speed_arc.clone();
                                            state_for_poll.write().batch_state.items[idx].progress = percent;
                                            if let Some(speed_arc) = speed_arc_clone {
                                                let speed = speed_arc.load(Ordering::Relaxed);
                                                state_for_poll.write().batch_state.items[idx].speed = format_speed(speed);
                                            }
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
                                batch_download_impl(url_list, progress_arcs, speed_arcs, cancel_flags).await;
                            });
                        },
                        style: "{state.read().theme.btn_solid(state.read().theme.color_primary(), \"12px\", \"6px\", \"14px\")} flex: 1; font-weight: 500;",
                        "Batch Download"
                    }
                }
            }

            // 进度显示
            if state.read().is_downloading && !state.read().batch_state.visible {
                WhiteCard {
                    shadow: CardShadow::Elevated,
                    div {
                        style: "display: flex; justify-content: space-between; align-items: center; margin-bottom: 10px;",
                        div {
                            style: "font-size: 15px; font-weight: 600; color: {state.read().theme.color_success()};",
                            "📥 下载中... {(state.read().download_progress * 100.0).round() as i32}%"
                        }
                        div {
                            style: "font-size: 12px; color: {TEXT_MUTED};",
                            "{state.read().download_speed}"
                        }
                    }
                    div {
                        style: "height: 12px; background: #e8e8e8; border-radius: 6px; overflow: hidden;",
                        div {
                            style: "height: 100%; width: {state.read().download_progress * 100.0}%; background: {state.read().theme.progress_gradient_success()}; border-radius: 6px; transition: width 0.3s ease;",
                        }
                    }
                    if !state.read().download_file_size.is_empty() {
                        div {
                            style: "font-size: 12px; color: {TEXT_MUTED}; margin-top: 8px;",
                            "📦 大小: {state.read().download_file_size}"
                        }
                    }
                    // 取消按钮（仅在 cancel_flag 存在时显示）
                    if state.read().cancel_flag.is_some() {
                        div {
                            style: "display: flex; justify-content: flex-end; margin-top: 10px;",
                            DestructivePillButton {
                                onclick: move |_| {
                                    if let Some(flag) = state.read().cancel_flag.clone() {
                                        flag.store(true, Ordering::Relaxed);
                                    }
                                },
                                "🛑 取消下载"
                            }
                        }
                    }
                }
            }

            // 批量下载进度显示
            if state.read().batch_state.visible {
                WhiteCard {
                    shadow: CardShadow::Elevated,
                    div {
                        style: "display: flex; justify-content: space-between; align-items: center; margin-bottom: 10px;",
                        div {
                            style: "font-size: 16px; font-weight: 700; background: {state.read().theme.gradient()}; -webkit-background-clip: text; -webkit-text-fill-color: transparent;",
                            "📚 批量下载 - {(state.read().batch_state.progress * 100.0).round() as i32}%"
                        }
                        button {
                            onclick: move |_| {
                                state.write().batch_state.visible = false;
                                state.write().batch_state.is_downloading = false;
                            },
                            style: "{state.read().theme.btn_danger(\"6px 14px\", \"8px\", \"12px\")}",
                            "⏹️ 取消"
                        }
                    }
                    div {
                        style: "height: 10px; {STYLE_PROGRESS_TRACK} margin-top: 8px;",
                        div {
                            style: "height: 100%; width: {state.read().batch_state.progress * 100.0}%; background: {state.read().theme.gradient()}; border-radius: 5px; transition: width 0.3s ease;",
                        }
                    }
                    div {
                        style: "font-size: 12px; color: {TEXT_MUTED}; margin-top: 6px;",
                        "{state.read().batch_state.status}"
                    }
                    // 各视频进度列表
                    div {
                        style: "display: flex; flex-direction: column; gap: 10px; margin-top: 16px;",
                        for (idx, item) in state.read().batch_state.items.iter().enumerate() {
                            if item.visible {
                                div {
                                    style: "display: flex; align-items: center; gap: 12px; padding: 12px; background: #f8f9fa; border-radius: 10px; transition: all 0.3s ease;",
                                    div {
                                        style: "height: 10px; {STYLE_PROGRESS_TRACK} flex: 1;",
                                        div {
                                            style: "height: 100%; width: {item.progress * 100.0}%; background: {state.read().theme.gradient()}; border-radius: 5px; transition: width 0.3s ease;",
                                        }
                                    }
                                    div {
                                        style: "width: 45px; font-size: 12px; font-weight: 600; color: {state.read().theme.color_accent()}; text-align: right;",
                                        "{(item.progress * 100.0).round() as i32}%"
                                    }
                                    div {
                                        style: "font-size: 11px; color: {TEXT_MUTED};",
                                        "{item.status}"
                                    }
                                    // 单项取消按钮（仅 pending/downloading 时显示）
                                    if item.status == "pending" || item.status == "downloading" {
                                        button {
                                            onclick: move |_| {
                                                let items = state.read().batch_state.items.clone();
                                                if let Some(it) = items.get(idx) {
                                                    if let Some(flag) = it.cancel_flag.clone() {
                                                        flag.store(true, Ordering::Relaxed);
                                                    }
                                                }
                                            },
                                            style: "{STYLE_BTN_ICON} width: 24px; height: 24px; background: {state.read().theme.color_danger()}; font-size: 12px; display: flex; align-items: center; justify-content: center; padding: 0;",
                                            "✕"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // 历史记录面板
            if state.read().show_history {
                WhiteCard {
                    shadow: CardShadow::Soft,
                    scrollable: true,
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
                        div {
                            style: "display: flex; gap: 8px;",
                            DestructivePillButton {
                                onclick: move |_| {
                                    // 清空全局历史
                                    clear_global_history();
                                    // 清空 UI 状态中的历史
                                    state.write().history.clear();
                                    state.write().show_history = false;
                                },
                                "🗑️ 清空历史"
                            }
                            CloseButton {
                                onclick: move |_| { state.write().show_history = false; },
                            }
                        }
                    }
                    if state.read().history.is_empty() {
                        div {
                            style: "text-align: center; padding: 40px; color: {TEXT_MUTED};",
                            div { style: "font-size: 48px; margin-bottom: 12px;", "📭" }
                            div { style: "font-size: 14px;", "暂无下载记录" }
                        }
                    } else {
                        div {
                            style: "display: flex; flex-direction: column; gap: 10px;",
                            for item in state.read().history.iter().rev().cloned() {
                                div {
                                    onclick: move |_| {
                                        // 点击历史记录 → 回填 URL
                                        state.write().url = item.url.clone();
                                        state.write().error_display = None;
                                    },
                                    style: "{state.read().theme.bg_card_white()} display: flex; align-items: center; gap: 12px; padding: 12px; cursor: pointer; transition: transform 0.15s, box-shadow 0.15s;",
                                    div {
                                        style: "{state.read().theme.bg_icon_success()} width: 40px; height: 40px; border-radius: 10px; display: flex; align-items: center; justify-content: center;",
                                        div { style: "font-size: 18px;", "✅" }
                                    }
                                    div {
                                        style: "flex: 1; min-width: 0;",
                                        div {
                                            style: "font-size: 13px; font-weight: 600; color: {TEXT_PRIMARY}; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;",
                                            "{item.title}"
                                        }
                                        div {
                                            style: "font-size: 11px; color: {TEXT_MUTED}; margin-top: 2px;",
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
