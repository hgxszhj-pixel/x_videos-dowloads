//! 主应用组件（Dioxus RSX 视图）

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

use dioxus::prelude::*;
use notify::{RecursiveMode, Watcher};
use std::sync::mpsc::RecvTimeoutError;
use std::time::Duration;

use crate::config::AppConfig;

use super::actions::{
    sync_course_action_to_state, sync_fetch_action_to_state, use_fetch_action,
    use_fetch_course_action,
};
use super::app_batch_progress::BatchProgressPanel;
use super::app_batch_video::BatchVideoPanel;
use super::app_course_pool::CoursePoolPanel;
use super::app_video_card::VideoInfoCard;
use super::components::{
    CardShadow, DestructivePillButton, ErrorPanel, ThemeSwatch, UrlInput, WhiteCard,
};
use super::handlers::{batch_download_impl, open_tuling_login, start_download_impl};
use super::poller::{use_progress_poller, PollMsg};
use super::store::{
    add_completed_download, get_history_items, persist_theme, reload_config, send_notification,
};
use super::theme::AppTheme;
use super::theme::{BG_SUBTLE, TEXT_MUTED};
use super::types::{
    AppState, BatchItemState, BatchState, CourseCollectionState, ErrorDisplay, VideoInfoDisplay,
};

/// 主应用组件
#[component]
pub(super) fn AppComponent() -> Element {
    let mut state = use_store(AppState::default);
    let video_info = use_signal(VideoInfoDisplay::default);

    // 进度信号(P0-2):独立 Signal 而非 store 字段,单字段写只触发依赖该 Signal 的
    // 进度条区域重渲染,不再因 progress/speed/size 变更而刷新整个 251 行 rsx。
    let mut progress_signal = use_signal(|| 0.0_f32);
    let mut speed_signal: Signal<String> = use_signal(String::new);
    let mut size_signal: Signal<String> = use_signal(String::new);
    // 剩余时间(由 poller.rs 根据 total_bytes/speed/progress 算好再写入,
    // 避免 UI 里反向解析字符串或重复读原子)
    let mut eta_signal: Signal<String> = use_signal(String::new);

    // 主题 Signal — components.rs 中 ThemeSwatch / ErrorPanel / 另一组件
    // 通过 use_context::<Signal<AppTheme>>() 读当前主题,避免订阅整个 AppState Store。
    // 这里必须提供(Signal<AppTheme> 是 Copy,不会 move 原 binding)。
    let mut theme_signal: Signal<AppTheme> = use_signal(|| state.read().theme.clone());
    use_context_provider(|| theme_signal);

    // 进度轮询 coroutine — 替代手写 spawn(async move { loop { ... } }) 轮询循环
    // P0-2: progress_signal/speed_signal/size_signal 经 PollMsg::Single 由下载按钮传,
    // use_progress_poller 不再需要这 3 个参数
    let poller = use_progress_poller(state);

    // config watcher shutdown flag — use_drop 必须在 render 期注册(Dioxus 0.7 hook placement),
    // 在 use_effect 内调用会违反 Rules of Hooks 并导致 cleanup 不可靠。
    // 实际 Arc 在下方 use_effect 里创建,通过此 signal 桥传给 use_drop。
    let mut watcher_shutdown: Signal<Option<Arc<AtomicBool>>> = use_signal(|| None);
    use_drop(move || {
        if let Some(flag) = watcher_shutdown.cloned() {
            flag.store(true, Ordering::Relaxed);
        }
    });

    // fetch action — 替代手写 submit_urls 闭包（latest-wins 自动 cancel）
    let mut fetch_action = use_fetch_action();
    sync_fetch_action_to_state(state, video_info, fetch_action);

    // 课程池 action — 与 fetch 互斥：URL 提交时先尝试 course，失败回退到单视频
    let mut course_action = use_fetch_course_action();
    sync_course_action_to_state(state, fetch_action, course_action);

    // 课程池进度轮询由 app_course_pool.rs:142 的 polling task 负责(2026-08-09 修复,
    // 带聚合 + 终态退出 + mount-once guard)。这里故意不重复 spawn,避免双 polling
    // 各跑一遍相同 slot 原子读 + store 写,造成 33 项 × 2 = 66 次冗余 op/500ms。

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
                // shutdown flag 通过 component-scoped signal 注册,use_drop 在 AppComponent 卸载时设 true
                let shutdown = Arc::new(AtomicBool::new(false));
                watcher_shutdown.set(Some(shutdown.clone()));
                let shutdown_thread = shutdown;

                std::thread::spawn(move || {
                    let (tx, rx) = std::sync::mpsc::channel();
                    let mut watcher = match notify::recommended_watcher(
                        move |res: Result<notify::Event, notify::Error>| {
                            // 之前 .ok() 静默吞掉 watcher 回调错误,改成显式 warn
                            if let Err(e) = res {
                                tracing::warn!("config watcher 回调错误: {}", e);
                                return;
                            }
                            let _ = tx.send(());
                        },
                    ) {
                        Ok(w) => Some(w),
                        Err(e) => {
                            tracing::warn!(
                                "无法创建 config watcher: {} (config 热重载将不可用)",
                                e
                            );
                            None
                        }
                    };

                    if let Some(ref mut w) = watcher {
                        if let Err(e) = w.watch(&parent, RecursiveMode::NonRecursive) {
                            tracing::warn!("config watcher watch({:?}) 失败: {}", parent, e);
                        }
                    }

                    // 500ms 超时循环 + shutdown 检查，避免 for _ in rx 永久阻塞
                    while !shutdown_thread.load(Ordering::Relaxed) {
                        match rx.recv_timeout(Duration::from_millis(500)) {
                            Ok(()) => reload_config(),
                            Err(RecvTimeoutError::Timeout) => continue,
                            Err(RecvTimeoutError::Disconnected) => break,
                        }
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

    // 触发 fetch action 的薄包装：清除旧 UI 状态 + 启动 action
    // use_action 自动 cancel 上一 in-flight（latest-wins）
    let mut trigger_fetch = move || {
        // 启动时清掉旧 UI 状态
        state.write().is_fetching = true;
        state.write().status_message = "Fetching...".to_string();
        state.write().error_display = None;
        state.write().has_video_info = false;
        fetch_action.call(state.read().url.clone());
    };

    // 触发课程池分析：成功后渲染下载池；失败回退到单视频
    let mut trigger_course = move || {
        if course_action.pending() {
            // 已有一次分析在跑（最新一次获胜，旧任务会被 cancel）
            // 二次点击不重复触发，避免并发 Chromium 锁 profile 失败
            return;
        }
        let url = state.read().url.clone();
        if url.is_empty() {
            return;
        }
        // profile 被其他 Chrome 实例占用时直接提示，避免启动崩溃
        let profile = crate::drission_client::default_profile_dir();
        if crate::drission_client::is_profile_dir_in_use(&profile) {
            state.write().error_display = Some(ErrorDisplay::new(
                "login",
                "浏览器 profile 被占用",
                "请先关闭本应用专用 headed 登录窗口，或结束残留的 Chrome 进程后重试。",
            ));
            return;
        }
        state.write().is_fetching = true;
        state.write().status_message = "分析课程中...".to_string();
        state.write().error_display = None;
        state.write().course_collection_state = CourseCollectionState::default();
        course_action.call(url);
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
                                theme_signal.set(AppTheme::Purple);
                                persist_theme("Purple");
                            },
                        }
                        ThemeSwatch {
                            theme: AppTheme::Ocean,
                            onclick: move |_| {
                                state.write().theme = AppTheme::Ocean;
                                theme_signal.set(AppTheme::Ocean);
                                persist_theme("Ocean");
                            },
                        }
                        ThemeSwatch {
                            theme: AppTheme::Sunset,
                            onclick: move |_| {
                                state.write().theme = AppTheme::Sunset;
                                theme_signal.set(AppTheme::Sunset);
                                persist_theme("Sunset");
                            },
                        }
                        ThemeSwatch {
                            theme: AppTheme::Forest,
                            onclick: move |_| {
                                state.write().theme = AppTheme::Forest;
                                theme_signal.set(AppTheme::Forest);
                                persist_theme("Forest");
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
                        on_submit: move |_: dioxus::events::KeyboardEvent| { trigger_fetch(); },
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
                        on_submit: move |_: dioxus::events::KeyboardEvent| { trigger_fetch(); },
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
                        onclick: move |_| trigger_fetch(),
                        style: "width: 140px; background: {state.read().theme.gradient()}; color: white; border: none; border-radius: 10px; font-size: 14px; font-weight: 600; box-shadow: 0 4px 15px rgba(0,0,0,0.2); transition: all 0.3s ease;",
                        if state.read().is_fetching { "加载中..." } else { "📥 获取信息" }
                    }
                    button {
                        onclick: move |_| trigger_course(),
                        disabled: (course_action.pending() || state.read().is_logging_in).then_some("disabled"),
                        style: if course_action.pending() || state.read().is_logging_in {
                            "padding: 0 20px; background: #4a4a6a; color: #aaa; border: none; border-radius: 10px; font-size: 14px; font-weight: 600; cursor: not-allowed;"
                        } else {
                            "padding: 0 20px; background: #6f5cff; color: white; border: none; border-radius: 10px; font-size: 14px; font-weight: 600; box-shadow: 0 4px 15px rgba(0,0,0,0.2); transition: all 0.3s ease;"
                        },
                        if course_action.pending() { "⏳ 分析中..." } else { "📚 分析课程" }
                    }
                    button {
                        onclick: move |_| {
                            let mut s = state;
                            if s.read().is_logging_in {
                                return;
                            }
                            let target_url = s.read().url.clone();
                            s.write().is_logging_in = true;
                            s.write().status_message = "正在打开 ego-lite(请在弹出窗口扫码,180s 自动关闭)...".to_string();
                            spawn(async move {
                                let result = open_tuling_login(&target_url).await;
                                s.write().is_logging_in = false;
                                match result {
                                    Ok(_) => {
                                        s.write().status_message = "登录窗口已关闭，请重新分析课程".to_string();
                                    }
                                    Err(e) => {
                                        s.write().error_display = Some(ErrorDisplay::new(
                                            "login",
                                            format!("打开 ego-lite 失败：{e}"),
                                            "请检查系统是否已安装 ego-lite (https://github.com/citrolabs/ego-lite) 或清理残留进程。",
                                        ));
                                    }
                                }
                            });
                        },
                        disabled: (state.read().is_logging_in || course_action.pending()).then_some("disabled"),
                        style: if state.read().is_logging_in || course_action.pending() {
                            "padding: 0 20px; background: #8a6420; color: #ccc; border: none; border-radius: 10px; font-size: 14px; font-weight: 600; cursor: not-allowed;"
                        } else {
                            "padding: 0 20px; background: #ffb547; color: white; border: none; border-radius: 10px; font-size: 14px; font-weight: 600; box-shadow: 0 4px 15px rgba(0,0,0,0.2); transition: all 0.3s ease;"
                        },
                        if state.read().is_logging_in { "⏳ 登录窗口中..." }
                        else if course_action.pending() { "🔒 分析中..." }
                        else { "🔑 打开登录窗口" }
                    }
                    button {
                        onclick: move |_| {
                            // 单事务写:5 个字段一次性提交,避免多次 state.write() micro-tx
                            // 让 ErrorPanel / Status 等 reactive 订阅者漏掉某次更新
                            let mut s = state.write();
                            s.url = String::new();
                            s.error_display = None;
                            s.status_message = "Enter video URL".to_string();
                            s.has_video_info = false;
                            s.course_collection_state = CourseCollectionState::default();
                        },
                        style: "padding: 0 20px; background: #f5f5f5; border: none; border-radius: 10px; font-size: 14px; color: #666; transition: all 0.3s ease;",
                        "🗑️ 清空"
                    }
                }
            }

            // 错误提示 - 结构化（ErrorDisplay 类型驱动，抽到 ErrorPanel 组件）
            ErrorPanel { state }

            // 设置面板 - 拆到 app_settings.rs (Step 2)
            if state.read().show_settings {
                super::app_settings::SettingsPanel { state }
            }

            // 视频信息 - 拆到 app_video_card.rs (Step 3)
            if state.read().has_video_info {
                VideoInfoCard { state, video_info }
            }

            // 批量视频信息显示 - 拆到 app_batch_video.rs (Step 4)
            if state.read().batch_video_state.visible {
                BatchVideoPanel { state, poller }
            }

            // 课程池下载面板 - 拆到 app_course_pool.rs (Step 6)
            if state.read().course_collection_state.visible {
                CoursePoolPanel { state, progress_signal }
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
                            // P0-2: 清空进度 Signal 而非 store 字段(保持兼容)
                            progress_signal.set(0.0);
                            speed_signal.set(String::new());
                            size_signal.set(String::new());
                            eta_signal.set(String::new());

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
                            // P0-2:克隆进度 Signal 句柄供 spawn 内 + poller 使用
                            let mut progress_signal_for_task = progress_signal;
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
                                        progress_signal_for_task.set(1.0);
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

                            // 启动单下载进度轮询（use_coroutine 调度，500ms 间隔，1% 阈值过滤）
                            poller.send(PollMsg::Single {
                                progress_arc: progress_arc.clone(),
                                speed_arc: speed_arc.clone(),
                                total_bytes_arc: total_bytes_arc.clone(),
                                progress_signal,
                                speed_signal,
                                size_signal,
                                eta_signal,
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
                            let batch_items: Vec<BatchItemState> = url_list.iter().enumerate().map(|(idx, _url)| {
                                BatchItemState {
                                    status: "pending".to_string(),
                                    progress: 0.0,
                                    speed: String::new(),
                                    visible: true,
                                    speed_arc: speed_arcs[idx].clone(),
                                    cancel_flag: cancel_flags[idx].clone(),
                                }
                            }).collect();

                            state.write().batch_state = BatchState {
                                visible: true,
                                progress: 0.0,
                                status: format!("准备下载 {} 个视频", url_list.len()),
                                items: batch_items,
                                aggregate_speed: String::new(),
                            };
                            // 同步 canonical flag,poller.rs:66 gate 才能正常轮询
                            state.write().is_downloading = true;

                            // 克隆 arc 供轮询使用
                            let progress_arcs_for_poll: Vec<Arc<AtomicU32>> = progress_arcs.iter().filter_map(|a| a.clone()).collect();
                            let total_count = url_list.len();

                            // 启动批量下载进度轮询（use_coroutine 调度）
                            poller.send(PollMsg::Batch {
                                progress_arcs: progress_arcs_for_poll,
                                total_count,
                            });

                            // 使用 spawn 在后台异步执行批量下载
                            let mut state = state;
                            spawn(async move {
                                batch_download_impl(url_list, progress_arcs, speed_arcs, cancel_flags).await;
                                state.write().batch_state.visible = false;
                                state.write().is_downloading = false;
                            });
                        },
                        style: "{state.read().theme.btn_solid(state.read().theme.color_primary(), \"12px\", \"6px\", \"14px\")} flex: 1; font-weight: 500;",
                        "Batch Download"
                    }
                }

            // 进度显示 — P0-2: 进度条读 Signal 而非 store 字段,
            // 重渲染范围从整个 AppComponent 缩到仅本区域
            if state.read().is_downloading && !state.read().batch_state.visible {
                WhiteCard {
                    shadow: CardShadow::Elevated,
                    div {
                        style: "display: flex; justify-content: space-between; align-items: center; margin-bottom: 10px;",
                        div {
                            style: "font-size: 15px; font-weight: 600; color: {state.read().theme.color_success()};",
                            "📥 下载中... {(progress_signal.cloned() * 100.0).round() as i32}%"
                        }
                        div {
                            style: "display: flex; gap: 12px; font-size: 12px; color: {TEXT_MUTED}; align-items: center;",
                            span { "⚡ {speed_signal.cloned()}" }
                            if !eta_signal.cloned().is_empty() {
                                span {
                                    style: "color: {state.read().theme.color_accent()}; font-weight: 600;",
                                    "⏱️ ETA {eta_signal.cloned()}"
                                }
                            }
                        }
                    }
                    div {
                        style: "height: 12px; background: #e8e8e8; border-radius: 6px; overflow: hidden;",
                        div {
                            style: "height: 100%; width: {progress_signal.cloned() * 100.0}%; background: {state.read().theme.progress_gradient_success()}; border-radius: 6px; transition: width 0.3s ease;",
                        }
                    }
                    if !size_signal.cloned().is_empty() {
                        div {
                            style: "font-size: 12px; color: {TEXT_MUTED}; margin-top: 8px;",
                            "📦 大小: {size_signal.cloned()}"
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

            // 批量下载进度显示 - 拆到 app_batch_progress.rs (Step 5)
            if state.read().batch_state.visible {
                BatchProgressPanel { state }
            }

            // 历史记录面板 - 拆到 app_history.rs (Step 1)
            if state.read().show_history {
                super::app_history::HistoryPanel { state }
            }
        }
    }
}
