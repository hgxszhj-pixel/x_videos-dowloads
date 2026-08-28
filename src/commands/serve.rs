//! serve 子命令：协作服务器与团队管理

use uuid::Uuid;

use crate::collaboration::server::db::Database;
use crate::collaboration::server::handler::MessageHandler;
use crate::collaboration::server::ws::WsServer;
use crate::collaboration::CollaborationClient;

/// 启动协作服务器
pub fn run_server(port: u16) {
    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime 创建失败");
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

/// 创建团队
pub async fn run_create_team(name: &str, server_url: &str, device_name: &str) {
    let device_id = Uuid::new_v4();

    println!("正在连接到服务器 {} ...", server_url);

    // 先用临时 team_id 连接
    let temp_team_id = Uuid::nil();
    let client = match CollaborationClient::connect(
        server_url,
        temp_team_id,
        device_id,
        device_name,
    )
    .await
    {
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
pub async fn run_join_team(invite_code: &str, server_url: &str, device_name: &str) {
    let device_id = Uuid::new_v4();

    println!("正在连接到服务器 {} ...", server_url);
    println!("邀请码: {}", invite_code);

    // 先用临时 team_id 连接
    let temp_team_id = Uuid::nil();
    let client = match CollaborationClient::connect(
        server_url,
        temp_team_id,
        device_id,
        device_name,
    )
    .await
    {
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
