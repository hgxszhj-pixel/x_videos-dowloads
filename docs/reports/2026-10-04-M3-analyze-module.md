# M3 报告 — analyze 子命令 (受 mole `mo analyze` 启发)

**日期**: 2026-10-04
**模块**: `src/commands/analyze.rs` (新建)
**状态**: ✅ 完成 + cargo build/test 全绿

## 变更清单

| 文件 | 变更 |
|------|------|
| `src/commands/analyze.rs` | 新建,~260 行 + 10 个 unit tests |
| `src/commands/mod.rs` | 加 `pub mod analyze;` |
| `src/cli.rs` | 加 `--analyze <DIR>` / `--analyze-json` |
| `src/main.rs` | dispatch 分支,在 status 之后插入 |

## 分析维度

| 维度 | 实现 | 输出 |
|------|------|------|
| 总览 | `total_files` / `total_bytes` | 文件数 + 人读字节 |
| 按扩展名 | `by_extension: HashMap<String, ExtStats>` | 表格: count / bytes / avg, 按 bytes 降序 |
| 年龄分桶 | `AgeBuckets { recent, week, month, older }` | < 1 天 / < 7 天 / < 30 天 / ≥ 30 天 |
| Top 10 | `top_files: Vec<TopFile>` | 按 size 降序,含 mtime |

## 设计要点

- **pure 函数**: `analyze` 仅读 FS,无副作用,unit test 友好
- **mtime 缺失兼容**: `metadata.modified()` 失败 → bucket 兜底 `Duration::MAX` → older
- **Top-N 排序**: 先 collect 全文件元组 → 排序 → take(10),不用外堆
- **JSON 双格式**: `--analyze` 人读表格 / `--analyze-json` 程序可读

## 验证记录

| 命令 | 结果 |
|------|------|
| `cargo build` | 0 errors, 3 warnings (本模块新增 2 个 unused 警告 grand_total/AnalysisReport, 非关键) |
| `cargo test commands::analyze` | **10 passed** |
| `cargo test -- --test-threads=1` 全量 | **349 passed**, 12 ignored |
| `cargo test` 全量(并行) | 348 passed + **1 pre-existing flaky** (`test_integration_4_fault_points`) |
| 手动 smoke `/Users/hgxszhj/Movies` | 33.62 GB / 27 文件 / mp4 占 100% 字节 / Top10 全是 JS 逆向课程 |

## ⚠️ Pre-existing flaky test (与本模块无关)

`core::trace::tests::test_integration_4_fault_points` 在并行模式下偶发失败:

```
panicked at src/core/trace.rs:740:9:
jq 排序应暴露 download_one + fetch_video_info,实得 ["download_one", "fetch_video_info_ok"]
```

**根因**: 测试断言 RAII `fetch_video_info` (实际 ≥20ms sleep) 应比静态 `fetch_video_info_ok` (固定 20ms) 耗时长,但并行测试负载下 RAII 实测 < 20ms → `fetch_video_info_ok` 反超 → 排序失败。

**确认非本模块引入**: 
- `git stash` 撤回本模块后,运行仍 flaky (并行模式)
- 隔离运行 `cargo test test_integration_4_fault_points` → passed
- 串行 `cargo test -- --test-threads=1` → **349 passed**

**建议**: 给该测试加 `#[serial]` 或重写断言用 ≥0 的相对差,本 PR 不动 (rule: 最小变更)

## 踩坑教训
| 坑 | 解决 |
|----|------|
| `Vec::from_iter` 拼接 `into_iter().take().map()` 编译器识别不出 | 加 `.collect::<Vec<_>>()` |

## CLI 用法

```bash
# 人读表格
./target/debug/x-video-downloader --analyze ./downloads

# JSON (CI/调度)
./target/debug/x-video-downloader --analyze ./downloads --analyze-json | jq
```

## 后续待办

1. **age-based clean 集成**: `clean --older-than 30d` 直接消费 analyze 的 older bucket
2. **history join**: 比对 history.json,识别 orphan 文件 (有文件但历史没条目) vs 已下载但文件缺失
3. **平台分组**: 按 URL host 拆分 (twitter / zhixueyun / bilibili 等) — 需 history join