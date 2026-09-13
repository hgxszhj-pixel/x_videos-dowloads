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

    /// SHANHU/万字 SaaS: 租户 ID (org_id)。SHANHU 是白标平台,不同租户需不同 org_id。
    /// token 字段通常包含 org_id(`wx_<hash>_<org_id>_<fans_id>`),也可通过 `--shanhu-login-org-id`
    /// 显式覆盖环境变量 `SHANHU_ORG_ID` 和内置默认值 27632958。
    #[arg(long = "shanhu-login-org-id", value_name = "ORG_ID")]
    pub shanhu_login_org_id: Option<u64>,
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
