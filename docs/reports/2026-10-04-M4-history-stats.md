# M4: history-stats 模块 (受 mole `mo history` 启发)

**日期**: 2026-10-04
**模块文件**: `src/commands/history_stats.rs` (487 行)
**CLI**: `--history-stats [--history-stats-json] [--history-stats-path PATH]`

## 改动清单
| 文件 | 改动 |
|------|------|
| `src/commands/history_stats.rs` | 新建 — compute_stats / format_summary / run_history_stats |
| `src/commands/mod.rs` | 注册 `pub mod history_stats;` |
| `src/cli.rs` | +3 fields: `history_stats` / `history_stats_json` / `history_stats_path` |
| `src/main.rs` | dispatch 块 (默认 path: `~/.x-video-downloader/history.json`) |

## 验证
| 步骤 | 结果 |
|------|------|
| `cargo build` | 0 errors, 3 warnings (pre-existing `must_use` + analyze.rs 的 grand_total/total) |
| `cargo test commands::history_stats` | 11 passed, 0 failed |
| `cargo test` (full suite) | 360 passed, 12 ignored (+11 from M4, 0 regression) |
| Smoke `--history-stats` | ✅ 4 条 / 75% 成功率 / 12.50 MB / Top-3 + 平台分布 |
| Smoke `--history-stats --json` | ✅ JSON 完整可序列化 |

## 统计指标 (11 项)
- total / completed / failed
- success_rate (0.0..=1.0,空历史为 0.0)
- total_bytes (仅累计 Completed,失败不计)
- avg_duration_secs (NaN/-inf 防御,缺失时 None)
- by_platform (BTreeMap + per-platform total/completed/failed/bytes)
- largest Top-10 (按字节降序,仅 Completed)
- recent_7d (基于 now 注入,便于测试)
- first_seen / last_seen (Entry 时间窗口)

## mole → x-video-downloader 映射

| mole | x-video-downloader |
|------|-------------------|
| `mo history` | `--history-stats` |

## 设计要点
1. **纯函数优先**: `compute_stats(entries, now)` 注入 `now`,所有时间窗口测试无需 sleep。
2. **NaN/inf 防御**: 累加 duration 前 `is_finite() && d >= 0.0`,测试 `avg_duration_ignores_invalid` 锁定。
3. **失败条目过滤**: file_size 仅在 `Completed` 状态计入 `total_bytes` 和 `largest`,测试 `stats_failed_excluded_from_bytes_and_largest` 锁定。
4. **human_bytes 自实现**: 不引 `bytesize` crate,本地 helper + `human_bytes_units` 测试覆盖 B/KB/MB/GB/TB + 溢出上界。
5. **platform_label fallback**: `serde_json::to_value(p)` 失败时回落到 `"unknown"`,避免 panic。
6. **默认路径策略**: `dirs::data_dir().join("x-video-downloader")` → `home_dir` fallback → `"./history.json"`,与 History 模块的 `data_dir.join("history.json")` 行为对齐。

## 踩坑
- `Result` 命名冲突: 文件顶部 use 过 `Result`,函数签名 `Result<()>` 被推导出 `Result<T,E>` 双泛型歧义 → 改 `anyhow::Result<()>` 显式。
- 内部 `use std::path::Path` 触发 unused warning → 移到文件顶部 use 列表。

## 未处理 (诚实报告)
- Pre-existing `must_use` warnings × 12 (platform/mod.rs 等) — 不动。
- Pre-existing `analyze.rs` 2 个 unused methods (`grand_total`/`total`) — 不动,非本模块范围。
- 单元测试覆盖了 11 个纯函数 case;CLI smoke 已用 fixture JSON 验证两个输出格式;未在用户真实 history.json 上跑 (本机无该文件)。