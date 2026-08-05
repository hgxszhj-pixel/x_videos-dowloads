//! 批量下载进度面板 — 提取自原 app.rs:1021-1103
//!
//! 实时显示批量下载总进度 + 各视频子进度 + 单项 ✕ 取消。
//! 渲染条件由 AppComponent 控制 (state.read().batch_state.visible),
//! 本组件内部不重复 guard。

use std::sync::atomic::Ordering;

use dioxus::prelude::*;

use super::components::{CardShadow, WhiteCard};
use super::theme::{STYLE_BTN_ICON, STYLE_PROGRESS_TRACK, TEXT_MUTED};
use super::types::AppState;

/// 批量下载进度面板
#[component]
pub(super) fn BatchProgressPanel(state: Store<AppState>) -> Element {
    rsx! {
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
                        // 遍历 items[*].cancel_flag 设 true,后台 task 真正停止(不再继续下载/更新已隐藏的 state)
                        let items = state.read().batch_state.items.clone();
                        for item in &items {
                            if let Some(flag) = &item.cancel_flag {
                                flag.store(true, Ordering::Relaxed);
                            }
                        }
                        state.write().batch_state.visible = false;
                        // 同步 canonical flag 让 poller 退出
                        state.write().is_downloading = false;
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
                                        // 单次 bounds check + get_mut 避免 list 变化后 items[idx] 越界 panic
                                        let mut s = state.write();
                                        if let Some(it) = s.batch_state.items.get_mut(idx) {
                                            if let Some(flag) = it.cancel_flag.clone() {
                                                flag.store(true, Ordering::Relaxed);
                                            }
                                            // 乐观 UI 更新：立即显示 cancelled，不等后台任务结束
                                            it.status = "cancelled".to_string();
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
}
