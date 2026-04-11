//! WebSocket 服务器

use crate::collaboration::crypto::auth::AuthToken;
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
        let (mut write, mut read) = ws_stream.split();

        // ===== 认证阶段 =====
        // 等待客户端发送认证 token（首条消息）
        let first_msg = read.next().await;
        let first_msg = match first_msg {
            Some(Ok(Message::Text(text))) => text,
            Some(Ok(Message::Close(_))) => {
                println!("客户端在认证阶段关闭连接: {}", remote_addr);
                return Ok(());
            }
            Some(Ok(Message::Ping(data))) => {
                // 自动回复 Ping
                let _ = write.send(Message::Pong(data)).await;
                let first_msg = read.next().await;
                match first_msg {
                    Some(Ok(Message::Text(text))) => text,
                    _ => {
                        eprintln!("认证阶段收到无效消息类型");
                        return Ok(());
                    }
                }
            }
            Some(Err(e)) => {
                eprintln!("读取认证消息失败: {}", e);
                return Err(e.into());
            }
            _ => {
                eprintln!("认证阶段收到无效消息类型");
                return Ok(());
            }
        };

        // 解析并验证 token
        let auth_result: Result<AuthToken, ()> = match first_msg.split_once(':') {
            Some((prefix, rest)) => {
                // 格式: "AUTH:token_string"
                if prefix != "AUTH" {
                    Err(())
                } else {
                    AuthToken::parse(rest).map_err(|_| ())
                }
            }
            None => Err(()),
        };

        let (team_id, device_id) = match auth_result {
            Ok(token) => {
                match token.verify() {
                    Ok(()) => {
                        println!("认证成功: team_id={}, device_id={}", token.team_id, token.device_id);
                        (token.team_id, token.device_id)
                    }
                    Err(e) => {
                        eprintln!("Token 验证失败: {:?}", e);
                        let _ = write.send(Message::Text(serde_json::to_string(&
                            crate::collaboration::types::ServerMessage::Error {
                                message: format!("认证失败: {:?}", e)
                            }
                        )?)).await;
                        let _ = write.send(Message::Close(None)).await;
                        return Ok(());
                    }
                }
            }
            Err(()) => {
                eprintln!("Token 解析失败: {}", first_msg);
                let _ = write.send(Message::Text(serde_json::to_string(&
                    crate::collaboration::types::ServerMessage::Error {
                        message: "无效的认证格式".to_string()
                    }
                )?)).await;
                let _ = write.send(Message::Close(None)).await;
                return Ok(());
            }
        };

        // ===== 认证通过，建立 session =====
        let session_key = Uuid::new_v4().to_string();

        // 创建 channel 用于发送消息给客户端
        let (tx, rx) = broadcast::channel::<String>(100);

        // 注册客户端（带 device_id/team_id）
        handler
            .register_client(
                session_key.clone(),
                device_id,
                team_id,
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
