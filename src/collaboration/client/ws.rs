//! WebSocket 客户端

use crate::collaboration::crypto::auth::AuthToken;
use crate::collaboration::types::{ClientMessage, ServerMessage};
use anyhow::Result;
use futures::{SinkExt, StreamExt};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};
use tokio::time::{sleep, Duration};
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;
use tracing::{warn, info, error};

/// 连接状态
#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionState {
    /// 已连接
    Connected,
    /// 连接中
    Connecting,
    /// 断开连接，等待重连
    Disconnected,
    /// 重连中
    Reconnecting { attempt: u64 },
    /// 永久断开（达到最大重试次数）
    Failed,
}

/// 最大重试次数
const MAX_RECONNECT_ATTEMPTS: u64 = 10;
/// 指数退避基础值（秒）
const BACKOFF_BASE_SECS: u64 = 1;
/// 最大退避时间（秒）
const MAX_BACKOFF_SECS: u64 = 30;

/// WebSocket 客户端
#[allow(dead_code)]
pub struct CollaborationClient {
    device_id: Uuid,
    team_id: Uuid,
    write_tx: mpsc::Sender<String>,
    msg_tx: broadcast::Sender<ServerMessage>,
    /// 连接状态
    pub state: Arc<AtomicU64>,
    /// 设备名称
    device_name: String,
    /// 服务器 URL
    server_url: String,
}

impl ConnectionState {
    pub fn from_u64(val: u64) -> Self {
        match val {
            0 => ConnectionState::Disconnected,
            1 => ConnectionState::Connecting,
            2 => ConnectionState::Connected,
            3 => ConnectionState::Failed,
            n => ConnectionState::Reconnecting { attempt: n - 4 },
        }
    }
}

/// 带有文件传输功能的协作客户端
///
/// 整合 CollaborationClient、ChunkedDownloader 和 FileServer，
/// 自动处理文件下载请求
#[allow(dead_code)]
pub struct CollaborationClientWithFileHandler {
    client: CollaborationClient,
    downloader: Arc<crate::collaboration::transfer::downloader::ChunkedDownloader>,
    file_server: Option<Arc<crate::collaboration::transfer::http_server::FileServer>>,
    download_dir: PathBuf,
}

#[allow(dead_code)]
impl CollaborationClient {
    #[allow(dead_code)]
    /// 连接服务器
    pub async fn connect(
        server_url: &str,
        team_id: Uuid,
        device_id: Uuid,
        device_name: &str,
    ) -> Result<Self> {
        let state = Arc::new(AtomicU64::new(1)); // Connecting

        let client = Self {
            device_id,
            team_id,
            write_tx: mpsc::channel(100).0,
            msg_tx: broadcast::channel(100).0,
            state,
            device_name: device_name.to_string(),
            server_url: server_url.to_string(),
        };

        // 初始化 WebSocket 连接
        client.init_ws_connection().await?;

        Ok(client)
    }

    /// 初始化 WebSocket 连接（供 connect 和重连使用）
    async fn init_ws_connection(&self) -> Result<()> {
        let (ws_stream, _) = tokio_tungstenite::connect_async(&self.server_url).await?;
        let (mut write, read) = ws_stream.split();

        // ===== 发送认证 token =====
        let auth_token = AuthToken::generate(self.team_id, self.device_id);
        let auth_msg = format!("AUTH:{}", auth_token);
        write.send(Message::Text(auth_msg)).await?;

        // 消息通道
        let (msg_tx, _) = broadcast::channel(100);
        let msg_tx_clone = msg_tx.clone();

        // 发送注册消息
        let register = ClientMessage::Register {
            device_id: self.device_id,
            team_id: self.team_id,
            name: self.device_name.clone(),
        };
        let register_json = serde_json::to_string(&register)?;
        write.send(Message::Text(register_json)).await?;

        // 获取写入的 sender
        let (write_tx, mut write_rx) = mpsc::channel::<String>(100);

        // WebSocket 写入循环
        let write_loop = async move {
            while let Some(json) = write_rx.recv().await {
                if write.send(Message::Text(json)).await.is_err() {
                    break;
                }
            }
        };

        // WebSocket 读取循环
        let read_loop = async move {
            let mut read = read;
            while let Some(msg) = read.next().await {
                if let Ok(Message::Text(text)) = msg {
                    if let Ok(server_msg) = serde_json::from_str::<ServerMessage>(&text) {
                        let _ = msg_tx_clone.send(server_msg);
                    }
                }
            }
        };

        // 心跳循环
        let device_id_clone = self.device_id;
        let write_tx_clone = write_tx.clone();
        let heartbeat_loop = async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
            loop {
                interval.tick().await;
                let heartbeat = ClientMessage::Heartbeat {
                    device_id: device_id_clone,
                };
                if let Ok(json) = serde_json::to_string(&heartbeat) {
                    if write_tx_clone.send(json).await.is_err() {
                        break;
                    }
                }
            }
        };

        // 并发运行所有循环
        tokio::spawn(async move {
            tokio::join!(write_loop, read_loop, heartbeat_loop);
        });

        // 更新连接状态为已连接
        self.state.store(2, Ordering::SeqCst);
        info!("WebSocket 连接已建立");

        Ok(())
    }

    /// 断开并重连
    async fn reconnect(&self) -> Result<()> {
        let mut attempt = 1u64;

        loop {
            if attempt > MAX_RECONNECT_ATTEMPTS {
                error!("达到最大重连次数 ({}), 放弃重连", MAX_RECONNECT_ATTEMPTS);
                self.state.store(3, Ordering::SeqCst); // Failed
                return Err(anyhow::anyhow!("达到最大重连次数"));
            }

            self.state.store(4 + attempt, Ordering::SeqCst); // Reconnecting { attempt }
            info!("尝试重连 (attempt {}/{})", attempt, MAX_RECONNECT_ATTEMPTS);

            // 计算指数退避时间
            let backoff_secs = std::cmp::min(
                BACKOFF_BASE_SECS * 2u64.pow(attempt as u32 - 1),
                MAX_BACKOFF_SECS,
            );
            sleep(Duration::from_secs(backoff_secs)).await;

            match self.init_ws_connection().await {
                Ok(()) => {
                    info!("重连成功!");
                    return Ok(());
                }
                Err(e) => {
                    error!("重连失败: {}", e);
                    attempt += 1;
                }
            }
        }
    }

    /// 获取当前连接状态
    pub fn connection_state(&self) -> ConnectionState {
        ConnectionState::from_u64(self.state.load(Ordering::SeqCst))
    }

    /// 发送消息
    pub async fn send(&self, msg: ClientMessage) -> Result<()> {
        let json = serde_json::to_string(&msg)?;
        self.write_tx.send(json).await?;
        Ok(())
    }

    /// 订阅消息
    pub fn subscribe(&self) -> broadcast::Receiver<ServerMessage> {
        self.msg_tx.subscribe()
    }

    /// 添加任务
    pub async fn add_task(&self, url: &str) -> Result<()> {
        let msg = ClientMessage::AddTask {
            url: url.to_string(),
        };
        self.send(msg).await
    }

    /// 认领任务
    pub async fn claim_task(&self, task_id: Uuid) -> Result<()> {
        let msg = ClientMessage::ClaimTask { task_id };
        self.send(msg).await
    }

    /// 更新进度
    pub async fn update_progress(&self, task_id: Uuid, progress: f64) -> Result<()> {
        let msg = ClientMessage::UpdateProgress { task_id, progress };
        self.send(msg).await
    }

    /// 任务完成
    pub async fn task_complete(
        &self,
        task_id: Uuid,
        local_path: std::path::PathBuf,
        file_size: u64,
    ) -> Result<()> {
        let msg = ClientMessage::TaskComplete {
            task_id,
            local_path,
            file_size,
        };
        self.send(msg).await
    }

    /// 请求文件
    pub async fn request_file(&self, task_id: Uuid) -> Result<()> {
        let msg = ClientMessage::RequestFile { task_id };
        self.send(msg).await
    }

    /// 离开团队
    pub async fn leave_team(&self) -> Result<()> {
        let msg = ClientMessage::LeaveTeam {
            device_id: self.device_id,
        };
        self.send(msg).await
    }

    /// 创建团队
    pub async fn create_team(&self, name: &str) -> Result<()> {
        let msg = ClientMessage::CreateTeam {
            name: name.to_string(),
        };
        self.send(msg).await
    }

    /// 加入团队
    pub async fn join_team(&self, invite_code: &str) -> Result<()> {
        let msg = ClientMessage::JoinTeam {
            invite_code: invite_code.to_string(),
        };
        self.send(msg).await
    }

    /// 获取设备ID
    pub fn device_id(&self) -> Uuid {
        self.device_id
    }

    /// 获取团队ID
    pub fn team_id(&self) -> Uuid {
        self.team_id
    }
}

#[allow(dead_code)]
impl CollaborationClientWithFileHandler {
    /// 创建带文件处理功能的客户端
    ///
    /// 内部创建独立文件服务器，默认端口 8080
    pub async fn new(
        server_url: &str,
        team_id: Uuid,
        device_id: Uuid,
        device_name: &str,
        download_dir: PathBuf,
        file_server_port: u16,
    ) -> Result<Self> {
        let client = CollaborationClient::connect(server_url, team_id, device_id, device_name).await?;

        let downloader = Arc::new(
            crate::collaboration::transfer::downloader::ChunkedDownloader::new(1024 * 1024) // 1MB chunk
        );

        // 创建并启动文件服务器
        let file_server = Arc::new(
            crate::collaboration::transfer::http_server::FileServer::new(file_server_port)
        );
        let file_server_clone = file_server.clone();
        tokio::spawn(async move {
            if let Err(e) = file_server_clone.start().await {
                eprintln!("文件服务器启动失败: {}", e);
            }
        });

        Ok(Self {
            client,
            downloader,
            file_server: Some(file_server),
            download_dir,
        })
    }

    /// 创建带文件处理功能的客户端（使用已有的 FileServer）
    ///
    /// 适用于需要复用现有文件服务器的场景
    pub async fn with_file_server(
        server_url: &str,
        team_id: Uuid,
        device_id: Uuid,
        device_name: &str,
        download_dir: PathBuf,
        file_server: Arc<crate::collaboration::transfer::http_server::FileServer>,
    ) -> Result<Self> {
        let client = CollaborationClient::connect(server_url, team_id, device_id, device_name).await?;

        let downloader = Arc::new(
            crate::collaboration::transfer::downloader::ChunkedDownloader::new(1024 * 1024)
        );

        Ok(Self {
            client,
            downloader,
            file_server: Some(file_server),
            download_dir,
        })
    }

    /// 启动消息监听循环
    ///
    /// 在后台任务中自动处理：
    /// - 收到 FileAvailable 时自动下载文件
    /// - 其他消息转发给订阅者
    pub async fn start_file_handler(&self) {
        let mut rx = self.client.subscribe();
        let downloader = self.downloader.clone();
        let download_dir = self.download_dir.clone();

        tokio::spawn(async move {
            while let Ok(msg) = rx.recv().await {
                match msg {
                    ServerMessage::FileAvailable { task_id, from_device: _, ip, port, filename, size } => {
                        println!("收到文件可用通知: task_id={}, from={}:{}, filename={}, size={}",
                            task_id, ip, port, filename, size);

                        // 确定输出路径
                        let output_path = download_dir.join(&filename);

                        // 发起下载（使用默认进度回调）
                        let progress_cb = |downloaded: u64, total: u64| {
                            let pct = if total > 0 { (downloaded as f64 / total as f64) * 100.0 } else { 0.0 };
                            println!("下载进度: {}/{} ({:.1}%)", downloaded, total, pct);
                        };

                        match downloader.download_from_peer(&ip, port, &task_id.to_string(), &output_path, progress_cb).await {
                            Ok(downloaded) => {
                                println!("文件下载完成: {} bytes", downloaded);
                            }
                            Err(e) => {
                                eprintln!("文件下载失败: {}", e);
                            }
                        }
                    }
                    _ => {
                        // 其他消息由订阅者处理
                    }
                }
            }
        });
    }

    /// 注册文件到文件服务器（任务完成时调用）
    ///
    /// 当本地任务完成下载后，调用此方法将文件注册到文件服务器，
    /// 以便其他对等设备可以请求此文件
    pub async fn register_completed_file(&self, task_id: Uuid, local_path: PathBuf) -> Result<()> {
        if let Some(ref server) = self.file_server {
            if let Err(e) = server.register_file(task_id, local_path).await {
                warn!("文件注册失败: {:?}", e);
            } else {
                println!("文件已注册到服务器: task_id={}", task_id);
            }
        }
        Ok(())
    }

    /// 获取内部的协作客户端（用于发送消息）
    pub fn client(&self) -> &CollaborationClient {
        &self.client
    }

    /// 获取文件服务器实例
    pub fn file_server(&self) -> Option<&Arc<crate::collaboration::transfer::http_server::FileServer>> {
        self.file_server.as_ref()
    }
}
