# M5: installer 模块 (受 mole `mo installer` 启发)

**日期**: 2026-10-04
**模块文件**: `src/commands/installer.rs` (455 行)
**CLI**: `--installer [--installer-json]`

## 改动清单
| 文件 | 改动 |
|------|------|
| `src/commands/installer.rs` | 新建 — collect_installer_report / parse_version_output / compare_versions / format_report / run_installer |
| `src/commands/mod.rs` | 注册 `pub mod installer;` |
| `src/cli.rs` | +2 fields: `installer` / `installer_json` |
| `src/main.rs` | dispatch 块 |

## 验证
| 步骤 | 结果 |
|------|------|
| `cargo build` | 0 errors, 4 warnings (1 个新 timeout_secs `#[allow(dead_code)]`,其余 pre-existing) |
| `cargo test commands::installer` | 15 passed, 0 failed |
| `cargo test` (full suite) | **375 passed**, 12 ignored (+15 from M5, 0 regression) |
| Smoke `--installer` | ✅ yt-dlp outdated (2026.03.17 vs 2026.8.19) + ffmpeg latest + 1 升级建议 |

## 设计要点

### 1. 三层架构
- `LatestVersionFetcher` trait + `HttpLatestVersionFetcher` 实现 → 抽象便于测试
- `collect_installer_report(opts)` 编排函数 → 隔离 OS 进程探测
- `classify(current, latest) -> status` 纯函数 → 业务规则

测试时注入 `static_fetcher(...)` 替代真实 HTTP,避免依赖外部网络。

### 2. 版本比较算法 (`compare_versions`)
按 `.` 分段 → 逐段 `u64` 比大小 → 缺失段补 0。
例:
- `"1.2.3"` vs `"1.2.4"` → Less
- `"1.2"` vs `"1.2.0"` → Equal (缺失段补 0)
- `"2024.10.7"` vs `"2024.10.07"` → Equal (前导 0 不影响)

### 3. 解析 `--version` 输出 (`parse_version_output`)
支持 stdout + stderr 拼接 + 跨行查找首个数字段。`v`-prefixed helper 可能误命中(如 "Copyright (c) 2024"),实际通过 `extract_version_token` 严格切到非版本字符为止。

### 4. 状态机
| current | latest | status |
|---------|--------|--------|
| None | _ | unavailable |
| _ | None | unknown |
| Less | _ | outdated |
| Equal | _ | latest |
| Greater | _ | ahead |

### 5. latest 数据源策略
- **yt-dlp**: `curl pypi.org/pypi/yt-dlp/json` → `info.version`
- **ffmpeg**: `brew list --versions ffmpeg` → 取最后一段(macOS 路径)
- **python3**: 跳过(太复杂,留 latest=None → status=unknown)

任一失败都 `None` 静默降级,不阻塞整体报告。

## 踩坑 (3 个)

### Test #1: `parse_ffmpeg_style`
原 `combined.lines().next()` 当 `output=""` 时把空行当作首行 → 误判。
修复:`first non-empty line` — `.lines().map(str::trim).find(|l| !l.is_empty())`。

### Test #2: `classify_outdated_when_current_less`
原测试调 `collect_installer_report` 拿真实 yt-dlp 版本 + 注入 latest,断言 "current=2024.10.07,status=latest"。但真实机器上 yt-dlp 是 "2026.03.17",assertion 直接 panic。
修复:测试只断言分类器逻辑(`classify()` 纯函数),不绑真实 probe。

### Test #3: `classify_ahead_status`
原测试用 `"99.0.0"` vs `"2024.10.07"` 期望 "ahead"。但我的版本比较把 `99` 和 `2024` 都作为数字段比较 → `99 < 2024` → 实际 "outdated"。
修复:用 `"2026.01.01"` vs `"2024.10.07"`,语义清晰无歧义。

## 收益 / 风险
- **收益**: 用户每次升级前可一键查看 yt-dlp 是否过期,smoke 证实本地 yt-dlp 已 outdated 一个 minor 版本。
- **风险**: HTTP latest 拉取受 pypi/brew 可用性影响,默认 fail-soft (`None` → `unknown`);不阻塞报告输出。

## mole → x-video-downloader 映射

| mole | x-video-downloader |
|------|-------------------|
| `mo installer` (管理 mole 自身安装) | `--installer` (检查依赖工具链) |

注:mole 的 installer 是"自管理",x-video-downloader 是"管理 yt-dlp / ffmpeg / python3 依赖"。语义不同但工作流位置对等。

## 未处理 (诚实报告)
- `timeout_secs` 字段本意给 HTTP fetcher 用,实际 `Command::new("curl").args(["--max-time", "10"])` 写死 10s。已加 `#[allow(dead_code)]` 而非删除,留作未来 API 表面扩展。
- ffmpeg latest 数据源仅限 `brew` (macOS),Linux/Windows 用户会拿到 `None` → `unknown`。
- python3 latest 故意跳过,避免引入额外复杂度。