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

use super::types::{format_eta, format_file_size, format_speed, AppState};

const PROGRESS_THRESHOLD: f32 = 0.01; // 1% 阈值
const POLL_INTERVAL_MS: u64 = 500;

/// 触发轮询的消息
pub(super) enum PollMsg {
    /// 单下载轮询：进度变化 ≥ 1% 才更新 UI
    Single {
        progress_arc: Arc<AtomicU32>,
        speed_arc: Arc<AtomicU64>,
        total_bytes_arc: Arc<AtomicU64>,
        /// P0-2:独立 Signal,poller 直接 set 而不写 store,缩小重渲染范围
        progress_signal: Signal<f32>,
        speed_signal: Signal<String>,
        size_signal: Signal<String>,
        /// 剩余时间(由 poller 根据 progress/total/speed 算好,UI 不再做反向解析)
        eta_signal: Signal<String>,
    },
    /// 批量下载轮询：每个视频进度 + 速度
    Batch {
        progress_arcs: Vec<Arc<AtomicU32>>,
        total_count: usize,
    },
}

/// 安装轮询 coroutine（在 AppComponent 顶层调用）
///
/// 注意:progress/speed/size Signal 不作为参数捕获,而是经 PollMsg::Single 由调用方传入
/// (因为下载启动时调用方拥有 Signal 句柄)。函数本身只持有 Store<AppState> 即可。
pub(super) fn use_progress_poller(mut state: Store<AppState>) -> Coroutine<PollMsg> {
    use_coroutine(move |mut rx: UnboundedReceiver<PollMsg>| async move {
        while let Some(msg) = rx.next().await {
            match msg {
                PollMsg::Single {
                    progress_arc,
                    speed_arc,
                    total_bytes_arc,
                    mut progress_signal,
                    mut speed_signal,
                    mut size_signal,
                    mut eta_signal,
                } => {
                    let mut last_progress = 0.0_f32;
                    let mut last_speed: u64 = 0;
                    let mut last_size: u64 = 0;
                    let mut last_eta: u64 = u64::MAX;
                    loop {
                        tokio::time::sleep(Duration::from_millis(POLL_INTERVAL_MS)).await;
                        if !state.read().is_downloading {
                            break;
                        }
                        let percent = progress_arc.load(Ordering::Relaxed) as f32 / 10000.0;
                        // P0-2: 写 Signal 而非 store → 进度条区域独立重渲染
                        // P1-3: speed/size 加阈值节流 (≥ 100 KiB/s 变化才更新)
                        if (percent - last_progress).abs() >= PROGRESS_THRESHOLD || percent >= 1.0 {
                            progress_signal.set(percent);
                            last_progress = percent;
                        }
                        let speed = speed_arc.load(Ordering::Relaxed);
                        if speed.abs_diff(last_speed) >= 100 * 1024 {
                            speed_signal.set(format_speed(speed));
                            last_speed = speed;
                        }
                        let total = total_bytes_arc.load(Ordering::Relaxed);
                        if total != last_size && total > 0 {
                            size_signal.set(format_file_size(total));
                            last_size = total;
                        }
                        // ETA: (剩余字节 / 当前速度) 秒,3 项都已知时才算
                        // 阈值 1s 减少 UI 抖动 (剩余时间常在 ±1s 浮动)
                        if speed > 0 && total > 0 && percent > 0.0 && percent < 1.0 {
                            let remaining_bytes =
                                ((1.0_f64 - percent as f64) * total as f64) as u64;
                            let eta_secs = remaining_bytes / speed;
                            if eta_secs.abs_diff(last_eta) >= 1 || last_eta == u64::MAX {
                                eta_signal.set(format_eta(eta_secs));
                                last_eta = eta_secs;
                            }
                        } else if percent >= 1.0 && last_eta != 0 {
                            eta_signal.set(String::new());
                            last_eta = 0;
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
                        let mut agg_speed_bytes: u64 = 0;
                        for (i, speed_arc_opt) in speed_arcs.iter().enumerate() {
                            if let Some(arc) = speed_arc_opt {
                                let speed = arc.load(Ordering::Relaxed);
                                per_item[i].1 = format_speed(speed);
                                agg_speed_bytes = agg_speed_bytes.saturating_add(speed);
                            }
                        }
                        // 单写事务
                        {
                            let mut s = state.write();
                            let total = (total_count as f32).max(1.0);
                            s.batch_state.progress = total_progress / total;
                            // 聚合速度:所有 active 项之和 (saturating 避免溢出)
                            s.batch_state.aggregate_speed = format_speed(agg_speed_bytes);
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
