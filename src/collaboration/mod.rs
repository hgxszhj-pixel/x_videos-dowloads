//! 协作下载模块
//!
//! 提供多设备实时协作下载功能

pub mod client;
pub mod crypto;
pub mod discovery;
pub mod server;
pub mod transfer;
pub mod types;

// 公开 API - 从子模块重新导出
#[allow(unused_imports)]
pub use client::ws::{CollaborationClient, CollaborationClientWithFileHandler};
#[allow(unused_imports)]
pub use client::queue::LocalQueue;
#[allow(unused_imports)]
pub use server::{MessageHandler, WsServer};
#[allow(unused_imports)]
pub use types::{ClientMessage, Device, ServerMessage, Task, TaskStatus, Team};

use crate::collaboration::server::db::Database;
use anyhow::Result;
use std::path::Path;
use std::sync::Arc;

/// 创建协作服务器
#[allow(dead_code)]
pub async fn start_server(addr: &str, db_path: &Path) -> Result<()> {
    let db = Arc::new(Database::open(db_path)?);
    let handler = Arc::new(MessageHandler::new(db));
    let server = WsServer::new(handler);

    server.start(addr).await?;
    Ok(())
}
