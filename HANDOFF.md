# X Video Downloader — 项目交接

**更新**: 2026-07-24（HEAD `3d43eae`, 196/196 tests pass, branch `gitbutler/workspace`）

---

## 🔥 working tree (未提交, 接手必 commit)

| 文件 | 改动 | 一句话 |
|------|------|--------|
| `CLAUDE.md` | 替换 | 135→36 行精简版（移除冗余 project summary / Intent Lock 范例 / 旧 build 命令） |
| `src/gui_dioxus/CLAUDE.md` | 新建 | 嵌套 GUI 错误提示规则（⚠️ + 红色 + 解决建议） |
| `.gitignore` | +2 行 | 例外 `download-agent-loop/SKILL.md`（其他 `.claude/skills/*` 仍忽略） |
| `SkillOpt/` | untracked | gitbutler workspace 残留，无 git 影响 |
| `.claude/` | untracked | gitbutler workspace 残留，无 git 影响 |

> **接手必做**：按各自 commit message 提交；Intent Lock 要求 `Intent: <一句话>` 一行。不要 `git add -A`！

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
6. Mutex poison 自动重建 — `unwrap_or_else(|err| err.into_inner())` + `tracing::warn!`（state-atomic 类型适用）

---

## 状态

| 检查 | 结果 |
|------|------|
| cargo test | ✅ 196 passed |
| cargo clippy --all-targets | ✅ 0 warnings |
| P0 待办 | ⏳ (1) `m3u8 0.1` → `hls4-client 1.0.4`（其余 3 项已 ✅） |

### P0 完成明细（2026-07-24）

| 项 | 状态 | SHA | 摘要 |
|----|------|-----|------|
| 删除死代码 `src/queue.rs` | ✅ | `3cf736f` | 602 行 0 caller → `git rm` |
| SQL DDL → `schema.sql` | ✅ | `b0791d8`（前置） | `db.rs:54` 改用 `include_str!` |
| `GLOBAL_REGISTRY` poison 自动重建 | ✅ | `3d43eae` | `resolver_chain.rs:39,74` panic → `into_inner()` |

---

## 已知问题

| 问题 | 处理 |
|------|------|
| 抖音 "Fresh cookies needed" | 导出 Netscape 格式 Cookie |
| TikTok 403/HTML | 配置 msToken |
| tokio shutdown panic | `2>/dev/null` 抑制 stderr |
| `cargo fmt --check` 报 ~490 处格式差异 | **pre-existing**（历史 commit 引入），无 rustfmt.toml |

---

## 近期 commits（近 7）

| SHA | 类型 | 内容 |
|-----|------|------|
| `3d43eae` | fix | `resolver_chain` poison panic 改为自动重建 |
| `3cf736f` | chore | 删除死代码 `src/queue.rs` (602 行) |
| `39a1338` | docs | 标注 cargo fmt --check 490 处 pre-existing 差异 |
| `c9109e0` | docs | 同步 c22634c 4 commits（store 测试 + HLS OOM + yt-dlp 进度 + Dioxus 标题） |
| `c22634c` | fix | Dioxus 0.7.9 launch 三参 + Config.with_window（窗口标题） |
| `27696c9` | fix | yt-dlp 改用 spawn + piped stdout（实时进度） |
| `daad859` | fix | HLS 分段合并改顺序读+立即写（修 OOM） |

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
