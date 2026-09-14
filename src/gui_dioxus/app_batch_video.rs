//! 批量视频列表面板 — 提取自原 app.rs:611-704
//!
//! 显示批量解析的视频列表 (idx + title + uploader + duration + 画质数),
//! 提供 per-item checkbox + 全选/全不选 + "▶ 下载选中 (N)" / "▶ 全部下载" + "⏹️ 取消" 按钮。
//! 选中模式与 app_course_pool.rs 的 CourseItemDisplay.selected 对齐。
//! 渲染条件由 AppComponent 控制 (state.read().batch_video_state.visible),
//! 本组件内部不重复 guard。

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64};
use std::sync::Arc;

use dioxus::prelude::*;

use super::components::{CardShadow, WhiteCard};
use super::handlers::batch_download_impl;
use super::poller::PollMsg;
use super::theme::{TEXT_MUTED, TEXT_PRIMARY};
use super::types::{
    AppState, AppStateStoreExt, BatchItemState, BatchState, BatchVideoStateStoreExt,
    VideoInfoDisplayStoreExt,
};

/// 批量视频列表面板 (含下载/取消按钮)
///
/// 接受 `poller` 参数以便触发进度轮询。`use_progress_poller` 是 AppComponent 创建的
/// 全局 Coroutine,新入口必须显式 `poller.send(PollMsg::Batch { ... })`,否则
/// poller 永远 idle,UI 进度条停在 0% (2026-09-14 BUG-A 修复,见 HANDOFF)。
#[component]
pub(super) fn BatchVideoPanel(
    state: Store<AppState>,
    poller: Coroutine<PollMsg>,
) -> Element {
    // 列表只读 —— 此面板渲染期间 items 不再变化 —— 旧代码 `items().iter()` 也能用。
    // 仍提取 sub-store 让结构与 batch_progress/course_pool 一致,未来要加交互(checkbox/删除)
    // 时不会因裸引用而无订阅翻车。
    //
    // 注:不能写 `Store<Vec<...>>` 标注 —— 链式 `.batch_video_state().items()` 实际返回
    // `MappedMutSignal` lens,不是默认 `WriteSignal`。让编译器自动推断。
    let items_store = state.batch_video_state().items();
    // 按钮文案 + disabled 条件都提到 rsx! 外 —— rsx! 解析器对 body 内 `let`/嵌套 `format!`
    // 处理不稳(见 app_course_pool.rs:270 注释),统一在宏外计算。
    let total = state.read().batch_video_state.items.len();
    let selected_count = state
        .read()
        .batch_video_state
        .items
        .iter()
        .filter(|i| i.selected)
        .count();
    let is_downloading = state.read().is_downloading;
    let button_label = if is_downloading {
        "⏳ 下载中...".to_string()
    } else if selected_count == total && selected_count > 0 {
        // 全选时显示 "全部下载" (等价旧行为),部分选中时显示 "(N)"
        "▶ 全部下载".to_string()
    } else {
        format!("▶ 下载选中 ({selected_count})")
    };
    let button_disabled = is_downloading || selected_count == 0;
    rsx! {
        WhiteCard {
            shadow: CardShadow::Soft,
            div {
                style: "display: flex; align-items: center; gap: 10px; margin-bottom: 14px;",
                div { style: "font-size: 24px;", "📋" }
                div {
                    style: "font-size: 16px; font-weight: 700; background: {state.read().theme.gradient()}; -webkit-background-clip: text; -webkit-text-fill-color: transparent;",
                    "批量视频列表 (已选 {state.read().batch_video_state.items.iter().filter(|i| i.selected).count()} / 共 {state.read().batch_video_state.items.len()})"
                }
            }
            // 全选 / 全不选 (镜像 app_course_pool.rs:213-232)
            div {
                style: "display: flex; gap: 12px; margin-bottom: 10px; align-items: center;",
                button {
                    onclick: move |_| {
                        let mut s = state.write();
                        for item in s.batch_video_state.items.iter_mut() {
                            item.selected = true;
                        }
                    },
                    style: "padding: 6px 14px; background: {state.read().theme.gradient()}; color: white; border: none; border-radius: 8px; font-size: 13px;",
                    "全选"
                }
                button {
                    onclick: move |_| {
                        let mut s = state.write();
                        for item in s.batch_video_state.items.iter_mut() {
                            item.selected = false;
                        }
                    },
                    style: "padding: 6px 14px; background: #f5f5f5; color: #444; border: none; border-radius: 8px; font-size: 13px;",
                    "全不选"
                }
            }
            div {
                style: "display: flex; flex-direction: column; gap: 10px; max-height: 320px; overflow-y: auto; padding-right: 4px;",
                // `items_store.index(idx).field()` per-item reactive 订阅,
                // 与 course_pool / batch_progress 模式一致。当前列表只读,
                // 这个改动主要是防御性 — 防止未来加交互时翻车。
                for idx in 0..items_store.len() {
                    div {
                        key: "{idx}",
                        style: "{state.read().theme.bg_card_white()} padding: 14px; transition: all 0.3s ease;",
                        // per-item checkbox (镜像 app_course_pool.rs:279-288)
                        input {
                            r#type: "checkbox",
                            checked: items_store.index(idx).selected(),
                            // 下载中禁用 checkbox,避免选中状态与实际下载错位
                            disabled: state.read().is_downloading,
                            onchange: move |evt: dioxus::events::FormEvent| {
                                let checked = evt.value() == "true";
                                state.write().batch_video_state.items[idx].selected = checked;
                            },
                        }
                        div {
                            style: "display: flex; align-items: center; gap: 10px; margin-bottom: 8px;",
                            div {
                                style: "width: 26px; height: 26px; background: {state.read().theme.gradient()}; border-radius: 50%; text-align: center; line-height: 26px; font-size: 12px; color: white; font-weight: 700;",
                                "{idx + 1}"
                            }
                            div {
                                style: "font-size: 14px; font-weight: 600; color: {TEXT_PRIMARY}; flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;",
                                "{items_store.index(idx).title()}"
                            }
                        }
                        div {
                            style: "font-size: 11px; color: {TEXT_MUTED}; padding-left: 36px;",
                            "👤 {items_store.index(idx).uploader()}  |  ⏱️ {items_store.index(idx).duration()}  |  📁 {items_store.index(idx).formats().len()} 种画质"
                        }
                    }
                }
            }
        }
        // 批量下载按钮
        div {
            style: "display: flex; gap: 12px; margin-top: 16px;",
            button {
                // P3-2:下载中禁用启动按钮;另加 0 选中守卫 (镜像 app_course_pool.rs:334,341)
                disabled: button_disabled,
                onclick: move |_| {
                    // 按 selected 预过滤 (镜像 app_course_pool.rs:336-340)
                    let items = state.read().batch_video_state.items.clone();
                    if items.is_empty() {
                        return;
                    }
                    let picked: Vec<_> = items.into_iter().filter(|i| i.selected).collect();
                    if picked.is_empty() {
                        state.write().status_message = "未选中任何视频".to_string();
                        return;
                    }
                    // 为每个下载项创建原子进度追踪 + 取消标志
                    let urls: Vec<String> = picked.iter().map(|i| i.url.clone()).collect();
                    let progress_arcs: Vec<Option<Arc<AtomicU32>>> = picked.iter().map(|_| Some(Arc::new(AtomicU32::new(0)))).collect();
                    let speed_arcs: Vec<Option<Arc<AtomicU64>>> = picked.iter().map(|_| Some(Arc::new(AtomicU64::new(0)))).collect();
                    let cancel_flags: Vec<Option<Arc<AtomicBool>>> = picked.iter().map(|_| Some(Arc::new(AtomicBool::new(false)))).collect();
                    let initial_items: Vec<BatchItemState> = (0..urls.len()).map(|idx| BatchItemState {
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
                        aggregate_speed: String::new(),
                    };
                    // 同步 canonical flag,poller.rs:66 gate 才能正常轮询
                    state.write().is_downloading = true;
                    state.write().batch_video_state.visible = false;
                    // BUG-A 修复 (2026-09-14): 必须通知 poller 进入批量轮询循环,
                    // 否则 poller 永远 idle,batch_state.items[i].progress 永远 = 0。
                    // 镜像 app.rs:525-532 老入口的步骤 4。
                    let progress_arcs_for_poll: Vec<Arc<AtomicU32>> = progress_arcs
                        .iter()
                        .filter_map(|a| a.clone())
                        .collect();
                    let total_count = urls.len();
                    poller.send(PollMsg::Batch {
                        progress_arcs: progress_arcs_for_poll,
                        total_count,
                    });
                    let mut state = state;
                    spawn(async move {
                        // BUG-D 诊断 (2026-09-14): 3-item batch 卡死 — 加 trace 定位
                        // 是 outer spawn 没 poll、impl 没进入、还是 inner spawn 没调度。
                        tracing::info!(
                            "[batch_video] outer spawn 启动 urls={}",
                            urls.len()
                        );
                        // BUG-B 修复 (2026-09-14): 消费 batch_download_impl 返回的
                        // (idx, url, success, status, progress) 结果,写回 store。
                        //
                        // Reviewer Suggestion #1 (2026-09-14): items 写回与
                        // is_downloading = false 合并到同一事务,避免 poller tick
                        // 落在两次写之间回写 progress = 0。
                        let results = batch_download_impl(urls, progress_arcs, speed_arcs, cancel_flags).await;
                        {
                            let mut s = state.write();
                            for (idx, _url, _success, status, progress) in results {
                                if let Some(item) = s.batch_state.items.get_mut(idx) {
                                    item.status = status;
                                    item.progress = progress;
                                }
                            }
                            s.batch_state.visible = false;
                            s.is_downloading = false;
                        }
                    });
                },
                style: "{state.read().theme.btn_success(\"14px\", \"12px\", \"15px\")} flex: 1;",
                "{button_label}"
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
