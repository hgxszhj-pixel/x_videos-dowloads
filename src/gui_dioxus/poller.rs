//! Progress polling coroutine
//!
//! 单一 `use_coroutine` 接收 `PollMsg` 消息，调度多个下载进度轮询循环。
//! 替代 `app.rs` 中两处手写 `spawn(async move { loop { ... } })`（约 67 行）。
//! `use_coroutine` 生命周期绑定 component，卸载自动清理（vs raw spawn 永不结束）。

use dioxus::prelude::*;
use futures_util::StreamExt;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use super::types::{format_file_size, format_speed, AppState};

const PROGRESS_THRESHOLD: f32 = 0.01; // 1% 阈值
const POLL_INTERVAL_MS: u64 = 500;

/// 触发轮询的消息
pub(super) enum PollMsg {
    /// 单下载轮询：进度变化 ≥ 1% 才更新 UI
    Single {
        progress_arc: Arc<AtomicU32>,
        speed_arc: Arc<AtomicU64>,
        total_bytes_arc: Arc<AtomicU64>,
    },
    /// 批量下载轮询：每个视频进度 + 速度
    Batch {
        progress_arcs: Vec<Arc<AtomicU32>>,
        total_count: usize,
    },
}

/// 安装轮询 coroutine（在 AppComponent 顶层调用）
pub(super) fn use_progress_poller(mut state: Store<AppState>) -> Coroutine<PollMsg> {
    use_coroutine(move |mut rx: UnboundedReceiver<PollMsg>| async move {
        while let Some(msg) = rx.next().await {
            match msg {
                PollMsg::Single {
                    progress_arc,
                    speed_arc,
                    total_bytes_arc,
                } => {
                    let mut last_progress = 0.0_f32;
                    let mut last_size: u64 = 0;
                    loop {
                        tokio::time::sleep(Duration::from_millis(POLL_INTERVAL_MS)).await;
                        if !state.read().is_downloading {
                            break;
                        }
                        let percent = progress_arc.load(Ordering::Relaxed) as f32 / 10000.0;
                        if (percent - last_progress).abs() >= PROGRESS_THRESHOLD || percent >= 1.0 {
                            state.write().download_progress = percent;
                            last_progress = percent;
                        }
                        let speed = speed_arc.load(Ordering::Relaxed);
                        state.write().download_speed = format_speed(speed);
                        let total = total_bytes_arc.load(Ordering::Relaxed);
                        if total != last_size && total > 0 {
                            state.write().download_file_size = format_file_size(total);
                            last_size = total;
                        }
                        if percent >= 1.0 {
                            break;
                        }
                    }
                }
                PollMsg::Batch {
                    progress_arcs,
                    total_count,
                } => {
                    loop {
                        tokio::time::sleep(Duration::from_millis(POLL_INTERVAL_MS)).await;
                        if !state.read().is_downloading {
                            break;
                        }
                        // 单 transaction:把所有 per-item 写入合并到一次 store.write(),
                        // 避免原版每项 2 reads + 2 writes (N 项 = 4N 次锁)
                        let mut total_progress = 0.0_f32;
                        let mut all_done = true;
                        // 先算每项的 progress/speed (只读原子,无锁)
                        let mut per_item: Vec<(f32, String)> =
                            Vec::with_capacity(progress_arcs.len());
                        for arc in progress_arcs.iter() {
                            let percent = arc.load(Ordering::Relaxed) as f32 / 10000.0;
                            total_progress += percent;
                            if percent < 1.0 {
                                all_done = false;
                            }
                            per_item.push((percent, String::new())); // 速度待会儿填
                        }
                        // 一次性获取所有 speed_arc,避免循环内重复 read lock
                        let speed_arcs: Vec<Option<Arc<AtomicU64>>> = {
                            let s = state.read();
                            s.batch_state
                                .items
                                .iter()
                                .map(|it| it.speed_arc.clone())
                                .collect()
                        };
                        for (i, speed_arc_opt) in speed_arcs.iter().enumerate() {
                            if let Some(arc) = speed_arc_opt {
                                let speed = arc.load(Ordering::Relaxed);
                                per_item[i].1 = format_speed(speed);
                            }
                        }
                        // 单写事务
                        {
                            let mut s = state.write();
                            let total = (total_count as f32).max(1.0);
                            s.batch_state.progress = total_progress / total;
                            for (i, (percent, speed)) in per_item.into_iter().enumerate() {
                                if let Some(item) = s.batch_state.items.get_mut(i) {
                                    item.progress = percent;
                                    item.speed = speed;
                                }
                            }
                        }
                        if all_done || total_progress >= (total_count as f32) {
                            break;
                        }
                    }
                }
            }
        }
    })
}
