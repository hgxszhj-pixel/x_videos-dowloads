# X Video Downloader — 项目交接

**更新**: 2026-07-19（HEAD `10909d8`, 196/196 tests pass, branch `gitbutler/workspace`）

---

## 🔥 working tree (未提交, 接手必 commit)

| 文件 | 改动 | 一句话 |
|------|------|--------|
| `src/hls/downloader.rs` | −30/+12 | 分段并行读全段 → 顺序读 + 立即写（修 OOM） |
| `src/downloader/mod.rs` | −21/+35 | `.output().await` → `.spawn()` + piped stdout（yt-dlp 实时进度） |
| `src/gui_dioxus/mod.rs` | −3/+6 | Dioxus 0.7.9 `launch` 3-arg + `Config::with_window(...)`（设窗口标题） |
| `src/main.rs` | −2 | `mod queue;` 移除（queue.rs 已弃用，文件待 `git rm`） |
| `src/queue.rs` | 删除 | **#![allow(dead_code)]** 0 caller，由用户执行 `git rm` |

> **接手必做**：逐文件 commit（不要 `git add -A`！），Intent Lock + `Intent:` 一行 + push

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

---

## 近期 commits（近 5）

| SHA | 类型 | 内容 |
|-----|------|------|
| `10909d8` | refactor | **拆 commands/download.rs (548行) 为 6 模块** |
| `832c9b8` | docs | audit GLOBAL_HISTORY mutex invariant (P0-3 audit) |
| `dbe3425` | perf | ResolverRegistry 提升为单例 |
| `cd59b78` | refactor | 拆 platform ↔ history 循环依赖 |
| `24bae6a` | fix | P0-1 历史状态 + P0-2 wget token 泄漏 |

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
