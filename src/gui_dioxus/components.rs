//! 可复用 Dioxus UI 组件
//!
//! 抽离自 `app.rs` 中重复出现的 RSX 模式（Card 容器 / 主题切换 swatch / 关闭 ×
//! / 浏览按钮 / 危险红胶囊 / URL 输入 / 设置文本 / 设置数字输入）。
//! 强类型 props 接管样式细节，避免外层 inline style 字符串蔓延。

use dioxus::events::{FormEvent, KeyboardEvent};
use dioxus::prelude::*;

use super::theme::{
    AppTheme, ALERT_RED, BORDER_DEFAULT, BG_SUBTLE, STYLE_BTN_BASE, STYLE_INPUT_BARE,
    TEXT_MUTED,
};
use super::types::AppState;

// ============================================================================
// WhiteCard — 半透明白色内容卡片
// ============================================================================

/// Card 阴影强度
#[derive(Clone, Copy, PartialEq)]
pub(super) enum CardShadow {
    /// 4px/20px/0.08 — 视频信息 / 批量列表 / 历史
    Soft,
    /// 8px/32px/0.12 — 设置 / 进度面板
    Elevated,
}

impl CardShadow {
    fn box_shadow(&self) -> &'static str {
        match self {
            CardShadow::Soft => "0 4px 20px rgba(0,0,0,0.08)",
            CardShadow::Elevated => "0 8px 32px rgba(0,0,0,0.12)",
        }
    }
}

/// 半透明白色内容卡片容器；6 处重复 pattern 抽于此（Settings / Video info / Batch list / Single progress / Batch progress / History）
#[component]
pub(super) fn WhiteCard(
    children: Element,
    #[props(default = CardShadow::Soft)] shadow: CardShadow,
    #[props(default = false)] scrollable: bool,
) -> Element {
    let scroll_style = if scrollable {
        "max-height: 400px; overflow-y: auto;"
    } else {
        ""
    };
    let style = format!(
        "padding: 20px; background: rgba(255,255,255,0.98); border-radius: 16px; box-shadow: {}; margin-top: 12px;{}",
        shadow.box_shadow(),
        scroll_style,
    );
    rsx! {
        div {
            style: "{style}",
            {children}
        }
    }
}

// ============================================================================
// ThemeSwatch — 圆形主题色切换按钮（28×28）
// ============================================================================

/// 顶部 4 个主题切换 swatch（紫 / 海洋 / 日落 / 森林）
/// 通过 `use_context::<Signal<AppTheme>>()` 读当前主题，
/// 当前主题等于 prop 时加 box-shadow ring 标记选中状态（Dioxus 0.7 lens 细粒度 reactivity）
#[component]
pub(super) fn ThemeSwatch(theme: AppTheme, onclick: EventHandler<MouseEvent>) -> Element {
    let current = use_context::<Signal<AppTheme>>();
    let is_selected = current.cloned() == theme;
    let ring = if is_selected {
        "box-shadow: 0 0 0 3px {ALERT_RED};"
    } else {
        ""
    };
    rsx! {
        button {
            onclick: move |e| onclick.call(e),
            style: "width: 28px; height: 28px; border-radius: 50%; border: 2px solid {theme.border_color()}; background: {theme.gradient()}; cursor: pointer; {ring}",
        }
    }
}

// ============================================================================
// CloseButton — 关闭 × 按钮（28×28 中性灰）
// ============================================================================

/// 关闭按钮（×）— Settings panel 和 History panel 顶部右上角（2 处）
#[component]
pub(super) fn CloseButton(onclick: EventHandler<MouseEvent>) -> Element {
    rsx! {
        button {
            onclick: move |e| onclick.call(e),
            style: "width: 28px; height: 28px; border-radius: 50%; border: none; background: {BG_SUBTLE}; color: {TEXT_MUTED}; font-size: 14px; cursor: pointer; display: flex; align-items: center; justify-content: center;",
            "×"
        }
    }
}

// ============================================================================
// ErrorPanel — 结构化错误提示（icon + 类型 + message + suggestion + 重试）
// ============================================================================

/// 结构化错误面板（替换 app.rs 中 38 行 inline RSX）
/// 主题通过 `use_context::<Signal<AppTheme>>()` 读取；点击"重试"清空 `error_display` + 重置 `status_message`
#[component]
pub(super) fn ErrorPanel(state: Store<AppState>) -> Element {
    let theme = use_context::<Signal<AppTheme>>();
    let t = theme.read();
    let err_opt = state.read().error_display.clone();

    match err_opt {
        Some(err) => {
            // 把 state rebind 为 mut，以便 move 进 onclick 闭包
            let mut state = state;
            // 提早求值样式字符串（避免与 `t` 的 immutable borrow 冲突）
            let bg_style = t.bg_error();
            let danger = t.color_danger();
            let text_danger = t.color_text_danger();
            let btn_style = t.btn_solid(t.color_danger(), "6px 14px", "16px", "12px");
            let icon = err.icon();
            rsx! {
                div {
                    style: "{bg_style} margin-top: 12px;",
                    div {
                        style: "display: flex; align-items: flex-start; gap: 12px;",
                        div {
                            style: "font-size: 24px; line-height: 1;",
                            "{icon}"
                        }
                        div {
                            style: "flex: 1;",
                            div {
                                style: "font-size: 14px; font-weight: 700; color: {danger}; margin-bottom: 4px;",
                                "下载失败 · {err.error_type}"
                            }
                            div {
                                style: "font-size: 13px; color: {text_danger};",
                                "{err.message}"
                            }
                            if !err.suggestion.is_empty() {
                                div {
                                    style: "font-size: 12px; color: #856404; background: #fff3cd; padding: 8px; border-radius: 4px; margin-top: 8px;",
                                    "💡 {err.suggestion}"
                                }
                            }
                            button {
                                onclick: move |_| {
                                    state.write().error_display = None;
                                    state.write().status_message = "Ready".to_string();
                                },
                                style: "{btn_style} margin-top: 10px;",
                                "🔄 重试 / 清除"
                            }
                        }
                    }
                }
            }
        }
        None => rsx! {},
    }
}

// ============================================================================
// BrowseButton — 蓝色实心浏览按钮（文件 / 目录选择）
// ============================================================================

/// 设置面板里"浏览..."按钮（输出目录 / Cookie 文件 2 处共享 `btn_solid(color_primary, ...)` 样式）
/// 主题通过 `use_context::<Signal<AppTheme>>()` 读取（AppComponent 提供）
#[component]
pub(super) fn BrowseButton(
    label: String,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    let theme = use_context::<Signal<AppTheme>>();
    let t = theme.read();
    let style = t.btn_solid(t.color_primary(), "6px 12px", "6px", "12px");
    rsx! {
        button {
            onclick: move |e| onclick.call(e),
            style: "{style}",
            "{label}"
        }
    }
}

// ============================================================================
// DestructivePillButton — 危险红胶囊按钮（取消 / 清空 destructive action）
// ============================================================================

/// 危险红胶囊按钮（取消下载 / 清空历史 2 处共享 `#ff4444` radius-16 样式）
#[component]
pub(super) fn DestructivePillButton(
    children: Element,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        button {
            onclick: move |e| onclick.call(e),
            style: "{STYLE_BTN_BASE} padding: 6px 14px; background: {ALERT_RED}; border-radius: 16px; font-size: 12px; font-weight: 600;",
            {children}
        }
    }
}

// ============================================================================
// UrlInput — 顶层 URL 输入框（裸 input，无 border，Enter 提交）
// ============================================================================

/// 顶层 URL 输入框（嵌在外层带边框的 div 中，input 本身裸，Enter 触发 submit）
#[component]
pub(super) fn UrlInput(
    placeholder: String,
    value: String,
    on_input: EventHandler<FormEvent>,
    on_submit: EventHandler<KeyboardEvent>,
    #[props(default = "15px".to_string())] font_size: String,
) -> Element {
    rsx! {
        input {
            r#type: "text",
            placeholder: "{placeholder}",
            value: "{value}",
            oninput: move |e| on_input.call(e),
            onkeydown: move |e| {
                if e.key() == Key::Enter {
                    on_submit.call(e);
                }
            },
            style: "{STYLE_INPUT_BARE} font-size: {font_size}; background: transparent;"
        }
    }
}

// ============================================================================
// SettingsTextInput — 设置面板文本输入框（完整 border + radius）
// ============================================================================

/// 设置面板文本输入框（output dir / msToken / cookie file 等）
#[component]
pub(super) fn SettingsTextInput(
    value: String,
    on_input: EventHandler<FormEvent>,
    #[props(default = "flex: 1".to_string())] width: String,
) -> Element {
    rsx! {
        input {
            r#type: "text",
            value: "{value}",
            oninput: move |e| on_input.call(e),
            style: "{width}; padding: 8px; border: 1px solid {BORDER_DEFAULT}; border-radius: 6px; font-size: 13px;"
        }
    }
}

// ============================================================================
// SettingsNumberInput — 设置面板数字输入框（u32 自动 parse + clamp）
// ============================================================================

/// 设置面板数字输入框（并发下载数等）
#[component]
pub(super) fn SettingsNumberInput(
    value: u32,
    min: u32,
    max: u32,
    on_change: EventHandler<u32>,
) -> Element {
    rsx! {
        input {
            r#type: "number",
            value: "{value}",
            min: "{min}",
            max: "{max}",
            oninput: move |evt| {
                if let Ok(n) = evt.value().parse::<u32>() {
                    on_change.call(n.clamp(min, max));
                }
            },
            style: "width: 100%; padding: 8px; border: 1px solid {BORDER_DEFAULT}; border-radius: 6px; font-size: 13px;"
        }
    }
}
