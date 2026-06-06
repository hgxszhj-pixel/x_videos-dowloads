# 项目交接文档 (Handoff)

**项目**: x_videos-dowloads (Rust x.com 视频下载器)
**更新时间**: 2026-06-06
**当前分支**: gitbutler/workspace
**Git**: `8498ee4` (性能升级 Phase 1-4 完成)

---

## 项目概述

多平台视频下载器，支持 x.com, TikTok, 抖音, Bilibili, 智学云等。

**技术栈:** Rust + tokio, Slint GUI, yt-dlp, ResolverRegistry

---

## 核心模块

| 模块 | 文件 | 说明 |
|------|------|------|
| **Platform** | `src/platform/` | 平台解析器 (原 resolvers/) |
| TikTok/DouYin/Bilibili | platform/tiktok/douyin/bilibili.rs | 各平台 |
| ZhiXueYunResolver | platform/zhixueyun.rs | zhixueyun.com (PDF) |
| YtDlpResolver | platform/mod.rs | 所有平台 (fallback) |
| CookiePool | platform/cookie_pool.rs | Cookie 池管理 |
| **Core 模块** | `src/core/` | 下载/混流/字幕/弹幕/队列核心功能 |
| **Queue** | `src/queue.rs` | 下载队列状态管理 (暂停/恢复/重试) |

---

## src/core/ 模块架构

```
src/core/
├── mod.rs          # 统一导出: downloader, muxer, subtitle, danmaku, queue
├── downloader.rs   # 视频下载引擎 (保留在根目录)
├── muxer.rs        # FFmpeg 音视频混流
├── subtitle.rs     # 字幕下载和格式转换
├── danmaku.rs      # Bilibili 弹幕下载和 ASS 转换
└── queue.rs        # 下载队列管理 (状态机, 并行控制)
```

## src/platform/ 模块架构

```
src/platform/
├── mod.rs          # PlatformResolver trait + ResolverRegistry
├── tiktok.rs       # TikTok 解析器
├── douyin.rs       # 抖音解析器
├── bilibili.rs     # Bilibili 解析器
├── douyin_api.rs   # 抖音 API 客户端
├── zhixueyun.rs    # 智学云解析器
├── cookie_pool.rs  # Cookie 池管理
└── url_matcher.rs  # URL 平台匹配
```

**导出方式**:
```rust
use crate::core::{VideoDownloader, mux, Subtitle, SubtitleFormat, ...};
```

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
| cargo test | ✅ 307 通过 (149 lib + 158 binary) |
| clippy | ✅ 0 errors (dead code warnings for muxer/subtitle) |

---

## 性能升级 (v0.2.0)

### Phase 1: HTTP 客户端 ✅
- gzip/brotli 压缩启用
- 连接池 32→64
- 添加 http2 feature

### Phase 2: 解析器优化 ✅
- 移除 `url.to_lowercase()` 冗余分配
- 简化 `select_resolver` 逻辑

### Phase 3: 下载器优化 ✅
- `merge_segments` 使用 `spawn_blocking` 异步化
- 避免阻塞 async runtime

### Phase 4: 测试补全 ✅
- 新增 8 个下载器测试
- 测试覆盖: 307 通过

---

## Phase 2 功能增强 (upgrade-plan-v1.md)

### 2.1 字幕支持 ✅
- `src/subtitle.rs` - 完整实现
- SubtitleFormat (SRT, VTT, ASS)
- vtt_to_srt 转换
- download_subtitle, save_subtitle 函数

### 2.2 弹幕支持 ❌
- **未实现** — 待开发
- 需要: `src/danmaku.rs` (新建)
- API 参考: `https://api.bilibili.com/x/v1/dm/list?pid={bvid}`

### 2.3 FFmpeg 混流 ✅
- `src/muxer.rs` - 完整实现
- check_ffmpeg, get_media_info, mux, mux_with_quality
- MuxerConfig, MediaInfo, MediaStream structs

---

## 已知问题

### GUI 卡顿
- **原因**: Slint 在 macOS 上渲染性能限制
- **状态**: 已知限制，暂未优化

### 进度条渲染
- Slint ProgressIndicator 可能不会实时刷新
- DEBUG 输出证明数据在更新，但 UI 渲染有延迟

### 抖音 Cookie 问题
- yt-dlp 报错 "Fresh cookies needed": 退出登录 → 清除 cookies → 重新登录 → 导出 Netscape 格式

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

---

## Phase 1.1: Platform Trait 重构 ✅
- `src/platform/mod.rs` — PlatformResolver trait (lines 67-103)
- `src/platform/tiktok.rs`, `douyin.rs`, `bilibili.rs` 等实现
- 重命名 resolvers/ → platform/ (10 files updated)
- trait 接口: platform(), supports_url(), get_video_info(), get_subtitles(), get_danmaku()

## Phase 1.2: 核心层分离 ✅
- `src/core/mod.rs` — 统一导出 downloader, muxer, subtitle, danmaku

## 未完成工作 (upgrade-plan-v1.md)

| Phase | 任务 | 状态 |
|-------|------|------|
| 1.1 | Platform Trait 重构 | ✅ 完成 |
| 1.2 | 核心层分离 | ✅ 完成 |
| 2.1 | 字幕支持 | ✅ 完成 |
| 2.2 | 弹幕支持 | ✅ 完成 |
| 2.3 | FFmpeg 混流 | ✅ 完成 |
| 3 | 认证系统增强 | ❌ 未开始 |
| 4 | 批量下载增强 | ❌ 未开始 |
| 5 | 用户体验优化 | ❌ 未开始 |