//! 课程池下载面板 (📚) — 提取自原 app.rs:707-839
//!
//! 课节复选框 + 全选/全不选 + 启动下载。
//! 渲染条件由 AppComponent 控制 (state.read().course_collection_state.visible),
//! 本组件内部不重复 guard。
//!
//! 包含 M7 修复:课程池下载完成后聚合成功/失败数 + 首条失败原因。
//! M11 修复在 handlers.rs course_download_pool_impl 内部(JoinError 可见)。

use std::sync::Arc;
use std::sync::atomic::Ordering;

use dioxus::prelude::*;

use super::components::{CardShadow, CloseButton, WhiteCard};
use super::handlers::{course_download_pool_impl, CourseItemSlot};
use super::theme::{BORDER_DEFAULT, TEXT_MUTED, TEXT_PRIMARY};
use super::types::AppState;

/// 把 atomic slot status (u8) 翻译回 CourseItemDisplay.status 字符串
fn slot_status_to_str(v: u8) -> &'static str {
    match v {
        1 => "resolving",
        2 => "downloading",
        3 => "completed",
        4 => "failed",
        5 => "skipped",
        _ => "pending",
    }
}

/// 课程池下载面板
#[component]
pub(super) fn CoursePoolPanel(
    state: Store<AppState>,
    /// P0-2:聚合进度信号,从 AppComponent 顶层 Signal 传入,polling task 写 Signal
    /// 而非 store.download_progress,避免整个 AppComponent 重渲染
    progress_signal: Signal<f32>,
) -> Element {
    rsx! {
        WhiteCard {
            shadow: CardShadow::Elevated,
            div {
                style: "display: flex; justify-content: space-between; align-items: center; margin-bottom: 14px;",
                div {
                    style: "font-size: 18px; font-weight: 700; color: {TEXT_PRIMARY};",
                    "📚 课程池（已选 {state.read().course_collection_state.items.iter().filter(|i| i.selected).count()} / 共 {state.read().course_collection_state.items.len()}）"
                }
                CloseButton {
                    onclick: move |_| { state.write().course_collection_state.visible = false; },
                }
            }
            // 全选/全不选 + 状态
            div {
                style: "display: flex; gap: 12px; margin-bottom: 10px; align-items: center;",
                button {
                    onclick: move |_| {
                        let mut cc = state.write();
                        for item in cc.course_collection_state.items.iter_mut() {
                            item.selected = true;
                        }
                    },
                    style: "padding: 6px 14px; background: {state.read().theme.gradient()}; color: white; border: none; border-radius: 8px; font-size: 13px;",
                    "全选"
                }
                button {
                    onclick: move |_| {
                        let mut cc = state.write();
                        for item in cc.course_collection_state.items.iter_mut() {
                            item.selected = false;
                        }
                    },
                    style: "padding: 6px 14px; background: #f5f5f5; color: #444; border: none; border-radius: 8px; font-size: 13px;",
                    "全不选"
                }
                div {
                    style: "flex: 1; font-size: 12px; color: {TEXT_MUTED};",
                    "{state.read().course_collection_state.status}"
                }
            }
            // 列表
            div {
                style: "max-height: 360px; overflow-y: auto; border: 1px solid {BORDER_DEFAULT}; border-radius: 10px;",
                for (idx, item) in state.read().course_collection_state.items.iter().enumerate() {
                    div {
                        key: "{idx}",
                        style: "display: flex; align-items: center; gap: 10px; padding: 8px 12px; border-bottom: 1px solid {BORDER_DEFAULT}; font-size: 13px; color: {TEXT_PRIMARY};",
                        input {
                            r#type: "checkbox",
                            checked: item.selected,
                            onchange: move |evt: dioxus::events::FormEvent| {
                                let checked = evt.value() == "true";
                                state.write().course_collection_state.items[idx].selected = checked;
                            },
                        }
                        span {
                            style: "min-width: 32px; color: {TEXT_MUTED};",
                            "{item.index}."
                        }
                        span {
                            style: "flex: 1;",
                            "{item.title}"
                        }
                        span {
                            style: "font-size: 11px; color: {TEXT_MUTED};",
                            "{item.status}"
                        }
                    }
                }
            }
            // 启动下载
            div {
                style: "display: flex; gap: 12px; margin-top: 12px;",
                button {
                    onclick: move |_| {
                        let cc = state.read().course_collection_state.clone();
                        if cc.items.is_empty() {
                            return;
                        }
                        let selected_count = cc.items.iter().filter(|i| i.selected).count();
                        if selected_count == 0 {
                            state.write().status_message = "未选中任何课节".to_string();
                            return;
                        }
                        // 为每项建一个进度槽（Arc<CourseItemSlot>），由 use_effect 轮询把原子值写回 Store
                        let slots: Vec<Arc<CourseItemSlot>> = (0..cc.items.len())
                            .map(|_| Arc::new(CourseItemSlot::default()))
                            .collect();
                        // 清掉残留的 batch_state(避免误显示"准备下载 1 个视频")
                        state.write().batch_state.visible = false;
                        state.write().is_downloading = true;
                        state.write().status_message = format!("开始下载 {selected_count} 个课节");
                        // 把每个 slot 挂到对应 item（供 use_effect 轮询）
                        {
                            let mut s = state.write();
                            for (i, slot) in slots.iter().enumerate() {
                                if let Some(it) = s.course_collection_state.items.get_mut(i) {
                                    it.slot = Some(slot.clone());
                                }
                            }
                        }
                        let mut state_for_task = state;
                        // 同步启动轮询 task:把 atomic slot.progress/status 推回 store,
                        // 修复"下载在跑但 UI 卡在 0%/resolving"的显示滞后 bug
                        let mut state_for_polling = state_for_task;
                        let slots_for_polling: Vec<Arc<CourseItemSlot>> = slots.clone();
                        // P0-2:进度信号句柄 move 进 spawn,聚合进度写到 Signal 而非 store
                        let mut progress_signal_for_polling = progress_signal;
                        spawn(async move {
                            let mut interval = tokio::time::interval(std::time::Duration::from_millis(500));
                            // 第一 tick 立即执行,不烧 500ms 空白
                            interval.tick().await;
                            loop {
                                // 1) 读所有 slot 的 (progress, status)
                                let snapshot: Vec<(u32, u8)> = state_for_polling
                                    .read()
                                    .course_collection_state
                                    .items
                                    .iter()
                                    .map(|i| {
                                        i.slot
                                            .as_ref()
                                            .map(|s| {
                                                (
                                                    s.progress.load(Ordering::Relaxed),
                                                    s.status.load(Ordering::Relaxed),
                                                )
                                            })
                                            .unwrap_or((0, 0))
                                    })
                                    .collect();
                                // 2) 写回 store (只在变化时写,避免无谓 render)
                                {
                                    let mut s = state_for_polling.write();
                                    let mut agg_percent = 0.0_f32;
                                    let mut agg_count = 0usize;
                                    for (i, (progress_raw, status_u8)) in snapshot.iter().enumerate() {
                                        if let Some(item) =
                                            s.course_collection_state.items.get_mut(i)
                                        {
                                            let percent = *progress_raw as f32 / 100.0;
                                            if (item.progress - percent).abs() > 0.05 {
                                                item.progress = percent;
                                            }
                                            let status_str =
                                                slot_status_to_str(*status_u8);
                                            if item.status != status_str {
                                                item.status = status_str.to_string();
                                            }
                                            // 累计聚合:每项的 percent 直接累加平均
                                            agg_percent += percent;
                                            agg_count += 1;
                                        }
                                    }
                                    // P0-2: 推到顶层 Signal 而非 store.download_progress
                                    // 缩小重渲染范围到进度条区域
                                    if agg_count > 0 {
                                        let overall = agg_percent / agg_count as f32 / 100.0;
                                        let current = progress_signal_for_polling.cloned();
                                        if (current - overall).abs() > 0.005 {
                                            progress_signal_for_polling.set(overall);
                                        }
                                    }
                                }
                                // 3) 所有项到达终态(completed/failed/skipped)→ 退出轮询
                                let all_done = snapshot
                                    .iter()
                                    .zip(slots_for_polling.iter())
                                    .all(|((_, st), _)| (3..=5).contains(st));
                                if all_done {
                                    break;
                                }
                                interval.tick().await;
                            }
                        });
                        spawn(async move {
                            let results = course_download_pool_impl(cc, slots).await;
                            // 聚合每项 (id, status) → 用户能看到成功/失败数 + 失败原因
                            let mut success = 0usize;
                            let mut failed = 0usize;
                            let mut first_failure: Option<String> = None;
                            for (_id, status) in &results {
                                if status == "completed" {
                                    success += 1;
                                } else if let Some(msg) = status.strip_prefix("failed: ") {
                                    failed += 1;
                                    if first_failure.is_none() {
                                        first_failure = Some(msg.to_string());
                                    }
                                }
                            }
                            let summary = match (success, failed) {
                                (0, 0) => "课程池下载结束(无课节)".to_string(),
                                (s, 0) => format!("课程池下载完成: 全部 {s} 个成功"),
                                (0, f) => format!("课程池下载失败: {f} 个失败。首条: {}", first_failure.as_deref().unwrap_or("")),
                                (s, f) => format!("课程池下载结束: {s} 成功 / {f} 失败。首条失败: {}", first_failure.as_deref().unwrap_or("")),
                            };
                            state_for_task.write().is_downloading = false;
                            state_for_task.write().status_message = summary;
                        });
                    },
                    style: "padding: 10px 20px; background: {state.read().theme.gradient()}; color: white; border: none; border-radius: 10px; font-size: 14px; font-weight: 600;",
                    "▶ 开始下载选中"
                }
            }
        }
    }
}
