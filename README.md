# X Video Downloader

一个高性能 Rust 视频下载器，支持 CLI 与 Dioxus GUI 两种模式，覆盖 X.com / Twitter / TikTok / 抖音 / Bilibili / 图灵(TulingPython) 等平台。

```
license: MIT
edition: Rust 2021
binary: x-video-downloader
```

## 特性

- **多平台解析**: X.com / Twitter / TikTok / 抖音 / Bilibili / 图灵（智学云需 feature 启用）
- **三级解析链**: yt-dlp → FxTwitter API → Scrapling（Cloudflare 绕过）
- **智能线程**: 根据网站特征与下载速度自适应调整并发
- **HLS/m3u8 支持**: 加密流（AES-128-CBC）、分段并发下载
- **Cookie 鉴权**: 课程类站点（图灵 / 智学云）通过 Chromium / ego-lite 解密 cookie 注入
- **历史 + 统计**: 下载历史、最近 7 天统计、磁盘分析
- **多主题 GUI**: 紫 / 海洋 / 日落 / 森林 + 结构化错误提示 + 智能下载按钮

## 快速开始

### 编译

```bash
cargo build --release
# 产物: target/release/x-video-downloader
```

### 运行

```bash
# CLI 单 URL
./target/release/x-video-downloader "<url>"

# GUI 模式
./target/release/x-video-downloader --gui

# 协作服务器（WIP: 团队模式）
./target/release/x-video-downloader --start-server
```

### 平台示例

| 平台 | URL 示例 | 说明 |
|------|---------|------|
| X / Twitter | `https://x.com/user/status/123` | yt-dlp 直接拉取 |
| TikTok | `https://www.tiktok.com/@user/video/123` | yt-dlp |
| 抖音 | `https://www.douyin.com/video/123` | X-Bogus 签名 |
| Bilibili | `https://www.bilibili.com/video/BV...` | yt-dlp |
| 图灵(Tuling) | `https://vip.tulingpyton.cn/p_t_pc/...` | 需扫码登录，CDP 注入 cookie |

## 配置文件

默认位置由 `directories` crate 解析，macOS 通常是 `~/Library/Application Support/x-video-downloader/config.toml`：

```toml
output_dir = "~/Downloads/x-videos"
cookie_file = "~/ego-lite-profile/Cookies"
douyin_ms_token = ""       # 抖音签名 msToken
concurrent_downloads = 4   # 批量下载并发数
```

GUI 内的 ⚙️ 设置面板可直接读写这些字段。

## 子命令 / 选项

```
Usage: x-video-downloader [OPTIONS] [URL]

Options:
  --gui                    启动 GUI 模式
  --output <DIR>           输出目录（覆盖配置）
  --cookie <FILE>          Cookie 文件路径
  --concurrent <N>         并发下载数
  --format <ID>            强制指定视频格式 ID
  --json                   输出 JSON（分析 / 状态等子命令）
  --analyze <DIR>          分析下载目录（按扩展名 / 年龄分桶 / Top-10）
  --clean <TARGET>         清理下载历史（parse-only 校验目标）
  --status                 显示系统状态（最近 7 天统计 + 配置 + 历史）
  --installer <ACTION>     安装器子命令（self-update / install-path）
  --serve                  启动协作服务器（WIP）
  --port <PORT>            协作服务器端口（默认 7878）
  --no-default-features    裁剪 zhixueyun feature
```

示例：

```bash
# 分析下载目录（含 Top-10 大文件）
x-video-downloader --analyze ~/Downloads/x-videos --json

# 查看最近 7 天下载统计
x-video-downloader --status

# 清理已下载条目（dry-run 校验）
x-video-downloader --clean all
```

## GUI 功能

- **URL 输入区**：粘贴 URL → 智能下载（单 URL → start_download_impl；多 URL → 自动批量）
- **课程池面板**：粘贴课程 URL → 分析 → 勾选课节 → 批量下载（需扫码登录）
- **进度面板**：实时进度条 + 速度 + ETA + 大小
- **历史面板**：最近下载条目，含重下 / 打开目录
- **错误面板**：结构化错误（类型 / 建议 / 重试）
- **主题切换**：右上角 4 个色环，浏览器原生 tooltip 提示
- **设置面板**：输出目录 / Cookie 文件 / 并发数 / 抖音 msToken

## 项目结构

```
src/
├── main.rs                       # CLI 入口（参数解析 + 分发）
├── commands/                     # 子命令模块
│   ├── analyze.rs                # 下载目录分析
│   ├── clean.rs                  # 历史清理
│   ├── history_stats.rs          # 7 天统计
│   ├── installer.rs              # self-update / 安装路径
│   ├── status.rs                 # 系统状态
│   └── path.rs                   # XDG 路径解析
├── gui_dioxus/                   # Dioxus 0.7 GUI（替代旧 Slint）
│   ├── app.rs                    # AppComponent + 智能下载按钮
│   ├── components.rs             # WhiteCard / ThemeSwatch / ErrorPanel / UrlInput
│   ├── theme.rs                  # 4 主题 + 按钮 helper（btn_success / btn_ghost_destructive）
│   ├── handlers.rs               # 下载实现（单 / 批量 / 课程池）
│   ├── poller.rs                 # 进度轮询 coroutine
│   ├── store.rs                  # GUI 状态 store
│   └── app_*.rs                  # 拆分后的面板（settings / video_card / batch / history / course_pool）
├── hls/                          # HLS/m3u8 下载（解析 + 加密流 + 并发）
├── collaboration/                # 团队模式（client / server / discovery）
├── platform/                     # 平台适配（zhixueyun_token / 图灵 cookie 注入）
├── downloader/                   # 4 线程分段并行下载
├── core/                         # 配置 / 类型 / 通用工具
└── utils/                        # 日志 / 通知 / 路径

scripts/                          # mutation_smoke.sh 等工具
tests/                            # 集成测试
benches/                          # criterion 基准（hls_concurrency / hls_parse）
docs/agents/                      # 项目级 Agent 任务（issue-tracker / triage-labels）
```

## 视频解析链

```
URL 输入
  ├─ yt-dlp (首选, 快速, 多格式)
  │     └─ 失败 ↓
  ├─ FxTwitter API (api.fxtwitter.com, X.com 兜底)
  │     └─ 失败 ↓
  └─ Scrapling (Python 浏览器自动化, Cloudflare 绕过)
        └─ 失败 → 错误面板显示重试建议
```

## 构建与开发

```bash
# Debug 构建
cargo build

# 单元测试
cargo test

# 严格 clippy
cargo clippy --all-targets -- -D warnings

# 格式化
cargo fmt

# 变异测试（防 AI 假测试）
bash scripts/mutation_smoke.sh smoke

# Release 构建（opt-level=3 + LTO + strip）
cargo build --release
```

## 依赖

### 运行时

- **yt-dlp** — 视频解析主路径
- **ffmpeg** — 合并分段视频（stream copy 模式，快）
- **ego-lite**（可选）— 小鹅通 / 图灵扫码登录窗口
- **Chrome / Chromium**（可选）— CDP 拦截 m3u8 / cookie 注入

### 编译时

- Rust 1.80+（edition 2021）
- Python 3.10+（Scrapling 解析器子进程）
- platform: macOS / Linux / Windows

## Feature Flags

| Feature | 默认 | 说明 |
|---------|------|------|
| `zhixueyun` | ✅ | 智学云（kc.zhixueyun.com）课程解析器 |
| `collaboration` | ❌ | 协作服务器/团队模式（WIP） |

```bash
# 最小构建（裁剪 zhixueyun）
cargo build --release --no-default-features

# 启用协作模式
cargo build --release --features collaboration
```

## 路线图

- [ ] 协作模式完成（--start-server / --create-team / --join-team）
- [ ] GUI 主题持久化跨会话
- [ ] 分布式下载（多 peer 拼速度）
- [ ] Web 管理界面（嵌入 Dioxus web feature）

## 许可

MIT License — 详见 [Cargo.toml](Cargo.toml) `license` 字段。

## 致谢

- [yt-dlp](https://github.com/yt-dlp/yt-dlp) — 视频解析主力
- [Dioxus](https://dioxuslabs.com/) — Rust GUI 框架（替代 Slint）
- [FxTwitter](https://github.com/FixTweet/FxTwitter) — X.com 解析兜底
- [ego-lite](https://github.com/citrolabs/ego-lite) — 扫码登录浏览器

---

> 内部开发协作见 [CLAUDE.md](CLAUDE.md)；Agent 任务分配见 [docs/agents/](docs/agents/)。