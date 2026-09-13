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
    if cli.shanhu_cache_info
        || cli.shanhu_add_hash.is_some()
        || cli.shanhu_session_info
        || cli.shanhu_login.is_some()
        || cli.shanhu_login_cookie.is_some()
    {
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
            match s.age_days {
                Some(d) => println!("  距今:     {} 天", d),
                None => println!("  距今:     <无法解析 saved_at>"),
            }
            if s.is_stale {
                println!();
                println!("  ⚠️  session 已超过 7 天阈值,videoAuth 可能返 code=100067 非法请求");
                println!("     建议重跑: x-video-downloader --shanhu-login <state.json>");
            }
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
        return;
    }

    if let Some(ref state_path) = cli.shanhu_login {
        handle_shanhu_login(state_path);
    }
    if let Some(ref cookie_str) = cli.shanhu_login_cookie {
        handle_shanhu_login_cookie(cookie_str);
    }
}

/// 从 camoufox MCP `export_state()` 导出的 JSON 文件导入登录态到本地 session。
///
/// 流程:读 JSON → 解析 cookies 数组 → 过滤 wanzibook / rexxi / tenwiseacademy 域 →
/// 转 Session → 原子写入 session 文件 → 输出 session_summary。
fn handle_shanhu_login(state_path: &std::path::PathBuf) {
    use crate::platform::{save_session, session_from_export, session_summary};

    // 1. 读文件
    let raw = match std::fs::read_to_string(state_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("错误: 读取 {} 失败: {}", state_path.display(), e);
            std::process::exit(1);
        }
    };

    // 2. 解析 JSON
    let v: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(e) => {
            eprintln!(
                "错误: JSON 解析失败: {}\n  提示: 文件应为 camoufox MCP `export_state()` 导出格式",
                e
            );
            std::process::exit(1);
        }
    };

    // 3. 提取 cookies 数组 (兼容 `{"cookies": [...]}` 与顶层 `[...]`)
    let cookies = v
        .get("cookies")
        .and_then(|c| c.as_array())
        .cloned()
        .or_else(|| v.as_array().cloned())
        .unwrap_or_default();

    if cookies.is_empty() {
        eprintln!(
            "错误: state 文件中没有 cookies 字段\n  提示: camoufox MCP `export_state()` 导出格式 {{\"cookies\": [...]}} 或顶层数组\n  当前文件: {}",
            state_path.display()
        );
        std::process::exit(1);
    }

    // 4. 过滤 SHANHU 相关域 (wanzibook.com 后端 / rexxi.cn 白标 / tenwiseacademy.cn CDN)
    let total = cookies.len();
    let shanhu_cookies: Vec<serde_json::Value> = cookies
        .into_iter()
        .filter(|c| {
            let domain = c.get("domain").and_then(|d| d.as_str()).unwrap_or("");
            crate::platform::is_shanhu_cookie_domain(domain)
        })
        .collect();
    let kept = shanhu_cookies.len();
    let skipped = total - kept;

    if kept == 0 {
        eprintln!(
            "错误: state 文件中无 SHANHU 相关域 cookies (wanzibook.com / rexxi.cn / tenwiseacademy.cn)\n  提示: 请在 camoufox 中登录 wx-bgfcbieh.rexxi.cn 后再 export_state\n  当前文件: {}",
            state_path.display()
        );
        std::process::exit(1);
    }

    // 5. 转换 + 原子写
    let session = session_from_export(&shanhu_cookies);
    if let Err(e) = save_session(&session) {
        eprintln!("错误: 写入 session 文件失败: {}", e);
        std::process::exit(1);
    }

    // 6. 输出摘要
    println!("✓ SHANHU session 已导入");
    println!("  源:   {}", state_path.display());
    println!(
        "  cookie 数: {} (跳过 {} 个非 SHANHU 域)",
        kept, skipped
    );
    let s = session_summary();
    if let Some(p) = s.path {
        println!("  保存到: {}", p);
    }
    println!("  含 token:  {}", if s.has_token { "是" } else { "否" });
    println!(
        "  保存时间:  {}",
        s.saved_at.as_deref().unwrap_or("<未知>")
    );
    println!();
    println!("  videoAuth 自动发现已可用 — 缺失章节会通过 session cookie 自动获取 cfileid");
    println!("  更新:重新 export_state 后再跑本命令 (覆盖保存)");
}

/// 从 Chrome DevTools Cookie header 字符串导入登录态(跳过 state.json 中间文件)。
///
/// 与 `handle_shanhu_login` 的区别:不读 JSON 文件,直接解析裸 cookie 字符串
/// `name=value; name2=value2`,统一标 `.wanzibook.com` 域(HttpOnly cookie 没法
/// inspect host)。复用 `session_from_export` + `save_session`。
///
/// 设计动机(2026-09-13):camoufox MCP v1.2.0 `export_state()` 不导出 HttpOnly cookie,
/// 用户反馈 `--shanhu-login <state.json>` 流程因 state.json 文件不存在/格式错而
/// 卡住。本入口让用户 1 分钟从 Chrome DevTools Network → Cookie header 复制后直接
/// 喂命令行,省去中间脚本与文件。
fn handle_shanhu_login_cookie(cookie_str: &str) {
    use crate::platform::{is_shanhu_cookie_domain, save_session, session_from_export, session_summary};

    // 1. 解析 cookie 字符串 → JSON cookies 数组(统一 .wanzibook.com)
    let cookies: Vec<serde_json::Value> = cookie_str
        .split(';')
        .filter_map(|piece| {
            let piece = piece.trim();
            if piece.is_empty() {
                return None;
            }
            let (name, value) = piece.split_once('=')?;
            let name = name.trim();
            let value = value.trim();
            if name.is_empty() || value.is_empty() {
                return None;
            }
            Some(serde_json::json!({
                "name": name,
                "value": value,
                "domain": ".wanzibook.com",
                "path": "/",
                "expires": -1,
                "httpOnly": true,
                "secure": true,
                "sameSite": "Lax",
            }))
        })
        .collect();

    if cookies.is_empty() {
        eprintln!(
            "错误: cookie 字符串解析后为空\n  提示: 格式 'name1=value1; name2=value2; ...'\n  示例: --shanhu-login-cookie 'PHPSESSID=abc; tengface_user_login_uuid=xyz'"
        );
        std::process::exit(1);
    }

    // 2. 过滤 SHANHU 域(全标 .wanzibook.com,理论上全过;保留 filter 是为防御)
    let total = cookies.len();
    let shanhu_cookies: Vec<serde_json::Value> = cookies
        .into_iter()
        .filter(|c| {
            let domain = c.get("domain").and_then(|d| d.as_str()).unwrap_or("");
            is_shanhu_cookie_domain(domain)
        })
        .collect();
    let kept = shanhu_cookies.len();
    let skipped = total - kept;

    if kept == 0 {
        eprintln!(
            "错误: 解析的 cookies 均不属 SHANHU 域(意外)\n  当前解析到 {} 个,domain 全部为 .wanzibook.com",
            total
        );
        std::process::exit(1);
    }

    // 3. 转换 + 原子写
    let session = session_from_export(&shanhu_cookies);
    if let Err(e) = save_session(&session) {
        eprintln!("错误: 写入 session 文件失败: {}", e);
        std::process::exit(1);
    }

    // 4. 输出摘要(与 handle_shanhu_login 风格对齐)
    println!("✓ SHANHU session 已导入(从 Chrome DevTools Cookie 字符串)");
    println!("  源:   Chrome DevTools Network → Request Headers → Cookie 行");
    println!("  cookie 数: {} (跳过 {} 个非 SHANHU 域)", kept, skipped);
    let s = session_summary();
    if let Some(p) = s.path {
        println!("  保存到: {}", p);
    }
    println!("  含 token:  {}", if s.has_token { "是" } else { "否" });
    println!(
        "  保存时间:  {}",
        s.saved_at.as_deref().unwrap_or("<未知>")
    );
    println!();
    println!("  videoAuth 自动发现已可用 — 缺失章节会通过 session cookie 自动获取 cfileid");
    println!("  更新:Chrome DevTools 重新复制 Cookie 后再跑本命令 (覆盖保存)");
}
