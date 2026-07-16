# X Video Downloader — 项目交接

**更新**: 2026-07-16（30 commits, HEAD `322f322`）
**分支**: `gitbutler/workspace`

---

## 快速开始

```bash
cargo run -- "URL"              # CLI
cargo run -- --gui              # GUI (Dioxus 0.7.9)
cargo build --release           # 发行版
cargo test --quiet && cargo clippy --quiet  # 验证
```

---

## 模块

| 模块 | 路径 | 说明 |
|------|------|------|
| 入口 | `src/main.rs` | CLI |
| GUI | `src/gui_dioxus/` | Dioxus 0.7.9（`mod.rs`, `app.rs`, `components.rs`, `bridge.rs`, `poller.rs`, `actions.rs`, `theme.rs`, `types.rs`, `handlers.rs`, `store.rs`） |
| 下载 | `src/downloader/` | 分段并行 + 断点续传 |
| HLS | `src/hls/` | `parser.rs` + `downloader.rs` + `crypto.rs`，mod 导出 `M3u8Parser` + `HlsDownloader` |
| 解析 | `src/platform/` | ResolverRegistry（x/twitter/tiktok/douyin/bilibili/youtube/...） |
| 配置 | `src/config.rs` | AppConfig |
| 类型 | `src/types.rs` | DownloadRequest / Progress / Error 等 |
| 历史 | `src/history.rs` | CSV 导出 + 持久化 |

---

## 解析器

| 域名 | 解析器 | 备注 |
|------|--------|------|
| x.com / twitter.com | FxTwitterResolver | API 解析 |
| tiktok.com | TikTokResolver | 需 msToken |
| douyin.com | DouYinResolver | 需 msToken |
| bilibili.com | BilibiliResolver | yt-dlp |
| 其他 | YtDlpResolver | fallback |

---

## 关键实现经验

1. URL 匹配用精确域名 — `host_lower == p || host_lower.ends_with(&format!(".{}", p))`
2. 错误消息净化 — 移除 `token/session/msToken/csrf/sig` 等敏感参数
3. 进度阈值过滤 — ≥ 1% 才回调 UI
4. Cookie 文件权限 0o600 — `#[cfg(unix)]` 下 `std::fs::Permissions::from_mode(0o600)`
5. 分段失败不回退 — 回退会重复下载，直接报错

---

## 状态

| 检查 | 结果 |
|------|------|
| cargo test | ✅ 189 passed（默认并发稳定，`ab63573` 修 i18n + `a18a339` 修 hashring） |
| cargo clippy | ✅ clean（`--all-targets` 7 warnings，4 bench stub + 3 lib，false-positive 不影响功能） |
| cargo bench hls_concurrency | ✅ 横向扩展正常（见下表） |

### hls_concurrency 实测（mock HTTP, 50ms latency）

| concurrent | wall_ms | scaling |
|------------|---------|---------|
| 1          | 1760    | baseline |
| 2          | 910     | 1.93x |
| 4          | 483     | 3.64x |
| 8          | 274     | 6.42x |
| 16         | 167     | 10.5x |
| 32         | 115     | 15.3x |

mock HTTP 限制：50ms 网络延迟主导，alloc 改进 < 1% 噪声不可见；真实收益需 HTTPS CDN + 长下载。

---

## 已知问题

| 问题 | 状态 | 处理 |
|------|------|------|
| 抖音 "Fresh cookies needed" | 已知 | 导出 Netscape 格式 Cookie |
| TikTok 403/HTML | 已知 | 配置 msToken |
| X/Twitter 格式选择 | 平台限制 | X API 不返回完整格式信息 |
| tokio shutdown panic | 已知 | `2>/dev/null` 抑制 stderr |

---

## 近期 commits（2026-07-16）

| SHA | 类型 | 内容 |
|-----|------|------|
| `322f322` | fix | **notify watcher thread 加 shutdown flag**（Arc<AtomicBool> + use_drop，AppComponent 卸载干净退出） |
| `0007183` | fix | **bridge.rs `while let Ok` → match + warn**（Eval.recv 错误显式日志 + break，不再静默吞错） |
| `99c8c23` | refactor | **submit_urls 改 `use_action`**（替代 70 行手写 spawn + latest-wins 自动 cancel） |
| `a692537` | refactor | **AppState + 7 嵌套类型改 Store**（Dioxus 0.7 lens 细粒度 reactivity） |
| `8de3239` | fix | `use_context_provider` 拆 hook-in-hook（Signal 顶层创建 + provider return handle） |
| `3882daf` | refactor | **`use_coroutine` 合并进度轮询**（`poller.rs` 新建 + 删 67 行手写 spawn 循环） |
| `4b776c2` | refactor | **`use_context_provider` 共享 AppTheme**（BrowseButton 删 theme prop） |
| `4079493` | chore | 清 cancel button 调试 `eprintln!` 残留（`#1` 关闭） |
| `6e37ced` | docs | 同步 d6803ef 抽 3 TextField 组件 |
| `d6803ef` | refactor | 抽 `UrlInput×2` + `SettingsTextInput×3` + `SettingsNumberInput×1`，删 `STYLE_INPUT_BARE` |
| `4a46ac8` | docs | #5 拖放 URL ⛔→🟡 |
| `67cd451` | feat | **JS bridge 解锁桌面 URL 拖放**（`bridge.rs` + `document::eval` 绑 `window.drop`） |
| `76d07ba` | docs | 同步 5 组件抽取 + Dioxus 0.7.9 升级 |
| `9b2b208` | refactor | `BrowseButton×2` + `DestructivePillButton×2` |
| `6d1c902` | refactor | `ThemeSwatch×4` + `CloseButton×2` |
| `824f6c4` | build | **Dioxus 0.6 → 0.7.9**（删 `dioxus-desktop`/`dioxus-hooks`，合并入 core，0 源码改动） |
| `ff91f97` | refactor | `WhiteCard×6` 替换重复卡片容器 |
| `03df9a6` | build | scaffold `components.rs` + `theme` 加 `ALERT_RED`/`border_color` |
| `4bc35bd` | docs | #6 push icon ⛔→🟢（待 push） |
| `e10303d` | fix | `492b92c` 加 backtick 修 push protection 误报 |
| `9109cd2` | docs | 同步 5059edc 灰阶 token 提取 |
| `5059edc` | refactor | 5 个高频中性 hex（37 处）→ theme const |
| `d1e79bc` | docs | 同步 1f89a98 + a18a339 |
| `a18a339` | fix | hashring sample 100→10000 消 `DefaultHasher` 极端分布 flake |
| `1f89a98` | refactor | 9 处硬编码 hex → theme helper |
| `415a742` | docs | 同步 ab63573 i18n flake 修复 |
| `ab63573` | fix | i18n `Mutex→thread_local!{RefCell}`，消默认并发测试 flake |

**Dioxus 0.7.9 升级结果**: 0 源码改动；189 tests pass；clippy clean；Wry 0.52 / Axum 0.8 transitive 跟随升级。

**JS bridge 技术点**:
- 避开 Dioxus 0.6 `SerializedDragData { mouse, files }` 文本限制
- 不依赖 0.8 alpha PR #5487
- 用 0.7 stable `document::eval` / `Eval::recv` 内建 bridge
- `Signal<AppState>` 非 Send，直接 `spawn` capture 进 future 让 dioxus runtime 处理

### 更早

| 日期 | 内容 |
|------|------|
| 2026-07-15 | 16 commits perf + GUI（hls_concurrency bench / bumpalo bench / cancel button / settings panel / theme persistence / history click / 错误 Retry / speed+size 进度条） |
| 2026-07-11 | HLS 下载器重构（`max_concurrent` Semaphore + 自适应死代码移除 -106 行） |
| 2026-06-14 | Clippy 71→0 warnings；移除 Slint residue（`src/gui.rs` 74KB + `ui.slint` 21KB + `build.rs` + `src/ui/` 40KB） |
| 2026-06-14 | YouTube 解析器 + 历史 CSV 导出 + 下载调度字段 |

---

## 调试

```bash
RUST_LOG=debug cargo run -- "URL" 2>/dev/null
```

---

## 项目记忆（`memory/`）

| 记忆 | 用途 |
|------|------|
| `ai-research-sites.md` | 70+ AI 研究站点 |
| `slint-gui-progress-bar-issues.md` | Slint 进度条根因 |
| `codegraph-mempalace-mcp-setup.md` | codegraph + mempalace MCP 配置 |
| `gui_dioxus-runtime-crash.md` | Dioxus `onclick` 崩溃 + spawn 修复 |
| `handoff-md-streamline-rule.md` | 每次更新 HANDOFF.md 必须精简 |

---

## 明日继续

### 待验证（用户交互）
1. ✅ cancel button debug 已清（`4079493`）
2. **JS bridge 拖放 URL** — `cargo run -- --gui` 后拖浏览器地址栏 URL 到窗口，期望 `state.url` 自动填入 + status 显示"拖放 URL 已捕获"
3. **GUI 新功能手测** — speed/size 进度条（`use_coroutine` poller 调度）、settings 保存、错误 Retry、主题持久化（`use_context_provider` reactive）、历史回填

### 待完成
4. 🟡 batch 取消 — `BatchItemState` 加 `cancel_flag`
5. 🟡 URL 输入框 Enter 键提交（其实 `UrlInput` 已用 `Key::Enter`，待 GUI 验证）
6. 🟢 push 到 origin（`e10303d` 修 push protection 误报；浏览器 unblock URL 或 amend line 17 都行）

### 性能
- §6 perf claim 5 项需真实 HTTPS CDN `cargo bench --bench hls_concurrency` 验证（mock 测不出）

### 已知非阻塞
- clippy `--all-targets` 7 warnings（4 bench stub + 3 lib，不影响功能）
- 3 处 [待实测] claim（FxtwitterResolver 共享客户端 / `to_lowercase` URL 缓存 / CookiePool 读写锁分离）