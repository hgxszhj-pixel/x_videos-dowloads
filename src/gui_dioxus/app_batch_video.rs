//! 批量视频列表面板 — 提取自原 app.rs:611-704
//!
//! 显示批量解析的视频列表 (idx + title + uploader + duration + 画质数),
//! 提供 "⬇️ 全部下载" + "⏹️ 取消" 按钮。
//! 渲染条件由 AppComponent 控制 (state.read().batch_video_state.visible),
//! 本组件内部不重复 guard。

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64};
use std::sync::Arc;

use dioxus::prelude::*;

use super::components::{CardShadow, WhiteCard};
use super::handlers::batch_download_impl;
use super::theme::{TEXT_MUTED, TEXT_PRIMARY};
use super::types::{AppState, BatchItemState, BatchState};

/// 批量视频列表面板 (含下载/取消按钮)
#[component]
pub(super) fn BatchVideoPanel(state: Store<AppState>) -> Element {
    rsx! {
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
                                style: "width: 26px; height: 26px; background: {state.read().theme.gradient()}; border-radius: 50%; text-align: center; line-height: 26px; font-size: 12px; color: white; font-weight: 700;",
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
                    let initial_items: Vec<BatchItemState> = items.iter().enumerate().map(|(idx, _i)| BatchItemState {
                        status: "pending".to_string(),
                        progress: 0.0,
                        speed: String::new(),
                        visible: true,
                        speed_arc: speed_arcs[idx].clone(),
                        cancel_flag: cancel_flags[idx].clone(),
                    }).collect();
                    state.write().batch_state = BatchState {
                        visible: true,
                        progress: 0.0,
                        status: format!("准备下载 {} 个视频", initial_items.len()),
                        items: initial_items,
                    };
                    // 同步 canonical flag,poller.rs:66 gate 才能正常轮询
                    state.write().is_downloading = true;
                    state.write().batch_video_state.visible = false;
                    let mut state = state;
                    spawn(async move {
                        batch_download_impl(urls, progress_arcs, speed_arcs, cancel_flags).await;
                        state.write().batch_state.visible = false;
                        state.write().is_downloading = false;
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
}