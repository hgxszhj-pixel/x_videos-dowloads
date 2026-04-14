//! SQLite 数据库层

use crate::collaboration::types::{Device, Task, TaskStatus, Team};
use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use uuid::Uuid;

/// 验证 IP 地址格式
fn validate_ip(ip: &str) -> Result<()> {
    ip.parse::<IpAddr>()
        .map(|_| ())
        .map_err(|_| anyhow!("Invalid IP address: {}", ip))
}

/// 验证端口号 (1-65535)
fn validate_port(port: u16) -> Result<()> {
    if port == 0 {
        return Err(anyhow!("Port must be between 1 and 65535"));
    }
    Ok(())
}

/// 将 uuid::Error 转换为 rusqlite::Error（用于 query_map 闭包内）
fn uuid_err_to_rusqlite(_: uuid::Error) -> rusqlite::Error {
    rusqlite::Error::InvalidParameterName("Invalid UUID in database".to_string())
}

/// 数据库封装
#[allow(dead_code)]
pub struct Database {
    conn: Mutex<Connection>,
}

#[allow(dead_code)]
impl Database {
    /// 打开或创建数据库
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL;")?; // 启用 WAL 模式
        let db = Self {
            conn: Mutex::new(conn),
        };
        db.init_schema()?;
        Ok(db)
    }

    /// 初始化表结构
    fn init_schema(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS teams (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                invite_code TEXT NOT NULL UNIQUE,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS devices (
                id TEXT PRIMARY KEY,
                team_id TEXT NOT NULL,
                name TEXT NOT NULL,
                public_ip TEXT,
                public_port INTEGER,
                last_seen TEXT NOT NULL,
                is_online INTEGER NOT NULL DEFAULT 0,
                FOREIGN KEY (team_id) REFERENCES teams(id)
            );

            CREATE TABLE IF NOT EXISTS tasks (
                id TEXT PRIMARY KEY,
                team_id TEXT NOT NULL,
                url TEXT NOT NULL UNIQUE,
                status TEXT NOT NULL,
                claimed_by TEXT,
                claimed_at TEXT,
                progress REAL NOT NULL DEFAULT 0,
                local_path TEXT,
                file_size INTEGER,
                created_by TEXT NOT NULL,
                created_at TEXT NOT NULL,
                version INTEGER NOT NULL DEFAULT 1,
                FOREIGN KEY (team_id) REFERENCES teams(id)
            );

            CREATE TABLE IF NOT EXISTS offline_messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                device_id TEXT NOT NULL,
                message TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_devices_team ON devices(team_id);
            CREATE INDEX IF NOT EXISTS idx_tasks_team ON tasks(team_id);
            CREATE INDEX IF NOT EXISTS idx_offline_messages_device ON offline_messages(device_id);
            "#,
        )?;
        Ok(())
    }

    // ========== Team 操作 ==========

    /// 创建团队
    pub fn create_team(&self, name: &str) -> Result<Team> {
        let team = Team {
            id: Uuid::new_v4(),
            name: name.to_string(),
            invite_code: generate_invite_code(),
            created_at: Utc::now(),
        };
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO teams (id, name, invite_code, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![
                team.id.to_string(),
                team.name,
                team.invite_code,
                team.created_at.to_rfc3339()
            ],
        )?;
        Ok(team)
    }

    /// 通过邀请码获取团队
    pub fn get_team_by_code(&self, invite_code: &str) -> Result<Option<Team>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT id, name, invite_code, created_at FROM teams WHERE invite_code = ?1")?;
        let mut rows = stmt.query(params![invite_code])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Team {
                id: Uuid::parse_str(&row.get::<_, String>(0)?)?,
                name: row.get(1)?,
                invite_code: row.get(2)?,
                created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(3)?)
                    .unwrap()
                    .with_timezone(&Utc),
            }))
        } else {
            Ok(None)
        }
    }

    // ========== Device 操作 ==========

    /// 注册设备
    pub fn register_device(&self, device: &Device) -> Result<()> {
        // 验证 IP 地址格式
        if let Some(ref ip) = device.public_ip {
            validate_ip(ip)?;
        }
        // 验证端口号
        if let Some(port) = device.public_port {
            validate_port(port)?;
        }
        let conn = self.conn.lock().unwrap();
        conn.execute(
            r#"INSERT INTO devices (id, team_id, name, public_ip, public_port, last_seen, is_online)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
               ON CONFLICT(id) DO UPDATE SET
                   name = excluded.name,
                   public_ip = excluded.public_ip,
                   public_port = excluded.public_port,
                   last_seen = excluded.last_seen,
                   is_online = excluded.is_online"#,
            params![
                device.id.to_string(),
                device.team_id.to_string(),
                device.name,
                device.public_ip,
                device.public_port,
                device.last_seen.to_rfc3339(),
                device.is_online as i32
            ],
        )?;
        Ok(())
    }

    /// 更新设备心跳
    pub fn update_device_heartbeat(&self, device_id: Uuid) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE devices SET last_seen = ?1, is_online = 1 WHERE id = ?2",
            params![Utc::now().to_rfc3339(), device_id.to_string()],
        )?;
        Ok(())
    }

    /// 获取团队的所有设备
    pub fn get_team_devices(&self, team_id: Uuid) -> Result<Vec<Device>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, team_id, name, public_ip, public_port, last_seen, is_online FROM devices WHERE team_id = ?1",
        )?;
        let rows = stmt.query_map(params![team_id.to_string()], |row| {
            Ok(Device {
                id: Uuid::parse_str(&row.get::<_, String>(0)?)
                    .map_err(|_| rusqlite::Error::InvalidParameterName("Invalid UUID in database".to_string()))?,
                team_id: Uuid::parse_str(&row.get::<_, String>(1)?)
                    .map_err(|_| rusqlite::Error::InvalidParameterName("Invalid UUID in database".to_string()))?,
                name: row.get(2)?,
                public_ip: row.get(3)?,
                public_port: row.get(4)?,
                last_seen: DateTime::parse_from_rfc3339(&row.get::<_, String>(5)?)
                    .unwrap()
                    .with_timezone(&Utc),
                is_online: row.get::<_, i32>(6)? != 0,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// 获取设备所属团队ID
    pub fn get_device_team_id(&self, device_id: Uuid) -> Result<Option<Uuid>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT team_id FROM devices WHERE id = ?1")?;
        let mut rows = stmt.query(params![device_id.to_string()])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Uuid::parse_str(&row.get::<_, String>(0)?)?))
        } else {
            Ok(None)
        }
    }

    /// 标记设备离线
    pub fn set_device_offline(&self, device_id: Uuid) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE devices SET is_online = 0 WHERE id = ?1",
            params![device_id.to_string()],
        )?;
        Ok(())
    }

    /// 获取设备信息
    #[allow(dead_code)]
    pub fn get_device(&self, device_id: Uuid) -> Result<Option<Device>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, team_id, name, public_ip, public_port, last_seen, is_online FROM devices WHERE id = ?1",
        )?;
        let mut rows = stmt.query(params![device_id.to_string()])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Device {
                id: Uuid::parse_str(&row.get::<_, String>(0)?)
                    .map_err(|_| rusqlite::Error::InvalidParameterName("Invalid UUID in database".to_string()))?,
                team_id: Uuid::parse_str(&row.get::<_, String>(1)?)
                    .map_err(|_| rusqlite::Error::InvalidParameterName("Invalid UUID in database".to_string()))?,
                name: row.get(2)?,
                public_ip: row.get(3)?,
                public_port: row.get(4)?,
                last_seen: DateTime::parse_from_rfc3339(&row.get::<_, String>(5)?)
                    .unwrap()
                    .with_timezone(&Utc),
                is_online: row.get::<_, i32>(6)? != 0,
            }))
        } else {
            Ok(None)
        }
    }

    // ========== Task 操作 ==========

    /// 创建任务
    pub fn create_task(&self, task: &Task, team_id: Uuid) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let result = conn.execute(
            r#"INSERT INTO tasks (id, team_id, url, status, claimed_by, claimed_at, progress, local_path, file_size, created_by, created_at, version)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)"#,
            params![
                task.id.to_string(),
                team_id.to_string(),
                task.url,
                format!("{:?}", task.status),
                task.claimed_by.map(|id| id.to_string()),
                task.claimed_at.map(|dt| dt.to_rfc3339()),
                task.progress,
                task.local_path.as_ref().map(|p| p.to_string_lossy().to_string()),
                task.file_size.map(|s| s as i64),
                task.created_by.to_string(),
                task.created_at.to_rfc3339(),
                task.version as i64
            ],
        );

        match result {
            Ok(_) => Ok(()),
            Err(rusqlite::Error::SqliteFailure(_, Some(msg))) if msg.contains("UNIQUE constraint failed") => {
                Err(anyhow!("URL already exists"))
            }
            Err(e) => Err(e.into()),
        }
    }

    /// 更新任务
    pub fn update_task(&self, task: &Task, _team_id: Uuid) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            r#"UPDATE tasks SET
                   status = ?1, claimed_by = ?2, claimed_at = ?3, progress = ?4,
                   local_path = ?5, file_size = ?6, version = ?7
               WHERE id = ?8"#,
            params![
                format!("{:?}", task.status),
                task.claimed_by.map(|id| id.to_string()),
                task.claimed_at.map(|dt| dt.to_rfc3339()),
                task.progress,
                task.local_path.as_ref().map(|p| p.to_string_lossy().to_string()),
                task.file_size.map(|s| s as i64),
                task.version as i64,
                task.id.to_string()
            ],
        )?;
        Ok(())
    }

    /// 获取团队的所有任务
    pub fn get_tasks_by_team(&self, team_id: Uuid) -> Result<Vec<Task>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, team_id, url, status, claimed_by, claimed_at, progress, local_path, file_size, created_by, created_at, version FROM tasks WHERE team_id = ?1",
        )?;
        let rows = stmt.query_map(params![team_id.to_string()], |row| {
            let status_str: String = row.get(3)?;
            let status = match status_str.as_str() {
                "New" => TaskStatus::New,
                "Queued" => TaskStatus::Queued,
                "Claimed" => TaskStatus::Claimed,
                "Complete" => TaskStatus::Complete,
                "Failed" => TaskStatus::Failed,
                _ => TaskStatus::New,
            };
            Ok(Task {
                id: Uuid::parse_str(&row.get::<_, String>(0)?)
                    .map_err(uuid_err_to_rusqlite)?,
                team_id: Uuid::parse_str(&row.get::<_, String>(1)?)
                    .map_err(uuid_err_to_rusqlite)?,
                url: row.get(2)?,
                status,
                claimed_by: row
                    .get::<_, Option<String>>(4)?
                    .map(|s| Uuid::parse_str(&s).map_err(uuid_err_to_rusqlite))
                    .transpose()?,
                claimed_at: row
                    .get::<_, Option<String>>(5)?
                    .map(|s| DateTime::parse_from_rfc3339(&s).unwrap().with_timezone(&Utc)),
                progress: row.get(6)?,
                local_path: row.get::<_, Option<String>>(7)?.map(PathBuf::from),
                file_size: row.get::<_, Option<i64>>(8)?.map(|s| s as u64),
                created_by: Uuid::parse_str(&row.get::<_, String>(9)?)
                    .map_err(uuid_err_to_rusqlite)?,
                created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(10)?)
                    .unwrap()
                    .with_timezone(&Utc),
                version: row.get::<_, i64>(11)? as u64,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// 获取设备认领的所有任务
    pub fn get_tasks_by_device(&self, device_id: Uuid) -> Result<Vec<Task>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, team_id, url, status, claimed_by, claimed_at, progress, local_path, file_size, created_by, created_at, version FROM tasks WHERE claimed_by = ?1",
        )?;
        let rows = stmt.query_map(params![device_id.to_string()], |row| {
            let status_str: String = row.get(3)?;
            let status = match status_str.as_str() {
                "New" => TaskStatus::New,
                "Queued" => TaskStatus::Queued,
                "Claimed" => TaskStatus::Claimed,
                "Complete" => TaskStatus::Complete,
                "Failed" => TaskStatus::Failed,
                _ => TaskStatus::New,
            };
            Ok(Task {
                id: Uuid::parse_str(&row.get::<_, String>(0)?)
                    .map_err(uuid_err_to_rusqlite)?,
                team_id: Uuid::parse_str(&row.get::<_, String>(1)?)
                    .map_err(uuid_err_to_rusqlite)?,
                url: row.get(2)?,
                status,
                claimed_by: row
                    .get::<_, Option<String>>(4)?
                    .map(|s| Uuid::parse_str(&s).map_err(uuid_err_to_rusqlite))
                    .transpose()?,
                claimed_at: row
                    .get::<_, Option<String>>(5)?
                    .map(|s| DateTime::parse_from_rfc3339(&s).unwrap().with_timezone(&Utc)),
                progress: row.get(6)?,
                local_path: row.get::<_, Option<String>>(7)?.map(PathBuf::from),
                file_size: row.get::<_, Option<i64>>(8)?.map(|s| s as u64),
                created_by: Uuid::parse_str(&row.get::<_, String>(9)?)
                    .map_err(uuid_err_to_rusqlite)?,
                created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(10)?)
                    .unwrap()
                    .with_timezone(&Utc),
                version: row.get::<_, i64>(11)? as u64,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// 获取被认领但可能超时的任务
    pub fn get_claimed_tasks(&self, team_id: Uuid) -> Result<Vec<Task>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, team_id, url, status, claimed_by, claimed_at, progress, local_path, file_size, created_by, created_at, version FROM tasks WHERE team_id = ?1 AND status = 'Claimed'",
        )?;
        let rows = stmt.query_map(params![team_id.to_string()], |row| {
            let status_str: String = row.get(3)?;
            let status = match status_str.as_str() {
                "New" => TaskStatus::New,
                "Queued" => TaskStatus::Queued,
                "Claimed" => TaskStatus::Claimed,
                "Complete" => TaskStatus::Complete,
                "Failed" => TaskStatus::Failed,
                _ => TaskStatus::New,
            };
            Ok(Task {
                id: Uuid::parse_str(&row.get::<_, String>(0)?)
                    .map_err(uuid_err_to_rusqlite)?,
                team_id: Uuid::parse_str(&row.get::<_, String>(1)?)
                    .map_err(uuid_err_to_rusqlite)?,
                url: row.get(2)?,
                status,
                claimed_by: row
                    .get::<_, Option<String>>(4)?
                    .map(|s| Uuid::parse_str(&s).map_err(uuid_err_to_rusqlite))
                    .transpose()?,
                claimed_at: row
                    .get::<_, Option<String>>(5)?
                    .map(|s| DateTime::parse_from_rfc3339(&s).unwrap().with_timezone(&Utc)),
                progress: row.get(6)?,
                local_path: row.get::<_, Option<String>>(7)?.map(PathBuf::from),
                file_size: row.get::<_, Option<i64>>(8)?.map(|s| s as u64),
                created_by: Uuid::parse_str(&row.get::<_, String>(9)?)
                    .map_err(uuid_err_to_rusqlite)?,
                created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(10)?)
                    .unwrap()
                    .with_timezone(&Utc),
                version: row.get::<_, i64>(11)? as u64,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// 释放任务（将其重置为 Queued 状态）
    pub fn release_task(&self, task_id: Uuid) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE tasks SET status = 'Queued', claimed_by = NULL, claimed_at = NULL, progress = 0, version = version + 1 WHERE id = ?1",
            params![task_id.to_string()],
        )?;
        Ok(())
    }

    /// 获取所有团队ID
    pub fn get_all_team_ids(&self) -> Result<Vec<Uuid>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT DISTINCT team_id FROM devices")?;
        let rows = stmt.query_map([], |row| {
            Uuid::parse_str(&row.get::<_, String>(0)?)
                .map_err(uuid_err_to_rusqlite)
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }


    // ========== 离线消息 ==========

    /// 保存离线消息
    pub fn save_offline_message(&self, device_id: Uuid, message: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO offline_messages (device_id, message, created_at) VALUES (?1, ?2, ?3)",
            params![device_id.to_string(), message, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    /// 获取设备的离线消息
    pub fn get_offline_messages(&self, device_id: Uuid) -> Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT message FROM offline_messages WHERE device_id = ?1 ORDER BY created_at")?;
        let rows = stmt.query_map(params![device_id.to_string()], |row| row.get(0))?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// 清除设备的离线消息
    pub fn clear_offline_messages(&self, device_id: Uuid) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM offline_messages WHERE device_id = ?1",
            params![device_id.to_string()],
        )?;
        Ok(())
    }
}

/// 生成 16 位高熵邀请码（使用密码学安全的 OsRng）
/// 字符集: 0-9, A-Z, a-z, !@#$%^&*  (共 50 个字符)
fn generate_invite_code() -> String {
    use rand::Rng;
    const CHARSET: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz!@#$%^&*";
    let mut rng = rand::rngs::OsRng;
    (0..16)
        .map(|_| {
            let i = rng.gen_range(0..CHARSET.len());
            CHARSET[i] as char
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_db() -> Database {
        let temp_dir = std::env::temp_dir();
        let db_path = temp_dir.join(format!("test_db_{}.db", Uuid::new_v4()));
        Database::open(db_path.as_path()).expect("Failed to create test database")
    }

    #[test]
    fn test_create_team() {
        let db = create_test_db();
        let team = db.create_team("Test Team").expect("Failed to create team");

        assert_eq!(team.name, "Test Team");
        assert_eq!(team.invite_code.len(), 16);
        assert!(team.invite_code.chars().all(|c| c.is_ascii_alphanumeric() || "!@#$%^&*".contains(c)));
    }

    #[test]
    fn test_get_team_by_code() {
        let db = create_test_db();
        let created = db.create_team("Test Team").expect("Failed to create team");

        let found = db.get_team_by_code(&created.invite_code)
            .expect("Failed to get team by code")
            .expect("Team not found");

        assert_eq!(found.id, created.id);
        assert_eq!(found.name, "Test Team");
    }

    #[test]
    fn test_get_team_by_code_not_found() {
        let db = create_test_db();
        let found = db.get_team_by_code("NONEXISTENT")
            .expect("Failed to query team");
        assert!(found.is_none());
    }

    #[test]
    fn test_register_device() {
        let db = create_test_db();
        let team = db.create_team("Test Team").expect("Failed to create team");

        let device = Device {
            id: Uuid::new_v4(),
            team_id: team.id,
            name: "Test Device".to_string(),
            public_ip: Some("192.168.1.1".to_string()),
            public_port: Some(8080),
            last_seen: Utc::now(),
            is_online: true,
        };

        db.register_device(&device).expect("Failed to register device");

        let devices = db.get_team_devices(team.id).expect("Failed to get team devices");
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].name, "Test Device");
    }

    #[test]
    fn test_update_device_heartbeat() {
        let db = create_test_db();
        let team = db.create_team("Test Team").expect("Failed to create team");

        let device = Device {
            id: Uuid::new_v4(),
            team_id: team.id,
            name: "Test Device".to_string(),
            public_ip: None,
            public_port: None,
            last_seen: Utc::now(),
            is_online: true,
        };

        db.register_device(&device).expect("Failed to register device");
        db.update_device_heartbeat(device.id).expect("Failed to update heartbeat");

        let retrieved = db.get_device(device.id)
            .expect("Failed to get device")
            .expect("Device not found");
        assert!(retrieved.is_online);
    }

    #[test]
    fn test_set_device_offline() {
        let db = create_test_db();
        let team = db.create_team("Test Team").expect("Failed to create team");

        let device = Device {
            id: Uuid::new_v4(),
            team_id: team.id,
            name: "Test Device".to_string(),
            public_ip: None,
            public_port: None,
            last_seen: Utc::now(),
            is_online: true,
        };

        db.register_device(&device).expect("Failed to register device");
        db.set_device_offline(device.id).expect("Failed to set device offline");

        let retrieved = db.get_device(device.id)
            .expect("Failed to get device")
            .expect("Device not found");
        assert!(!retrieved.is_online);
    }

    #[test]
    fn test_create_and_get_task() {
        let db = create_test_db();
        let team = db.create_team("Test Team").expect("Failed to create team");
        let device_id = Uuid::new_v4();

        let task = Task::new("https://example.com/video".to_string(), team.id, device_id);
        db.create_task(&task, team.id).expect("Failed to create task");

        let tasks = db.get_tasks_by_team(team.id).expect("Failed to get tasks");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].url, "https://example.com/video");
        assert_eq!(tasks[0].status, TaskStatus::New);
    }

    #[test]
    fn test_update_task() {
        let db = create_test_db();
        let team = db.create_team("Test Team").expect("Failed to create team");
        let device_id = Uuid::new_v4();

        let mut task = Task::new("https://example.com/video".to_string(), team.id, device_id);
        db.create_task(&task, team.id).expect("Failed to create task");

        task.status = TaskStatus::Claimed;
        task.claimed_by = Some(device_id);
        task.progress = 50.0;
        db.update_task(&task, team.id).expect("Failed to update task");

        let tasks = db.get_tasks_by_team(team.id).expect("Failed to get tasks");
        assert_eq!(tasks[0].status, TaskStatus::Claimed);
        assert_eq!(tasks[0].progress, 50.0);
    }

    #[test]
    fn test_release_task() {
        let db = create_test_db();
        let team = db.create_team("Test Team").expect("Failed to create team");
        let device_id = Uuid::new_v4();

        let mut task = Task::new("https://example.com/video".to_string(), team.id, device_id);
        task.status = TaskStatus::Claimed;
        task.claimed_by = Some(device_id);
        db.create_task(&task, team.id).expect("Failed to create task");

        db.release_task(task.id).expect("Failed to release task");

        let tasks = db.get_tasks_by_team(team.id).expect("Failed to get tasks");
        assert_eq!(tasks[0].status, TaskStatus::Queued);
        assert!(tasks[0].claimed_by.is_none());
    }

    #[test]
    fn test_get_tasks_by_device() {
        let db = create_test_db();
        let team = db.create_team("Test Team").expect("Failed to create team");
        let device_id = Uuid::new_v4();

        let mut task = Task::new("https://example.com/video1".to_string(), team.id, device_id);
        task.status = TaskStatus::Claimed;
        task.claimed_by = Some(device_id);
        db.create_task(&task, team.id).expect("Failed to create task");

        let task2 = Task::new("https://example.com/video2".to_string(), team.id, device_id);
        db.create_task(&task2, team.id).expect("Failed to create task");

        let tasks = db.get_tasks_by_device(device_id).expect("Failed to get tasks by device");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].url, "https://example.com/video1");
    }

    #[test]
    fn test_offline_messages() {
        let db = create_test_db();
        let device_id = Uuid::new_v4();

        db.save_offline_message(device_id, "Message 1").expect("Failed to save message");
        db.save_offline_message(device_id, "Message 2").expect("Failed to save message");

        let messages = db.get_offline_messages(device_id).expect("Failed to get messages");
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0], "Message 1");
        assert_eq!(messages[1], "Message 2");

        db.clear_offline_messages(device_id).expect("Failed to clear messages");
        let messages = db.get_offline_messages(device_id).expect("Failed to get messages");
        assert!(messages.is_empty());
    }

    #[test]
    fn test_invite_code_is_unique() {
        let db = create_test_db();
        let team1 = db.create_team("Team 1").expect("Failed to create team 1");
        let team2 = db.create_team("Team 2").expect("Failed to create team 2");

        assert_ne!(team1.invite_code, team2.invite_code);
    }

    #[test]
    fn test_invite_code_format() {
        let db = create_test_db();
        let team = db.create_team("Test Team").expect("Failed to create team");

        // 邀请码应该是 16 字符
        assert_eq!(team.invite_code.len(), 16);

        // 邀请码应该包含字母、数字和特殊字符
        assert!(team.invite_code.chars().all(|c| c.is_ascii_alphanumeric() || "!@#$%^&*".contains(c)));
    }

    #[test]
    fn test_get_device_team_id() {
        let db = create_test_db();
        let team = db.create_team("Test Team").expect("Failed to create team");

        let device = Device {
            id: Uuid::new_v4(),
            team_id: team.id,
            name: "Test Device".to_string(),
            public_ip: None,
            public_port: None,
            last_seen: Utc::now(),
            is_online: true,
        };

        db.register_device(&device).expect("Failed to register device");

        let found_team_id = db.get_device_team_id(device.id)
            .expect("Failed to get device team id")
            .expect("Team ID should not be None");
        assert_eq!(found_team_id, team.id);
    }

    #[test]
    fn test_get_device_team_id_not_found() {
        let db = create_test_db();
        let fake_id = Uuid::new_v4();

        let found = db.get_device_team_id(fake_id)
            .expect("Failed to query device team id");
        assert!(found.is_none());
    }

    #[test]
    fn test_get_all_team_ids() {
        let db = create_test_db();
        let team1 = db.create_team("Team 1").expect("Failed to create team 1");
        let team2 = db.create_team("Team 2").expect("Failed to create team 2");

        let device1 = Device {
            id: Uuid::new_v4(),
            team_id: team1.id,
            name: "Device 1".to_string(),
            public_ip: None,
            public_port: None,
            last_seen: Utc::now(),
            is_online: true,
        };
        let device2 = Device {
            id: Uuid::new_v4(),
            team_id: team2.id,
            name: "Device 2".to_string(),
            public_ip: None,
            public_port: None,
            last_seen: Utc::now(),
            is_online: true,
        };

        db.register_device(&device1).expect("Failed to register device1");
        db.register_device(&device2).expect("Failed to register device2");

        let team_ids = db.get_all_team_ids().expect("Failed to get all team ids");
        assert_eq!(team_ids.len(), 2);
        assert!(team_ids.contains(&team1.id));
        assert!(team_ids.contains(&team2.id));
    }

    #[test]
    fn test_get_claimed_tasks() {
        let db = create_test_db();
        let team = db.create_team("Test Team").expect("Failed to create team");
        let device_id = Uuid::new_v4();

        // 创建多个任务
        let mut task1 = Task::new("https://example.com/video1".to_string(), team.id, device_id);
        task1.status = TaskStatus::Claimed;
        task1.claimed_by = Some(device_id);
        db.create_task(&task1, team.id).expect("Failed to create task1");

        let task2 = Task::new("https://example.com/video2".to_string(), team.id, device_id);
        db.create_task(&task2, team.id).expect("Failed to create task2");

        let mut task3 = Task::new("https://example.com/video3".to_string(), team.id, device_id);
        task3.status = TaskStatus::Claimed;
        task3.claimed_by = Some(device_id);
        db.create_task(&task3, team.id).expect("Failed to create task3");

        let claimed = db.get_claimed_tasks(team.id).expect("Failed to get claimed tasks");
        assert_eq!(claimed.len(), 2);
    }

    #[test]
    fn test_get_tasks_by_team_empty() {
        let db = create_test_db();
        let team = db.create_team("Test Team").expect("Failed to create team");

        let tasks = db.get_tasks_by_team(team.id).expect("Failed to get tasks");
        assert!(tasks.is_empty());
    }

    #[test]
    fn test_multiple_devices_same_team() {
        let db = create_test_db();
        let team = db.create_team("Test Team").expect("Failed to create team");

        for i in 0..5 {
            let device = Device {
                id: Uuid::new_v4(),
                team_id: team.id,
                name: format!("Device {}", i),
                public_ip: None,
                public_port: None,
                last_seen: Utc::now(),
                is_online: true,
            };
            db.register_device(&device).expect("Failed to register device");
        }

let devices = db.get_team_devices(team.id).expect("Failed to get team devices");
        assert_eq!(devices.len(), 5);
    }

    #[test]
    fn test_create_task_duplicate_url() {
        let db = create_test_db();
        let team = db.create_team("Test Team").expect("Failed to create team");
        let device_id = Uuid::new_v4();
        let url = "https://example.com/video".to_string();

        // 创建第一个任务
        let task1 = Task::new(url.clone(), team.id, device_id);
        db.create_task(&task1, team.id).expect("First task should be created");

        // 尝试创建相同 URL 的任务应该失败
        let task2 = Task::new(url, team.id, device_id);
        let result = db.create_task(&task2, team.id);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("URL already exists"));
    }
}
