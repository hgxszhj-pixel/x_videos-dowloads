//! 消息处理器

use crate::collaboration::crypto::hashring::HashRing;
use crate::collaboration::server::db::Database;
use crate::collaboration::types::{
    ClientMessage, ServerMessage, Task, TaskStatus,
};
use anyhow::Result;
use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

/// WebSocket 连接会话信息
struct Session {
    device_id: Uuid,
    team_id: Uuid,
    tx: broadcast::Sender<String>,
}

/// 消息处理器
pub struct MessageHandler {
    db: Arc<Database>,
    sessions: Arc<RwLock<HashMap<String, Session>>>,
    hash_ring: Arc<RwLock<HashRing>>,
}

impl MessageHandler {
    pub fn new(db: Arc<Database>) -> Self {
        Self {
            db,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            hash_ring: Arc::new(RwLock::new(HashRing::new())),
        }
    }

    /// 注册客户端连接
    pub async fn register_client(
        &self,
        key: String,
        device_id: Uuid,
        team_id: Uuid,
        tx: broadcast::Sender<String>,
    ) {
        // 如果 device_id 已存在，移除旧 session
        if device_id != Uuid::nil() {
            let mut sessions = self.sessions.write().await;
            // 找出所有该 device_id 的旧 key 并删除
            let keys_to_remove: Vec<_> = sessions
                .iter()
                .filter(|(_, s)| s.device_id == device_id)
                .map(|(k, _)| k.clone())
                .collect();
            for k in keys_to_remove {
                sessions.remove(&k);
            }
        }

        self.sessions.write().await.insert(
            key,
            Session {
                device_id,
                team_id,
                tx,
            },
        );
    }

    /// 更新 session 的设备信息
    pub async fn update_session(&self, key: &str, device_id: Uuid, team_id: Uuid) {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(key) {
            session.device_id = device_id;
            session.team_id = team_id;
        }
    }

    /// 注销客户端
    pub async fn unregister_client_by_key(&self, key: &str) {
        self.sessions.write().await.remove(key);
    }

    /// 通过设备ID发送消息
    pub async fn send_to_device(&self, device_id: Uuid, message: ServerMessage) -> Result<()> {
        let json = serde_json::to_string(&message)?;
        let sessions = self.sessions.read().await;

        for (_, session) in sessions.iter() {
            if session.device_id == device_id {
                // 忽略发送错误（客户端可能已断开）
                let _ = session.tx.send(json.clone());
            }
        }
        Ok(())
    }

    /// 广播给团队所有设备
    pub async fn broadcast_to_team(&self, team_id: Uuid, message: ServerMessage) -> Result<()> {
        let json = serde_json::to_string(&message)?;
        let sessions = self.sessions.read().await;

        for (_, session) in sessions.iter() {
            if session.team_id == team_id {
                let _ = session.tx.send(json.clone());
            }
        }
        Ok(())
    }

    /// 处理客户端消息
    pub async fn handle_message(&self, key: &str, msg: ClientMessage) {
        let (device_id, team_id) = {
            let sessions = self.sessions.read().await;
            match sessions.get(key) {
                Some(s) => (s.device_id, s.team_id),
                None => {
                    eprintln!("未注册的 session: {}", key);
                    return;
                }
            }
        };

        match msg {
            ClientMessage::Register { .. } => {
                // 已在新连接时处理
            }
            ClientMessage::CreateTeam { name } => {
                self.handle_create_team(key, name).await;
            }
            ClientMessage::JoinTeam { invite_code } => {
                self.handle_join_team(key, invite_code).await;
            }
            ClientMessage::Heartbeat { .. } => {
                self.handle_heartbeat(device_id).await;
            }
            ClientMessage::AddTask { url } => {
                self.handle_add_task(device_id, team_id, url).await;
            }
            ClientMessage::ClaimTask { task_id } => {
                self.handle_claim_task(device_id, team_id, task_id).await;
            }
            ClientMessage::UpdateProgress { task_id, progress } => {
                self.handle_update_progress(device_id, team_id, task_id, progress).await;
            }
            ClientMessage::TaskComplete {
                task_id,
                local_path,
                file_size,
            } => {
                self.handle_task_complete(device_id, team_id, task_id, local_path, file_size)
                    .await;
            }
            ClientMessage::RequestFile { task_id } => {
                self.handle_request_file(device_id, team_id, task_id).await;
            }
            ClientMessage::LeaveTeam { .. } => {
                self.handle_leave_team(device_id).await;
            }
        }
    }

    /// 处理心跳消息
    async fn handle_heartbeat(&self, device_id: Uuid) {
        if let Err(e) = self.db.update_device_heartbeat(device_id) {
            eprintln!("更新心跳失败: {}", e);
        }
    }

    /// 处理创建团队消息
    async fn handle_create_team(&self, key: &str, name: String) {
        // 创建团队
        let team = match self.db.create_team(&name) {
            Ok(t) => t,
            Err(e) => {
                let _ = self.send_error(key, format!("创建团队失败: {}", e)).await;
                return;
            }
        };

        // 更新 session 的 team_id（避免死锁：不在持有读锁时获取写锁）
        let device_id = {
            let sessions = self.sessions.read().await;
            sessions.get(key).map(|s| s.device_id)
        };

        if let Some(device_id) = device_id {
            // 获取写锁并更新
            let mut sessions_write = self.sessions.write().await;
            if let Some(s) = sessions_write.get_mut(key) {
                s.team_id = team.id;
            }
            drop(sessions_write);

            let _ = self.send_to_device(
                device_id,
                ServerMessage::TeamCreated { team: team.clone() },
            ).await;
        }

        // 注册设备到团队
        // 注意: public_ip 和 public_port 在设备加入时通过 STUN 检测获取
        // 此处设为 None，后续设备 Register 时会更新
        let device = crate::collaboration::types::Device {
            id: Uuid::nil(), // 临时，Register 时会更新
            team_id: team.id,
            name: name.clone(),
            public_ip: None, // TODO: 创建团队时可选择进行 STUN 检测
            public_port: None,
            last_seen: chrono::Utc::now(),
            is_online: true,
        };
        if let Err(e) = self.db.register_device(&device) {
            eprintln!("注册设备失败: {}", e);
        }
    }

    /// 处理加入团队消息
    async fn handle_join_team(&self, key: &str, invite_code: String) {
        // 通过邀请码查找团队
        let team = match self.db.get_team_by_code(&invite_code) {
            Ok(Some(t)) => t,
            Ok(None) => {
                let _ = self.send_error(key, "邀请码无效".to_string()).await;
                return;
            }
            Err(e) => {
                let _ = self.send_error(key, format!("查询团队失败: {}", e)).await;
                return;
            }
        };

        // 更新 session 的 team_id（避免死锁：不在持有读锁时获取写锁）
        let device_id = {
            let sessions = self.sessions.read().await;
            sessions.get(key).map(|s| s.device_id)
        };

        if let Some(device_id) = device_id {
            let mut sessions_write = self.sessions.write().await;
            if let Some(s) = sessions_write.get_mut(key) {
                s.team_id = team.id;
            }
            drop(sessions_write);

            let _ = self.send_to_device(
                device_id,
                ServerMessage::TeamJoined { team: team.clone() },
            ).await;
        }

        // 注册设备到团队
        // 注意: public_ip 和 public_port 应在客户端通过 STUN 检测后更新
        // 此处设为 None，等待客户端 Register 消息时更新
        let device = crate::collaboration::types::Device {
            id: Uuid::nil(),
            team_id: team.id,
            name: "NewDevice".to_string(),
            public_ip: None, // TODO: 加入团队时可选择进行 STUN 检测
            public_port: None,
            last_seen: chrono::Utc::now(),
            is_online: true,
        };
        if let Err(e) = self.db.register_device(&device) {
            eprintln!("注册设备失败: {}", e);
        }
    }

    /// 发送错误消息给客户端
    async fn send_error(&self, key: &str, message: String) -> Result<(), anyhow::Error> {
        let sessions = self.sessions.read().await;
        if let Some(session) = sessions.get(key) {
            let _ = session.tx.send(serde_json::to_string(&ServerMessage::Error { message })?);
        }
        Ok(())
    }

    /// 处理添加任务消息
    async fn handle_add_task(&self, device_id: Uuid, team_id: Uuid, url: String) {
        // 创建任务（数据库 UNIQUE 约束处理 URL 冲突）
        let mut task = Task::new(url, team_id, device_id);
        task.status = TaskStatus::Queued;

        if let Err(e) = self.db.create_task(&task, team_id) {
            // 数据库唯一约束冲突错误
            let error_msg = if e.to_string().contains("URL already exists") {
                "URL already exists".to_string()
            } else {
                format!("Failed to create task: {}", e)
            };
            let _ = self.send_to_device(
                device_id,
                ServerMessage::Error { message: error_msg },
            ).await;
            return;
        }

        // 一致性哈希分配
        if let Some(owner_id) = self.hash_ring.read().await.get_owner(&task.url).await {
            task.status = TaskStatus::Claimed;
            task.claimed_by = Some(owner_id);
            task.claimed_at = Some(Utc::now());
            let _ = self.db.update_task(&task, team_id);

            let _ = self.send_to_device(
                owner_id,
                ServerMessage::TaskClaimed {
                    task_id: task.id,
                    device_id: owner_id,
                },
            ).await;
        }

        // 广播任务
        let _ = self.broadcast_to_team(
            team_id,
            ServerMessage::TaskAdded { task },
        ).await;
    }

    /// 处理认领任务消息
    async fn handle_claim_task(&self, device_id: Uuid, team_id: Uuid, task_id: Uuid) {
        if let Ok(tasks) = self.db.get_tasks_by_team(team_id) {
            if let Some(mut task) = tasks.into_iter().find(|t| t.id == task_id) {
                if task.status != TaskStatus::Queued {
                    let _ = self.send_to_device(
                        device_id,
                        ServerMessage::Error {
                            message: "Task not available".to_string(),
                        },
                    ).await;
                    return;
                }

                task.status = TaskStatus::Claimed;
                task.claimed_by = Some(device_id);
                task.claimed_at = Some(Utc::now());
                task.version += 1;

                if let Err(e) = self.db.update_task(&task, team_id) {
                    eprintln!("更新任务失败: {}", e);
                    return;
                }

                let _ = self.broadcast_to_team(
                    team_id,
                    ServerMessage::TaskClaimed {
                        task_id,
                        device_id,
                    },
                ).await;
                return;
            }
        }

        let _ = self.send_to_device(
            device_id,
            ServerMessage::Error {
                message: "Task not found".to_string(),
            },
        ).await;
    }

    /// 处理更新进度消息
    async fn handle_update_progress(
        &self,
        device_id: Uuid,
        team_id: Uuid,
        task_id: Uuid,
        progress: f64,
    ) {
        if let Ok(tasks) = self.db.get_tasks_by_team(team_id) {
            if let Some(mut task) = tasks.into_iter().find(|t| t.id == task_id) {
                if task.claimed_by != Some(device_id) {
                    return; // 不是认领者，无权更新
                }

                task.progress = progress;
                task.version += 1;

                if let Err(e) = self.db.update_task(&task, team_id) {
                    eprintln!("更新进度失败: {}", e);
                    return;
                }

                let _ = self.broadcast_to_team(
                    team_id,
                    ServerMessage::TaskUpdated { task },
                ).await;
            }
        }
    }

    /// 处理任务完成消息
    async fn handle_task_complete(
        &self,
        device_id: Uuid,
        team_id: Uuid,
        task_id: Uuid,
        local_path: std::path::PathBuf,
        file_size: u64,
    ) {
        if let Ok(tasks) = self.db.get_tasks_by_team(team_id) {
            if let Some(mut task) = tasks.into_iter().find(|t| t.id == task_id) {
                if task.claimed_by != Some(device_id) {
                    return;
                }

                task.status = TaskStatus::Complete;
                task.local_path = Some(local_path);
                task.file_size = Some(file_size);
                task.progress = 1.0;
                task.version += 1;

                if let Err(e) = self.db.update_task(&task, team_id) {
                    eprintln!("更新任务完成状态失败: {}", e);
                    return;
                }

                let _ = self.broadcast_to_team(
                    team_id,
                    ServerMessage::TaskUpdated { task },
                ).await;
            }
        }
    }

    /// 处理文件请求消息
    async fn handle_request_file(&self, device_id: Uuid, team_id: Uuid, task_id: Uuid) {
        if let Ok(tasks) = self.db.get_tasks_by_team(team_id) {
            if let Some(task) = tasks.iter().find(|t| t.id == task_id) {
                if let Some(owner_id) = task.claimed_by {
                    // 获取文件所有者的公网地址
                    let (ip, port) = if let Ok(Some(owner)) = self.db.get_device(owner_id) {
                        (
                            owner.public_ip.unwrap_or_else(|| "0.0.0.0".to_string()),
                            owner.public_port.unwrap_or(0),
                        )
                    } else {
                        ("0.0.0.0".to_string(), 0)
                    };

                    let _ = self.send_to_device(
                        device_id,
                        ServerMessage::FileAvailable {
                            task_id,
                            from_device: owner_id,
                            ip,
                            port,
                            filename: task
                                .local_path
                                .as_ref()
                                .map(|p| p.file_name().unwrap_or_default().to_string_lossy().to_string())
                                .unwrap_or_default(),
                            size: task.file_size.unwrap_or(0),
                        },
                    ).await;
                }
            }
        }
    }

    /// 处理离开团队消息
    async fn handle_leave_team(&self, device_id: Uuid) {
        if let Err(e) = self.db.set_device_offline(device_id) {
            eprintln!("标记设备离线失败: {}", e);
        }
        self.hash_ring.write().await.remove_device(&device_id).await;
    }

    /// 检查所有团队的任务超时
    pub async fn check_all_task_timeouts(&self) {
        let team_ids = match self.db.get_all_team_ids() {
            Ok(ids) => ids,
            Err(e) => {
                eprintln!("获取团队ID失败: {}", e);
                return;
            }
        };

        for team_id in team_ids {
            self.check_task_timeouts(team_id).await;
        }
    }

    /// 检查任务超时（超过5分钟未完成则释放）
    async fn check_task_timeouts(&self, team_id: Uuid) {
        let tasks = match self.db.get_claimed_tasks(team_id) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("获取认领任务失败: {}", e);
                return;
            }
        };

        let now = chrono::Utc::now();
        for task in tasks {
            if let Some(claimed_at) = task.claimed_at {
                let elapsed = now.signed_duration_since(claimed_at);
                if elapsed.num_minutes() >= 5 && task.progress < 1.0 {
                    // 超时，释放任务
                    if let Err(e) = self.db.release_task(task.id) {
                        eprintln!("释放超时任务失败: {}", e);
                        continue;
                    }

                    let _ = self.broadcast_to_team(
                        team_id,
                        ServerMessage::TaskReleased {
                            task_id: task.id,
                            reason: "timeout".to_string(),
                        },
                    ).await;
                }
            }
        }
    }

}
