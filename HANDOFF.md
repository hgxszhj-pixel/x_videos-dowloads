# 项目交接文档 (Handoff)

**项目**: x_videos-dowloads (Rust x.com 视频下载器)
**更新时间**: 2026-05-10
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

**平台解析器:**
| 解析器 | 文件 | 支持 |
|--------|------|------|
| TikTokResolver | tiktok.rs | tiktok.com, vm.tiktok.com |
| DouYinResolver | douyin.rs | douyin.com, v.douyin.com |
| BilibiliResolver | bilibili.rs | bilibili.com, b23.tv |
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

**Token 管理:**
| 管理器 | 文件 | 功能 |
|--------|------|------|
| TokenManager | token_manager.rs | msToken/ttwid/webid 生成 |

### GUI (gui.rs + ui.slint)

**已完成:**
- 网络状态指示器 (Online/Checking/Offline)
- ResolverRegistry 自动平台选择
- DouYinResolver msToken 自动注入 (从 config 或环境变量 DOUYIN_MS_TOKEN)
- Fetch Info / Download / Save / Clear 按钮
- 实时下载进度条 (100ms 更新间隔，1% 变化阈值)
- 文件大小人性化显示
- 动态下载线程数显示 (根据速度自动调整 4-8 线程)
- Cookie 文件传递给了 yt-dlp

**已知问题:**
- [ ] 中文字体显示为小方块 - Slint 的 font-family 属性在 Window 上不支持，需要逐个 Text 元素设置
- [ ] 进度条在 YouTube 下载时不更新 - yt-dlp 在非 TTY 模式下不输出进度到 stderr/stdout
- [ ] Slint binding loop 警告 - 进度条 Rectangle 的 width 属性绑定导致循环警告

### 三级视频解析链

1. **ResolverRegistry** → 选择对应平台解析器
2. **yt-dlp** (首选) - 支持 cookie 传递
3. **FxTwitter API** → api.fxtwitter.com (Twitter fallback)
4. **Scrapling** → Python 浏览器自动化 (最后 fallback)

### 直接下载器 (downloader.rs)

- 4-8 线程分段并行
- 自适应线程调整 (基于速度阈值)
- `DownloadProgress` 结构体包含 `active_threads` 字段

### 下载进度

- `progress_arc: Arc<AtomicU32>` - 存储 `percent * 100` (0-100)
- `speed_arc: Arc<AtomicU64>` - 存储速度 bytes/s
- `threads_arc: Arc<AtomicU32>` - 存储当前线程数
- 进度更新线程 100ms 间隔，减少 UI 重绘

---

## 构建

```bash
cargo build --release
./target/release/x-video-downloader --gui
```

---

## 最近提交

```
be38f5b - docs: update README and HANDOFF
0a4a4e7 - feat: GUI improvements and network status indicator
4e8fa4f - fix: use map_while for yt-dlp progress parsing
```

---

## 待办 (优先级排序)

### P0 - 关键问题

1. **进度条不更新** - yt-dlp 在非 TTY 模式下进度不输出
   - 尝试方案: 使用 `--progress-template` 参数强制输出
   - 或使用 PTY 模拟 (script 命令)
   - 或在 yt-dlp 层面使用 `-q --progress` 参数组合

2. **中文字体显示** - 需要在每个 Text 元素添加 `font-family: "PingFang SC"`

### P1 - 重要功能

3. **DouYin feed URL 解析** - jingxuan/sports?modal_id=xxx 需进一步测试
4. **clippy 检查修复** - 运行 `cargo clippy` 查看警告 (112 个警告)

### P2 - 体验优化

5. **窗口调整限制** - Slint Window 的 min-width/max-width 属性
6. **可拖动分隔条** - 需要自定义 MouseArea 实现 Splitter

---

## 依赖

- yt-dlp: `brew install yt-dlp`
- Python 3: 用于 Scrapling 模块
- Rust 1.95+
- sm3: 0.5 (A-Bogus SM3 国密哈希)

---

## 代码位置

- GUI 入口: `src/gui.rs` - `run_gui()` 函数
- UI 定义: `ui.slint` - Slint 语法
- 历史管理: `src/history.rs` - `History` 结构体
- 下载器: `src/downloader.rs` - `VideoDownloader`
- yt-dlp: `src/yt_dlp.rs` - `YtDlp::build_download_args()`
- 类型定义: `src/types.rs` - `DownloadProgress`, `VideoInfo`
- 解析器注册: `src/resolvers/mod.rs` - `ResolverRegistry`
- 抖音 API: `src/resolvers/douyin_api.rs` - `DouYinApiClient`
- 配置管理: `src/config.rs` - `AppConfig::load()`

---

## 配置

**配置文件:** `~/.config/x_video_downloader/config.toml` (或 `AppConfig::config_path()`)

**DouYin msToken 配置:**
```bash
# 方式1: 环境变量
export DOUYIN_MS_TOKEN="your_ms_token_here"

# 方式2: 配置文件
[profile]
douyin_ms_token = "your_ms_token_here"
```

**DouYin Cookie 文件配置 (完整登录):**
如果 msToken 不够用（"Fresh cookies needed" 错误），需要导出完整 cookies：

1. 安装 EditThisCookie 或 Cookie-Editor 浏览器扩展
2. 登录抖音后导出 Netscape 格式 cookie
3. 保存到 `~/.config/x_video_downloader/douyin_cookies.txt`
4. 在配置文件中指定:
```toml
[profile]
douyin_cookie_file = "~/.config/x_video_downloader/douyin_cookies.txt"
```

**支持的环境变量:**
- `DOUYIN_MS_TOKEN` - 抖音 msToken

---

## 调试技巧

- GUI 启动时会打印 `DEBUG: GUI started, log file: ...` 日志
- `eprintln!("DEBUG yt-dlp stderr: {}", line)` 会输出 yt-dlp 的 stderr
- `parse_ytdlp_progress()` 函数解析进度，格式: `[download] 45.0% of ~2.50MB`
- Slint 回调中使用 `invoke_from_event_loop` 线程安全更新 UI
- 进度条使用 `HorizontalLayout { alignment: start }` 包裹防止 binding loop
- 设置 `RUST_LOG=debug` 查看详细日志

---

## 已知问题根因分析

### 进度条不更新

yt-dlp 检测是否连接到 TTY 来决定是否输出进度。当作为子进程被捕获时:
- stderr 被 `BufReader` 读取，但 yt-dlp 可能输出到 `stdout` 或根本不输出
- 尝试的修复:
  1. 同时捕获 stdout 和 stderr ✓
  2. 使用 `--progress-template` 强制格式 - 不生效
  3. 使用 `script` 命令模拟 PTY - 未测试

### 中文字体显示

Slint 的 `Window` 组件不支持 `font-family` 属性。需要逐个 Text 元素添加:
```slint
Text {
    font-family: "PingFang SC";
    ...
}
```

### 抖音视频解析

feed URL `https://www.douyin.com/jingxuan/sports?modal_id=xxx` 解析流程:
1. 提取 modal_id 参数
2. 通过 `DouYinApiClient::get_video_by_modal_id()` 获取视频信息
3. 需要有效的 msToken 才能获取完整视频数据

---

## CLAUDE.md 规则

项目使用双层规则结构:
- **编程理念 (1-12)**: Karpathy 风格 + Rust 特定规范
- **AI 辅助原则 (T5-T11)**: 模型使用、Token 预算、冲突处理等
- **项目特定规则**: Slint GUI 禁则、测试要求、调试流程

详见 `CLAUDE.md`
