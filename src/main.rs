//! X Video Downloader - 主程序入口
//!
//! 命令行工具，用于下载 x.com (Twitter) 视频

mod collaboration;
mod config;
mod downloader;
mod gui;
mod history;
mod types;
mod yt_dlp;

use anyhow::Result;
use clap::{Parser, ValueEnum};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tracing::{error, info, Level};
use tracing_subscriber::FmtSubscriber;
use uuid::Uuid;

use crate::collaboration::server::ws::WsServer;
use crate::collaboration::CollaborationClient;
use crate::config::AppConfig;
use crate::downloader::{DownloaderConfig, VideoDownloader};
use crate::history::History;
use crate::types::{format_bytes, sanitize_filename, DownloadRequest, DownloadResult, VideoInfo, DEFAULT_USER_AGENT};
use crate::yt_dlp::YtDlp;

/// 命令行参数
#[derive(Parser, Debug)]
#[command(name = "x-video-downloader")]
#[command(version = "0.1.0")]
#[command(about = "X.com 视频下载器 - 使用 yt-dlp 下载视频", long_about = None)]
struct Cli {
    /// 视频 URL
    #[arg(value_name = "URL")]
    url: Option<String>,

    /// 批量文件（每行一个 URL）
    #[arg(short, long, value_name = "FILE")]
    batch: Option<PathBuf>,

    /// 输出目录
    #[arg(short, long, value_name = "DIR", default_value = ".")]
    output: PathBuf,

    /// 视频格式
    #[arg(short, long, value_name = "FORMAT")]
    format: Option<String>,

    /// 仅下载音频
    #[arg(short = 'x', long = "audio-only")]
    audio_only: bool,

    /// 下载字幕
    #[arg(long = "subs")]
    subtitles: bool,

    /// 指定输出文件名
    #[arg(long, value_name = "NAME")]
    filename: Option<String>,

    /// 显示可用格式
    #[arg(long = "list-formats")]
    list_formats: bool,

    /// 使用代理
    #[arg(long, value_name = "PROXY")]
    proxy: Option<String>,

    /// Cookie 文件路径
    #[arg(long, value_name = "FILE")]
    cookies: Option<PathBuf>,

    /// 自定义 User-Agent
    #[arg(long, value_name = "AGENT")]
    user_agent: Option<String>,

    /// 详细输出
    #[arg(short, long)]
    verbose: bool,

    /// 不使用 yt-dlp（直接下载）
    #[arg(long = "direct")]
    direct: bool,

    /// 启动GUI界面
    #[arg(long = "gui")]
    gui: bool,

    /// 创建团队（协作模式）
    #[arg(long = "create-team")]
    create_team: Option<String>,

    /// 加入团队（协作模式）
    #[arg(long = "join-team")]
    join_team: Option<String>,

    /// 协作服务器地址
    #[arg(long = "server", default_value = "ws://localhost:9000")]
    server_addr: String,

    /// 设备名称
    #[arg(long = "device-name", default_value = "MyDevice")]
    device_name: String,

    /// 初始化配置文件
    #[arg(long = "init-config")]
    init_config: bool,

    /// 查看下载历史
    #[arg(long = "history")]
    history: bool,

    /// 添加书签
    #[arg(long = "bookmark")]
    bookmark: Option<String>,

    /// 列出所有书签
    #[arg(long = "bookmarks")]
    bookmarks: bool,

    /// 清空下载历史
    #[arg(long = "clear-history")]
    clear_history: bool,

    /// 搜索历史
    #[arg(long = "search", value_name = "QUERY")]
    search: Option<String>,

    /// 启动协作服务器
    #[arg(long = "start-server")]
    start_server: bool,

    /// 服务器端口
    #[arg(long = "port", default_value = "9000")]
    server_port: u16,
}

/// 下载模式
#[derive(Debug, ValueEnum, Clone)]
enum DownloadMode {
    /// 最佳质量视频
    Best,
    /// 最佳视频+音频
    BestVideoAudio,
    /// 仅音频
    Audio,
    /// 指定格式
    Custom,
}

fn main() {
    // 解析命令行参数
    let cli = Cli::parse();

    // 初始化配置文件
    if cli.init_config {
        match AppConfig::init_config_file() {
            Ok(Some(path)) => {
                println!("已创建配置文件: {:?}", path);
            }
            Ok(None) => {
                println!("配置文件已存在");
            }
            Err(e) => {
                eprintln!("创建配置文件失败: {}", e);
                std::process::exit(1);
            }
        }
        return;
    }

    // 加载配置文件
    let config = match AppConfig::load() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("加载配置文件失败: {}", e);
            std::process::exit(1);
        }
    };

    // 如果指定了 --gui，则启动GUI模式
    if cli.gui {
        gui::run_gui();
        return;
    }

    // 历史和书签管理
    if cli.history || cli.bookmarks || cli.bookmark.is_some() || cli.clear_history || cli.search.is_some() {
        if let Err(e) = run_history_commands(cli) {
            eprintln!("历史命令执行失败: {}", e);
            std::process::exit(1);
        }
        return;
    }

    // 启动协作服务器
    if cli.start_server {
        run_server(cli.server_port);
        return;
    }

    // 协作模式
    if cli.create_team.is_some() || cli.join_team.is_some() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        if let Some(ref name) = cli.create_team {
            runtime.block_on(run_create_team(name, &cli.server_addr, &cli.device_name));
        } else if let Some(ref invite_code) = cli.join_team {
            runtime.block_on(run_join_team(invite_code, &cli.server_addr, &cli.device_name));
        }
        return;
    }

    // 初始化日志（根据 verbose 参数设置级别）
    let log_level = if cli.verbose { Level::DEBUG } else { Level::INFO };
    FmtSubscriber::builder()
        .with_max_level(log_level)
        .with_target(false)
        .init();

    // 运行CLI应用
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(r) => r,
        Err(e) => {
            error!("创建 Tokio 运行时失败: {}", e);
            std::process::exit(1);
        }
    };
    if let Err(e) = runtime.block_on(run(cli, config)) {
        error!("错误: {}", e);
        std::process::exit(1);
    }
}

async fn run(cli: Cli, config: AppConfig) -> Result<()> {
    let start_time = Instant::now();

    // 初始化历史管理器
    let mut history = History::new(&config.output_dir)?;

    // 检查 yt-dlp 是否可用
    let ytdlp = YtDlp::new();

    if !ytdlp.is_available() {
        anyhow::bail!("错误: 未找到 yt-dlp。请安装 yt-dlp: https://github.com/yt-dlp/yt-dlp#installation");
    }

    info!("X Video Downloader v{}", env!("CARGO_PKG_VERSION"));
    info!("yt-dlp 版本: {}", ytdlp.version().unwrap_or_else(|| "未知".to_string()));

    // 处理 URL
    let urls = if let Some(ref url) = cli.url {
        vec![url.clone()]
    } else if let Some(batch_file) = &cli.batch {
        // 从文件读取 URL 列表
        let content = tokio::fs::read_to_string(&batch_file).await?;
        content
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty() && !s.starts_with('#'))
            .collect()
    } else {
        anyhow::bail!("请提供视频 URL 或使用 -b/--batch 指定批量文件");
    };

    info!("待下载视频数: {}", urls.len());

    // 下载每个视频
    for (index, url) in urls.iter().enumerate() {
        if urls.len() > 1 {
            println!("\n[{}/{}] 处理: {}", index + 1, urls.len(), url);
        }

        // 获取视频信息
        let video_info = if cli.direct {
            // 直接下载模式
            None
        } else {
            Some(get_video_info(&ytdlp, url, &cli).await?)
        };

        // 显示格式列表
        if cli.list_formats {
            if let Some(ref info) = video_info {
                print_video_formats(info);
            }
            continue;
        }

        // 下载视频
        if cli.direct {
            // 直接下载（使用 reqwest）
            let result = download_direct(url, &cli, start_time).await?;
            // 记录到历史
            if result.success {
                history.add_entry(url.clone(), None, Some(result.output_path.clone()), Some(result.file_size), None)?;
            }
        } else if let Some(ref info) = video_info {
            // 使用 yt-dlp 下载
            download_with_ytdlp(&ytdlp, info, &cli, &config, start_time).await?;
            // 记录到历史（duration 转换为 f64）
            let duration_secs = info.duration.map(|d| d as f64);
            history.add_entry(url.clone(), Some(info.title.clone()), None, None, duration_secs)?;
        }
    }

    let elapsed = start_time.elapsed().as_secs_f64();
    println!("\n完成! 总耗时: {:.2}s", elapsed);

    Ok(())
}

/// 获取视频信息
async fn get_video_info(ytdlp: &YtDlp, url: &str, cli: &Cli) -> Result<VideoInfo> {
    let mut yt = ytdlp.clone();

    // 应用自定义配置
    if let Some(ref ua) = cli.user_agent {
        yt = yt.with_user_agent(ua.clone());
    }

    if let Some(ref cookie) = cli.cookies {
        yt = yt.with_cookie_file(cookie.clone());
    }

    if let Some(ref proxy) = cli.proxy {
        yt = yt.with_proxy(proxy.clone());
    }

    // 获取视频信息
    let info = yt.get_video_info(url)?;

    Ok(info)
}

/// 打印可用格式
fn print_video_formats(info: &VideoInfo) {
    println!("\n视频: {}", info.title);
    println!("时长: {}s", info.duration.unwrap_or(0));
    println!("\n可用格式:");
    println!("{:<10} {:<8} {:<15} {:<12}", "ID", "扩展名", "分辨率", "大小");
    println!("{}", "-".repeat(50));

    for fmt in &info.formats {
        let size = fmt
            .filesize
            .map(format_bytes)
            .unwrap_or_else(|| "N/A".to_string());

        let resolution = fmt
            .resolution
            .clone()
            .unwrap_or_else(|| "N/A".to_string());

        println!(
            "{:<10} {:<8} {:<15} {:<12}",
            fmt.format_id, fmt.ext, resolution, size
        );
    }
}

/// 使用 yt-dlp 下载
async fn download_with_ytdlp(
    ytdlp: &YtDlp,
    info: &VideoInfo,
    cli: &Cli,
    config: &AppConfig,
    _start_time: Instant,
) -> Result<()> {
    // 确定输出路径（CLI参数 > 配置文件 > 默认值）
    let output_dir = if cli.output == PathBuf::from(".") {
        config.output_dir.clone()
    } else {
        cli.output.clone()
    };

    // 验证并规范化输出目录为绝对路径
    let output_dir = validate_output_dir(&output_dir)?;

    // 确保输出目录存在
    tokio::fs::create_dir_all(&output_dir).await?;

    // 确定文件名
    let filename = if let Some(ref name) = cli.filename {
        sanitize_filename(name)
    } else {
        sanitize_filename(&info.title)
    };

    // 确定格式
    // 检测是否为直接视频URL（video.twimg.com）
    let is_direct_url = info.url.contains("video.twimg.com");
    let format = cli.format.clone().or_else(|| {
        if cli.audio_only {
            Some("bestaudio".to_string())
        } else if is_direct_url {
            // 直接视频URL不支持合并格式，使用best
            Some("best".to_string())
        } else {
            // 设为 None，让 build_download_args 使用智能默认格式选择
            None
        }
    });

    // 构建 yt-dlp 命令
    let output_path = output_dir.join(format!("{}.%(ext)s", filename));

    let mut yt = ytdlp.clone();

    // 应用配置
    if let Some(ref ua) = cli.user_agent {
        yt = yt.with_user_agent(ua.clone());
    }

    if let Some(ref cookie) = cli.cookies {
        yt = yt.with_cookie_file(cookie.clone());
    }

    if let Some(ref proxy) = cli.proxy {
        yt = yt.with_proxy(proxy.clone());
    }

    // 执行下载
    info!("开始下载: {}", info.title);

    // 使用统一的参数构建函数
    let args = yt.build_download_args(
        &info.url,
        &output_path.to_string_lossy(),
        format.as_deref(),
        cli.audio_only,
        cli.subtitles,
    );

    // 执行命令（带实时进度）
    println!("执行: yt-dlp {}", args.join(" "));

    use tokio::process::Command;

    // 使用 wait_with_output 避免管道死锁，同时捕获输出
    let output = Command::new(&yt.executable)
        .args(&args)
        .output()
        .await?;

    println!(); // 换行

    if output.status.success() {
        println!("下载完成: {}", info.title);
    } else {
        println!("下载失败: {} (退出码: {:?})", info.title, output.status.code());
    }

    Ok(())
}

/// 直接下载（使用 reqwest）
async fn download_direct(url: &str, cli: &Cli, _start_time: Instant) -> Result<DownloadResult> {
    println!("直接下载模式: {}", url);

    let config = DownloaderConfig {
        user_agent: cli.user_agent.clone(),
        proxy: cli.proxy.clone(),
        ..Default::default()
    };

    let downloader = VideoDownloader::with_config(config)?;

    // 确定输出路径
    let output_dir = if cli.output == PathBuf::from(".") {
        get_default_output_dir()
    } else {
        cli.output.clone()
    };

    // 验证并规范化输出目录为绝对路径
    let output_dir = validate_output_dir(&output_dir)?;

    tokio::fs::create_dir_all(&output_dir).await?;

    // 提取文件名
    let filename = if let Some(ref name) = cli.filename {
        sanitize_filename(name)
    } else {
        "video.mp4".to_string()
    };

    let output_path = output_dir.join(&filename);

    let request = DownloadRequest::new(url, &output_path)
        .with_user_agent(
            cli.user_agent.clone().unwrap_or_else(|| DEFAULT_USER_AGENT.to_string())
        );

    // 进度回调
    let progress_callback: Arc<dyn Fn(types::DownloadProgress) + Send + Sync> = Arc::new(|progress: types::DownloadProgress| {
        print!(
            "\r下载进度: {:.1}% | 速度: {} | 已下载: {}",
            progress.percent * 100.0,
            format_bytes(progress.speed as u64),
            format_bytes(progress.bytes_downloaded)
        );
    });

    let result = downloader.download(&request, Some(progress_callback)).await?;

    if result.success {
        println!("\n下载完成: {}", result.output_path.display());
    } else {
        println!("\n下载失败: {}", result.error_message.as_deref().unwrap_or("未知错误"));
    }

    Ok(result)
}

/// 获取默认输出目录
fn get_default_output_dir() -> PathBuf {
    directories::UserDirs::new()
        .and_then(|dirs| dirs.video_dir().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// 验证并规范化输出目录为绝对路径
fn validate_output_dir(dir: &PathBuf) -> Result<PathBuf> {
    let absolute = if dir.is_absolute() {
        dir.clone()
    } else {
        std::fs::canonicalize(dir)
            .map_err(|e| anyhow::anyhow!("无法将路径转换为绝对路径 {:?}: {}", dir, e))?
    };

    // 验证目录是否存在或是可创建的
    if !absolute.exists() {
        if let Some(parent) = absolute.parent() {
            if !parent.exists() && parent.to_string_lossy() != "" {
                anyhow::bail!("父目录不存在: {:?}", parent);
            }
        }
    } else if !absolute.is_dir() {
        anyhow::bail!("输出路径不是目录: {:?}", absolute);
    }

    Ok(absolute)
}

/// 历史和书签命令
fn run_history_commands(cli: Cli) -> Result<()> {
    let config = AppConfig::load()?;
    let mut history = History::new(&config.output_dir)?;

    // 清空历史
    if cli.clear_history {
        history.clear()?;
        println!("下载历史已清空");
        return Ok(());
    }

    // 添加书签
    if let Some(ref url) = cli.bookmark {
        let title = cli.search.clone(); // 复用 search 作为标题
        match history.add_bookmark(url.clone(), title, None) {
            Ok(_) => println!("已添加书签: {}", url),
            Err(e) => eprintln!("添加书签失败: {}", e),
        }
        return Ok(());
    }

    // 列出书签
    if cli.bookmarks {
        let bookmarks = history.get_bookmarks();
        if bookmarks.is_empty() {
            println!("暂无书签");
        } else {
            println!("\n书签列表:");
            for b in bookmarks {
                println!("  - {} ({})", b.title.as_deref().unwrap_or(&b.url), b.url);
                if let Some(ref note) = b.note {
                    println!("    备注: {}", note);
                }
            }
        }
        return Ok(());
    }

    // 搜索历史
    if let Some(ref query) = cli.search {
        let results = history.search(query);
        if results.is_empty() {
            println!("未找到匹配的历史记录");
        } else {
            println!("\n搜索结果 ({} 条):", results.len());
            for entry in results {
                println!("  - {} ({})", entry.title.as_deref().unwrap_or(&entry.url), entry.downloaded_at.format("%Y-%m-%d %H:%M"));
            }
        }
        return Ok(());
    }

    // 查看历史
    let entries = history.get_recent(20);
    if entries.is_empty() {
        println!("暂无下载历史");
    } else {
        println!("\n最近下载 ({} 条):", entries.len());
        for entry in entries {
            let size = entry.file_size.map(format_bytes).unwrap_or_else(|| "N/A".to_string());
            println!("  - {} | {} | {}", entry.title.as_deref().unwrap_or(&entry.url), size, entry.downloaded_at.format("%Y-%m-%d %H:%M"));
        }
    }

    Ok(())
}

// ========== 协作功能 ==========

use crate::collaboration::server::handler::MessageHandler;
use crate::collaboration::server::db::Database;

/// 启动协作服务器
fn run_server(port: u16) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let addr = format!("0.0.0.0:{}", port);

        // 初始化数据库
        let db_path = directories::ProjectDirs::from("com", "x-video-downloader", "collaboration")
            .map(|dirs| dirs.data_dir().join("collaboration.db"))
            .unwrap_or_else(|| std::path::PathBuf::from("collaboration.db"));

        let db = Database::open(&db_path).expect("无法打开数据库");
        let db = std::sync::Arc::new(db);

        // 初始化消息处理器
        let handler = std::sync::Arc::new(MessageHandler::new(db));
        let server = WsServer::new(handler);

        println!("启动协作服务器: {}", addr);
        if let Err(e) = server.start(&addr).await {
            eprintln!("服务器错误: {}", e);
        }
    });
}

#[allow(unused_imports)]
use crate::collaboration::Team;

/// 创建团队
async fn run_create_team(name: &str, server_url: &str, device_name: &str) {
    let device_id = Uuid::new_v4();

    println!("正在连接到服务器 {} ...", server_url);

    // 先用临时 team_id 连接
    let temp_team_id = Uuid::nil();
    let client = match CollaborationClient::connect(server_url, temp_team_id, device_id, device_name).await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("连接失败: {}", e);
            return;
        }
    };

    // 发送创建团队请求
    if let Err(e) = client.create_team(name).await {
        eprintln!("发送创建请求失败: {}", e);
        return;
    }

    println!("已连接! 设备 ID: {}", device_id);

    // 监听消息
    let mut rx = client.subscribe();
    println!("\n等待团队创建响应... (按 Ctrl+C 退出)");

    while let Ok(msg) = rx.recv().await {
        println!("收到消息: {:?}", msg);
    }
}

/// 加入团队
async fn run_join_team(invite_code: &str, server_url: &str, device_name: &str) {
    let device_id = Uuid::new_v4();

    println!("正在连接到服务器 {} ...", server_url);
    println!("邀请码: {}", invite_code);

    // 先用临时 team_id 连接
    let temp_team_id = Uuid::nil();
    let client = match CollaborationClient::connect(server_url, temp_team_id, device_id, device_name).await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("连接失败: {}", e);
            return;
        }
    };

    // 发送加入团队请求
    if let Err(e) = client.join_team(invite_code).await {
        eprintln!("发送加入请求失败: {}", e);
        return;
    }

    println!("已连接! 设备 ID: {}", device_id);

    // 监听消息
    let mut rx = client.subscribe();
    println!("\n等待团队响应... (按 Ctrl+C 退出)");

    while let Ok(msg) = rx.recv().await {
        println!("收到消息: {:?}", msg);
    }
}
