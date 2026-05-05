# 项目交接文档 (Handoff)

**项目**: x_videos-dowloads (Rust x.com 视频下载器)
**生成时间**: 2026-04-27
**最后更新**: 2026-05-01
**当前分支**: main (GitButler 虚拟分支: gitbutler/workspace)
**Git 状态**: 使用 GitButler 管理虚拟分支

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
│    - 绕过 Cloudflare Turnstile 挑战                         │
│    - 返回 HTML，从中提取视频 URL                               │
└─────────────────────────────────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────────────────────────────────┐
│ 4. 直接下载 (video.twimg.com 直链)                          │
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
- **2026-04-30 修复**: 分段失败时自动回退到单线程下载
- **2026-05-01 优化**: 使用 `spawn_blocking` 避免阻塞 async runtime

---

## 已知问题与修复 (2026-05-01)

### 问题 1: 输出文件名包含 %(ext)s 占位符

**症状**: 下载完成后文件名为 `chudai_.%(ext)s` 而非 `chudai.mp4`

**根因**: 直接 URL 使用了 yt-dlp 的模板格式

**修复**: 提取视频格式扩展名生成正确文件名

### 问题 2: yt-dlp 和 FxTwitter 都失败时的后备方案

**解决方案**: 新增 Scrapling 模块作为第三级解析器

### 问题 3: 封IP风险和下载速度优化

**解决方案**: 网站特征学习 + 自适应线程控制

---

## 代码优化 (2026-05-01)

> Phase 1-4 优化已完成，详见 git log。核心问题：异步阻塞、进度条、竞态条件、性能优化。

### Phase 1: 关键异步阻塞修复

| 修复项 | 文件 | 方案 |
|--------|------|------|
| Nested Runtime | gui.rs | 预创建单一 Arc<Runtime> |
| TempDir 阻塞 | downloader.rs | spawn_blocking 包装 |
| Progress 线程 | gui.rs | std::thread::sleep() |

### Phase 2: GUI 进度条 + 竞态修复

| 问题 | 修复 |
|------|------|
| 进度条不更新/跳跃 | download_segment()只更新总量，后台任务统一计算 |
| 进度条从中间扩展 | HorizontalLayout { alignment: start } |
| Slint width/min-width 冲突 | 只用 min_width/max_width |
| 中文字体显示方块 | font-family: "PingFang SC" (单个字体) |
| get_or_create TOCTOU | 双检查锁定模式 |

### Phase 3: 性能优化

| 优化项 | 修复 |
|--------|------|
| fxtwitter.rs 媒体迭代 | 直接迭代，避免 Vec 分配 |
| clippy 修复 | rsplit('/').next() → next_back() 等 |

### Phase 4: 代码质量

- 删除未使用函数: detect_speed_limit(), check_speed_limit(), list_domains(), stats()

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

# clippy 检查
cargo clippy -- -D warnings
```

---

## 测试用 URL

```
https://x.com/HopePJ/status/2048259171158741395
https://x.com/film_korean9/status/2049539100588577014
https://x.com/elvis_mwanza1/status/2048648323280822482
```

**测试结果**:

| URL | 文件大小 | 下载时间 | 平均速度 |
|-----|----------|----------|----------|
| film_korean9 | 386 MB | 39.56s | 10.7 MB/s |
| elvis_mwanza1 | 1.17 GB | 110.34s | 10.8 MB/s |

**结论**: 速度稳定在 ~10.7-10.8 MB/s，瓶颈在 `video.twimg.com` 的上行带宽

---

## 最近提交

```
<PENDING> - refactor: website_profile.rs using ProfileTemplate enum
<PENDING> - test: add unit tests for config.rs, yt_dlp.rs, scrapling_extractor.rs
<PENDING> - fix: clippy errors (too_many_arguments, unnecessary_cast, etc.)
<PENDING> - fix: website_profile.rs AntiCrawlerType derive Default
<PENDING> - test: add unit tests for types.rs (sanitize, errors, progress)
190be3a - GitButler Workspace Commit
b4e7c27 - update commit
9dd6899 - fix: use std::process::Command instead of tokio::Command
e1ece79 - fix: use std::thread instead of tokio::spawn_blocking
15fed43 - fix: run FxTwitter in spawn_blocking with Tokio runtime
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
5. [x] 清理 gui.rs 中的未使用变量警告 - 已完成
6. [x] 文件扩展名修复 - 已从 URL 提取扩展名
7. [x] 懒加载网站配置 - 已实现按需加载
8. [x] **异步阻塞修复** - gui.rs nested runtime + downloader.rs TempDir
9. [x] **竞态条件修复** - get_or_create 双检查锁定
10. [x] **媒体迭代优化** - 消除中间 Vec 分配
11. [x] **clippy 修复** - 4 处代码质量修复
12. [x] **website_profile.rs 重构** - ProfileTemplate 枚举减少样板代码 (861→614行)
13. [x] **测试覆盖率提升** - 新增 40+ 单元测试 (133→174)
14. [ ] 添加更多集成测试
15. [ ] Slint UI warnings (172个 padding/layout warnings)

---

## GitButler

项目使用 [GitButler](https://gitbutler.com) 管理虚拟分支。详见 `but --help` 或 https://gitbutler.com

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

### 给接手的同事

1. 运行 `cargo build --release` 确保编译通过
2. 确保 Python 环境有 scrapling: `pip install scrapling`
3. 测试 URL: `cargo run -- "https://x.com/HopePJ/status/2048259171158741395"`
4. 查看日志了解解析链: `INFO yt-dlp 解析失败...` → `INFO FxTwitter 解析成功!`

### 遗留问题

1. **website_profile.rs 死代码**: 某些方法标记了 `#[allow(dead_code)]`，预留 API 但未使用

---

## GitNexus 代码分析集成 (2026-05-01)

### 概述

GitNexus 是基于知识图谱的代码分析工具，可以为 Claude Code 提供深度的代码结构理解能力。

### 已完成配置

| 项目 | 状态 | 说明 |
|------|------|------|
| **OrbStack** | ✅ 已安装 | Docker 替代品，运行 Linux 容器 |
| **GitNexus CLI** | ✅ 已安装 | `npm link` 到本地 GitNexus 源码 |
| **GitNexus Skills** | ✅ 已安装 | 8 个 skills 在 `~/.claude/skills/` |
| **MCP 配置** | ✅ 已配置 | `~/.claude/mcp.json` 中的 gitnexus 条目 |
| **代码索引** | ✅ 已完成 | 1,292 nodes, 2,827 edges, 38 clusters |

### OrbStack 说明

OrbStack 是 Docker 的轻量替代品：
- 提供与 Docker 兼容的 API（Docker socket 兼容）
- 比 Docker Desktop 启动更快，资源占用更少
- 支持 x86_64 和 ARM64 容器
- 当前 `docker` 命令已通过 OrbStack 执行

### 使用方式

**分析项目（需要 Docker/OrbStack）：**
```bash
docker run --rm -v /Users/hgxszhj/x_videos-dowloads:/repo -w /repo ghcr.io/abhigyanpatwari/gitnexus:latest node /app/gitnexus/dist/cli/index.js analyze
```

**GitNexus Skills（Claude Code 中使用）：**
- `gitnexus-cli` - 分析、状态、清理命令
- `gitnexus-exploring` - 代码探索和查询
- `gitnexus-debugging` - 调试相关分析
- `gitnexus-refactoring` - 重构相关分析
- `gitnexus-impact-analysis` - 影响范围分析
- `gitnexus-pr-review` - PR 审查
- `gitnexus-guide` - 使用指南
- `gitnexus-analyzer` - 知识图谱分析

**触发条件：** 在 Claude Code 中提到"分析代码结构"、"理解项目依赖"、"代码关系图"时使用。

### 索引结果

```
Repository indexed successfully (26.5s)
1,292 nodes | 2,827 edges | 38 clusters | 113 flows
```

生成的索引存储在 `.gitnexus/` 目录中。

---

## 详细模块参考

> 详见各模块源码：src/downloader.rs, src/website_profile.rs, src/fxtwitter.rs, src/gui.rs 等

---

### 下一步工作

1. [已完成] website_profile.rs 重构 - ProfileTemplate 枚举 (861→614行)
2. [已完成] 测试覆盖率提升 - 新增 40+ 单元测试 (133→174)
3. [ ] 添加更多集成测试
4. [ ] Slint UI warnings (172个 padding/layout warnings)
