//! 设置面板 (⚙️) — 提取自原 app.rs:433-556
//!
//! 输出目录 / msToken / Cookie 文件 / 并发下载数设置。
//! ⚙️ 按钮本身留在 app.rs TopBar (settings 切换显示),
//! 本组件只负责设置面板内容(由 AppComponent 在 show_settings=true 时挂载)。

use std::path::PathBuf;

use dioxus::prelude::*;

use super::components::{
    BrowseButton, CardShadow, CloseButton, SettingsNumberInput, SettingsTextInput, WhiteCard,
};
use super::theme::{BG_SUBTLE, TEXT_PRIMARY, TEXT_SECONDARY};
use super::types::{AppState, ErrorDisplay};
use crate::config::AppConfig;

/// 设置面板(内容部分,不含 ⚙️ 按钮)
#[component]
pub(super) fn SettingsPanel(state: Store<AppState>) -> Element {
    rsx! {
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
}
