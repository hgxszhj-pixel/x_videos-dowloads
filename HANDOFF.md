# 项目交接文档 (Handoff)

**项目**: x_videos-dowloads (Rust x.com 视频下载器)
**更新时间**: 2026-06-02
**当前分支**: gitbutler/workspace
**Git**: `81b83e8`

---

## 项目概述

多平台视频下载器，支持 x.com, TikTok, 抖音, Bilibili, 智学云等。

**技术栈:** Rust + tokio, Slint GUI, yt-dlp, ResolverRegistry

---

## 核心模块

| 解析器 | 文件 | 支持平台 |
|--------|------|----------|
| TikTok/DouYin/Bilibili | tiktok/douyin/bilibili.rs | 各平台 |
| ZhiXueYunResolver | zhixueyun.rs | zhixueyun.com (PDF) |
| YtDlpResolver | mod.rs | 所有平台 (fallback) |
| FxTwitterResolver | fxtwitter.rs | x.com 视频解析 |
| CookiePool | cookie_pool.rs | Cookie 池管理 |

---

## 进度条实时更新修复 (2026-06-02)

### 问题
批量下载时进度条没有实时显示。

### 修复内容

1. **Direct URL 下载进度回调** (gui.rs:626)
   - 之前传入 `None`，导致进度无法更新

2. **yt-dlp 异步进度解析** (gui.rs:660-725)
   - 使用 tokio async 进程捕获 stderr
   - 解析 `[download] 45.0% of ~2.50MB at 2.50MiB/s` 格式

3. **UI 更新轮询线程** (gui.rs:551-600)
   - 每 200ms 检查 progress_arc 并更新 UI
   - 更新 `batch-progress` (float 0-100) 和 `download-speed`

4. **x.com 视频检测** (fxtwitter.rs:262-267)
   - media_type 为 undefined 但有 variants 的视频也能检测

---

## GUI 进度条规则

### 进度存储格式
- 存储: `progress_arc.store((percent * 100.0) as u32)` (范围 0-10000)
- UI 显示: `progress / 100.0` 转换为百分比

### 进度条终止条件
- `while progress_arc.load() < 10000`

### 布局规则
- `HorizontalLayout { alignment: start }`

---

## 批量下载功能

- URL 输入框支持空格或逗号分隔多个 URL
- Fetch Info 逐个解析 URL 并加入下载池
- Download 按钮从下载池批量下载所有视频
- 并行下载使用 `future::join_all`

---

## 项目质量

| 检查项 | 状态 |
|--------|------|
| cargo build | ✅ 0 errors |
| cargo test | ✅ 307 通过 (升级后 +14) |
| clippy | ✅ |

---

## 性能升级 (v0.2.0)

### Phase 1: HTTP 客户端 ✅
- gzip/brotli 压缩启用
- 连接池 32→64
- 添加 http2 feature

### Phase 2: 解析器优化 ✅
- 移除 `url.to_lowercase()` 冗余分配
- 简化 `select_resolver` 逻辑

### Phase 3: 下载器优化 ⚠️
- `merge_segments` 使用 `std::fs` 阻塞 I/O
- 待 v0.2.0 使用 `tokio::fs` + `spawn_blocking` 重构

### Phase 4: 测试补全 ✅
- 新增 8 个下载器测试
- 测试覆盖: 307 通过

---

## 已知问题

### GUI 卡顿
- **原因**: Slint 在 macOS 上渲染性能限制
- **状态**: 已知限制，暂未优化

### 进度条渲染
- Slint ProgressIndicator 可能不会实时刷新
- DEBUG 输出证明数据在更新，但 UI 渲染有延迟

---

## 抖音 Cookie 问题

yt-dlp 报错 "Fresh cookies needed": 退出登录 → 清除 cookies → 重新登录 → 导出 Netscape 格式

配置: `~/.config/x-video-downloader/config.toml`

---

## 下载命令

```bash
cargo run -- "URL" --direct --proxy "http://127.0.0.1:7890"  # CLI
cargo run -- --gui                                               # GUI
```

---

## 调试

- yt-dlp stderr: `[download] 45.0% of ~2.50MB`
- 详细日志: `RUST_LOG=debug`
