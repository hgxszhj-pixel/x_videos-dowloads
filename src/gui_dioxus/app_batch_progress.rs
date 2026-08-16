//! 批量下载进度面板 — 提取自原 app.rs:1021-1103
//!
//! 实时显示批量下载总进度 + 各视频子进度 + 单项 ✕ 取消。
//! 渲染条件由 AppComponent 控制 (state.read().batch_state.visible),
//! 本组件内部不重复 guard。

use std::sync::atomic::Ordering;

use dioxus::prelude::*;

use super::components::{CardShadow, WhiteCard};
use super::theme::{STYLE_BTN_ICON, STYLE_PROGRESS_TRACK, TEXT_MUTED};
use super::types::{AppState, AppStateStoreExt, BatchItemStateStoreExt, BatchStateStoreExt};

/// 批量下载进度面板
#[component]
pub(super) fn BatchProgressPanel(state: Store<AppState>) -> Element {
    // 提一个 reactive `Store<Vec<BatchItemState>>` 到 rsx! 外 —— 旧代码
    // `state.read().batch_state.items().iter()` 走 `Store::iter()` 返回 per-item
    // sub-store 集合本身是订阅对象,但解引用字段 `.progress / .status / .visible`
    // 走的是裸引用,无 signal 订阅。改用 `items_store.index(idx).field()` 让每个
    // 字段访问都触发精确重渲染 —— 否则 UI 一直卡在 pending。
    //
    // 注:不能写 `Store<Vec<...>>` 标注 —— 链式 `.batch_state().items()` 实际返回
    // `MappedMutSignal` lens,不是默认 `WriteSignal`。让编译器自动推断。
    let items_store = state.batch_state().items();
    rsx! {
        WhiteCard {
            shadow: CardShadow::Elevated,
            div {
                style: "display: flex; justify-content: space-between; align-items: center; margin-bottom: 10px;",
                div {
                    style: "display: flex; align-items: baseline; gap: 10px;",
                    div {
                        style: "font-size: 16px; font-weight: 700; background: {state.read().theme.gradient()}; -webkit-background-clip: text; -webkit-text-fill-color: transparent;",
                        "📚 批量下载 - {(state.read().batch_state.progress * 100.0).round() as i32}%"
                    }
                    // P3:聚合下载速度 (poller.rs:130 已写 batch_state.aggregate_speed)
                    div {
                        style: "font-size: 12px; color: {TEXT_MUTED}; font-weight: 500;",
                        "⚡ {state.read().batch_state.aggregate_speed}"
                    }
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
            //
            // 关键:`items_store.clone().index(idx).field()` 让每个 idx 自己的 signal 触发订阅,
            // 否则只跟踪 Vec 长度,字段变化不会刷新进度条/百分比/状态文字。
            //
            // 注:rsx! for body 不允许 `let` 绑定,每个属性位置直接构造 sub-store (.clone().index(idx))。
            div {
                style: "display: flex; flex-direction: column; gap: 10px; margin-top: 16px;",
                for idx in 0..items_store.len() {
                    // 字段访问器返回 `Store<T>`(sub-store),调用 `()` 才能拿到 T 值。
                    // `Store<T>` 实现了 `Deref<Target = dyn Fn() -> T>`,所以 `store()()` → T。
                    // attribute/text 位置 rsx! 走 `IntoAttributeValue/IntoDynNode` 自动 cloned,
                    // 所以属性位置不需要双括号;`if` 条件 / 算术表达式需要。
                    // 注:Store 实现了 Copy,不需要 .clone() —— `index(idx)` 拿快照即可。
                    if items_store.index(idx).visible()() {
                        div {
                            style: "display: flex; align-items: center; gap: 12px; padding: 12px; background: #f8f9fa; border-radius: 10px; transition: all 0.3s ease;",
                            div {
                                style: "height: 10px; {STYLE_PROGRESS_TRACK} flex: 1;",
                                div {
                                    style: "height: 100%; width: {items_store.index(idx).progress()() * 100.0}%; background: {state.read().theme.gradient()}; border-radius: 5px; transition: width 0.3s ease;",
                                }
                            }
                            div {
                                style: "width: 45px; font-size: 12px; font-weight: 600; color: {state.read().theme.color_accent()}; text-align: right;",
                                "{(items_store.index(idx).progress()() * 100.0).round() as i32}%"
                            }
                            // P3-3:显示下载速度 (poller.rs:130 已写 item.speed)
                            div {
                                style: "width: 70px; font-size: 11px; color: {TEXT_MUTED}; text-align: right;",
                                "{items_store.index(idx).speed()}"
                            }
                            div {
                                style: "font-size: 11px; color: {TEXT_MUTED};",
                                "{items_store.index(idx).status()}"
                            }
                            // 单项取消按钮（仅 pending/downloading 时显示）
                            if items_store.index(idx).status()() == "pending"
                                || items_store.index(idx).status()() == "downloading"
                            {
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
