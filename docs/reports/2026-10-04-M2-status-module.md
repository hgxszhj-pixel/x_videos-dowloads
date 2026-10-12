# M2 报告 — status 子命令 (受 mole `mo status` 启发)

**日期**: 2026-10-04
**模块**: `src/commands/status.rs` (新建)
**状态**: ✅ 完成 + cargo build/test 全绿

## 变更清单

| 文件 | 变更 |
|------|------|
| `src/commands/status.rs` | 新建,~280 行 + 9 个 unit tests |
| `src/commands/mod.rs` | 加 `pub mod status;` |
| `src/cli.rs` | 加 `--status` / `--status-json` |
| `src/main.rs` | dispatch 分支,在 clean 之后插入 |

## 健康检查项

| 检查 | 实现 | 严重等级触发 |
|------|------|------------|
| `yt-dlp --version` | `which::which` + Command | missing → Error |
| `ffmpeg -version` | 同上 | 同上 |
| `python3 --version` | 同上 | 同上 |
| 输出目录可写 | 写 probe 文件后删 | 不存在/不可写 → Error |
| 磁盘剩余空间 | `df -k <dir>` | < 100MB Error / < 1GB Warn |
| 历史文件可读 | `File::open` | 不存在 → Warn |
| cookie 文件可读 | 同上 | 同上 |

## 设计要点

- **每个 check 独立函数**: `check_command` / `check_writable` / `check_disk_space` / `check_file_readable`,可单独 unit test
- **`Severity` 三级** (Ok/Warn/Error): `HealthReport::worst()` 返回最严重的,降级时 impact 一目了然
- **JSON 双格式**: `--status` 给人读, `--status-json` 给 CI/监控
- **跨平台兼容**: `free_bytes` 在 unix 走 df,非 unix 返回 None → warn 而非 panic

## 验证记录

| 命令 | 结果 |
|------|------|
| `cargo build` | 0 errors |
| `cargo test commands::status` | **9 passed** |
| `cargo test` 全量 | **339 passed**, 12 ignored (基线 320+9+8,无回归) |
| 手动 smoke | 真实环境显示: 5 ok (yt-dlp/ffmpeg/python/目录可写/磁盘) + 1 warn (history 文件未生成) |

## 踩坑教训
| 坑 | 解决 |
|----|------|
| `impl Into<String>` 接 `&str` 字面量,`.into()` 模糊(Into<Cow<str>> / Into<String> 都满足) | 改 API 为 `impl Into<String>`,调用处对 `&str` 字面量不用 `.into()` — 编译器提示 `&str: Into<String> trivially holds` |
| 6 errors → `&str.into()` 在多 `Into` impl 下歧义 | 全部移除冗余 `.into()` |

## CLI 用法

```bash
# 人读
./target/debug/x-video-downloader --status

# JSON (CI/监控)
./target/debug/x-video-downloader --status --status-json | jq
```

## 后续待办

1. **缓存检查结果**: 重复跑 `--status` 不重复 spawn 命令 (加 `--status-cache-ttl`)
2. **网络检查**: yt-dlp 远端版本对比,提示更新
4. **多磁盘**: 多 output_dir 时聚合报告