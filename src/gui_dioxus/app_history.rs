//! 历史记录面板 (📜) — 提取自原 app.rs:1105-1174
//!
//! 显示下载历史,支持点击回填 URL + 清空全部。
//! 渲染条件由 AppComponent 控制 (state.read().show_history),
//! 本组件内部不重复 guard。

use dioxus::prelude::*;

use super::components::{CardShadow, CloseButton, DestructivePillButton, WhiteCard};
use super::store::clear_global_history;
use super::theme::{TEXT_MUTED, TEXT_PRIMARY};
use super::types::AppState;

/// 历史记录面板
#[component]
pub(super) fn HistoryPanel(state: Store<AppState>) -> Element {
    rsx! {
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