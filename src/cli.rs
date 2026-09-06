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

    /// SHANHU/万字 SaaS: 添加 file_id → hash 映射到本地缓存。
    /// 格式: `<file_id>:<hash>` (hash 为 32 字符 hex)。
    /// 用法:浏览器抓包拿到 `<hash>.m3u8` 后,执行
    /// `--shanhu-add-hash 3877025xxx:2586671d83683fda9fba38f4ab74225e`。
    #[arg(long = "shanhu-add-hash", value_name = "FILE_ID:HASH")]
    pub shanhu_add_hash: Option<String>,

    /// SHANHU/万字 SaaS: 显示本地 hash 缓存路径与条目数。
    #[arg(long = "shanhu-cache-info")]
    pub shanhu_cache_info: bool,

    /// SHANHU/万字 SaaS: 显示登录 session 文件路径与状态 (cookie 数 / token / saved_at)。
    /// 用于排查 videoAuth 自动发现失效时,确认 session 是否过期。
    #[arg(long = "shanhu-session-info")]
    pub shanhu_session_info: bool,
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
