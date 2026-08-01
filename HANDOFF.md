# X Video Downloader — 项目交接

**更新**: 2026-08-01（X 下载速度自适应 + GUI 14 项修复 + app.rs 拆分 (1177→651 行) + 11 处死代码字段清理, 225/225 + 6 ignored tests pass, branch `gitbutler/workspace`）

---

## 🔥 working tree（未提交，按主题拆 commit）

| 主题 | 文件路径（绝对） |
|------|----------|
| **CDP 切换** (`Scrapling` → `Drission`) | `/Users/hgxszhj/x_videos-dowloads/Cargo.toml`<br>`/Users/hgxszhj/x_videos-dowloads/src/main.rs`<br>`/Users/hgxszhj/x_videos-dowloads/src/drission_client.rs` *(新建)* |
| **课程池 trait** | `/Users/hgxszhj/x_videos-dowloads/src/platform/mod.rs` |
| **小鹅通抓课节/递归/字段别名** | `/Users/hgxszhj/x_videos-dowloads/src/platform/tulingpython.rs` |
| **GUI 状态扩展** | `/Users/hgxszhj/x_videos-dowloads/src/gui_dioxus/types.rs`<br>`/Users/hgxszhj/x_videos-dowloads/src/gui_dioxus/actions.rs` |
| **课程池 + 登录按钮 + slot 轮询** | `/Users/hgxszhj/x_videos-dowloads/src/gui_dioxus/handlers.rs`<br>`/Users/hgxszhj/x_videos-dowloads/src/gui_dioxus/app.rs` |
| **真实响应 fixture** | `/Users/hgxszhj/x_videos-dowloads/tests/fixtures/tuling_course_real.json`<br>`/Users/hgxszhj/x_videos-dowloads/tests/fixtures/tuling_channel_real.json`<br>`/Users/hgxszhj/x_videos-dowloads/tests/fixtures/tuling_course_nav_real.json`<br>`/Users/hgxszhj/x_videos-dowloads/tests/fixtures/tuling_column_items_real.json` |
| **settings.json** (注册 marketplace) | `/Users/hgxszhj/x_videos-dowloads/.claude/settings.json` |
| **X 下载速度自适应 (2026-08-01)** | `/Users/hgxszhj/x_videos-dowloads/src/fxtwitter.rs`<br>`/Users/hgxszhj/x_videos-dowloads/src/website_profile.rs`<br>`/Users/hgxszhj/x_videos-dowloads/src/downloader/mod.rs`<br>`/Users/hgxszhj/x_videos-dowloads/src/commands/download/strategy_ytdlp.rs`<br>`/Users/hgxszhj/x_videos-dowloads/src/commands/download/strategy_direct.rs`<br>`/Users/hgxszhj/x_videos-dowloads/src/gui_dioxus/handlers.rs` |
| **GUI 修复 14 项 (2026-08-01)** | `/Users/hgxszhj/x_videos-dowloads/src/gui_dioxus/app.rs` *(H3/H4/H5/H6/H7full/H1/H2)*<br>`/Users/hgxszhj/x_videos-dowloads/src/gui_dioxus/actions.rs` *(H7 文案 + fetch plumbing)*<br>`/Users/hgxszhj/x_videos-dowloads/src/gui_dioxus/handlers.rs` *(M11 JoinError)*<br>`/Users/hgxszhj/x_videos-dowloads/src/gui_dioxus/components.rs` *(M9 ErrorPanel)*<br>`/Users/hgxszhj/x_videos-dowloads/src/gui_dioxus/store.rs` *(M8 watcher + M10 escape + 测试)*<br>`/Users/hgxszhj/x_videos-dowloads/src/gui_dioxus/types.rs` *(M9 label() + 测试)*<br>`/Users/hgxszhj/x_videos-dowloads/src/gui_dioxus/poller.rs` *(M6 单 transaction)* |
| **app.rs 按 feature 拆分 (2026-08-01, commit baa3328)** | `/Users/hgxszhj/x_videos-dowloads/src/gui_dioxus/app_video_card.rs` *(新建)*<br>`/Users/hgxszhj/x_videos-dowloads/src/gui_dioxus/app_batch_video.rs` *(新建)*<br>`/Users/hgxszhj/x_videos-dowloads/src/gui_dioxus/app_batch_progress.rs` *(新建)*<br>`/Users/hgxszhj/x_videos-dowloads/src/gui_dioxus/app_course_pool.rs` *(新建)*<br>`/Users/hgxszhj/x_videos-dowloads/src/gui_dioxus/app_settings.rs` *(新建)*<br>`/Users/hgxszhj/x_videos-dowloads/src/gui_dioxus/app_history.rs` *(新建)*<br>`/Users/hgxszhj/x_videos-dowloads/src/gui_dioxus/app.rs` *(1177→651 行)*<br>`/Users/hgxszhj/x_videos-dowloads/src/gui_dioxus/mod.rs` *(7 个 mod 声明)* |
| **死代码字段清理 (2026-08-01, commit 092c6b7)** | `/Users/hgxszhj/x_videos-dowloads/src/gui_dioxus/types.rs` *(删 11 字段)*<br>`/Users/hgxszhj/x_videos-dowloads/src/gui_dioxus/theme.rs` *(删 2 方法)*<br>`/Users/hgxszhj/x_videos-dowloads/src/gui_dioxus/handlers.rs` *(删 status_str)*<br>`/Users/hgxszhj/x_videos-dowloads/src/gui_dioxus/store.rs` *(删 HistoryItem.path setter)*<br>`/Users/hgxszhj/x_videos-dowloads/src/gui_dioxus/app.rs`+`actions.rs` *(删字段 setter)* |

> **接手必做**: 按上表主题 commit;每条 message 必须含 `Intent: <一句话>` (`.git/hooks/commit-msg` 拦截)。`SkillOpt/` `.claude/` 是 gitbutler workspace 残留,不进 git。

---

## 快速开始

```bash
cargo run -- "URL"              # CLI
cargo run -- --gui              # GUI (Dioxus 0.7.9)
cargo build --release           # 发行版
cargo test --quiet && cargo clippy --quiet --all-targets  # 验证
# GUI 日志（macOS）:tail -f ~/Library/Application\ Support/com.xvd.x-video-downloader/x-video-downloader.log
```

---

## 模块路径

| 模块 | 路径 |
|------|------|
| 入口 | `/Users/hgxszhj/x_videos-dowloads/src/main.rs` |
| GUI | `/Users/hgxszhj/x_videos-dowloads/src/gui_dioxus/`（mod, app, components, bridge, poller, actions, theme, types, handlers, store） |
| 下载 | `/Users/hgxszhj/x_videos-dowloads/src/downloader/` |
| HLS | `/Users/hgxszhj/x_videos-dowloads/src/hls/`（parser + downloader + crypto） |
| 解析 | `/Users/hgxszhj/x_videos-dowloads/src/platform/`（ResolverRegistry singleton） |
| 下载命令 | `/Users/hgxszhj/x_videos-dowloads/src/commands/download/`（dispatch, history_writer, strategy_direct, strategy_native, strategy_ytdlp） |
| URL 工具 | `/Users/hgxszhj/x_videos-dowloads/src/utils/url_matcher.rs` |
| CDP 浏览器 | `/Users/hgxszhj/x_videos-dowloads/src/drission_client.rs`（Drission Rust 封装） |

## 解析器

| 域名 | 解析器 | 课程池 |
|------|--------|-------|
| x.com / twitter.com | FxTwitterResolver (API) | ❌ |
| tiktok.com | TikTokResolver (需 msToken) | ❌ |
| douyin.com | DouYinResolver (需 msToken) | ❌ |
| bilibili.com | BilibiliResolver (yt-dlp) | ❌ |
| **vip.tulingpyton.cn / tulingpyton.cn** | **TulingPythonResolver (Drission CDP + 持久 profile)** | **✅ 递归 list_collection** |
| 其他 | YtDlpResolver (fallback) | ❌ |

---

## 小鹅通课程池 (Drission CDP, 2026-07-28)

### 入口
- GUI 主面板「📚 分析课程」→ `course_action.call(url)` → `fetch_course_collection_impl`
- 「🔑 打开登录窗口」→ `open_tuling_login(url)` 弹本地浏览器;**优先级**:ego-lite → Google Chrome → 系统默认 `open <url>`(2026-07-31 加);180s 后 AppleScript 仅关 tulingpyton 标签(不打扰用户其他窗口)

### 数据流
```
[课程页 URL] → fetch_course_collection_impl
  ├─ is_logged_in (CDP 拦截 common.public.data.get 探 user_info.user_id)
  ├─ list_collection(url) 递归展开(visited: HashSet 防环)
  │   ├─ l_xxx (课节) → 保留 jump_url
  │   └─ p_xxx (子课程) → 递归
  └─ 0 条 → 回退单视频流程
       ↓
CourseCollectionState → 渲染下载池（复选框/全选/状态）→ 用户点「开始下载」
       ↓
course_download_pool_impl
  ├─ resolve_items_serially (CDP 串行,共享 profile)
  │   └─ 优先 item.jump_url, fallback `page_url+resource_id`
  └─ download_resolved_concurrently (Semaphore 并发,默认 3)
      └─ slot.progress/ status 原子回写,GUI use_effect 500ms 轮询
```

### 字段映射 (3 套别名)
| 端点 | `id` | `name` |
|------|------|--------|
| `xe.micro_page.pc.course_nav.get` | `spu_id` | `title` |
| `xe.course.business_go.column.items.get` | `resource_id` | `resource_title` |
| 部分 fixture | `id` | `name` |
→ `extract_video_list` 同时识别三套;`l_*` ID 优先保留为课节。

### 关键实现
- **CDP 抓课节 API**: `drision-rs` 替代旧 Python `patchright` 子进程(快 3x,无 Python 依赖)
- **持久 profile (2026-07-31 改)**: `default_profile_dir()` 优先级 **ego-lite (`~/Library/Application Support/com.citrolabs.ego.lite/`) → Google Chrome (`~/Library/Application Support/Google/Chrome/`) → 项目自建 (`~/Library/Caches/com.xvd.x-video-downloader/tulingpyton_profile/`)**;被 SingletonLock 锁住的跳过。用户已用 ego-lite / Chrome 登录小鹅通时 Drission 直接复用,无需再开登录窗口;两者都没装/被锁时回退自建(此时必须先在 GUI「打开登录窗口」扫码登录)
- **登录态检测**: 不读本地 cookie(加密),改通过 `xe.micro_page.common.public.data.get` 响应探 `user_info.user_id`
- **SingletonLock 防炸**: `is_profile_dir_in_use` 检查残留 SingletonLock,二次点击直接提示不启动
- **GUI 登录浏览器优先级 (2026-07-31 加)**: `detect_login_browser` 按 `/Applications/ego lite.app` → `/Applications/Google Chrome.app` → 默认 `open <url>` 顺序选浏览器;`launch_browser` + `close_browser_tuling_tab` 抽象共用,主函数 4 行(detect → launch → sleep → close)
- **m3u8 拦截** (`capture`): 监听 `m3u8` + `xe.`;同一页含多个 `data.list` 响应时优先选 `l_*` 项 (避免递归展开到 `p_xxx` 形成死循环)
- **课节自动播放**: `run_js` 注入 `video.play()` + 点 `[class*=play]` 按钮
- **jump_url 改写 (2026-07-31 修)**: API 返回的 `/v4/course/alive/l_xxx?type=2&...` **单独访问返"暂无内容"**(ego-browser 实测);
  改写到 `/p/t_pc/live_pc/pc/l_xxx?product_id=p_xxx&app_id=appssry6rs71641` 才是真实可用的 player URL。
  `list_collection_impl` 在 `l_*` 分支里用 `extract_product_id_from_url`(优先 `product_id=`,fallback `pro_id=`)
  抽 product_id,app_id 来自 item 字段。少这一行改写 → 整池下载全 404。
- **column.items 翻页 (2026-07-31 加)**: `xe.course.business_go.column.items.get/2.0.0` 单页 20 项,
  `data.total > 20` 时(本次 25 期/fixture 27 期)由 `DrissionClient::capture` 自动注入 JS 调 page 2..N,
  合并进首条响应的 `data.list`。Helper: `read_total_and_page_size`/`extract_pagination_params`/
  `build_pagination_js`/`merge_paginated_items`(模块私有,4 个单元测试覆盖)。

### 已知限制
- 必须 ego-lite / Chrome / 自建 profile 之一已登录;headless/匿名课节页只返"内容暂无法观看"
- 单条直播 `live_pc/pc/l_xxx` (无 `product_id`) → `is_standalone_live` 返回空,回落单视频
- 登录 cookie 频繁过期 (Code:2506 当前用户不存在),需用户重扫码

### 测试 (219 passed / 6 ignored)
- `course_api_substring_matches_real_tuling_course_endpoints` — 防 COURSE_API 拼错回归
- `extract_video_list_handles_course_nav_spuid_and_title` — spu_id/title 别名
- `extract_video_list_handles_column_items_resource_id_and_title` — resource_id 别名
- `extract_product_id_from_url_recognises_both_keys` — (2026-07-31 加) jump_url 改写前提
- `canonicalize_url_strips_query_and_fragment` — 防环 visited key
- `read_total_and_page_size_handles_both_field_names` — (2026-07-31 加) 翻页 total 字段兼容 `total`/`total_count`
- `extract_pagination_params_reads_jump_url_or_fallback` — (2026-07-31 加) column_id 提取
- `find_id_in_url_handles_query_and_fragment` — (2026-07-31 加) URL 解析辅助
- `build_pagination_js_emits_correct_fetch_calls` — (2026-07-31 加) 翻页 JS 模板
- `integration_course_pool` — 真实 E2E 递归拉课节
- `integration_diag_lesson_player` — 课节页深度诊断
- `single_video_download_e2e` — 3 模式:A=小鹅通/B=BYPASS_LOGIN/C=USE_PUBLIC_TEST (公共 mux.dev m3u8)
- `test_build_title_prefers_text` / `_treats_whitespace_as_empty` / `_falls_back_to_handle_and_id` / `_falls_back_to_id_only` / `_ultimate_fallback` — (2026-08-01 加) FxTwitter 标题回退链 4 级
- `test_recommended_segments_for_size` — (2026-08-01 加) website_profile 文件大小 3 分桶
- `test_error_display_label_covers_all_known_types` / `_falls_back_for_unknown_type` / `_icon_and_label_are_consistent` — (2026-08-01 加) ErrorDisplay 错误类型标签
- `test_escape_applescript_string_handles_quotes_and_backslashes` / `_replaces_control_chars` / `_preserves_normal_text` — (2026-08-01 加) AppleScript 注入防御

---

## 关键实现经验

1. URL 匹配用精确域名 — `host_lower == p || host_lower.ends_with(&format!(".{}", p))`
2. 错误消息净化 — 移除 `token/session/msToken/csrf/sig`
3. 进度阈值过滤 — ≥ 1% 才回调 UI
4. Cookie 文件权限 0o600 — `#[cfg(unix)]` 下 `std::fs::Permissions::from_mode(0o600)`
5. 分段失败不回退 — 直接报错
6. Mutex poison 自动重建 — `unwrap_or_else(|err| err.into_inner())` + `tracing::warn!`
7. m3u8 签名 TTL → 不缓存,逐项实时拦截
8. **CDP profile 单实例 → 解析串行,下载可并发** (Semaphore)
9. **Dioxus Store 非 Sync → 跨 task 进度用原子 slot + `use_effect` 500ms 轮询**
10. 路径安全 — 复用 `crate::types::sanitize_filename` 防路径遍历,勿手写过滤
11. **CDP 监听过宽 → 数据 list 重复**;同一页多个 `data.list` 响应必须按前缀(`l_`/`p_`)区分优先级
12. **CDP 解析跳板 → 优先用 `jump_url`** 而非 `page_url+resource_id` (后者触发登录跳转)
13. **媒体专属推文无文字 → title 回退链 (2026-08-01)**:`build_title(text → @screen_id → x_tweet_id → x_video)`,缺一不可
14. **EMA 50/50 权重 (2026-08-01)**: 同会话第 2 次下载即反映新测速;旧公式 `(old*3+new)/4` 收敛太慢
15. **按文件大小分段 (2026-08-01)**: `<50MB→1段 / 50-500MB→4段 / >500MB→max`;`video.twimg.com` 仍锁 `max_threads=1` (实测 4 路触发 "Downloaded X bytes, expected Y")
16. **`VideoDownloader::download` 改 `&mut self`** → 5 个 call site (`strategy_ytdlp.rs`×2 / `strategy_direct.rs`×1 / `handlers.rs`×2) 必须加 `let mut`
17. **Dioxus hook placement (2026-08-01)**: `use_drop` 必须在 render 期注册,不能在 `use_effect` 内调用;Signal 桥接 (component-scoped `Signal<Option<Arc<AtomicBool>>>` + use_effect set) 是推荐模式
18. **mount-once guard (2026-08-01)**: 读 state 的 use_effect 会随 state 变化重跑,spawn 内循环每 500ms 写 state 又触发 effect → 任务指数增长;用 component-scoped `Signal<bool>` guard,循环退出时清
19. **canonical is_downloading flag (2026-08-01)**: `AppState.is_downloading` 是 `poller.rs` 的 gate;批量按钮只设 `batch_state.is_downloading` 不设 canonical → poller 退出 → 进度假死。两处必须同步
20. **bounds check 一次到位 (2026-08-01)**: 列表变化场景下 `items.get(idx)` + `items[idx]` 是经典 panic;`get_mut(idx)` 单次检查 + 单次写入
21. **AppleScript 注入防御 (2026-08-01)**: 字符串转义必须**先反斜杠再双引号**(反之会重复 escape),控制字符 (\n/\t/\r) → 空格
22. **结构化错误标签 (2026-08-01)**: ErrorPanel 硬编码 "下载失败" 违反项目 GUI 规则;`ErrorDisplay::label()` 6 个类型 + fallback,与 `icon()` 一致覆盖
23. **dead_code 精确注解 (2026-08-01)**: `#![allow(dead_code)]` 整模块压制 → 14 处 `#[allow(dead_code)]` 逐项 + 注释说明保留原因(API 复用/迁移残留/未来直读)。`cargo check` 0 warnings
24. **app.rs 按 feature 拆分 (2026-08-01)**: 1100+ 行单组件 → 7 个 < 200 行子组件 + app.rs 651 行 (-45%)。hooks 全留 AppComponent (跨面板共享,无法下沉)。hooks 全留顶层组件,子组件纯 RSX 渲染 + 事件回调
25. **死代码字段清理 (2026-08-01)**: 14 处 L12 注解中 11 处真正安全删除(setter 同步删除 + UI 不读)。KEEP 3 处有真实用途(speed/speed_arc/cancel_flag)。`cargo check` 0 warnings 维持

---

## 状态

| 检查 | 结果 |
|------|------|
| cargo test | ✅ 225 passed, 6 ignored (2026-08-01 GUI 14 项 + app.rs 拆分 + 死代码清理) |
| cargo check | ✅ 0 warnings |
| cargo clippy --all-targets | ✅ 0 warnings (pre-existing main.rs:108) |

## 已跳过的高 ROI 项 (建议独立 PR)

| # | 内容 | 跳过原因 |
|---|------|---------|
| 1 | 共享 `ChromiumBrowser` 跨整池 | 需改造 `DrissionClient` 暴露复用 API |
| 2 | 轮询改 `Notify`/channel 事件驱动 | 依赖架构级统一抽象 |
| 3 | `ItemRow` / `ListKind` / `ItemStatus` 统一抽象 | 跨 4 文件 |
| 4 | 统一 `BatchItemState` / `CourseItemDisplay` | 跨 types/handlers/app/poller |
| 5 | `download_one` 改调 `start_download_impl` | 错误类型不一致 |
| 6 | `handlers.rs` 502 行按 feature 拆分 | 本次 app.rs 拆分已落地,handlers 是下一目标 |

---

## 已知问题

| 问题 | 处理 |
|------|------|
| 抖音 "Fresh cookies needed" | 导出 Netscape 格式 Cookie |
| TikTok 403/HTML | 配置 msToken |
| tokio shutdown panic | `2>/dev/null` 抑制 stderr |
| 小鹅通未登录 → "内容暂无法观看" | GUI 启动本地浏览器(ego-lite → Chrome → 默认)180s 后 AppleScript 关 tulingpyton 标签 |
| 小鹅通登录过期 (Code:2506) | 用户重扫码后复用 profile |
| Chrome SingletonLock 残留 | 二次启动前 `is_profile_dir_in_use` 拦截,提示用户关 headed Chrome(优先级检测时被锁的 profile 自动跳过) |
| `cargo fmt --check` 报 ~490 处格式差异 | **pre-existing**,无 rustfmt.toml |

---

## 调试

```bash
RUST_LOG=debug cargo run -- "URL" 2>/dev/null

# 真实课节 E2E (模式 A: 已登录)
SINGLE_VIDEO_URL='https://vip.tulingpyton.cn/.../l_xxx' \
  cargo test single_video_download_e2e -- --ignored --nocapture
# 模式 C: 完全跳过小鹅通,验证 HLS 下载链路
USE_PUBLIC_TEST=1 cargo test single_video_download_e2e -- --ignored --nocapture

# 交互式 headed 登录
cargo test integration_login_and_capture -- --ignored --nocapture

# GUI 日志
tail -f ~/Library/Application\ Support/com.xvd.x-video-downloader/x-video-downloader.log
```

---

## 项目记忆

- 跨项目决策/偏好 → `~/memory/` (SessionStart hook 自动注入)
- 当日流水 → `~/memory/daily/{YYYY-MM-DD}.md` (Stop hook 自动追加)
- "复盘" → 手动提炼到 preferences / decisions / projects
- Agent 自动记忆索引 → `/Users/hgxszhj/.claude/projects/-Users-hgxszhj-x-videos-dowloads/memory/MEMORY.md`
