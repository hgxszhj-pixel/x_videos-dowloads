# Handoff Document

> 精简版：保留代码修改 + 验证状态 + 决策结论，丢弃分析过程与调研细节。

## Working Tree 状态 (2026-07-08)

**全部 refactor + clippy 已 commit，working tree 干净。** 10 commits（master `6433dad`）：

```
6433dad fix(clippy): silence dead_code warnings on HLS public API + mechanical fixes
7e830d0 chore(docs+infra): Multi-Agent doc + .claudeignore + refactor plan + scripts
0e2f34d chore: remove dead lib.rs + tests/types_test.rs
9a4b242 fix(collaboration): handle poisoned mutexes + dead code cleanup
42a9cd9 feat(hls): add HLS downloader + nnyy/tulingpython resolvers
96fe901 refactor(gui): split gui_dioxus into 6-module structure
abbaafd refactor(downloader): split into parallel/config/merge submodules
492b92c refactor(cli): extract cli + commands modules from main.rs
6c366ba refactor(core): split core module into submodules
2c4ddb7 chore(deps): add HLS dependencies
```

**验证状态：** `cargo check` 0 errors, `cargo test --bin x-video-downloader` **185 passed**, `cargo clippy -- -D warnings` 0 issues.

**测试命令变：** `cargo test --lib` 已失效（lib.rs 已删），改用 `cargo test --bin x-video-downloader`。

---

## Session: 2026-07-08

### 上下文压缩工作流

- **执行：** 手动 `/compact "保留代码修改与文件路径，丢弃分析过程"`
- **决策：** 不引入自动触发（评估 Skill/Rule/Hook/Env var，最终手动已够简单）
- **若改主意：** `~/.claude/settings.json` 加 `"CLAUDE_AUTOCOMPACT_PCT_OVERRIDE": "80"`

### `.claudeignore`（已 commit 7e830d0）

覆盖：`node_modules/` `dist/` `build/` `.next/` `/target/` `__pycache__/` `*.lock` `package-lock.json` `*.log` `.git/` `.idea/` `.vscode/` `*.png/jpg/svg/ico` `.cache/` `coverage/`

⚠️ 未确认 Claude Code 是否原生支持；不支持时回退到 `~/.claude/CLAUDE.md` 加硬性提示。

### Commit 拆分执行（9 → 10 commits）

按 docs/REFACTOR_PLAN.md 计划，混合 working tree 中的 Phase 1.x/3.1 + HLS feature + 协作 fix + 死代码清理，拆分为：

| # | Commit | 文件 | 备注 |
|---|--------|------|------|
| 1 | `chore(deps)` | `Cargo.toml` | m3u8/aes/cbc/generic-array/typenum/lru |
| 2 | `refactor(core)` | `core/{mod,cache,constants,progress,segment}.rs` | Phase 1.1 |
| 3 | `refactor(cli)` | `main.rs` + `cli.rs` + `commands/` + `resolver_chain.rs` | Phase 1.2, main 1062→126 行 |
| 4 | `refactor(downloader)` | `downloader.rs`(D) + `downloader/` | Phase 3.1 |
| 5 | `refactor(gui)` | `gui_dioxus.rs`(D) + `gui_dioxus/` 6 模块 | Phase 1.3 |
| 6 | `feat(hls)` | `hls/` + `platform/{nnyy,tulingpython,mod,url_matcher}.rs` + `scrapling_{client,extractor}.rs` + `types.rs` | 新 feature |
| 7 | `fix(collaboration)` | `collaboration/{client/ws,server/db}.rs` + `i18n.rs` + `history.rs` | poisoned mutex 防御 + 死代码 + flaky test fix |
| 8 | `chore: remove lib.rs` | `lib.rs`(D) + `tests/types_test.rs`(D) | 消除双编译 |
| 9 | `chore(docs+infra)` | `CLAUDE.md` + `.claudeignore` + `REFACTOR_PLAN.md` + `scripts/` | docs + tooling |
| 10 | `fix(clippy)` | 10 files | `cargo clippy -- -D warnings` → 0 issues |

**关键决策：**
- 9-commit 而非原计划 5-commit：实际改动比预期多（HLS + 协作 fix 混在工作树）
- HLS public API 全部加 `#[allow(dead_code)]`：公开 API 表面，不应被 clippy 当死代码删
- `mod.rs` 保留 `#![allow(dead_code)]`：避免后续子模块继承警告风暴
- 用 `cargo clippy --fix` 处理 mechanical lints（clone_on_copy/needless_borrow 等），手动加 `#[allow(dead_code)]` 处理 public API
- `cargo clippy --fix` 会错误地删 `#[cfg(test)]` 内的 import，需手动恢复

**验证：** `cargo check` 0 errors, `cargo test --bin x-video-downloader` **185 passed**, `cargo clippy -- -D warnings` 0 issues.

---

## Session: 2026-07-07 — 重构执行

### 提交历史

```
e82ec15 docs(handoff): 记录 Phase 2.2/2.3/2.4 重构执行结果
ae9911a refactor(dead_code): 删除 14 个零使用 dead_code 方法/类型 (Phase 2.4)
b0791d8 refactor(db): 提取 SQLite schema 到独立 schema.sql (Phase 2.3)
2b63bea refactor(history): 删除 DownloadStatus 未使用变体 InProgress/Bookmarked (Phase 2.2)
```

### Phase 1.x — 模块拆分（代码在 working tree，未 commit）

| Phase | 文件 | 结果 |
|-------|------|------|
| 1.1 | `src/core/mod.rs` | 删 re-export 死代码（24-32 行），破循环依赖 |
| 1.2 | `main.rs` 1062 → 126 行（-88%） | 新增 `cli.rs` (132) + `resolver_chain.rs` (165) + `commands/{download,history,serve,path,mod}.rs` |
| 1.3 | `gui_dioxus.rs` 1188 → 6 模块 | `gui_dioxus/{mod,app,handlers,types,store,theme}.rs`；`AppComponent` 整体不拆（Signal 闭包语义风险） |
| 3.1 | `downloader.rs` 1137 → 4 模块 | `downloader/{mod,config,parallel,merge}.rs`；descendant 可见性 + `pub(super)` 跨模块调用 |

**关键技术决策：**
- `mod.rs` 保留 `#![allow(dead_code)]`，子模块继承 → 无新 warning
- `pub use config::DownloaderConfig;` 对外重导出，调用方零改动
- Dioxus 子组件不拆：后台 CLI 无法逐按钮验证交互，拆有静默回归风险

**验证：** `cargo check` 0 errors，`cargo test --lib` 184 passed，warning 数与 baseline 持平。

### Phase 2.x — 清理

| Phase | 改动 | 文件 |
|-------|------|------|
| 2.2 | 删 `DownloadStatus::InProgress/Bookmarked` | `src/history.rs` |
| 2.3 | db.rs SQL → `schema.sql`（include_str!） | `src/collaboration/server/{db.rs,schema.sql}` |
| 2.4 | 删 14 个 dead_code 项（203 行 / 8 文件） | `i18n.rs`/`discovery.rs`/`queue.rs`/`transfer/downloader.rs`/`danmaku.rs`/`douyin_api.rs`/`zhixueyun.rs`/`collaboration/discovery/mod.rs` |

**2.4 保留的反例（被测试/serde 误判为 dead）：**
- `fxtwitter.rs` 7 个结构体 — serde 反序列化必需
- `cookie_pool::CookieEntry` / `douyin_api::VideoDetailResponse` — 同文件内 impl
- `queue.rs::mark_complete` 等 — `#[cfg(test)]` 引用
- `history.rs::remove_bookmark` 等 — tests 引用
- `types.rs` 11 个 DownloadRequest builder — 预留 API

**经验：** `grep uses_elsewhere=0` 对 `#[allow(dead_code)]` 强误导。方法级判断准确，字段级常因 serde 误报。删除前必须 grep + `#[cfg(test)]` 扫描 + cargo check 三重验证。

### 已知问题（未修）

- **预存 flaky 测试：** `i18n::tests::test_font_family_chinese` 并发竞态（全局 `CURRENT_LANG` Mutex）→ **已修复**（commit 9a4b242 引入 `with_lang()` test helper 保存/恢复当前语言）
- **lib + bin 双编译：** → **已解决**（commit 0e2f34d 删除 lib.rs + tests/types_test.rs）
- **Phase 1.3 + 3.1 代码未 commit** → **已 commit**（见顶部新提交列表）

---

## Session: 2026-07-04 — HLS / 小鹅通

### 文件变更

| 改动 | 文件 |
|------|------|
| HLS 下载器（parser/crypto/downloader） | `src/hls/{mod,parser,crypto,downloader}.rs` |
| nnyy.in 解析器 | `src/platform/nnyy.rs` |
| 小鹅通解析器 | `src/platform/tulingpython.rs` |
| `--referer` CLI 参数 | `src/cli.rs` + `HlsDownloader::referer()` |
| HLS 指数退避重试（10 次，delay=min(100ms×2^n, 20s)）+ skip_unavailable | `src/hls/downloader.rs` |
| HLS 代理支持（socks5/http/https） | `HlsDownloader::with_config()` |
| CatCatch Chrome 扩展（m3u8 拦截） | `scripts/cat-catch/{manifest.json,background.js,popup/}` |
| TulingPython 课程列表 | `src/platform/tulingpython.rs` (`extract_video_list()`) |
| HLS m3u8 自动检测走 HlsDownloader | `download_direct()` |
| 文件名自动提取 | `extract_filename_from_url()` |

### 关键 bug 修复

- `scrapling_client.rs`: 删 `quote()` 包裹 URL bug，改用 dict wrapper `_state`（Python nonlocal 对 module var 无效）
- `main.rs` HLS 输出路径 `%(ext)s` → `.mp4`

### 已知限制

- nnyy.in TLS `builder error`（hnts.ymuuy.com 需特殊配置或代理）
- 小鹅通 sign 签名有时效 → 必须 CatCatch 实时拦截
- 小鹅通 m3u8 → 403（sign 过期）；referer 参数已生效但 sign 仍需实时抓取

### 新增依赖 (Cargo.toml)

```toml
m3u8 = "0.1"
aes = "0.8"
cbc = "0.2"
generic-array = "0.14"
typenum = "1.17"
lru = "0.10"
```

### 历史下载测试

| URL | 输出 | 备注 |
|-----|------|------|
| `x.com/ask_your99357/.../2071801495159452092` | `~/Movies/chudai_test.mp4` (895.7MB) | FxTwitter 解析 → video.twimg.com 直链 |
| `nnyy.in/dianshiju/202552712.html` | `~/Movies/wuzhi.mp4` (381.9MB, 49:08) | yt-dlp 走 m3u8，AES-128，1920x1080 |

---

## Session: 2026-07-03 及更早 — 工具调研

### 工具状态（结论，不展开过程）

| 工具 | 状态 | 备注 |
|------|------|------|
| Agent-Reach v1.4.0 | ✅ 已安装 | 8/16 渠道（GitHub/YouTube/微信/V2EX/RSS/Exa/Jina/B站） |
| Caveman skill | ✅ 已装，未默认启用 | 需手动加 settings.json permissions；只压缩输出 |
| headroom 0.2.0 | ⚠️ 仅 proxy 命令 | 与 MiniMax `x-api-key` 认证不兼容 |
| GSD hooks | ❌ 已清理 | 8 个 gsd-*.sh/.js 引用不存在文件 |
| Unlimited-OCR | ⏸️ 等 torch 2.4+ | `scripts/unlimited_ocr_{cli,api,mscope}.py` 已写 |

### 早期代码修改（2026-06）

| 日期 | 改动 | 文件 |
|------|------|------|
| 2026-06-30 | downloader 拆 core 模块 + Multi-Agent 写入 CLAUDE.md | `src/core/{constants,cache,progress,segment}.rs`、`CLAUDE.md` |
| 2026-06-29 | 测试编译修复 + clippy warning | `tests/types_test.rs`、`src/yt_dlp.rs` |
| 2026-06-26 | HeadCache 简化（45→5 行）+ lru crate | `src/downloader.rs`、`Cargo.toml` |
| 2026-06-22 | x.com 走 FxTwitterResolver + try_resolve_all 兜底 + YouTube URL 提取 | `src/fxtwitter.rs`、`src/platform/mod.rs`、`src/yt_dlp.rs` |

---

## 环境配置摘要

**Claude Code 配置：**
- Backend: `api.minimaxi.com`（MiniMax 代理，非 Anthropic 官方）
- Model: `MiniMax-M3`（非 Claude 系列）
- 计费主体: MiniMax（订阅/按量取决于用户在 MiniMax 后台选择）
- 项目仓库: gitbutler/workspace（HEAD: e82ec15）

**关键 CLAUDE.md 规则：**
- 12 Rules（Think/Simplicity/Surgical/Goal-Driven/Model-only-judge/Token-budget/Conflict-expose/Read-before-write/Test-intent/Checkpoint/Convention/Fail-loud）
- 安全规则：禁读 `.env/.pem/.key`、拦截 `git push --force`、拦截 `curl | bash`、禁通配符 `rm -rf`
- 编程理念：谨慎优于速度、简单优于复杂、解决今天的问题而非明天的问题