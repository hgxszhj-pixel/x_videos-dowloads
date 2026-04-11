//! 本地任务队列

use crate::collaboration::types::{Task, TaskStatus};
use anyhow::Result;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// 本地队列
#[allow(dead_code)]
pub struct LocalQueue {
    tasks: Vec<Task>,
    team_id: Uuid,
    device_id: Uuid,
    state_file: PathBuf,
}

impl LocalQueue {
    /// 创建新队列
    #[allow(dead_code)]
    pub fn new(device_id: Uuid, team_id: Uuid, state_dir: &Path) -> Result<Self> {
        let state_file = state_dir.join("queue_state.json");
        let mut queue = Self {
            tasks: Vec::new(),
            team_id,
            device_id,
            state_file,
        };
        queue.load()?;
        Ok(queue)
    }

    /// 添加任务
    #[allow(dead_code)]
    pub fn add_task(&mut self, url: &str) -> Result<Task> {
        // 检查 URL 是否已存在
        if self.tasks.iter().any(|t| t.url == url) {
            anyhow::bail!("URL 已存在");
        }

        let task = Task::new(url.to_string(), self.team_id, self.device_id);
        self.tasks.push(task.clone());
        self.save()?;
        Ok(task)
    }

    /// 认领任务
    #[allow(dead_code)]
    pub fn claim_task(&mut self, task_id: Uuid) -> Result<()> {
        let task = self
            .tasks
            .iter_mut()
            .find(|t| t.id == task_id)
            .ok_or_else(|| anyhow::anyhow!("任务不存在"))?;

        if task.status != TaskStatus::Queued {
            anyhow::bail!("任务不可认领");
        }

        task.status = TaskStatus::Claimed;
        task.claimed_by = Some(self.device_id);
        task.claimed_at = Some(chrono::Utc::now());
        self.save()?;
        Ok(())
    }

    /// 更新进度
    #[allow(dead_code)]
    pub fn update_progress(&mut self, task_id: Uuid, progress: f64) -> Result<()> {
        let task = self
            .tasks
            .iter_mut()
            .find(|t| t.id == task_id)
            .ok_or_else(|| anyhow::anyhow!("任务不存在"))?;

        task.progress = progress;
        self.save()?;
        Ok(())
    }

    /// 标记完成
    #[allow(dead_code)]
    pub fn mark_complete(
        &mut self,
        task_id: Uuid,
        local_path: PathBuf,
        file_size: u64,
    ) -> Result<()> {
        let task = self
            .tasks
            .iter_mut()
            .find(|t| t.id == task_id)
            .ok_or_else(|| anyhow::anyhow!("任务不存在"))?;

        task.status = TaskStatus::Complete;
        task.local_path = Some(local_path);
        task.file_size = Some(file_size);
        self.save()?;
        Ok(())
    }

    /// 获取任务
    #[allow(dead_code)]
    pub fn get_task(&self, task_id: Uuid) -> Option<&Task> {
        self.tasks.iter().find(|t| t.id == task_id)
    }

    /// 获取所有任务
    #[allow(dead_code)]
    pub fn get_all_tasks(&self) -> &[Task] {
        &self.tasks
    }

    /// 更新任务状态为 Queued（模拟服务器分配）
    #[allow(dead_code)]
    pub fn update_status_to_queued(&mut self, task_id: Uuid) -> Result<()> {
        let task = self
            .tasks
            .iter_mut()
            .find(|t| t.id == task_id)
            .ok_or_else(|| anyhow::anyhow!("任务不存在"))?;
        task.status = TaskStatus::Queued;
        self.save()?;
        Ok(())
    }

    /// 从服务器同步任务
    #[allow(dead_code)]
    pub fn sync_from_server(&mut self, tasks: Vec<Task>) -> Result<()> {
        // 合并策略：服务器优先，但保留本地新任务
        let local_new: Vec<_> = self
            .tasks
            .iter()
            .filter(|t| t.status == TaskStatus::New)
            .cloned()
            .collect();

        self.tasks = tasks;
        for task in local_new {
            if !self.tasks.iter().any(|t| t.url == task.url) {
                self.tasks.push(task);
            }
        }

        self.save()?;
        Ok(())
    }

    /// 保存到文件
    fn save(&self) -> Result<()> {
        let json = serde_json::to_string_pretty(&self.tasks)?;
        std::fs::create_dir_all(self.state_file.parent().unwrap())?;
        std::fs::write(&self.state_file, json)?;
        Ok(())
    }

    /// 从文件加载
    fn load(&mut self) -> Result<()> {
        if !self.state_file.exists() {
            return Ok(());
        }
        let json = std::fs::read_to_string(&self.state_file)?;
        self.tasks = serde_json::from_str(&json)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_local_queue_add_task() {
        let temp_dir = tempfile::tempdir().unwrap();
        let device_id = Uuid::new_v4();
        let team_id = Uuid::new_v4();
        let mut queue = LocalQueue::new(device_id, team_id, temp_dir.path()).unwrap();

        let task = queue.add_task("https://example.com/video1").unwrap();
        assert_eq!(task.url, "https://example.com/video1");
        assert_eq!(task.status, TaskStatus::New);
    }

    #[test]
    fn test_local_queue_duplicate_url() {
        let temp_dir = tempfile::tempdir().unwrap();
        let device_id = Uuid::new_v4();
        let team_id = Uuid::new_v4();
        let mut queue = LocalQueue::new(device_id, team_id, temp_dir.path()).unwrap();

        queue.add_task("https://example.com/video").unwrap();
        let result = queue.add_task("https://example.com/video");
        assert!(result.is_err());
    }

    #[test]
    fn test_local_queue_claim_task() {
        let temp_dir = tempfile::tempdir().unwrap();
        let device_id = Uuid::new_v4();
        let team_id = Uuid::new_v4();
        let mut queue = LocalQueue::new(device_id, team_id, temp_dir.path()).unwrap();

        let task = queue.add_task("https://example.com/video").unwrap();
        // 模拟服务器分配后的状态
        queue.update_status_to_queued(task.id).unwrap();
        queue.claim_task(task.id).unwrap();

        let claimed = queue.get_task(task.id).unwrap();
        assert_eq!(claimed.status, TaskStatus::Claimed);
        assert_eq!(claimed.claimed_by, Some(device_id));
    }

    #[test]
    fn test_local_queue_cannot_claim_completed() {
        let temp_dir = tempfile::tempdir().unwrap();
        let device_id = Uuid::new_v4();
        let team_id = Uuid::new_v4();
        let mut queue = LocalQueue::new(device_id, team_id, temp_dir.path()).unwrap();

        let task = queue.add_task("https://example.com/video").unwrap();
        queue.mark_complete(task.id, PathBuf::from("/path/to/file.mp4"), 1024).unwrap();

        // 完成任务不能再被认领
        let result = queue.claim_task(task.id);
        assert!(result.is_err());
    }

    #[test]
    fn test_local_queue_update_progress() {
        let temp_dir = tempfile::tempdir().unwrap();
        let device_id = Uuid::new_v4();
        let team_id = Uuid::new_v4();
        let mut queue = LocalQueue::new(device_id, team_id, temp_dir.path()).unwrap();

        let task = queue.add_task("https://example.com/video").unwrap();
        queue.update_progress(task.id, 0.5).unwrap();

        let updated = queue.get_task(task.id).unwrap();
        assert_eq!(updated.progress, 0.5);
    }

    #[test]
    fn test_local_queue_mark_complete() {
        let temp_dir = tempfile::tempdir().unwrap();
        let device_id = Uuid::new_v4();
        let team_id = Uuid::new_v4();
        let mut queue = LocalQueue::new(device_id, team_id, temp_dir.path()).unwrap();

        let task = queue.add_task("https://example.com/video").unwrap();
        queue.mark_complete(task.id, PathBuf::from("/path/to/file.mp4"), 1024).unwrap();

        let completed = queue.get_task(task.id).unwrap();
        assert_eq!(completed.status, TaskStatus::Complete);
        assert!(completed.local_path.is_some());
        assert_eq!(completed.file_size, Some(1024));
    }
}
