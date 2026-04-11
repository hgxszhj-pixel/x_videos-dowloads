//! SQLite 数据库层

use crate::collaboration::types::{Device, Task, TaskStatus, Team};
use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use uuid::Uuid;

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
                url TEXT NOT NULL,
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
                id: Uuid::parse_str(&row.get::<_, String>(0)?).expect("Invalid UUID in database"),
                team_id: Uuid::parse_str(&row.get::<_, String>(1)?).expect("Invalid UUID in database"),
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
                id: Uuid::parse_str(&row.get::<_, String>(0)?).expect("Invalid UUID in database"),
                team_id: Uuid::parse_str(&row.get::<_, String>(1)?).expect("Invalid UUID in database"),
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
        conn.execute(
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
        )?;
        Ok(())
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
                id: Uuid::parse_str(&row.get::<_, String>(0)?).expect("Invalid UUID in database"),
                team_id: Uuid::parse_str(&row.get::<_, String>(1)?).expect("Invalid UUID in database"),
                url: row.get(2)?,
                status,
                claimed_by: row.get::<_, Option<String>>(4)?.map(|s| Uuid::parse_str(&s).expect("Invalid claimed_by UUID in database")),
                claimed_at: row
                    .get::<_, Option<String>>(5)?
                    .map(|s| DateTime::parse_from_rfc3339(&s).unwrap().with_timezone(&Utc)),
                progress: row.get(6)?,
                local_path: row.get::<_, Option<String>>(7)?.map(PathBuf::from),
                file_size: row.get::<_, Option<i64>>(8)?.map(|s| s as u64),
                created_by: Uuid::parse_str(&row.get::<_, String>(9)?).expect("Invalid UUID in database"),
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
                id: Uuid::parse_str(&row.get::<_, String>(0)?).expect("Invalid UUID in database"),
                team_id: Uuid::parse_str(&row.get::<_, String>(1)?).expect("Invalid UUID in database"),
                url: row.get(2)?,
                status,
                claimed_by: row.get::<_, Option<String>>(5)?.map(|s| Uuid::parse_str(&s).expect("Invalid claimed_by UUID in database")),
                claimed_at: row
                    .get::<_, Option<String>>(6)?
                    .map(|s| DateTime::parse_from_rfc3339(&s).unwrap().with_timezone(&Utc)),
                progress: row.get(7)?,
                local_path: row.get::<_, Option<String>>(8)?.map(PathBuf::from),
                file_size: row.get::<_, Option<i64>>(9)?.map(|s| s as u64),
                created_by: Uuid::parse_str(&row.get::<_, String>(10)?).expect("Invalid UUID in database"),
                created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(11)?)
                    .unwrap()
                    .with_timezone(&Utc),
                version: row.get::<_, i64>(12)? as u64,
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
                id: Uuid::parse_str(&row.get::<_, String>(0)?).expect("Invalid UUID in database"),
                team_id: Uuid::parse_str(&row.get::<_, String>(1)?).expect("Invalid UUID in database"),
                url: row.get(2)?,
                status,
                claimed_by: row.get::<_, Option<String>>(4)?.map(|s| Uuid::parse_str(&s).expect("Invalid claimed_by UUID in database")),
                claimed_at: row
                    .get::<_, Option<String>>(5)?
                    .map(|s| DateTime::parse_from_rfc3339(&s).unwrap().with_timezone(&Utc)),
                progress: row.get(6)?,
                local_path: row.get::<_, Option<String>>(7)?.map(PathBuf::from),
                file_size: row.get::<_, Option<i64>>(8)?.map(|s| s as u64),
                created_by: Uuid::parse_str(&row.get::<_, String>(9)?).expect("Invalid UUID in database"),
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
            Ok(Uuid::parse_str(&row.get::<_, String>(0)?).expect("Invalid UUID in database"))
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

/// 生成 12 位邀请码（使用密码学安全的 OsRng）
fn generate_invite_code() -> String {
    use rand::Rng;
    let mut rng = rand::rngs::OsRng;
    (0..12)
        .map(|_| {
            let i = rng.gen_range(0..36);
            if i < 10 {
                (b'0' + i) as char
            } else {
                (b'A' + i - 10) as char
            }
        })
        .collect()
}
