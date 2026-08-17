# X Video Downloader — 项目交接

**更新**: 2026-08-17 (OpenCode 多 provider + 全局 git 安全防御 — grok 设为默认 + gitleaks pre-commit 拦截密钥)
**上次**: 2026-08-16 (P1 enum 重构 + 2 个 bug 修复:课程池不进历史 / 进度条假 56%)
**再上次**: 2026-08-16 (P1 重构 + 进度条 UX + Dioxus Store 修复三件事,见下文合并章节)
**更早**: 2026-08-13 (GUI 响应瓶颈 P0/P1/P2 + runtime 升级 multi_thread)
**更早**: 2026-08-09 (小鹅通端到端下载 6 节课 8.5GB 验证通过 + 流水线转 .mp4)

---

## 教学 workspace 维护约定 (`.teach/`,gitignored)

`.teach/` 是项目**经验沉淀区**,不是 session 日志。

**写 LR 当** | **写 HANDOFF session 当** (二选一,不要两写)
---|---
编译错误反复出现才搞懂 | 改了 N 个 bug
反汇编依赖源码才懂 | 配置 X → Y
跨 2+ 文件复用模式 | 性能改进 N%
用户说"我懂了" | milestone 达成

**格式与触发器**: `.teach/HOWTO-WRITE-LR.md`
**当前 LR 数量**: 7 条 (`learning-records/0001-` ~ `0007-`)

**原则**: HANDOFF 写"做了什么",teach 写"学到了什么"。

---

## Session: 2026-08-17 — OpenCode 多 provider 配置 + 全局 git 安全防御

### 改动 1: OpenCode 1.18.18 接入 wuzuapi.com 多模型代理

合并参考文档 `/Users/hgxszhj/Documents/wuzuapi.txt` 到 `~/.config/opencode/opencode.json`:

| Provider | SDK 包 | 模型 |
|---|---|---|
| `anthropic` (必须此名) | `@ai-sdk/anthropic` | claude-fable-5, opus-5, sonnet-5, haiku-4-5 |
| `wuzuapi-grok` (改名) | `@ai-sdk/openai-compatible` | grok-4.5(裁剪后仅 1 个) |

**SDK 命名约束**(反向工程经验,已写入 `~/.claude/projects/.../memory/opencode-ai-sdk-provider-key-constraints.md`):
- `@ai-sdk/anthropic` 硬编码 provider key 必须叫 `anthropic`,改名后模型注册为空
- `@ai-sdk/openai-compatible` 允许任意名

**清理 Antigravity**(`opencode-antigravity-auth@1.6.0` 插件 + 5 个 `antigravity-*` 模型条目)。删后原生 google 模型仍由 OpenCode 内置自动注册,保留 `provider.google` 空块维持 schema 合法。

**缓存清理** 释放 ~507M:`~/.cache/opencode/` (models.json + packages)、`antigravity-accounts.json`、`oh-my-opencode.json.bak.2026-01-24*`。

**全局默认模型**: `wuzuapi-grok/grok-4.5`(`opencode.json` 顶层 `model` 字段)。

**当前 `~/.local/share/opencode/auth.json` 共 9 个 provider**:`wuzuapi-grok`、`google`(OAuth)、`deepseek`、`openrouter`、`zai-coding-plan`、`zhipuai-coding-plan`、`minimax-cn`、`minimax-cn-coding-plan`、`anthropic`(本会话后续从 `opencode.json` 迁移,见改动 3)。

### 改动 2: 全局 git 安全防御

**a. 装 gitleaks 8.30.1**(`brew install gitleaks`)

**b. 扩展 `~/.gitignore_global`**:加 14 个密钥文件名 glob(`*.pem`、`*.key`、`*.p12`、`*.pfx`、`*.p8`、`*.keystore`、`service-account*.json`、`*-credentials.json`、`.aws/credentials`、`.gcp/credentials`、`id_rsa`/`id_ed25519`/`id_ecdsa`/`id_dsa`、`.opencode/auth.json`)。保留用户原有 3 条(`*~`、`.DS_Store`、`**/.claude/settings.local.json`)。

**踩坑** — 第一版用 `*secret*`/`*credentials*` 宽 glob,导致 smoke test 时含 "secret" 的正常文件名被 git 忽略 → gitleaks 扫不到东西。修正为**只匹配具体文件名**,不匹配路径关键字。

**c. 改 `~/.git-hooks/pre-commit-user`**(用户原有的 Rust clippy 脚本;GitButler 管理的 `pre-commit` 不动):在顶部插入 gitleaks 扫描,**所有仓库生效**(不只 Rust)。检测到 secret → exit 1 阻止 commit。Smoke test 通过:
- 干净文件 → exit 0
- 含 `sk-proj-...` / `ghp_...` / `AKIA...` → exit 1 拦截

**链路**: `git commit` → GitButler `pre-commit` → `pre-commit-user` → gitleaks protect --staged → (Rust 仓库) cargo clippy + cargo test

**d. 备份**(便于回退):
- `~/.gitconfig.bak.2026-08-17T06-46-44`
- `~/.config/opencode/opencode.json.bak.2026-08-17T06-07-52` (合并 wuzuapi 前)
- `~/.config/opencode/opencode.json.bak.rename.2026-08-17T06-47-49` (provider 改名前)
- `~/.local/share/opencode/auth.json.bak.rename.2026-08-17T06-47-49` (provider 改名前)

### 改动 3: 失败快速修复 + grok-4.5 context 上限校验

**问题**: `opencode run -m wuzuapi-grok/grok-4.5 "ping"` 挂起 **308 秒**,TUI 只显示 `Sisyphus online` 横幅(oh-my-openagent 启动信息),"ping" 从未被处理。

**根因链**(诊断套路已写入 memory `opencode-wuzuapi-upstream-silent-retry.md`):
1. wuzuapi 把上游 Grok 失败伪装成 OpenAI 兼容 `{"type":"upstream_error"}` → 反而触发 AI SDK 自动重试
2. `@ai-sdk/openai-compatible` 默认 `maxRetries: 2` + 指数 backoff(~10s)
3. OpenCode TUI 额外重试层 + 流式 socket hang → 累计 ~5 分钟

**隔离测试**: curl 直接打 wuzuapi,6.79s 拿到 `upstream_error` (HTTP 502),确认非网络/Key 问题。

**修复**(3 处配置改动):
| 文件 | 字段 | 变更 |
|------|------|------|
| `opencode.json` | `provider.wuzuapi-grok.options.maxRetries` | `0`(不重试 upstream_error) |
| `opencode.json` | `provider.wuzuapi-grok.options.timeout` | `30000`(单请求最多 30s) |
| `opencode.json` | `provider.anthropic.options` | 同步加 `maxRetries: 0` + `timeout: 30000`(预防 anthropic 上游挂同样挂起) |
| `opencode.json` | `provider.wuzuapi-grok.models.grok-4.5.limit.context` | `500000 → 256000`(对齐官方 Grok-4 256K,避免超限被截断) |
| `auth.json` | 新增 `anthropic.key` | 从 `opencode.json.provider.anthropic.options.apiKey` 迁移(消除双存储分裂,与 8 个其他 provider 一致) |
| `opencode.json` | 删除 `provider.anthropic.options.apiKey` | 见上行 |

**效果**: `opencode run -m wuzuapi-grok/grok-4.5 "ping"` 从 **308s → 41.6s (7.4x 提速)**,响应正常。上游挂时 fail-fast 应在 ~30s 内报错。

**备份**:
- `opencode.json.bak.failfast.2026-08-17T08-28-43`
- `opencode.json.bak.context-fix.2026-08-17T08-41-47`
- `opencode.json.bak.sync-anthropic.2026-08-17T08-46-43`
- `auth.json.bak.sync-anthropic.2026-08-17T08-46-43`

### 已知风险 / 未做

- **`wuzuapi-grok` provider `apiKey` 仍明文存储在 `opencode.json`**(同其他 provider 风格,且 `.opencode/auth.json` 已在 `~/.gitignore_global`);若未来纳入 dotfiles 仓库需统一移到 `.env` + 环境变量插值
- **未启用 GPG commit 签名** (`commit.gpgsign`) — 需要时再说
- **`~/.omo/omo.jsonc` 已统一到 grok-4.5**(2026-08-17 本会话续):12 个 builtin agents + 8 个 categories 全部走 `wuzuapi-grok/grok-4.5`;清掉 Antigravity/opencode-gpt-5.x/kimi 等失效引用;`opencode run` 默认 sisyphus 已确认走 grok-4.5
- **`grok-4.5` 真实 context 上限未实测** — wuzuapi 标 "grok-4.5" 但 xAI 官方无此模型,实测 200K payload 时上游再次挂,未能确认 256K 是否保守。如出现频繁超限,可下调至 128K
- **oh-my-opencode v3.4.0 → v4.19.4 有可用更新** — 暂不升级(避免 schema 变更破坏 omo.jsonc),需要时再说

---

## Session: 2026-08-16 — P1 重构 + 进度条 UX + Dioxus Store 修复(三件事合并)

### 1. P1: `set_status` 改用 `CourseItemStatus` 枚举 + 2 个用户反馈 bug

`CourseItemStatus` 7 variants (Pending/Resolving/Downloading/Completed/Failed/Skipped/Exists),`set_status(&str)` 改 `&CourseItemStatus` 签名,编译器强制覆盖所有 case。

- **Bug 1** — 课程池新下载不进历史 → `add_completed_download` 在 `course_download_pool_impl::download_resolved_concurrently` 完成后补调 + `state.write().history = get_history_items()` 立即刷新 HistoryPanel
- **Bug 2** — 底部进度条假 56%(5×100% exists + 4×0% pending 平均)→ 新增 `aggregate_course_progress` helper 只统计 active 状态 + `is_terminal()` 方法去掉 `(3..=6)` magic

### 2. 进度条 UX 详细化 + 跳过已下载

| 区域 | 新增字段 |
|---|---|
| 顶部单下载 | + ETA (`eta_signal: Signal<String>`) |
| 批量头部 | + 聚合速度 (active 项 `saturating_add`) |
| 课程池每行 | + 速度 + 已下载/总字节 (`format_size_fraction`) |

新增 helpers: `format_eta(seconds)`、`format_size_fraction(d, t)`。新增 `status=6 = exists`:`resolve_items_serially` 在 CDP 解析前检查 `output_path` 大小 > 0 → 跳过解析+下载,UI 标 `✓ 已下载`(0 字节占位文件视为不存在,损坏文件留作"强制重下"扩展点)。

### 3. Dioxus Store 字段级订阅修复(根因级)

**症状**: 后端进度推 50%/100%,UI 卡在 `pending` 不刷新(课程池 + 批量下载都中招)。

**根因**: `state.read().batch_state.items.iter()` 走 `Vec::iter()` 快照 — Dioxus `Store<Vec<T>>::iter()` **只跟踪 Vec 长度**(shallow track),字段变化不触发 sub-render。

**修复模式**(3 文件统一):
```rust
let items_store = state.course_collection_state().items();  // Store<T> 实现 Copy,不要 .clone()
for idx in 0..items_store.len() {
    let item = items_store.index(idx);
    div { "{item.title()}" }                                    // 自动 cloned
    if item.visible()() { ... }                                 // 条件需双括号调用
}
```

`derive(Store)` 自动生成 `<StructName>StoreExt` trait;链式返回 `Store<T, MappedMutSignal<...>>` 让编译器推断。P3 同步:课程池/批量每行下载速度、全部下载按钮 disabled 反馈。

### 改动文件(7 个)

`handlers.rs` (`CourseItemStatus` + impl + `set_status` + `add_completed_download` + `is_terminal()` + 6 测试) · `app_course_pool.rs` (`slot_status_to_str` + `aggregate_course_progress` + 5 测试 + pool 完成后刷新 + 字段订阅) · `app_batch_progress.rs` (头部聚合 + 每行速度 + 字段订阅) · `app_batch_video.rs` (`BatchState.aggregate_speed` + 按钮 disabled + 字段订阅) · `types.rs` (`format_eta` + `format_size_fraction` + `CourseItemDisplay.{speed,downloaded_bytes,total_bytes}`) · `poller.rs` (ETA + 聚合速度 + 阈值节流) · `app.rs` (`eta_signal` + 顶部 UI)

**验证**: `cargo build` ✅ | `cargo clippy --all-targets` ✅ 0 warning | `cargo test` ✅ **256 passed, 0 failed, 19 ignored**(本会话 +12 测试:6 enum/is_terminal + 5 聚合 + 1 单元)

**rsx! 限制**(LR0006): `{}` 内不支持 `if`,inline 条件显示需提为 helper。

**知识沉淀 →** `.teach/` (gitignored): 7 条 learning records (`0001-` ~ `0007-`) + Tufte 速查表 `reference/dioxus-0.7-reactivity.html` + 项目 mission/glossary 标准化。详见 `.teach/MISSION.md`。

---

## Session: 2026-08-13 — GUI 性能改进 + runtime 升级

**问题**: GUI 按钮点击延迟 ~500ms、进度条跳变粗糙、课程池 33 项同时下载时尤为明显。

**根因**: `AppState` 单一 Store + 双 500ms polling task 写整个 store → 每次进度 tick 触发整个 AppComponent (251 行 rsx) 重渲染。

**7 处改进 (按影响排序)** — 内存: `gui_dioxus-runtime-context-crash.md` (Dioxus hook 配对表 + smoke test 必要性):

| # | 文件 | 改动 |
|---|------|------|
| P0-1 | `gui_dioxus/app.rs` (-60) | 删重复 use_effect polling task,保留新课程池 task (有聚合/退出守卫/单事务写) |
| P0-2 | `gui_dioxus/app.rs` + `poller.rs` (+35) | 拆 3 个独立 `use_signal<f32/String>` (progress/speed/size),poller 写 Signal 不写 store,缩小重渲染范围到进度条区域 |
| P1-1 | `gui_dioxus/app.rs` | `theme_signal` + `use_context_provider` 保留 — **components.rs 有 3 个 use_context 消费者 (ThemeSwatch/ErrorPanel/AnotherPanel),删除会 panic** |
| P1-2 | `gui_dioxus/app.rs` + `store.rs` | `persist_theme` helper 合并 `state.theme = X` + `cfg.save()`,4 个 ThemeSwatch 共享 |
| P1-3 | `gui_dioxus/poller.rs` | speed/size 阈值节流 (≥100 KiB/s 变化才更新) |
| P2-1 | `main.rs` | `new_current_thread` → `new_multi_thread().worker_threads(4)`,前提: reqwest::blocking 预初始化 + GLOBAL_HISTORY 锁内无 await |
| P2-2 | `gui_dioxus/poller.rs` | (本次未做 — Vec clone 已通过 impl Default 消除) |

**新 commit**: `aefd94b perf(gui): split progress signals, upgrade multi_thread runtime, refactor theme/poller (P0/P1/P2)` (6 files / +195/-115 surgical)

**working tree 残留**: `src/drission_client.rs` 仍 unstaged — 混合分支 owner ~1400 行重构 + 本会话 5 个 pre-existing clippy fix (let_unit_value × 3 + manual_range_patterns + nonminimal_bool),留给 owner 一并 commit。

**教训**: `git commit -- <files>` 提交的是整个文件快照 (含 pre-existing 修改),不是 hunks。需 `git reset HEAD~1 --mixed` + `git stash` + 重 add 自有文件。

---

## 端到端里程碑: 小鹅通课程 → 本地 .mp4 (2026-08-09)

**课程**: `https://vip.tulingpyton.cn/p/t_pc/course_pc_detail/column/p_6a66e8d7e4b0694c35307f38`
**结果**: 6 条课节全部下载到 `/Users/hgxszhj/Movies`,共 8.5 GB .mp4,总耗时 1125s (~19min)

| 课节 | 大小 |
|---|---|
| js逆向01 AI逆向 - 逆向环境搭建与Agent配置-2026-7-27 | 1.5 GB |
| JS逆向02 AI逆向 - js语法基础 - 1-2026-07-29 | 980 MB |
| JS逆向03 AI逆向 - js语法基础 - 2-2026-07-31 | 1.4 GB |
| JS逆向04 AI逆向 - js语法基础 - 3-2026-08-03 | 1.1 GB |
| JS逆向05 AI逆向 - 浏览器调试技巧 - 1-2026-08-05 | 1.9 GB |
| JS逆向06 AI逆向 - hook 技术-2026-8-7 | 1.6 GB |

**6 处根因修复** (内存详记: `tulingpyton-ego-lite-cookie-injection.md`):

1. **`src/hls/parser.rs::extract_base_url`** — 保留 m3u8 所在目录路径,签名 CDN 对路径敏感 (`+` 修复)
2. **`src/drission_client.rs::capture()` 临时 profile** — `std::env::temp_dir()/xvd_diag_<pid>_<idx>` 替代项目自建 profile,防 stgw 400 字节限制 + 隔离 stale cookies
3. **`src/drission_client.rs::diag_download_all_lessons` 流水线** — 每条课节 HLS 下载 .ts 成功后**立即** `ffmpeg -c copy` 转 .mp4 + 删 .ts
4. **`src/platform/tulingpython.rs::list_collection_impl` reqwest-fastpath** — macOS + column_id → 先 `fetch_column_items_reqwest()` 直击 `column.items.get/2.0.0`,失败 fallback Drission (40+ 并发 POST 触发 WebSocket 重置修复)
5. **`src/platform/tulingpython.rs::get_video_info` diag-fastpath** — macOS + `live_pc/pc/l_*` URL → 调 `fetch_m3u8_via_diag(player_url, idx)` (临时 profile + 12 cookies 注入 + 8 轮 JS 触发 play),失败 fallback Drission
6. **`src/gui_dioxus/app_course_pool.rs` 进度/状态 polling task** — 500ms 读所有 slot 原子值 → 写 store (单项进度/状态);同时累加聚合 → 写 `state.download_progress` 驱动底部进度条

**端到端链路**:
```
decrypt_ego_lite_cookies_for_vip() (Keychain + PBKDF2 + AES-128-CBC v10)
  → reqwest POST column.items.get/2.0.0 (0.43s,绕开 headless SPA)
  → Drission headless Chromium 临时 profile + CDP set_cookies (12 cookies)
  → 改写 jump_url → /live_pc/pc/l_xxx → tab.get + 4 轮 JS 触发 play
  → 拦 m3u8 (xet.tech playlist_eof.m3u8 + sign)
  → HlsDownloader 拉 m3u8 + 8 并发 .ts + 顺序合并 → ffmpeg -c copy → .mp4
```

---

**已落地的最近 commits** (今日,按时间倒序):

| Commit | 主题 |
|---|---|
| `12eca70` | `fix(gui): course-pool aggregation excludes pending/parsing/exists` |
| `fef948f` | `fix(gui): record course-pool downloads to global history` |
| `58b0b6f` | `refactor(gui): replace parallel status switches with CourseItemStatus enum` |
| `661fab8` | `test(gui): add unit tests for new code (P0)` |
| `192ba2d` | `fix(gui): hide misleading "0 B" size column` |
| `0e80ce4` | `fix(gui): unified status display with emoji icons (P2)` |
| `ede263a` | `perf(gui): per-row progress/speed/size for course pool` |
| `4a25dc9` | `feat(gui): skip already-downloaded lessons (skip-existing)` |

**再之前的 commits**: 见 `git log --oneline -20`(全部 commit 含 `Intent:` 行,`.git/hooks/commit-msg` 强制)。

---

## 快速开始

```bash
cargo run -- "URL"                              # CLI
cargo run -- --gui                              # GUI (Dioxus 0.7.9)
cargo test --quiet && cargo clippy --quiet --all-targets
tail -f /tmp/x-video-downloader.log             # GUI 日志

# 小鹅通端到端 (需 ego-lite 已登录)
cargo test --bin x-video-downloader -- --ignored \
  drission_client::tests::diag_download_all_lessons --nocapture

# 单课程课节列表 smoke test (0.6s 验证 reqwest-fastpath)
cargo test --bin x-video-downloader -- --ignored \
  drission_client::tests::diag_reqwest_new_url --nocapture
```

---

## 模块路径

| 模块 | 绝对路径 |
|------|----------|
| 入口 | `src/main.rs` |
| GUI 根 | `src/gui_dioxus/` (app, app_video_card, app_batch_video, app_batch_progress, app_course_pool, app_settings, app_history, components, handlers, poller, store, types, theme, actions, bridge) |
| 下载 | `src/downloader/` (mod, config, parallel, merge) + `src/core/segment.rs` |
| HLS | `src/hls/` (mod, parser, downloader, crypto) |
| 解析 | `src/platform/` (ResolverRegistry singleton) |
| 命令 | `src/commands/download/` (mod, dispatch, history_writer, strategy_direct/native/ytdlp) |
| CDP 浏览器 + 登录检测 | `src/drission_client.rs` |

## 解析器

| 域名 | 解析器 | 课程池 |
|------|--------|-------|
| x.com / twitter.com | FxTwitterResolver (API) | ❌ |
| tiktok.com | TikTokResolver (需 msToken) | ❌ |
| douyin.com | DouYinResolver (需 msToken) | ❌ |
| bilibili.com | BilibiliResolver (yt-dlp) | ❌ |
| **vip.tulingpyton.cn** | TulingPythonResolver (Drission CDP) | ✅ 递归 |
| nnyy.in | NnyyResolver | ❌ |
| youtube.com | YouTubeResolver | ❌ |
| 其他 | YtDlpResolver (fallback) | ❌ |

**智学云 (`kc.zhixueyun.com`) 解析器已存在但未启用** — 文件 `src/platform/zhixueyun.rs` (748 行) + `src/platform/zhixueyun_token/mod.rs` (94 行),设计为 SSO + Bearer__token 直击 API + 课程章节递归。详见 `~/.claude/projects/-Users-hgxszhj-x-videos-dowloads/memory/zhixueyun-*.md`。

---

## 小鹅通课程池 — 关键路径

- GUI 入口: `course_action.call(url)` → `fetch_course_collection_impl` (`src/gui_dioxus/handlers.rs:221`)
- 登录窗口: `open_tuling_login(url)` (`src/gui_dioxus/handlers.rs:285`)
- 登录态检测: `is_logged_in(url)` (`src/drission_client.rs`)
  - macOS: SQLite + Keychain 解密 ego-lite cookies (`decrypt_ego_lite_cookies_for_vip`)
  - fallback: Drission headless (`check_login_at`)
- 3 态返回: `LoggedIn` / `SessionExpired` / `NotLoggedIn`
- **临时 profile** (每条课节独立): `std::env::temp_dir()/xvd_diag_<pid>_<idx>` —— 防 stgw 400 + 隔离 stale cookies
- 翻页 helper: `read_total_and_page_size`, `extract_pagination_params`, `build_pagination_js`, `merge_paginated_items` (`src/drission_client.rs`)

---

## HLS 下载关键实现 — `src/hls/`

| 机制 | 函数 / 位置 |
|------|------|
| 并发限流 | `Semaphore::new(self.config.max_concurrent)` |
| 分段重试 | `download_segment_with_retry` (exponential backoff + Retry-After 优先) |
| 取消标志 | `cancel_flag: Arc<AtomicBool>` |
| 进度回调 | per-segment 节流 ≤50 次,累计字节 + 起始时间算 speed |
| 顺序合并 | 不读所有分段到内存,逐段 read + write |
| 加密 (AES-128-CBC) | `decrypt_segment` (大段 ≥1MB 走 `spawn_blocking`) |
| **`extract_base_url`** | **保留 m3u8 所在目录路径,签名 CDN 对路径敏感** |

---

## 状态

| 检查 | 结果 |
|------|------|
| `cargo test` | ✅ **256 passed, 0 failed, 19 ignored** (含 4 个 drission_client 诊断测试 + 5 个 tulingpython fixture 测试 `#[ignore]`;今日 +6 个新测试:5 个聚合 + 1 个 is_terminal) |
| `cargo fmt --check` | ✅ 0 diff (今日 working tree 全部 fmt-clean) |
| `cargo clippy --all-targets` | ✅ 0 warning (今日修了 GUI 5 + drission_client.rs 5 + 课程池聚合 magic 数字,均已 commit) |

## 已知问题

- 抖音 "Fresh cookies needed" → 导出 Netscape 格式 Cookie
- TikTok 403/HTML → 配置 msToken
- tokio shutdown panic → `2>/dev/null` 抑制 stderr
- 小鹅通未登录 → "内容暂无法观看",GUI 启动 AppleScript/Drission headed 让用户扫码
- 小鹅通登录过期 (Code:2506) → `is_logged_in` 返回 `SessionExpired`
- Chrome SingletonLock 残留 → `is_profile_dir_in_use` 拦截
- **5 个 tulingpython 单元测试 `#[ignore]`** → `tests/fixtures/` 4 个 JSON 在 10 GB 清理中误删,需重新抓取真实登录响应后取消 ignore
- **2 个 [[bench]] 已注释** → `benches/` 目录已删,后续如需 benchmark 重新建 `benches/` 后恢复

## 本会话清理记录 (2026-08-09)

8 项删除(`track_quality.sh` / `spec_contract.py` / 3 个 unlimited_ocr / 旧 pre-commit / 4 个 fixtures / 2 个 benches) + 6 项 gitignore(`.codegraph/` 等防 6.4 MB codegraph.db 入库) + README 重写(149 → 327 行)。

## 文档归档 (2026-08-16)

**Single source of truth**: `/HANDOFF.md` (root)。`docs/HANDOFF.md` 281 行 stale 副本已删除,新文档不再保留 docs/ 副本。

## 记忆入口

- 跨项目决策/偏好 → `~/memory/`
- 当日流水 → `~/memory/daily/{YYYY-MM-DD}.md`
- Agent 自动记忆索引 → `/Users/hgxszhj/.claude/projects/-Users-hgxszhj-x-videos-dowloads/memory/MEMORY.md`
