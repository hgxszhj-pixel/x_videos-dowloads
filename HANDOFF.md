# X Video Downloader - 项目交接文档

**更新时间**: 2026-07-15  
**分支**: gitbutler/workspace (`d1933bb`)  
**最后提交**: Claude Code (MiniMax-M3)

---

## 项目概述

多平台视频下载器，支持 X/Twitter, TikTok, 抖音, Bilibili 等。  
**技术栈**: Rust + tokio, Dioxus GUI, yt-dlp, ResolverRegistry

## 快速开始

```bash
cargo run -- "URL"              # CLI
cargo run -- --gui              # GUI (Dioxus)
cargo build --release           # 发行版
```

## 核心模块

| 模块 | 文件 | 说明 |
|------|------|------|
| Platform | `src/platform/` | 平台解析器 (TikTok/DouYin/Bilibili 等) |
| GUI | `src/gui_dioxus.rs` | Dioxus GUI (~1100行) |
| Downloader | `src/downloader.rs` | 分段并行下载 + 断点续传 |
| Resolver | `src/platform/mod.rs` | ResolverRegistry + PlatformResolver trait |

## 解析器选择

| URL 域名 | 解析器 | 说明 |
|----------|--------|------|
| x.com/twitter.com | FxTwitterResolver | API 解析 |
| tiktok.com | TikTokResolver | 需要 msToken |
| douyin.com | DouYinResolver | 需要 msToken |
| bilibili.com | BilibiliResolver | yt-dlp |
| 其他 | YtDlpResolver | yt-dlp fallback |

---

## 关键实现经验

1. **URL 匹配必须精确域名** — `contains("bilibili.com")` 会误匹配 `notbilibili.com`；改用 `host_lower == p || host_lower.ends_with(&format!(".{}", p))`
2. **错误消息必须净化 URL** — 移除 `token, session, msToken, csrf, sig` 等敏感参数
3. **进度更新用阈值过滤** — 进度变化 ≥ 1% 才回调 UI
4. **Cookie 文件权限 0o600** — `#[cfg(unix)]` 下 `std::fs::Permissions::from_mode(0o600)`
5. **分段下载失败不回退** — 回退会重复下载，直接报错

---

## 项目状态

| 检查项 | 状态 |
|--------|------|
| cargo build | ✅ 0 errors |
| cargo test | ✅ 189 passed |
| cargo clippy | ✅ 0 warnings（`--all-targets`: 7 warnings, 4 在 bench stub, 3 在 lib 待修） |

**最近验证**: 2026-07-15 — 7 perf commits merged (含 1 修复), 189 tests pass, clippy clean

## 已知问题

| 问题 | 状态 | 解决 |
|------|------|------|
| 抖音 "Fresh cookies needed" | 已知 | 导出 Netscape 格式 Cookie |
| TikTok 403/HTML | 已知 | 配置 msToken |
| X/Twitter 格式选择 | 平台限制 | X API 不返回完整格式信息 |
| tokio shutdown panic | 已知 | `2>/dev/null` 抑制 stderr |
| clippy --all-targets 7 warnings（4 bench + 3 lib） | 待修 | 见状态表 |

---

## 近期完成

### 2026-07-15 — perf + GUI 改进（14 commits，HEAD `d1933bb`）

| Commit | 类型 | 文件 | 内容 |
|--------|------|------|------|
| `3cbc531` | docs | `HANDOFF.md` | 5 处 claim 标 [待实测] + 修 clippy 状态 |
| `1855660` | perf | `src/queue.rs` | RwLock→Mutex |
| `6fd5dd5` | perf | `src/queue.rs` | 移除 `DownloadResult` 冗余 clone |
| `37b2ec4` | perf | `src/yt_dlp.rs` + 5 caller | sync→async + `spawn_blocking` |
| `27a1acf` | perf | `src/scrapling_client.rs` | `OnceLock` 缓存 Python 路径 |
| `edcd9fe` | test | `Cargo.toml` + `benches/hls_concurrency.rs`（新） | HLS 并发 sweep benchmark |
| `ae32b60` | fix | `src/queue.rs` | 1855660 漏改的 4 处 `.read()` → `.lock()` |
| `c8f9e3d` | docs | `HANDOFF.md` | 精简 405→126 行 |
| `d435dc5` | fix | `src/gui_dioxus/{types,handlers,app}.rs` | 进度条显示 speed + size |
| `c330a76` | feat | `src/{types,downloader/mod,downloader/parallel,gui_dioxus/{types,handlers,app}}.rs` | 单下载取消按钮 |
| `0760b4e` | debug | `src/gui_dioxus/app.rs` | cancel onclick 加 eprintln（待定位） |
| `ea15108` | refactor | `src/gui_dioxus/{types,handlers,app}.rs` | 错误 UI 结构化（ErrorDisplay 替换字符串匹配 + Retry 按钮） |
| `120fd27` | docs | `HANDOFF.md` | 同步 2 个 GUI commit |
| `d1933bb` | feat | `src/gui_dioxus/{types,app}.rs` | 设置面板（4 字段 + rfd 文件选择） |

`37b2ec4` 调用方: `src/resolver_chain.rs`, `src/platform/{bilibili,douyin,mod,tiktok,youtube}.rs`

**剩余**: §6 5 处 perf claim [待实测]（需 `cargo bench`）；batch 取消未实现；cancel button 点击未确认（debug commit 0760b4e 待用户反馈）；设置面板仅 4/14 字段（proxy/user_agent/format/subtitles/audio_only/max_retries/server_addr/device_name/log_level/douyin_cookie_file 仍需手编配置）；未 push

### 2026-07-11 — HLS 下载器重构

- `max_concurrent` 真实生效（Semaphore，3b5a7a0 + 3947209）
- 自适应并发死代码移除 (-106 行, 01b0599)

### 2026-06-14 — Clippy + Slint 清理

- 71 clippy warnings → 0（`#[allow(dead_code)]` 抑制 Dioxus RSX 误报）
- 移除 Slint residue: `src/gui.rs` (74KB), `ui.slint` (21KB), `build.rs`, `src/ui/` (40KB)

### 2026-06-14 — 新增功能

- YouTube 解析器: `src/platform/youtube.rs`
- 历史 CSV 导出: `src/history.rs::export_csv` + `csv = "1.3"` 依赖
- 下载调度字段: `src/types.rs` (`DownloadRequest.schedule_time`)

### Phase 1-2 性能优化（[待实测]）

- URL 匹配: `src/platform/url_matcher.rs`, `src/platform/mod.rs`
- CookiePool 读写锁分离: `src/platform/cookie_pool.rs`

---

## 调试

```bash
RUST_LOG=debug cargo run -- "URL" 2>/dev/null   # 详细日志（抑制 tokio panic）
```

## omp 安装（本地开发模式）

```bash
cd ~/Downloads/oh-my-pi-main && bun install && bun run install:dev
```
API key 配置见 `~/.omp/agent/models.yml`（不在此处粘贴）。