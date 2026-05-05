# x_videos-dowloads

Rust x.com 视频下载器，支持 CLI 和 GUI，具有三级视频源解析功能。

## 功能特性

- **CLI 模式**: `cargo run -- "https://x.com/user/status/123456"`
- **GUI 模式**: `cargo run -- --gui` 或 `./target/release/x-video-downloader --gui`
- **三级解析链**: yt-dlp → FxTwitter API → Scrapling (Cloudflare 绕过)
- **自适应线程**: 根据网站特征和下载速度智能调整线程数
- **网站特征学习**: 记录热门网站的防爬机制和下载特征

## 项目结构

```
x_videos-dowloads/
├── src/
│   ├── main.rs              # CLI 入口
│   ├── lib.rs               # 库入口
│   ├── config.rs            # 配置管理
│   ├── types.rs             # 类型定义
│   ├── downloader.rs        # 直接下载器 (4线程分段并行)
│   ├── yt_dlp.rs           # yt-dlp 集成
│   ├── fxtwitter.rs         # FxTwitter API 解析器
│   ├── scrapling_client.rs  # Scrapling Cloudflare 绕过
│   ├── scrapling_extractor.rs # HTML 视频 URL 提取
│   ├── website_profile.rs   # 网站特征学习
│   ├── gui.rs              # GUI (Slint v1.15.1)
│   ├── history.rs          # 下载历史/书签
│   └── collaboration/       # 分布式协作模块
├── ui.slint                 # Slint UI 定义
├── Cargo.toml
└── README.md
```

## GUI 功能

- 网络状态指示器 (Online/Checking/Offline)
- Fetch Info / Download / Save / Clear 按钮
- 实时下载进度条 (速度/ETA/线程数)
- 文件大小人性化显示 (KB/MB/GB)
- Cookie 文件选择
- 输出目录配置

## 视频解析链

```
1. yt-dlp (首选) → 快速解析，支持多种格式
2. FxTwitter API → api.fxtwitter.com
3. Scrapling → Python 浏览器自动化绕过 Cloudflare
```

## 构建

```bash
cargo build              # Debug 构建
cargo build --release   # 发布构建
cargo run -- --gui      # 运行 GUI
```

## 依赖

- yt-dlp (需安装)
- Python 3 (用于 Scrapling 模块)

## 最近提交

- `0a4a4e7` - feat: GUI improvements and network status indicator
- `4e8fa4f` - fix: use map_while instead of filter_map for yt-dlp progress parsing
