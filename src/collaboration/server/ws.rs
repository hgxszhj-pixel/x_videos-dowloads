//! WebSocket 服务器

use crate::collaboration::server::handler::MessageHandler;
use crate::collaboration::types::ClientMessage;
use anyhow::Result;
use futures_util::{StreamExt, SinkExt};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

/// WebSocket 服务器
pub struct WsServer {
    handler: Arc<MessageHandler>,
}

impl WsServer {
    pub fn new(handler: Arc<MessageHandler>) -> Self {
        Self { handler }
    }

    /// 启动服务器
    pub async fn start(&self, addr: &str) -> Result<()> {
        let listener = TcpListener::bind(addr).await?;
        println!("协作服务器监听: {}", addr);

        // 启动超时检测后台任务
        let handler = self.handler.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(60));
            loop {
                interval.tick().await;
                // 检查所有团队的认领超时任务
                // 注意：这里需要遍历所有设备获取 team_id
                // 简化处理：每分钟检查一次
                handler.check_all_task_timeouts().await;
            }
        });

        loop {
            let (stream, remote_addr) = listener.accept().await?;
            let handler = self.handler.clone();

            tokio::spawn(async move {
                if let Err(e) = Self::handle_connection(handler, stream, remote_addr).await {
                    eprintln!("连接处理错误: {}", e);
                }
            });
        }
    }

    async fn handle_connection(
        handler: Arc<MessageHandler>,
        stream: tokio::net::TcpStream,
        remote_addr: std::net::SocketAddr,
    ) -> Result<()> {
        let ws_stream = tokio_tungstenite::accept_async(stream).await?;
        let (mut write, read) = ws_stream.split();

        // 生成 session key
        let session_key = Uuid::new_v4().to_string();

        // 创建 channel 用于发送消息给客户端
        let (tx, rx) = broadcast::channel::<String>(100);

        // 预注册 (无 device_id/team_id)
        handler
            .register_client(
                session_key.clone(),
                Uuid::nil(),
                Uuid::nil(),
                tx.clone(),
            )
            .await;

        // 读取消息任务
        let session_key_clone = session_key.clone();
        let handler_read = handler.clone();
        let read_future = async move {
            let mut read = read;
            while let Some(msg) = read.next().await {
                match msg {
                    Ok(Message::Text(text)) => {
                        if let Ok(client_msg) = serde_json::from_str::<ClientMessage>(&text) {
                            // 如果是 Register 消息，先更新 session 的设备信息
                            if let ClientMessage::Register {
                                device_id,
                                team_id,
                                ..
                            } = &client_msg
                            {
                                handler_read
                                    .update_session(&session_key_clone, *device_id, *team_id)
                                    .await;
                            }
                            handler_read
                                .handle_message(&session_key_clone, client_msg)
                                .await;
                        }
                    }
                    Ok(Message::Close(_)) => {
                        break;
                    }
                    Err(e) => {
                        eprintln!("WebSocket 读取错误: {}", e);
                        break;
                    }
                    _ => {}
                }
            }
        };

        // 写入消息任务
        let write_future = async {
            let mut rx = rx;
            while let Ok(msg) = rx.recv().await {
                if write.send(Message::Text(msg)).await.is_err() {
                    break;
                }
            }
        };

        // 同时运行读写任务
        let handler_cleanup = handler.clone();
        tokio::select! {
            _ = read_future => {}
            _ = write_future => {}
        }

        // 取消注册
        handler_cleanup.unregister_client_by_key(&session_key).await;

        println!("客户端断开: {}", remote_addr);
        Ok(())
    }
}
