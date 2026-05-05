# 项目交接文档 (Handoff)

**项目**: x_videos-dowloads (Rust x.com 视频下载器)
**更新时间**: 2026-05-05
**当前分支**: gitbutler/workspace

---

## 项目概述

基于 Rust 的 x.com 视频下载器，具有三级视频源解析和 GUI 界面。

**技术栈:**
- Rust + tokio
- Slint v1.15.1 GUI
- yt-dlp 视频下载
- FxTwitter API
- Scrapling (Cloudflare 绕过)

---

## 核心功能

### GUI (gui.rs + ui.slint)

- 网络状态指示器 (Online/Checking/Offline)
- Fetch Info / Download / Save / Clear 按钮
- 实时下载进度条
- 文件大小人性化显示

### 三级视频解析链

1. **yt-dlp** (首选)
2. **FxTwitter API** → api.fxtwitter.com
3. **Scrapling** → Python 浏览器自动化

### 直接下载器 (downloader.rs)

- 4-8 线程分段并行
- 自适应线程调整
- 网站特征学习

---

## 构建

```bash
cargo build --release
./target/release/x-video-downloader --gui
```

---

## 最近提交

```
0a4a4e7 - feat: GUI improvements and network status indicator
4e8fa4f - fix: use map_while for yt-dlp progress parsing
```

---

## 待办

- [ ] clippy 检查修复 (已有错误需清理)
- [ ] Slint UI binding loop warnings
- [ ] 更多集成测试

---

## 依赖

- yt-dlp: `brew install yt-dlp`
- Python 3: 用于 Scrapling 模块
