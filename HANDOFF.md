# X Video Downloader — 项目交接

**更新**: 2026-08-05（rustfmt 配置 + 90 文件格式化 + HLS per-segment 进度 + 课程池解析离线化 + STUN 注释清理 + 7 文档归档,232 tests pass）

---

## 📜 最近 commits (2026-08-05)

| Commit | 主题 | 改动文件 |
|---|---|---|
| `3029c23` | `test(tulingpython): extract parse_channel_items for offline E2E coverage` | `src/platform/tulingpython.rs` (抽离纯函数 + 3 测试 9→12) + `src/drission_client.rs` (E2E 注释指向离线覆盖) |
| `241365d` | `docs(collaboration): clarify STUN flow in team handler comments` | `src/collaboration/server/handler.rs` (两处 TODO → 准确机制说明) |
| `ef4bad3` | `feat(hls): per-segment progress callback with throttling` | `src/hls/downloader.rs` (per-segment 回调 + ≤50 次节流,经验 #26) |
| `bda810b` | `docs(plans): mark tuling course pool design as implemented + archive README` | `docs/plans/2026-07-27-*.md` 状态更新 + `docs/plans/archive/README.md` 归档原则 |
| `59e4bea` | `chore(fmt): add rustfmt.toml + apply cargo fmt across workspace` | `rustfmt.toml` (新) + 80+ 个 .rs 纯格式改动 + 7 docs/plans 归档到 archive/ |
| `95b1fbb` | `fix(downloader): TempDir 改 OS temp + merge atomic rename 杜绝 .tmpXXX/ 孤儿` | `src/downloader/parallel.rs` + `src/downloader/merge.rs` + `src/types.rs` |
| `853ae12` | `fix(drission): Keychain service 名是 "ego safe storage" 不是 "ego lite Safe Storage"` | `src/drission_client.rs` |
| `609e265` | `fix(drission): macOS 用 SQLite+Keychain 解密 ego-lite cookies` | `Cargo.toml` + `src/drission_client.rs` |
| `d6cd948` | `feat(platform): 切换 Scrapling → Drission CDP` | `Cargo.toml` + `src/main.rs` + `src/drission_client.rs` + `src/platform/*` |

> 全部 commit 含 `Intent: <一句话>` (`.git/hooks/commit-msg` 强制)。`SkillOpt/` 是 gitbutler workspace 残留。

> 全部 commit 含 `Intent: <一句话>` (`.git/hooks/commit-msg` 强制)。`SkillOpt/` 是 gitbutler workspace 残留。

---

## 快速开始

```bash
cargo run -- "URL"              # CLI
cargo run -- --gui              # GUI (Dioxus 0.7.9)
cargo test --quiet && cargo clippy --quiet --all-targets
# GUI 日志: tail -f /tmp/x-video-downloader.log
```

---

## 模块路径 (绝对)

| 模块 | 路径 |
|------|------|
| 入口 | `/Users/hgxszhj/x_videos-dowloads/src/main.rs` |
| GUI | `/Users/hgxszhj/x_videos-dowloads/src/gui_dioxus/` |
| 下载 | `/Users/hgxszhj/x_videos-dowloads/src/downloader/` |
| HLS | `/Users/hgxszhj/x_videos-dowloads/src/hls/` (parser, downloader, crypto) |
| 解析 | `/Users/hgxszhj/x_videos-dowloads/src/platform/` (ResolverRegistry singleton) |
| 下载命令 | `/Users/hgxszhj/x_videos-dowloads/src/commands/download/` |
| URL 工具 | `/Users/hgxszhj/x_videos-dowloads/src/utils/url_matcher.rs` |
| CDP 浏览器 + 登录检测 | `/Users/hgxszhj/x_videos-dowloads/src/drission_client.rs` |
| 配置 | `/Users/hgxszhj/x_videos-dowloads/src/config.rs` |
| 持久化 | `/Users/hgxszhj/x_videos-dowloads/src/history.rs` |

## GUI 子模块

| 文件 | 路径 |
|------|------|
| App 入口 + TopBar + hooks | `/Users/hgxszhj/x_videos-dowloads/src/gui_dioxus/app.rs` |
| 单视频卡片 | `/Users/hgxszhj/x_videos-dowloads/src/gui_dioxus/app_video_card.rs` |
| 批量视频列表 | `/Users/hgxszhj/x_videos-dowloads/src/gui_dioxus/app_batch_video.rs` |
| 批量进度面板 | `/Users/hgxszhj/x_videos-dowloads/src/gui_dioxus/app_batch_progress.rs` |
| 课程池面板 | `/Users/hgxszhj/x_videos-dowloads/src/gui_dioxus/app_course_pool.rs` |
| 设置面板 | `/Users/hgxszhj/x_videos-dowloads/src/gui_dioxus/app_settings.rs` |
| 历史记录 | `/Users/hgxszhj/x_videos-dowloads/src/gui_dioxus/app_history.rs` |
| 公共组件 | `/Users/hgxszhj/x_videos-dowloads/src/gui_dioxus/components.rs` |
| 业务逻辑 | `/Users/hgxszhj/x_videos-dowloads/src/gui_dioxus/handlers.rs` |
| 进度轮询 | `/Users/hgxszhj/x_videos-dowloads/src/gui_dioxus/poller.rs` |
| 状态/Store | `/Users/hgxszhj/x_videos-dowloads/src/gui_dioxus/store.rs` |
| 类型 | `/Users/hgxszhj/x_videos-dowloads/src/gui_dioxus/types.rs` |
| 主题 | `/Users/hgxszhj/x_videos-dowloads/src/gui_dioxus/theme.rs` |
| 事件 Action | `/Users/hgxszhj/x_videos-dowloads/src/gui_dioxus/actions.rs` |
| 拖放 URL bridge | `/Users/hgxszhj/x_videos-dowloads/src/gui_dioxus/bridge.rs` |

---

## 解析器

| 域名 | 解析器 | 课程池 | 文件 |
|------|--------|-------|------|
| x.com / twitter.com | FxTwitterResolver (API) | ❌ | `src/platform/fxtwitter.rs` |
| tiktok.com | TikTokResolver (需 msToken) | ❌ | `src/platform/tiktok.rs` |
| douyin.com | DouYinResolver (需 msToken) | ❌ | `src/platform/douyin.rs` |
| bilibili.com | BilibiliResolver (yt-dlp) | ❌ | `src/platform/bilibili.rs` |
| **vip.tulingpyton.cn / tulingpyton.cn** | **TulingPythonResolver (Drission CDP + 持久 profile)** | **✅ 递归 list_collection** | `src/platform/tulingpyton.rs` + `src/platform/tulingpython.rs` |
| nnyy.in | NnyyResolver | ❌ | `src/platform/nnyy.rs` |
| youtube.com | YouTubeResolver | ❌ | `src/platform/youtube.rs` |
| 其他 | YtDlpResolver (fallback) | ❌ | `src/platform/ytdlp.rs` |

---

## 小鹅通课程池 (Drission CDP)

### 入口
- GUI: `course_action.call(url)` → `fetch_course_collection_impl` (`src/gui_dioxus/handlers.rs:221`)
- 登录窗口: `open_tuling_login(url)` (`src/gui_dioxus/handlers.rs:285`)

### 登录态检测 (macOS SQLite+Keychain + Drission fallback)
```
1. macOS:is_logged_in_via_ego_lite()
   → decrypt_ego_lite_cookies_for_vip() — rusqlite 读 ~/Library/Application Support/Citro Labs/ego lite/Default/Cookies
   → security find-generic-password -s "ego safe storage" -w 拿 Keychain 密码
   → PBKDF2-HMAC-SHA1(password, "saltysalt", 1003, 16) 派生 AES-128 密钥
   → AES-128-GCM 解密 v10 cookies (Cookie value)
   → reqwest 带这些 cookies 打 xe.micro_page.common.public.data.get → 读 user_info.user_id
2. fallback (非 macOS / ego-lite 未开 / SQLite 失败): is_logged_in()
   → check_login_at(project_profile, url) — Drission headless 启动 + 拦 common.public.data.get
```

### profile 目录 (2026-08-01 第三次重构)
```
~/Library/Caches/com.xvd.x-video-downloader/tulingpyton_profile/   ← 唯一,仅 Drission 使用
```
**不再复用**系统浏览器 (ego-lite / Chrome) profile — 跨浏览器 Keychain "<ProductName> Safe Storage" 加密密钥不匹配,服务端 Code:2506。

### 关键实现 (`src/drission_client.rs`)
| 功能 | 行/函数 |
|------|---------|
| `default_profile_dir()` | 永远返回项目自建 profile (系统浏览器 profile 已被放弃) |
| `is_profile_dir_in_use()` | SingletonLock / SingletonCookie 探测 |
| `LoginStatus` enum | `LoggedIn` / `SessionExpired` / `NotLoggedIn` |
| `is_logged_in(url)` | macOS 优先 SQLite+Keychain 解密 ego-lite cookies,fallback Drission headless |
| `decrypt_ego_lite_cookies_for_vip()` (macOS) | rusqlite + security CLI + PBKDF2 + AES-GCM 解密 v10 cookies |
| `is_logged_in_via_ego_lite()` (macOS) | 用上面 cookies 调 reqwest → user_info |
| `check_login_at(profile, url)` | 单 URL Drission 探针 (fallback) |
| `log_candidate_profiles()` | 扫描 Application Support 诊断 |
| `open_login_window(url)` | macOS AppleScript 开 ego-lite;其他平台 Drission headed |
| `open_login_window_via_ego_lite(url)` (macOS) | osascript open location |
| 翻页 helper | `read_total_and_page_size`, `extract_pagination_params`, `build_pagination_js`, `merge_paginated_items` |

### 登录态分类 (`is_logged_in_via_ego_lite` / `check_login_at`)
- `LoggedIn`        `data.user_info.user_id` 是非空字符串
- `SessionExpired`  msg 含 "Code:2506" / "当前用户不存在" (cookie 还在但服务端拒绝识别)
- `NotLoggedIn`     user_info 为 null 或缺失

### `src/platform/tulingpyton.rs` 关键实现
- `list_collection(url)` 递归展开 (`visited: HashSet` 防环)
- 字段映射: `extract_video_list` 识别 3 套别名 (spu_id/title, resource_id/resource_title, id/name)
- `l_*` 优先保留为课节
- `jump_url` 改写: `/v4/course/alive/l_xxx?type=2` → `/p/t_pc/live_pc/pc/l_xxx?product_id=...&app_id=...` (`extract_product_id_from_url`)

### `src/platform/tulingpython.rs` 关键实现
- CDP 抓课节 API: `drission-rs` (替代旧 Python `patchright`)
- `capture`: 监听 `m3u8` + `xe.`，多响应时按 `l_*` / `p_*` 前缀区分优先级
- `run_js` 注入 `video.play()` + 点 `[class*=play]`
- column.items 翻页: `data.total > 20` 时由 `DrissionClient::capture` 自动注入 JS 调 page 2..N

### 测试 (232 passed / 8 ignored)

| 测试 | 文件 | 用途 |
|------|------|------|
| `course_api_substring_matches_real_tuling_course_endpoints` | `src/drission_client.rs` | 防 COURSE_API 拼错 |
| `extract_video_list_handles_course_nav_spuid_and_title` | `src/platform/tulingpyton.rs` | spu_id/title 别名 |
| `extract_video_list_handles_column_items_resource_id_and_title` | `src/platform/tulingpyton.rs` | resource_id 别名 |
| `extract_product_id_from_url_recognises_both_keys` | `src/platform/tulingpyton.rs` | jump_url 改写前提 |
| `canonicalize_url_strips_query_and_fragment` | `src/platform/tulingpyton.rs` | 防环 visited key |
| `read_total_and_page_size_handles_both_field_names` | `src/drission_client.rs` | 翻页 total 字段兼容 |
| `extract_pagination_params_reads_jump_url_or_fallback` | `src/drission_client.rs` | column_id 提取 |
| `build_pagination_js_emits_correct_fetch_calls` | `src/drission_client.rs` | 翻页 JS 模板 |
| `test_profile_dir_returns_self_built_under_project_cache` | `src/drission_client.rs` | 验证 default_profile_dir 走项目自建 |
| `integration_course_pool` | `tests/` | 真实 E2E 递归拉课节 |
| `integration_diag_lesson_player` | `tests/` | 课节页深度诊断 |
| `single_video_download_e2e` | `tests/` | A/B/C 三模式 (A=小鹅通, B=BYPASS_LOGIN, C=USE_PUBLIC_TEST) |
| `integration_login_and_capture` | `tests/` | 交互式 headed 登录 |
| `test_build_title_*` (5) | `src/fxtwitter/` | FxTwitter 标题回退链 4 级 |
| `test_recommended_segments_for_size` | `src/website_profile.rs` | 文件大小 3 分桶 |
| `test_error_display_label_*` (3) | `src/gui_dioxus/types.rs` | ErrorDisplay 标签 |
| `test_escape_applescript_string_*` (3) | `src/gui_dioxus/bridge.rs` | AppleScript 注入防御 |
| `diag_is_logged_in_real_profile` | `src/drission_client.rs` | 隔离诊断 is_logged_in |

---

## 关键实现经验

| # | 规则 | 文件 |
|---|------|------|
| 1 | URL 匹配用精确域名: `host_lower == p \|\| host_lower.ends_with(&format!(".{}", p))` | `src/utils/url_matcher.rs` |
| 2 | 错误消息净化: 移除 token/session/msToken/csrf/sig | `src/commands/download/history_writer.rs` |
| 3 | 进度阈值过滤: ≥ 1% 才回调 UI | `src/downloader/mod.rs` |
| 4 | Cookie 文件权限 0o600 (`#[cfg(unix)]` + `Permissions::from_mode(0o600)`) | `src/platform/*.rs` |
| 5 | 分段失败不回退,直接报错 | `src/hls/downloader.rs` |
| 6 | Mutex poison 自动重建: `unwrap_or_else(\|err\| err.into_inner())` + `tracing::warn!` | 全局 |
| 7 | m3u8 签名 TTL 不缓存,逐项实时拦截 | `src/platform/tulingpython.rs` |
| 8 | CDP profile 单实例 → 解析串行,下载可并发 (Semaphore) | `src/gui_dioxus/handlers.rs:311` |
| 9 | Dioxus Store 非 Sync → 跨 task 进度用原子 slot + `use_effect` 500ms 轮询 | `src/gui_dioxus/poller.rs` |
| 10 | 路径安全: 复用 `crate::types::sanitize_filename` 防路径遍历 | `src/gui_dioxus/handlers.rs:481` |
| 11 | CDP 监听过宽 → 数据 list 重复: 多响应按 `l_` / `p_` 前缀区分优先级 | `src/platform/tulingpython.rs` |
| 12 | CDP 解析跳板: 优先 `jump_url` 而非 `page_url+resource_id` (后者触发登录跳转) | `src/platform/tulingpyton.rs` |
| 13 | 媒体专属推文无文字 → title 回退链 `text → @screen_id → x_tweet_id → x_video` | `src/fxtwitter/` |
| 14 | EMA 50/50 权重: 同会话第 2 次下载即反映新测速 (旧公式收敛太慢) | `src/website_profile.rs` |
| 15 | 按文件大小分段: `<50MB→1段 / 50-500MB→4段 / >500MB→max`;`video.twimg.com` 锁 `max_threads=1` | `src/downloader/mod.rs` |
| 16 | `VideoDownloader::download` 改 `&mut self` → 5 个 call site 必须加 `let mut` | 全局 |
| 17 | Dioxus hook placement: `use_drop` 必须在 render 期注册,Signal 桥接是推荐模式 | `src/gui_dioxus/app.rs` |
| 18 | mount-once guard: 读 state 的 use_effect 会重跑,需 component-scoped `Signal<bool>` | `src/gui_dioxus/poller.rs` |
| 19 | canonical `is_downloading` flag: `AppState.is_downloading` 是 poller gate,批量按钮必须同步 | `src/gui_dioxus/app.rs` |
| 20 | bounds check 一次到位: `get_mut(idx)` 单次检查 + 单次写入 | `src/gui_dioxus/app_batch_progress.rs` |
| 21 | AppleScript 注入防御: 先反斜杠再双引号 (反之重复 escape),控制字符 → 空格 | `src/gui_dioxus/bridge.rs` |
| 22 | 结构化错误标签: ErrorPanel 硬编码字符串违反 GUI 规则,6 个类型 + fallback | `src/gui_dioxus/types.rs` |
| 23 | dead_code 精确注解: 14 处 `#[allow(dead_code)]` 逐项 + 注释保留原因 | 全局 |
| 24 | app.rs 按 feature 拆分: 7 个 < 200 行子组件 + app.rs 651 行 (-45%) | `src/gui_dioxus/app.rs` |
| 25 | 死代码字段清理: 14 处 L12 注解中 11 处安全删除 (KEEP 3 处: speed/speed_arc/cancel_flag) | `src/gui_dioxus/types.rs` |
| 26 | `is_logged_in(url)` 必须用调用方传入的 URL (SPA 首页不触发 user_info API → 误判未登录) | `src/gui_dioxus/handlers.rs:234` + `src/drission_client.rs:101` |
| 27 | macOS app profile 路径 ≠ Bundle ID: ego-lite 真实路径是 `~/Library/Application Support/Citro Labs/ego lite/` (父目录是发行公司 Citro Labs) | `src/drission_client.rs:57-82` |
| 28 | LoginStatus 必须 3 态: LoggedIn / SessionExpired / NotLoggedIn。Code:2506 ≠ NotLoggedIn (cookie 还在但服务端不认),GUI 需分别给"重扫码"和"打开登录窗口"指引 | `src/drission_client.rs` + `src/gui_dioxus/handlers.rs` |
| 29 | macOS Chromium cookie 加密密钥 = Keychain entry "<ProductName> Safe Storage"(ego-lite 实测是 **"ego safe storage"** 小写)。跨浏览器互解不出对方 cookies → 服务端 Code:2506。**唯一可靠方案**: 直接读 ego-lite SQLite + Keychain 解密出 cookies,再 reqwest 直发 API — 不切 profile、不开新浏览器、不靠 AppleScript(ego-lite 字典无 execute javascript) | `src/drission_client.rs` (`decrypt_ego_lite_cookies_for_vip`) |
| 30 | `tracing` subscriber 必须在实际跑日志的代码路径上 init。`cli.gui → run_gui()` 早 return 时,main() 里的 init 是 dead code — GUI 日志全丢。**位置**: `src/gui_dioxus/mod.rs::run_gui()` 顶部 | `src/main.rs` + `src/gui_dioxus/mod.rs` |
| 31 | LoginStatus 判定必须严格:`user_id` 是 **非空字符串** 才算登录。`user_id: null` (Code:2506 响应常见) 用 `is_some()` 误判 true。Code: `uid = ui.get("user_id").and_then(\|v\| v.as_str()).filter(\|s\| !s.is_empty())` | `src/drission_client.rs` (`is_logged_in_via_ego_lite` + `check_login_at`) |
| 32 | **`TempDir::new_in(user_dir)` 是泄漏的反模式**: `Drop` 不保证跑 (`kill -9` / 信号 / GUI 强制退出),必在用户目录留 `.tmpXXX/` 孤儿。**两条根治路径**: ① `TempDir::new()` 走 OS temp (推荐,简单);② merge 阶段写到 `<output>.tmp` + `std::fs::rename()` atomic rename 到 `<output>` (POSIX `rename(2)` 同 FS 原子)。后者同时保证 merge 中断 → 残留 `*.mp4.tmp` 而不是半截 `<output>` | `/Users/hgxszhj/x_videos-dowloads/src/downloader/parallel.rs` (`TempDir::new()`) + `/Users/hgxszhj/x_videos-dowloads/src/downloader/merge.rs` (`append_tmp_suffix` + atomic rename) + `/Users/hgxszhj/x_videos-dowloads/src/types.rs` (`validate_output_path` 标 dead_code) |
| 33 | per-segment 回调节流: 1000+ 分段视频会触发回调风暴,按 `max(1, total/50)` 取模节流 (≤50 次) + GUI 端 500ms 轮询二次吃掉 | `src/hls/downloader.rs` |
| 34 | 网络耦合代码离线测试: 抽离 `parse_channel_items(json) -> Option<(Vec, Vec)>` 纯函数,网络路径用 Option=None 表示"非该路径",消除 `#[ignore]` E2E 留下的解析盲点 | `src/platform/tulingpython.rs` |

---

## 状态

| 检查 | 结果 |
|------|------|
| `cargo test` | ✅ 232 passed, 8 ignored (2026-08-05 rustfmt + 离线 E2E 覆盖) |
| `cargo check` | ✅ 0 warnings |
| `cargo clippy --all-targets` | ✅ 0 warnings |

## 已知问题

| 问题 | 处理 |
|------|------|
| 抖音 "Fresh cookies needed" | 导出 Netscape 格式 Cookie |
| TikTok 403/HTML | 配置 msToken |
| tokio shutdown panic | `2>/dev/null` 抑制 stderr |
| 小鹅通未登录 → "内容暂无法观看" | GUI 启动 AppleScript 打开 ego-lite (macOS) 或 Drission headed (其他平台),让用户扫码 |
| 小鹅通登录过期 (Code:2506) | is_logged_in 返回 SessionExpired,GUI 提示"重扫码" |
| Chrome SingletonLock 残留 | `is_profile_dir_in_use` (`src/drission_client.rs:80`) 拦截 |
| `~/Movies/.tmpXXX/` 历史 37 个孤儿 (2026-08-04 前) | 用户手动删,保留 `.tmpYtWA2P/`。代码侧已根治 (2026-08-04 commit 95b1fbb),新下载不会再产生 |

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

# 隔离诊断 is_logged_in (2026-08-01 加)
cargo test diag_is_logged_in_real_profile -- --ignored --nocapture

# GUI 日志
tail -f /tmp/x-video-downloader.log

# ego-lite profile 路径探测
ls ~/Library/Application\ Support/ | grep -i ego

# AppleScript 手动验证 (复制粘贴到 Script Editor)
osascript -e 'tell application "ego lite"
    activate
    set URL of active tab of front window to "https://vip.tulingpyton.cn"
    delay 2
    set jsResult to (execute javascript "
        var xhr = new XMLHttpRequest();
        xhr.open("'"'"'POST'"'"'", "https://vip.tulingpyton.cn/xe.micro_page.common.public.data.get/1.0.0", false);
        xhr.setRequestHeader("'"'"'Content-Type'"'"'", "application/json");
        xhr.send(JSON.stringify({type: "pc"}));
        xhr.responseText;
    ")
    return jsResult
end tell'
```

---

## 项目记忆

- 跨项目决策/偏好 → `~/memory/`
- 当日流水 → `~/memory/daily/{YYYY-MM-DD}.md`
- "复盘" → 手动提炼到 preferences / decisions / projects
- Agent 自动记忆索引 → `/Users/hgxszhj/.claude/projects/-Users-hgxszhj-x-videos-dowloads/memory/MEMORY.md`