# 项目交接文档 (Handoff)

**项目**: x_videos-dowloads (Rust x.com 视频下载器)
**生成时间**: 2026-04-27
**当前分支**: main
**Git 状态**: 有未提交更改

---

## 项目概述

基于 Rust 的 x.com 视频下载器，具有三级视频源解析、协作下载和网站特征学习功能。

**核心技术栈:**
- Rust (tokio, reqwest)
- yt-dlp 视频下载
- **Slint v1.15.1** GUI 框架
- Scrapling (Python) - Cloudflare 绕过
- WebSocket 协作通信
- SQLite (rusqlite) 数据持久化
- rfd 文件对话框
- **网站特征学习** - 自适应线程+防爬机制感知

---

## 项目架构

```
┌─────────────────────────────────────────────────────────────────┐
│                         CLI/GUI 入口                            │
│                    main.rs / gui.rs                              │
└─────────────────────┬───────────────────────────────────────────┘
                      │
┌─────────────────────▼───────────────────────────────────────────┐
│                     核心模块 (lib.rs)                            │
│  config.rs │ types.rs │ history.rs │ downloader.rs │ yt_dlp.rs  │
│  fxtwitter.rs │ scrapling_client.rs │ scrapling_extractor.rs   │
│  website_profile.rs │
└─────────────────────┬───────────────────────────────────────────┘
                      │
        ┌──────────────┴──────────────┐
        │                             │
┌───────▼──────────┐      ┌──────────▼──────────┐
│   直接下载器      │      │   yt-dlp 下载器     │
│  downloader.rs   │      │    yt_dlp.rs        │
│  (4线程分段并行) │      └─────────────────────┘
└──────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│                    三级视频解析链                                 │
├─────────────────────────────────────────────────────────────────┤
│  1. yt-dlp (首选) → 快速解析                                    │
│  2. FxTwitter API → api.fxtwitter.com                         │
│  3. Scrapling → Python 浏览器自动化绕过 Cloudflare              │
└─────────────────────────────────────────────────────────────────┘
```

---

## 模块职责

| 模块 | 文件 | 职责 |
|------|------|------|
| **CLI** | main.rs | 命令行参数解析，下载流程控制 |
| **配置** | config.rs | 配置文件加载/保存 (~/.config/) |
| **类型** | types.rs | DownloadRequest, VideoInfo, VideoFormat |
| **历史** | history.rs | 下载历史和书签 (JSON 文件) |
| **下载器** | downloader.rs | 直接 HTTP 下载，4线程分段并行 |
| **yt-dlp** | yt_dlp.rs | yt-dlp 集成，视频信息解析 |
| **FxTwitter** | fxtwitter.rs | FxTwitter API 解析 tweet URL |
| **Scrapling Client** | scrapling_client.rs | Python subprocess 调用 scrapling 绕过 Cloudflare |
| **Scrapling Extractor** | scrapling_extractor.rs | 从 HTML 提取视频 URL |
| **网站特征** | website_profile.rs | 网站特征学习，自适应线程，防爬机制感知 |
| **GUI** | gui.rs | Slint 框架图形界面 |
| **协作** | collaboration/ | 分布式下载协作 |

---

## 核心工作流程

### 视频下载流程 (三级解析链)

```
用户输入 URL (x.com/twitter.com 视频链接)
         │
         ▼
┌─────────────────────────────────────────────────────────────┐
│ 1. yt-dlp 尝试解析并下载                                      │
│    - 优先使用 yt-dlp 获取视频信息                             │
│    - 失败时启用 FxTwitter 作为 fallback                      │
└─────────────────────────────────────────────────────────────┘
         │
         ▼ (yt-dlp 失败时)
┌─────────────────────────────────────────────────────────────┐
│ 2. FxTwitter API 解析                                        │
│    - 调用 https://api.fxtwitter.com/status/{tweet_id}       │
│    - 从响应中提取真实视频 URL                                  │
└─────────────────────────────────────────────────────────────┘
         │
         ▼ (FxTwitter 失败时)
┌─────────────────────────────────────────────────────────────┐
│ 3. Scrapling (Cloudflare 绕过)                                │
│    - 使用 Python scrapling StealthyFetcher                   │
│    - 绕过 Cloudflare Turnstile 挑战                          │
│    - 返回 HTML，从中提取视频 URL                              │
└─────────────────────────────────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────────────────────────────────┐
│ 4. 直接下载 (video.twimg.com 直链)                           │
│    - 使用 downloader.rs 的 VideoDownloader                   │
│    - 4线程分段并行下载                                       │
│    - 输出文件名为 {title}.{ext} (非 %(ext)s)               │
└─────────────────────────────────────────────────────────────┘
```

---

## 新增模块详解

### src/scrapling_client.rs - Scrapling Client

通过 Python subprocess 调用 [scrapling](https://github.com/D4Vinci/Scrapling) 框架：

```rust
pub struct ScraplingClient {
    pub async fn fetch(&self, url: &str) -> Result<ScraplingResponse>
    pub async fn get_video_info(&self, url: &str) -> Result<VideoInfo>
}

pub fn needs_cloudflare_bypass(url: &str) -> bool
```

**Python 依赖** (需提前安装):
```bash
pip install scrapling curl_cffi patchright browserforge
```

### src/scrapling_extractor.rs - Video URL Extractor

从 HTML 中提取视频 URL：

```rust
pub fn extract_video_url(html: &str) -> Option<String>
pub fn extract_title(html: &str) -> Option<String>
pub fn extract_all_video_sources(html: &str) -> Vec<VideoFormat>
```

**提取策略**:
1. 解析 `window.__config` JSON
2. 提取 `<meta property="og:video">` 标签
3. 直接搜索 `video.twimg.com` URL

---

## 关键文件说明

### src/fxtwitter.rs - FxTwitter API 解析器

**重要**: 这个模块负责解析 tweet URL，返回真实视频地址。

```rust
// API 调用
let api_url = format!("https://api.fxtwitter.com/status/{}", tweet_id);
let response = client.get(&api_url)
    .header("Accept", "application/json")
    .send().await;
```

**数据结构** (从 API 响应提取):
- `tweet.text` - 推文文本 (用作标题)
- `tweet.author.name` - 作者名称
- `tweet.media.all[].variants` - 视频 variant 列表
- `tweet.media.all[].formats` - 视频格式列表

### src/downloader.rs - 直接下载器

**分段并行下载**:
- 默认 4 线程并行
- 使用 `tempfile::TempDir` 自动清理临时文件
- 流式合并避免内存翻倍

---

## 已知问题与修复 (2026-04-27)

### 问题 1: 输出文件名包含 %(ext)s 占位符

**症状**: 下载完成后文件名为 `chudai_.%(ext)s` 而非 `chudai.mp4`

**根因**: 直接 URL 使用了 yt-dlp 的模板格式

**修复**: 提取视频格式扩展名生成正确文件名

```rust
// 修复前
let output_path = output_dir.join(format!("{}.%(ext)s", filename));

// 修复后
let ext = info.formats.first()
    .map(|f| f.ext.clone())
    .unwrap_or_else(|| "mp4".to_string());
let final_output_path = output_dir.join(format!("{}.{}", filename, ext));
```

### 问题 2: yt-dlp 和 FxTwitter 都失败时的后备方案

**解决方案**: 新增 Scrapling 模块作为第三级解析器

当 FxTwitter 失败时：
1. 初始化 `ScraplingClient`
2. 调用 `StealthyFetcher.fetch(url, headless=True, solve_cloudflare=True)`
3. 从返回的 HTML 中提取视频 URL
4. 使用 Rust VideoDownloader 下载

### 问题 3: 封IP风险和下载速度优化

**解决方案**: 网站特征学习 + 自适应线程控制

网站特征表 (30+ 网站):
```
video.twimg.com: Cloudflare, 2-4线程, UA轮换
github.com: RateLimit, 2-6线程
youtube.com: RateLimit, 2-8线程
reddit.com: Cloudflare+CookieAuth, 2-4线程
```

自适应逻辑:
- 速度 < 500KB/s → 增加线程 (+1)
- 速度 > 1.5MB/s → 减少线程 (-1)
- 失败3次+ → 降低max_threads

配置文件: `~/.config/x-video-downloader/website_profiles.json`

---

## 构建和测试

```bash
# 构建
cargo build

# 发布构建
cargo build --release

# 运行测试
cargo test

# 运行 CLI
cargo run -- "https://x.com/user/status/123456"

# 运行 GUI
./target/release/x-video-downloader --gui
```

---

## 测试用 URL

```
https://x.com/HopePJ/status/2048259171158741395?s=20
```

**测试结果**:
- yt-dlp → ❌ (需要认证)
- FxTwitter → ✅ (成功获取 863MB 视频)
- 输出文件: `chudai.mp4`

---

## 最近提交

```
fe1b47e - docs: 更新交接文档 - 新增题型支持扩展章节
0880c31 - chore: 清理 slides 目录，移除 node_modules 依赖
464eeeb - feat: 完善AI批处理模块 + 新增测试
7eedb7b - docs: 添加项目交接文档
4df59c1 - fix: 修复中文文本截断的字节边界panic
```

---

## Python 环境要求

Scrapling 模块需要 Python 环境和以下依赖:

```bash
# 安装 scrapling
pip install scrapling

# 安装依赖库
pip install curl_cffi patchright browserforge

# 安装 Playwright 浏览器
python -m playwright install chromium
```

**Python 路径检测顺序**:
1. `~/.pyenv/shims/python3`
2. `~/anaconda3/bin/python3`
3. `python3` (PATH)

---

## 待办事项

1. [x] Scrapling 模块集成 - 已完成
2. [x] 输出文件名修复 - 已完成
3. [x] 网站特征学习系统 - 已完成
4. [x] 自适应线程控制 - 已完成
5. [ ] 清理 gui.rs 中的未使用变量警告
6. [ ] 添加更多测试用例

---

## 技术参考

- [FxTwitter API](https://api.fxtwitter.com/) - 第三方 Twitter API
- [yt-dlp](https://github.com/yt-dlp/yt-dlp) - 视频下载后端
- [Scrapling](https://github.com/D4Vinci/Scrapling) - Cloudflare 绕过框架
- [Slint](https://slint.dev/) - Rust GUI 库 v1.15.1
- [rfd](https://github.com/nalexandrous/rfd) - 原生文件对话框
- [reqwest](https://docs.rs/reqwest/) - HTTP 客户端 (rustls-tls feature)

---

## 交接清单

### 代码变更 (2026-04-27)
- [x] 新增 scrapling_client.rs - Scrapling Python subprocess 封装
- [x] 新增 scrapling_extractor.rs - HTML 视频 URL 提取
- [x] 新增 website_profile.rs - 网站特征学习系统（30+网站默认配置）
- [x] 修改 downloader.rs - 自适应线程控制
- [x] 修改 main.rs - 添加 Scrapling 解析链
- [x] 修改 main.rs - 修复输出文件名 %(ext)s 问题
- [x] Cargo.toml - 添加 `which = "6"` 依赖

### 测试验证
- [x] cargo build - 构建成功
- [x] cargo test - 133 tests passed
- [x] 真实 URL 测试 - https://x.com/HopePJ/status/2048259171158741395 成功下载 863MB

### 给接手的同事
1. 运行 `cargo build --release` 确保编译通过
2. 确保 Python 环境有 scrapling: `pip install scrapling`
3. 测试 URL: `cargo run -- "https://x.com/HopePJ/status/2048259171158741395"`
4. 查看日志了解解析链: `INFO yt-dlp 解析失败...` → `INFO FxTwitter 解析成功!`