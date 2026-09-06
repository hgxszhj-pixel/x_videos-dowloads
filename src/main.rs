//! X Video Downloader - 主程序入口
//!
//! 命令行工具，用于下载 x.com (Twitter) 视频。
//! 仅负责命令路由；各子命令实现见 [`commands`] 模块。

mod cli;
#[cfg(feature = "collaboration")]
mod collaboration;
mod commands;
mod config;
mod core;
mod danmaku;
mod downloader;
mod drission_client;
mod fxtwitter;
mod gui_dioxus;
mod history;
mod hls;
mod http_client;
mod i18n;
mod muxer;
mod platform;
mod resolver_chain;
mod scrapling_client;
mod scrapling_extractor;
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

    // SHANHU/万字 SaaS: 管理本地 hash 缓存(用于解析 file_id → m3u8 URL 映射)。
    // 此处必须先于 --gui / --history / 其它 dispatch,确保 CLI 子命令独立可用。
    if cli.shanhu_cache_info || cli.shanhu_add_hash.is_some() || cli.shanhu_session_info {
        handle_shanhu_cache_commands(&cli);
        return;
    }

    // 如果指定了 --gui，则启动GUI模式
    if cli.gui {
        gui_dioxus::run_gui();
        return;
    }

    // 历史和书签管理
    if cli.history
        || cli.bookmarks
        || cli.bookmark.is_some()
        || cli.clear_history
        || cli.search.is_some()
    {
        if let Err(e) = commands::history::run_history_commands(cli) {
            eprintln!("历史命令执行失败: {}", e);
            std::process::exit(1);
        }
        return;
    }

    // 协作命令（feature = "collaboration"，默认不编译）
    #[cfg(feature = "collaboration")]
    {
        // 启动协作服务器
        if cli.start_server {
            commands::serve::run_server(cli.server_port);
            return;
        }

        // 协作模式
        if cli.create_team.is_some() || cli.join_team.is_some() {
            let runtime = tokio::runtime::Runtime::new().expect("tokio runtime 创建失败");
            if let Some(ref name) = cli.create_team {
                runtime.block_on(commands::serve::run_create_team(
                    name,
                    &cli.server_addr,
                    &cli.device_name,
                ));
            } else if let Some(ref invite_code) = cli.join_team {
                runtime.block_on(commands::serve::run_join_team(
                    invite_code,
                    &cli.server_addr,
                    &cli.device_name,
                ));
            }
            return;
        }
    }

    // 初始化日志（根据 verbose 参数设置级别）。GUI 模式在 run_gui() 里初始化 (2026-08-01 修)。
    if !cli.gui {
        let log_level = if cli.verbose {
            Level::DEBUG
        } else {
            Level::INFO
        };
        FmtSubscriber::builder()
            .with_max_level(log_level)
            .with_target(false)
            .init();
    }

    // 预初始化共享 HTTP 客户端，避免在 runtime 内触发 once_cell panic
    let _ = crate::http_client::shared();

    // 预初始化 ZhiXueYunResolver 的 blocking client（必须在 runtime 创建前）
    // 否则 reqwest::blocking::Client 会在 runtime 内创建阻塞池导致 panic
    #[cfg(feature = "zhixueyun")]
    let _ = crate::platform::ZhiXueYunResolver::new_preinit();

    // 运行 CLI 应用 - 使用多线程 runtime (P2-1)
    //
    // 历史:曾用 new_current_thread 避免 blocking pool panic。但实测 GUI 卡顿
    // 主要源于 current_thread 在 8 并发 .ts 下载 + GUI 渲染同时进行时,无 worker
    // 余量处理 Dioxus 的细粒度反应更新。
    //
    // 升级 multi_thread 的安全前提:
    // 1. reqwest::blocking::Client 仍须在 runtime 创建前预初始化(避免 runtime 内
    //    自动创建 blocking pool),见上方 ZhiXueYunResolver::new_preinit。
    // 2. std::sync::Mutex (GLOBAL_HISTORY) 在 multi_thread 下同样安全 —— 锁持有区间
    //    无 .await 调用,经 store.rs:test_global_history_no_lock_across_await_invariant 验证。
    //    详见 store.rs:21-31 SAFETY 注释。
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_io()
        .enable_time()
        .build()
        .expect("tokio runtime 创建失败");
    runtime
        .block_on(commands::download::run(cli, config))
        .unwrap_or_else(|e| {
            error!("错误: {}", e);
        });
    std::process::exit(0);
}

/// 处理 SHANHU/万字 SaaS 的本地 hash 缓存子命令(纯 CLI,无须 runtime)。
///
/// - `--shanhu-cache-info` 显示缓存路径与条目数
/// - `--shanhu-add-hash <FILE_ID:HASH>` 写入单条映射
fn handle_shanhu_cache_commands(cli: &crate::cli::Cli) {
    use crate::platform::{hash_cache_path, load_hash_cache, save_hash_entry};

    if cli.shanhu_cache_info {
        match hash_cache_path() {
            Some(p) => {
                let map = load_hash_cache();
                println!("SHANHU hash 缓存");
                println!("  路径: {}", p.display());
                println!("  条目数: {}", map.len());
                if !map.is_empty() {
                    println!("  内容:");
                    for (k, v) in map.iter().take(10) {
                        println!("    {} → {}", k, v);
                    }
                    if map.len() > 10 {
                        println!("    ... 还有 {} 条", map.len() - 10);
                    }
                }
            }
            None => {
                eprintln!("错误: 无法定位 XDG config dir");
                std::process::exit(1);
            }
        }
        return;
    }

    if cli.shanhu_session_info {
        use crate::platform::session_summary;
        let s = session_summary();
        println!("SHANHU login session");
        match &s.path {
            Some(p) => println!("  路径: {}", p),
            None => println!("  路径: <无法定位 XDG config dir>"),
        }
        if s.cookie_count == 0 {
            println!("  状态: 未配置 (无 cookies)");
            println!();
            println!("  videoAuth 自动发现当前不可用 — 缺失章节需手工 `--shanhu-add-hash`");
            println!();
            println!("  配置方式 (二选一):");
            println!("    1. 浏览器手动:登录 wx-bgfcbieh.rexxi.cn 后,DevTools → Application →");
            println!("       Cookies → 复制 .wanzibook.com 下的所有 cookie 到上述路径 JSON");
            println!("    2. 自动化脚本:用 camoufox 打开课程页,扫码登录,export_state → 转 shanhu-session.json");
        } else {
            println!("  cookie 数: {}", s.cookie_count);
            println!("  含 token:  {}", if s.has_token { "是" } else { "否" });
            println!("  保存时间:  {}", s.saved_at.as_deref().unwrap_or("<未知>"));
            println!();
            println!("  videoAuth 自动发现可用,缺失章节会通过 session cookie 自动获取 cfileid");
        }
        return;
    }

    if let Some(ref pair) = cli.shanhu_add_hash {
        let (file_id, hash) = match pair.split_once(':') {
            Some((f, h)) => (f.trim(), h.trim()),
            None => {
                eprintln!(
                    "错误: 格式应为 <FILE_ID>:<HASH>,实际: {}\n  示例: --shanhu-add-hash 3877025xxx:2586671d83683fda9fba38f4ab74225e",
                    pair
                );
                std::process::exit(1);
            }
        };
        if file_id.is_empty() || hash.is_empty() {
            eprintln!("错误: file_id 与 hash 均不可为空");
            std::process::exit(1);
        }
        // hash 简单合法性校验:32 字符 hex
        if hash.len() != 32 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
            eprintln!(
                "警告: hash 长度 {} 应为 32 字符,内容: {}\n  仍然写入(用户可能用了非标准 hash)",
                hash.len(),
                hash
            );
        }
        match save_hash_entry(file_id, hash) {
            Ok(()) => {
                println!("✓ 已写入: {} → {}", file_id, hash);
                if let Some(p) = hash_cache_path() {
                    println!("  缓存: {}", p.display());
                }
            }
            Err(e) => {
                eprintln!("错误: 写入失败: {}", e);
                std::process::exit(1);
            }
        }
    }
}
