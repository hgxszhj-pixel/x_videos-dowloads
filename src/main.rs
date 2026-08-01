//! X Video Downloader - 主程序入口
//!
//! 命令行工具，用于下载 x.com (Twitter) 视频。
//! 仅负责命令路由；各子命令实现见 [`commands`] 模块。

mod cli;
mod collaboration;
mod commands;
mod config;
mod core;
mod danmaku;
mod downloader;
mod fxtwitter;
mod gui_dioxus;
mod hls;
mod history;
mod http_client;
mod i18n;
mod muxer;
mod platform;
mod resolver_chain;
mod scrapling_client;
mod scrapling_extractor;
mod drission_client;
mod subtitle;
mod types;
mod utils;
mod website_profile;
mod yt_dlp;

use clap::Parser;
use tracing::{error, Level};
use tracing_subscriber::FmtSubscriber;

use crate::cli::Cli;
use crate::config::AppConfig;

fn main() {
    // 设置语言环境，避免 ICU4X 日语分词模型错误
    std::env::set_var("LANG", "en_US.UTF-8");

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
        gui_dioxus::run_gui();
        return;
    }

    // 历史和书签管理
    if cli.history || cli.bookmarks || cli.bookmark.is_some() || cli.clear_history || cli.search.is_some() {
        if let Err(e) = commands::history::run_history_commands(cli) {
            eprintln!("历史命令执行失败: {}", e);
            std::process::exit(1);
        }
        return;
    }

    // 启动协作服务器
    if cli.start_server {
        commands::serve::run_server(cli.server_port);
        return;
    }

    // 协作模式
    if cli.create_team.is_some() || cli.join_team.is_some() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        if let Some(ref name) = cli.create_team {
            runtime.block_on(commands::serve::run_create_team(name, &cli.server_addr, &cli.device_name));
        } else if let Some(ref invite_code) = cli.join_team {
            runtime.block_on(commands::serve::run_join_team(invite_code, &cli.server_addr, &cli.device_name));
        }
        return;
    }

    // 初始化日志（根据 verbose 参数设置级别）。GUI 模式在 run_gui() 里初始化 (2026-08-01 修)。
    if !cli.gui {
        let log_level = if cli.verbose { Level::DEBUG } else { Level::INFO };
        FmtSubscriber::builder()
            .with_max_level(log_level)
            .with_target(false)
            .init();
    }

    // 预初始化共享 HTTP 客户端，避免在 runtime 内触发 once_cell panic
    let _ = crate::http_client::shared();

    // 预初始化 ZhiXueYunResolver 的 blocking client（必须在 runtime 创建前）
    // 否则 reqwest::blocking::Client 会在 runtime 内创建阻塞池导致 panic
    let _ = crate::platform::ZhiXueYunResolver::new_preinit();

    // 运行CLI应用 - 使用当前线程 runtime 避免 blocking pool panic
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .unwrap();
    runtime.block_on(commands::download::run(cli, config)).unwrap_or_else(|e| {
        error!("错误: {}", e);
    });
    std::process::exit(0);
}
