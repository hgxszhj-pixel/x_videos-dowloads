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
use super::types::{
    format_size_fraction, format_speed, AppState, AppStateStoreExt,
    CourseCollectionStateStoreExt, CourseItemDisplayStoreExt,
};

/// 把 atomic slot status (u8) 翻译回 CourseItemDisplay.status 字符串
fn slot_status_to_str(v: u8) -> &'static str {
    match v {
        1 => "resolving",
        2 => "downloading",
        3 => "completed",
        4 => "failed",
        5 => "skipped",
        6 => "exists",
        _ => "pending",
    }
}

/// UI 展示用的状态文案 (rsx! {} 不允许 if-else,提一个函数绕开)
/// 统一覆盖所有 6 个 status,避免半行带图标/半行裸字符串的视觉割裂
fn status_display(s: &str) -> String {
    match s {
        "exists" => "✓ 已下载".to_string(),
        "completed" => "✅ 已完成".to_string(),
        "failed" => "❌ 失败".to_string(),
        "skipped" => "⏭️ 已跳过".to_string(),
        "downloading" => "⬇️ 下载中".to_string(),
        "resolving" => "🔍 解析中".to_string(),
        // pending 或未来新增 status 原样输出,不 panic
        _ => s.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 所有 6 个 status 必须命中已知分支(防止新增 status 时漏改 UI 显示)
    #[test]
    fn test_status_display_covers_all_known_statuses() {
        assert_eq!(status_display("exists"), "✓ 已下载");
        assert_eq!(status_display("completed"), "✅ 已完成");
        assert_eq!(status_display("failed"), "❌ 失败");
        assert_eq!(status_display("skipped"), "⏭️ 已跳过");
        assert_eq!(status_display("downloading"), "⬇️ 下载中");
        assert_eq!(status_display("resolving"), "🔍 解析中");
    }

    /// pending 或未知 status 走 fallback (原样输出),不 panic
    /// —— 与 ErrorDisplay::label 的 fallback 策略一致
    #[test]
    fn test_status_display_falls_back_for_unknown() {
        assert_eq!(status_display("pending"), "pending");
        assert_eq!(status_display("future_thing"), "future_thing");
        assert_eq!(status_display(""), "");
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
    // 在 rsx! 外构造 reactive sub-store。
    // 注:链式 `.course_collection_state().items()` 返回 `Store<Vec<CourseItemDisplay>, MappedMutSignal<...>>`,
    // 不是默认 `Store<T, WriteSignal<T>>` —— 不能用 `Store<T>` 标注,会让编译器期望 `WriteSignal` lens。
    // 让编译器自动推断。per-item sub-store 上的字段访问器触发 reactive 订阅。
    let items_store = state.course_collection_state().items();
    rsx! {
        WhiteCard {
            shadow: CardShadow::Elevated,
            div {
                style: "display: flex; justify-content: space-between; align-items: center; margin-bottom: 14px;",
                div {
                    style: "font-size: 18px; font-weight: 700; color: {TEXT_PRIMARY};",
                    "📚 课程池（已选 {state.read().course_collection_state.items.iter().filter(|i| i.selected).count()} / 共 {items_store.len()}）"
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
            //
            // 关键:`items_store.clone().index(idx)` 拿到 `Store<CourseItemDisplay>` sub-store
            // (每个 idx 自己的 signal)。`.selected() / .status() / .title() / .index()` 是
            // `CourseItemDisplayStoreExt` 生成的访问器,调用 sub-store 作为 Fn() 触发订阅 ——
            // 字段变化时仅该 idx 重渲染,不会触发整个列表。
            // 旧代码 `state.read().items.iter()` 走 `Vec::iter()` 返回 `&CourseItemDisplay` 快照,
            // 无订阅,UI 永远停在 pending。
            //
            // 注:`Store::index(self, idx)` 消费 self,每次调用都需要 .clone()。
            // rsx! 不允许 for body 内 `let` 绑定(语法解析限制),所以每个属性位置直接构造 sub-store。
            div {
                style: "max-height: 360px; overflow-y: auto; border: 1px solid {BORDER_DEFAULT}; border-radius: 10px;",
                for idx in 0..items_store.len() {
                    // 字段访问器返回 sub-store,if 条件 / 算术需要 ()() 双括号。
                    // 仅在 active 状态显示进度条 — terminal (completed/failed/skipped/cancelled) 隐藏。
                    div {
                        key: "{idx}",
                        style: "display: flex; align-items: center; gap: 10px; padding: 8px 12px; border-bottom: 1px solid {BORDER_DEFAULT}; font-size: 13px; color: {TEXT_PRIMARY};",
                        input {
                            r#type: "checkbox",
                            checked: items_store.index(idx).selected(),
                            // 下载中禁用 checkbox,避免选中状态与实际下载错位
                            disabled: state.read().is_downloading,
                            onchange: move |evt: dioxus::events::FormEvent| {
                                let checked = evt.value() == "true";
                                state.write().course_collection_state.items[idx].selected = checked;
                            },
                        }
                        span {
                            style: "min-width: 32px; color: {TEXT_MUTED};",
                            "{items_store.index(idx).index()}"
                        }
                        span {
                            style: "flex: 1;",
                            "{items_store.index(idx).title()}"
                        }
                        span {
                            style: "font-size: 11px; color: {TEXT_MUTED}; min-width: 80px; text-align: right;",
                            "{status_display(&items_store.index(idx).status()())}"
                        }
                        // P3-1:per-item 进度条 + 速度/大小/ETA — 仅 active 状态显示
                        if items_store.index(idx).status()() == "resolving"
                            || items_store.index(idx).status()() == "downloading"
                        {
                            div {
                                style: "width: 90px; height: 6px; background: #e0e0e0; border-radius: 3px; overflow: hidden;",
                                div {
                                    style: "height: 100%; width: {items_store.index(idx).progress()() * 100.0}%; background: {state.read().theme.gradient()}; transition: width 0.3s ease;",
                                }
                            }
                            // 详细信息行:速度 + 大小分数(仅 total_bytes 已知时显示,避免 6.42 MiB/s + 0 B 误导)
                            div {
                                style: "display: flex; gap: 10px; padding-left: 42px; font-size: 10px; color: {TEXT_MUTED}; white-space: nowrap;",
                                span {
                                    style: "min-width: 60px;",
                                    "{items_store.index(idx).speed()}"
                                }
                                if items_store.index(idx).total_bytes()() > 0 {
                                    span {
                                        style: "min-width: 130px;",
                                        "{format_size_fraction(items_store.index(idx).downloaded_bytes()(), items_store.index(idx).total_bytes()())}"
                                    }
                                }
                            }
                        }
                    }
                }
            }
            // 启动下载
            div {
                style: "display: flex; gap: 12px; margin-top: 12px;",
                button {
                    // P3-2:下载中禁用启动按钮,避免双击触发第二轮 download pool
                    disabled: state.read().is_downloading,
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
                                // 1) 读所有 slot 的 (progress, status, speed, total_bytes)
                                let snapshot: Vec<(u32, u8, u64, u64)> = state_for_polling
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
                                                    s.speed.load(Ordering::Relaxed),
                                                    s.total_bytes.load(Ordering::Relaxed),
                                                )
                                            })
                                            .unwrap_or((0, 0, 0, 0))
                                    })
                                    .collect();
                                // 2) 写回 store (只在变化时写,避免无谓 render)
                                {
                                    let mut s = state_for_polling.write();
                                    let mut agg_percent = 0.0_f32;
                                    let mut agg_count = 0usize;
                                    for (i, (progress_raw, status_u8, speed_raw, total_bytes_raw)) in
                                        snapshot.iter().enumerate()
                                    {
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
                                            // 速度:格式化字符串写回,UI 显示
                                            let speed_str = format_speed(*speed_raw);
                                            if item.speed != speed_str {
                                                item.speed = speed_str;
                                            }
                                            // 总字节:已知 (>0) 时写回
                                            if *total_bytes_raw > 0 {
                                                item.total_bytes = *total_bytes_raw;
                                                // 已下载 = percent × total
                                                let downloaded =
                                                    ((percent as f64 / 100.0)
                                                        * *total_bytes_raw as f64)
                                                        as u64;
                                                item.downloaded_bytes = downloaded;
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
                                // 3) 所有项到达终态(completed/failed/skipped/exists)→ 退出轮询
                                let all_done = snapshot
                                    .iter()
                                    .zip(slots_for_polling.iter())
                                    .all(|((_, st, _, _), _)| (3..=6).contains(st));
                                if all_done {
                                    break;
                                }
                                interval.tick().await;
                            }
                        });
                        spawn(async move {
                            let results = course_download_pool_impl(cc, slots).await;
                            // 聚合每项 (id, status) → 用户能看到成功/失败/已下载数 + 失败原因
                            let mut success = 0usize;
                            let mut failed = 0usize;
                            let mut exists = 0usize;
                            let mut first_failure: Option<String> = None;
                            for (_id, status) in &results {
                                if status == "completed" {
                                    success += 1;
                                } else if status == "exists" {
                                    exists += 1;
                                } else if let Some(msg) = status.strip_prefix("failed: ") {
                                    failed += 1;
                                    if first_failure.is_none() {
                                        first_failure = Some(msg.to_string());
                                    }
                                }
                            }
                            let summary = match (success, failed, exists) {
                                (0, 0, 0) => "课程池下载结束(无课节)".to_string(),
                                (s, 0, e) => format!("课程池下载完成: {s} 新下 + {e} 已存在"),
                                (0, f, e) => format!("课程池下载失败: {f} 个失败 / {e} 已存在。首条: {}", first_failure.as_deref().unwrap_or("")),
                                (s, f, e) => format!("课程池下载结束: {s} 新下 / {f} 失败 / {e} 已存在。首条失败: {}", first_failure.as_deref().unwrap_or("")),
                            };
                            state_for_task.write().is_downloading = false;
                            state_for_task.write().status_message = summary;
                        });
                    },
                    style: "padding: 10px 20px; background: {state.read().theme.gradient()}; color: white; border: none; border-radius: 10px; font-size: 14px; font-weight: 600;",
                    if state.read().is_downloading {
                        "⏳ 下载中..."
                    } else {
                        "▶ 开始下载选中"
                    }
                }
            }
        }
    }
}
