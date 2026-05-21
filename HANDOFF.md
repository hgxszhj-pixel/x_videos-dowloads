# 项目交接文档 (Handoff)

**项目**: x_videos-dowloads (Rust x.com 视频下载器)
**更新时间**: 2026-05-21
**当前分支**: gitbutler/workspace

---

## 项目概述

基于 Rust 的多平台视频下载器，支持 x.com (Twitter), TikTok, 抖音, Bilibili。

**技术栈:**
- Rust + tokio
- Slint v1.15.1 GUI
- yt-dlp 视频下载 (主要)
- ResolverRegistry 平台选择
- X-Bogus / A-Bogus 签名 (SM3)
- FxTwitter API (Twitter fallback)

---

## 核心功能

### 多平台解析器 (src/resolvers/)

| 解析器 | 文件 | 支持 |
|--------|------|------|
| TikTokResolver | tiktok.rs | tiktok.com, vm.tiktok.com |
| DouYinResolver | douyin.rs | douyin.com, v.douyin.com |
| BilibiliResolver | bilibili.rs | bilibili.com, b23.tv |
| ZhiXueYunResolver | zhixueyun.rs | zhixueyun.com (PDF 下载) |
| CookiePool | cookie_pool.rs | Cookie 池管理 |
| YtDlpResolver | mod.rs | 所有平台 (fallback) |

**签名模块:**
| 模块 | 文件 | 算法 |
|------|------|------|
| X-Bogus | xbogus.rs | MD5 + RC4 + 自定义 Base64 |
| A-Bogus | abogus.rs | SM3 + RC4 + 自定义 Base64 (抖音) |

**API 客户端:**
| 客户端 | 文件 | 用途 |
|--------|------|------|
| DouYinApiClient | douyin_api.rs | 抖音 API 直接调用 |

---

## 三级视频解析链

1. **ResolverRegistry** → 选择对应平台解析器
2. **yt-dlp** (首选) - 支持 cookie 传递
3. **FxTwitter API** → api.fxtwitter.com (Twitter fallback)

---

## GUI 进度条要点

**关键规则**（详见 CLAUDE.md）:
- `download_segment()` 只更新 `total_downloaded`，不发回调
- 存储格式：`progress_arc.store((p.percent * 100.0) as u32)`
- UI 更新：`progress_arc.load() as f32 / 100.0`
- 终止条件：`while progress_arc_for_update.load(...) < 100`
- 进度条使用 `HorizontalLayout { alignment: start }`

---

## 抖音 Cookie 问题

**问题描述:**
- yt-dlp 和 lux 都报错 "Fresh cookies (not necessarily logged in) are needed"
- DouYin API 返回 HTTP 200 但 body 为空

**解决方案:**
1. 退出浏览器中的抖音登录
2. 清除浏览器中所有 douyin.com cookies
3. 重新登录抖音账号
4. 立即导出新的 Netscape 格式 cookies
5. 保存到 `~/.cookies/douyin_cookies.txt`

**配置文件:** `~/.config/x-video-downloader/config.toml`

---

## 待办 (优先级排序)

### P0 - 关键问题

1. **进度条不更新** - yt-dlp 在非 TTY 模式下进度不输出
2. **中文字体显示** - Slint 的 font-family 属性在 Window 上不支持

### P1 - 重要功能

3. **clippy 检查修复** - 运行 `cargo clippy` 查看警告

---

## 已知问题根因

### 进度条不更新

yt-dlp 检测是否连接到 TTY 来决定是否输出进度。当作为子进程被捕获时:
- stderr 被 `BufReader` 读取，但 yt-dlp 可能输出到 `stdout` 或根本不输出

### 中文字体显示

Slint 的 `Window` 组件不支持 `font-family` 属性。需要逐个 Text 元素添加:
```slint
Text {
    font-family: "PingFang SC";
    ...
}
```

---

## 调试技巧

- GUI 启动时会打印 `DEBUG: GUI started, log file: ...` 日志
- `eprintln!("DEBUG yt-dlp stderr: {}", line)` 会输出 yt-dlp 的 stderr
- `parse_ytdlp_progress()` 函数解析进度，格式: `[download] 45.0% of ~2.50MB`
- Slint 回调中使用 `invoke_from_event_loop` 线程安全更新 UI
- 设置 `RUST_LOG=debug` 查看详细日志