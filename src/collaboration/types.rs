//! 协作模块核心类型定义

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

/// 团队
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct Team {
    pub id: Uuid,
    pub name: String,
    pub invite_code: String,
    pub created_at: DateTime<Utc>,
}

/// 设备
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    pub id: Uuid,
    pub team_id: Uuid,
    pub name: String,
    pub public_ip: Option<String>,
    pub public_port: Option<u16>,
    pub last_seen: DateTime<Utc>,
    pub is_online: bool,
}

/// 任务状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TaskStatus {
    #[default]
    New,
    Queued,
    Claimed,
    Complete,
    Failed,
}

/// 下载任务
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: Uuid,
    pub team_id: Uuid,
    pub url: String,
    pub status: TaskStatus,
    pub claimed_by: Option<Uuid>,
    pub claimed_at: Option<DateTime<Utc>>,
    pub progress: f64,
    pub local_path: Option<PathBuf>,
    pub file_size: Option<u64>,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub version: u64,
}

impl Task {
    pub fn new(url: String, team_id: Uuid, created_by: Uuid) -> Self {
        Self {
            id: Uuid::new_v4(),
            team_id,
            url,
            status: TaskStatus::New,
            claimed_by: None,
            claimed_at: None,
            progress: 0.0,
            local_path: None,
            file_size: None,
            created_by,
            created_at: Utc::now(),
            version: 1,
        }
    }
}

/// 客户端消息 (设备 -> 服务器)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMessage {
    Register {
        device_id: Uuid,
        team_id: Uuid,
        name: String,
    },
    CreateTeam {
        name: String,
    },
    JoinTeam {
        invite_code: String,
    },
    Heartbeat {
        device_id: Uuid,
    },
    AddTask {
        url: String,
    },
    ClaimTask {
        task_id: Uuid,
    },
    UpdateProgress {
        task_id: Uuid,
        progress: f64,
    },
    TaskComplete {
        task_id: Uuid,
        local_path: PathBuf,
        file_size: u64,
    },
    RequestFile {
        task_id: Uuid,
    },
    LeaveTeam {
        device_id: Uuid,
    },
}

/// 服务器消息 (服务器 -> 设备)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ServerMessage {
    Registered {
        device_id: Uuid,
    },
    TeamCreated {
        team: Team,
    },
    TeamJoined {
        team: Team,
    },
    TeamDevices {
        devices: Vec<Device>,
    },
    TaskAdded {
        task: Task,
    },
    TaskClaimed {
        task_id: Uuid,
        device_id: Uuid,
    },
    TaskUpdated {
        task: Task,
    },
    TaskReleased {
        task_id: Uuid,
        reason: String,
    },
    FileAvailable {
        task_id: Uuid,
        from_device: Uuid,
        ip: String,
        port: u16,
        filename: String,
        size: u64,
    },
    DeviceOnline {
        device: Device,
    },
    DeviceOffline {
        device_id: Uuid,
    },
    Error {
        message: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_new() {
        let device_id = Uuid::new_v4();
        let team_id = Uuid::new_v4();
        let task = Task::new("https://example.com/video".to_string(), team_id, device_id);

        assert_eq!(task.url, "https://example.com/video");
        assert_eq!(task.team_id, team_id);
        assert_eq!(task.status, TaskStatus::New);
        assert_eq!(task.claimed_by, None);
        assert_eq!(task.progress, 0.0);
        assert_eq!(task.version, 1);
    }

    #[test]
    fn test_task_status_default() {
        assert_eq!(TaskStatus::default(), TaskStatus::New);
    }

    #[test]
    fn test_client_message_serialization() {
        let device_id = Uuid::new_v4();
        let team_id = Uuid::new_v4();
        let msg = ClientMessage::Register {
            device_id,
            team_id,
            name: "test".to_string(),
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"Register\""));
        assert!(json.contains("\"device_id\""));
        assert!(json.contains("\"team_id\""));
    }

    #[test]
    fn test_server_message_serialization() {
        let device_id = Uuid::new_v4();
        let msg = ServerMessage::Registered { device_id };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"Registered\""));
    }

    #[test]
    fn test_task_serialization() {
        let device_id = Uuid::new_v4();
        let team_id = Uuid::new_v4();
        let task = Task::new("https://example.com/video".to_string(), team_id, device_id);

        let json = serde_json::to_string(&task).unwrap();
        assert!(json.contains("https://example.com/video"));
        assert!(json.contains("\"status\":\"New\""));
    }
}
