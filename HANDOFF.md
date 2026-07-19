# X Video Downloader — 项目交接

**更新**: 2026-07-19（HEAD `c22634c`, 196/196 tests pass, branch `gitbutler/workspace`）

---

## 🔥 working tree (未提交, 接手必 commit)

| 文件 | 改动 | 一句话 |
|------|------|--------|
| `src/main.rs` | −2 | `mod queue;` 移除（queue.rs 已弃用，文件待 `git rm`） |
| `src/queue.rs` | 删除 | **#![allow(dead_code)]** 0 caller，由用户执行 `git rm` |
| `SkillOpt` | untracked | gitbutler workspace 已删除文件，git 无影响 |

> **接手必做**：用户自行 `git rm src/queue.rs` → 一次性 commit（main.rs + queue.rs 一并）；不要 `git add -A`！

---

## 快速开始

```bash
cargo run -- "URL"              # CLI
cargo run -- --gui              # GUI (Dioxus 0.7.9)
cargo build --release           # 发行版
cargo test --quiet && cargo clippy --quiet --all-targets  # 验证
```

---

## 模块路径

| 模块 | 路径 |
|------|------|
| 入口 | `src/main.rs` |
| GUI | `src/gui_dioxus/`（mod.rs, app.rs, components.rs, bridge.rs, poller.rs, actions.rs, theme.rs, types.rs, handlers.rs, store.rs） |
| 下载 | `src/downloader/` |
| HLS | `src/hls/`（parser.rs + downloader.rs + crypto.rs） |
| 解析 | `src/platform/`（ResolverRegistry，singleton 化） |
| 下载命令 | `src/commands/download/`（mod.rs + dispatch.rs + history_writer.rs + strategy_direct.rs + strategy_native.rs + strategy_ytdlp.rs） |
| URL 工具 | `src/utils/url_matcher.rs`（避免 platform ↔ history 循环） |
| 配置 | `src/config.rs` |
| 类型 | `src/types.rs` |
| 历史 | `src/history.rs` |

## 解析器

| 域名 | 解析器 |
|------|--------|
| x.com / twitter.com | FxTwitterResolver（API） |
| tiktok.com | TikTokResolver（需 msToken） |
| douyin.com | DouYinResolver（需 msToken） |
| bilibili.com | BilibiliResolver（yt-dlp） |
| 其他 | YtDlpResolver（fallback） |

---

## 关键实现经验

1. URL 匹配用精确域名 — `host_lower == p || host_lower.ends_with(&format!(".{}", p))`
2. 错误消息净化 — 移除 `token/session/msToken/csrf/sig`
3. 进度阈值过滤 — ≥ 1% 才回调 UI
4. Cookie 文件权限 0o600 — `#[cfg(unix)]` 下 `std::fs::Permissions::from_mode(0o600)`
5. 分段失败不回退 — 回退会重复下载，直接报错

---

## 状态

| 检查 | 结果 |
|------|------|
| cargo test | ✅ 196 passed (新加 11 个回归测试: singleton + mutex audit + dispatch + history_writer) |
| cargo clippy --all-targets | ✅ 0 warnings |

---

## 已知问题

| 问题 | 处理 |
|------|------|
| 抖音 "Fresh cookies needed" | 导出 Netscape 格式 Cookie |
| TikTok 403/HTML | 配置 msToken |
| tokio shutdown panic | `2>/dev/null` 抑制 stderr |
| `cargo fmt --check` 报 ~490 处格式差异 | **pre-existing**（5c27679/6433dad 等历史 commit 引入），无 rustfmt.toml。修法：单独 `cargo fmt` 一个 commit，或加 rustfmt.toml 配置（建议同时加 `[profile.dev]`，避免引发 CI 动荡） |

---

## 近期 commits（近 5）

| SHA | 类型 | 内容 |
|-----|------|------|
| `c22634c` | fix | Dioxus 0.7.9 launch 三参 + Config.with_window（窗口标题） |
| `27696c9` | fix | yt-dlp 改用 spawn + piped stdout（实时进度） |
| `daad859` | fix | HLS 分段合并改顺序读+立即写（修 OOM） |
| `0298f76` | test | 用无锁路径测试替代 poison-recovery（修复 cargo test 顺序问题） |
| `d8449ea` | docs | 同步 phase 2.2-2.4 + P0-3 audit |

---

## 调试

```bash
RUST_LOG=debug cargo run -- "URL" 2>/dev/null
```

---

## 项目记忆

- 跨项目决策/偏好 → `~/memory/`（SessionStart hook 自动注入）
- 当日流水 → `~/memory/daily/{YYYY-MM-DD}.md`（Stop hook 自动追加）
- "复盘" → 手动提炼到 preferences / decisions / projects
- Agent 自动记忆索引 → `~/.claude/projects/-Users-hgxszhj-x-videos-dowloads/memory/MEMORY.md`
