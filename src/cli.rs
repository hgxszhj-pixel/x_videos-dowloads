//! 命令行参数定义

use clap::{Parser, ValueEnum};
use std::path::PathBuf;

/// 命令行参数
#[derive(Parser, Debug)]
#[command(name = "x-video-downloader")]
#[command(version = "0.1.0")]
#[command(about = "X.com 视频下载器 - 使用 yt-dlp 下载视频", long_about = None)]
pub struct Cli {
    /// 视频 URL
    #[arg(value_name = "URL")]
    pub url: Option<String>,

    /// 批量文件（每行一个 URL）
    #[arg(short, long, value_name = "FILE")]
    pub batch: Option<PathBuf>,

    /// 输出目录
    #[arg(short, long, value_name = "DIR", default_value = ".")]
    pub output: PathBuf,

    /// 视频格式
    #[arg(short, long, value_name = "FORMAT")]
    pub format: Option<String>,

    /// 仅下载音频
    #[arg(short = 'x', long = "audio-only")]
    pub audio_only: bool,

    /// 下载字幕
    #[arg(long = "subs")]
    pub subtitles: bool,

    /// 指定输出文件名
    #[arg(long, value_name = "NAME")]
    pub filename: Option<String>,

    /// 显示可用格式
    #[arg(long = "list-formats")]
    pub list_formats: bool,

    /// 使用代理
    #[arg(long, value_name = "PROXY")]
    pub proxy: Option<String>,

    /// HTTP Referer 头（用于绕过防盗链）
    #[arg(long, value_name = "URL")]
    pub referer: Option<String>,

    /// Cookie 文件路径
    #[arg(long, value_name = "FILE")]
    pub cookies: Option<PathBuf>,

    /// 自定义 User-Agent
    #[arg(long, value_name = "AGENT")]
    pub user_agent: Option<String>,

    /// 详细输出
    #[arg(short, long)]
    pub verbose: bool,

    /// 不使用 yt-dlp（直接下载）
    #[arg(long = "direct")]
    pub direct: bool,

    /// 启动GUI界面
    #[arg(long = "gui")]
    pub gui: bool,

    /// 创建团队（协作模式）
    #[cfg(feature = "collaboration")]
    #[arg(long = "create-team")]
    pub create_team: Option<String>,

    /// 加入团队（协作模式）
    #[cfg(feature = "collaboration")]
    #[arg(long = "join-team")]
    pub join_team: Option<String>,

    /// 协作服务器地址
    #[cfg(feature = "collaboration")]
    #[arg(long = "server", default_value = "ws://localhost:9000")]
    pub server_addr: String,

    /// 设备名称
    #[cfg(feature = "collaboration")]
    #[arg(long = "device-name", default_value = "MyDevice")]
    pub device_name: String,

    /// 初始化配置文件
    #[arg(long = "init-config")]
    pub init_config: bool,

    /// 查看下载历史
    #[arg(long = "history")]
    pub history: bool,

    /// 添加书签
    #[arg(long = "bookmark")]
    pub bookmark: Option<String>,

    /// 列出所有书签
    #[arg(long = "bookmarks")]
    pub bookmarks: bool,

    /// 清空下载历史
    #[arg(long = "clear-history")]
    pub clear_history: bool,

    /// 搜索历史
    #[arg(long = "search", value_name = "QUERY")]
    pub search: Option<String>,

    /// 启动协作服务器
    #[cfg(feature = "collaboration")]
    #[arg(long = "start-server")]
    pub start_server: bool,

    /// 服务器端口
    #[cfg(feature = "collaboration")]
    #[arg(long = "port", default_value = "9000")]
    pub server_port: u16,

    /// SHANHU/万字 SaaS: 添加 file_id → m3u8 URL 映射到本地缓存。
    /// 格式: `<file_id>:<mp4_url>` (mp4_url 为完整 m3u8 URL,从 videoAuth 响应 `data.video.mp4Url` 取)。
    /// 用法:浏览器 DevTools 拦截 videoAuth 响应, 复制 `data.video.mp4Url` 字段值,
    /// `--shanhu-add-hash 3877025xxx:https://11145xxx.tenwiseacademy.cn/.../<32hex>.m3u8`。
    #[arg(long = "shanhu-add-hash", value_name = "FILE_ID:HASH")]
    pub shanhu_add_hash: Option<String>,

    /// SHANHU/万字 SaaS: 显示本地 hash 缓存路径与条目数。
    #[arg(long = "shanhu-cache-info")]
    pub shanhu_cache_info: bool,

    /// SHANHU/万字 SaaS: 显示登录 session 文件路径与状态 (cookie 数 / token / saved_at)。
    /// 用于排查 videoAuth 自动发现失效时,确认 session 是否过期。
    #[arg(long = "shanhu-session-info")]
    pub shanhu_session_info: bool,

    /// SHANHU/万字 SaaS: 从 camoufox MCP `export_state()` 导出的 JSON 文件
    /// 导入登录态 cookies,写入本地 session 文件。
    ///
    /// 用法:在 camoufox 中扫码登录 wx-bgfcbieh.rexxi.cn,调 `export_state()`
    /// 保存到本地,然后运行 `--shanhu-login <path-to-state.json>`。
    /// 会自动过滤非 `.wanzibook.com` 域 cookie (保留 wx-bgfcbieh.rexxi.cn 等 SHANHU 租户子域)。
    #[arg(long = "shanhu-login", value_name = "STATE_JSON")]
    pub shanhu_login: Option<PathBuf>,

    /// SHANHU/万字 SaaS: 从 Chrome DevTools Cookie header 字符串直接导入登录态
    /// (跳过 state.json 中间文件,适合 camoufox MCP 不可用时手动 fallback)。
    ///
    /// 用法:Chrome DevTools → Network → 任意 wanzibook 请求 → Request Headers
    /// → Cookie: 行 → 复制 cookie 值(裸字符串,不要 "Cookie:" 前缀)→ 喂本参数。
    /// 建议用环境变量避免 shell history 泄露:`COOKIE="..."; x-video-downloader --shanhu-login-cookie "$COOKIE"`。
    #[arg(long = "shanhu-login-cookie", value_name = "COOKIE_STRING")]
    pub shanhu_login_cookie: Option<String>,

    /// SHANHU/万字 SaaS: 从浏览器 localStorage 导入登录 token。
    /// token 是 SHANHU/万字 API 鉴权的核心字段(2026-09-13 实测:listClass/listSections/videoAuth
    /// 全部 POST form-urlencoded 时需要 token in body,不是 Authorization header)。
    ///
    /// 用法:浏览器 DevTools → Application → Local Storage → `token` 字段值 →
    /// 复制(`wx_<hash>_<org_id>_<fans_id>` 格式)→ 喂本参数。
    /// 通常与 `--shanhu-login-org-id` 配合使用。
    #[arg(long = "shanhu-login-token", value_name = "TOKEN")]
    pub shanhu_login_token: Option<String>,

    /// SHANHU/万字 SaaS: 直接从 macOS 本机 ego-lite 浏览器导入登录态 cookies。
    /// ego-lite 是 macOS 原生 Chromium 内核浏览器, 已登录 SHANHU (rexxi.cn /
    /// wanzibook.com / tenwiseacademy.cn) 后, 跑这条命令一次性写完 session,
    /// 跳过 camoufox MCP state.json 流程 (2026-09-14 BUG-D 调查期间切换)。
    ///
    /// 注意: 仅导入 cookies, **不导入 localStorage token** (Chrome Cookies SQLite
    /// 不含 localStorage, 需 LevelDB 解码 — Phase 2 follow-up)。
    /// videoAuth 用 token 鉴权, 还需要配合 `--shanhu-login-token <TOKEN>` 喂 token。
    ///
    /// 首次运行会弹 macOS Keychain 授权窗口 (选"总是允许")。
    /// 备选 fallback: `--shanhu-login-cookie <COOKIE_STRING>` 手动粘。
    #[cfg(target_os = "macos")]
    #[arg(long = "shanhu-login-ego-lite")]
    pub shanhu_login_ego_lite: bool,

    /// SHANHU/万字 SaaS: 租户 ID (org_id)。SHANHU 是白标平台,不同租户需不同 org_id。
    /// token 字段通常包含 org_id(`wx_<hash>_<org_id>_<fans_id>`),也可通过 `--shanhu-login-org-id`
    /// 显式覆盖环境变量 `SHANHU_ORG_ID` 和内置默认值 27632958。
    #[arg(long = "shanhu-login-org-id", value_name = "ORG_ID")]
    pub shanhu_login_org_id: Option<u64>,

    /// 触发 clean 子命令 (受 mole `clean` 启发):扫描下载目录,清理 stale/orphan/empty/duplicate 文件。
    /// 必须配合 `--clean-target` 指定清理目标(默认 all)。
    #[arg(long = "clean", value_name = "DIR")]
    pub clean: Option<PathBuf>,

    /// clean 子命令: 仅打印候选,不实际删除 (受 mole `clean --dry-run` 启发)
    #[arg(long = "clean-dry-run")]
    pub clean_dry_run: bool,

    /// clean 子命令: 清理目标类别。`empty` | `temp` | `duplicate` | `all`(默认 all)
    #[arg(long = "clean-target", value_name = "KIND", default_value = "all")]
    pub clean_target: String,

    /// clean 子命令: 白名单文件(可多次使用),命中文件即使匹配也跳过删除
    #[arg(long = "clean-whitelist", value_name = "PATH")]
    pub clean_whitelist: Vec<PathBuf>,

    /// 触发 status 子命令 (受 mole `mo status` 启发): 检查 yt-dlp/ffmpeg/python/磁盘/目录可写/历史文件健康度。
    #[arg(long = "status")]
    pub status: bool,

    /// status 子命令: 输出 JSON 格式(便于 CI/监控集成)
    #[arg(long = "status-json")]
    pub status_json: bool,

    /// 触发 analyze 子命令 (受 mole `mo analyze` 启发): 分析下载目录,输出总大小/扩展名分布/年龄分桶/Top-10 大文件。
    #[arg(long = "analyze", value_name = "DIR")]
    pub analyze: Option<PathBuf>,

    /// analyze 子命令: 输出 JSON 格式
    #[arg(long = "analyze-json")]
    pub analyze_json: bool,

    /// 触发 history-stats 子命令 (受 mole `mo history` 启发): 扫描历史文件,输出成功率/累计字节/平台分布/Top-10/最近 7d。
    #[arg(long = "history-stats")]
    pub history_stats: bool,

    /// history-stats 子命令: 输出 JSON 格式
    #[arg(long = "history-stats-json")]
    pub history_stats_json: bool,

    /// history-stats 子命令: 历史文件路径(默认 ~/.x-video-downloader/history.json)
    #[arg(long = "history-stats-path", value_name = "PATH")]
    pub history_stats_path: Option<std::path::PathBuf>,

    /// 触发 installer 子命令 (受 mole `mo installer` 启发): 检查 yt-dlp/ffmpeg/python3 版本对齐,给出升级建议。
    #[arg(long = "installer")]
    pub installer: bool,

    /// installer 子命令: 输出 JSON 格式
    #[arg(long = "installer-json")]
    pub installer_json: bool,
}

/// 下载模式
#[derive(Debug, ValueEnum, Clone)]
pub enum DownloadMode {
    /// 最佳质量视频
    Best,
    /// 最佳视频+音频
    BestVideoAudio,
    /// 仅音频
    Audio,
    /// 指定格式
    Custom,
}
