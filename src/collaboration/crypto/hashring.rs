//! 一致性哈希环

use crate::collaboration::types::Device;
use std::collections::BTreeMap;
use std::hash::{Hash, Hasher, DefaultHasher};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// 一致性哈希环
pub struct HashRing {
    #[allow(dead_code)]
    nodes: Arc<RwLock<BTreeMap<u64, Uuid>>>, // hash -> device_id
    #[allow(dead_code)]
    devices: Arc<RwLock<std::collections::HashMap<Uuid, Device>>>,
}

impl HashRing {
    pub fn new() -> Self {
        Self {
            nodes: Arc::new(RwLock::new(BTreeMap::new())),
            devices: Arc::new(RwLock::new(std::collections::HashMap::new())),
        }
    }

    /// 添加设备到环中
    #[allow(dead_code)]
    pub async fn add_device(&self, device: &Device) {
        let hash = self.hash_device(device.id);
        self.nodes.write().await.insert(hash, device.id);
        self.devices.write().await.insert(device.id, device.clone());
    }

    /// 从环中移除设备
    pub async fn remove_device(&self, device_id: &Uuid) {
        let hash = self.hash_device(*device_id);
        self.nodes.write().await.remove(&hash);
        self.devices.write().await.remove(device_id);
    }

    /// 获取 URL 对应的设备 ID
    pub async fn get_owner(&self, url: &str) -> Option<Uuid> {
        let nodes = self.nodes.read().await;
        if nodes.is_empty() {
            return None;
        }

        let url_hash = self.hash_url(url);

        // 找到 >= url_hash 的第一个节点
        if let Some((_, id)) = nodes.range(url_hash..).next() {
            return Some(*id);
        }

        // 环回第一个节点
        nodes.iter().next().map(|(_, id)| *id)
    }

    fn hash_device(&self, device_id: Uuid) -> u64 {
        let mut h = DefaultHasher::new();
        device_id.as_bytes().hash(&mut h);
        h.finish()
    }

    fn hash_url(&self, url: &str) -> u64 {
        let mut h = DefaultHasher::new();
        url.hash(&mut h);
        h.finish()
    }
}

impl Default for HashRing {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_hash_ring_basic() {
        let ring = HashRing::new();

        let device1 = Device {
            id: Uuid::new_v4(),
            team_id: Uuid::new_v4(),
            name: "device1".to_string(),
            public_ip: None,
            public_port: None,
            last_seen: chrono::Utc::now(),
            is_online: true,
        };

        let device2 = Device {
            id: Uuid::new_v4(),
            team_id: device1.team_id,
            name: "device2".to_string(),
            public_ip: None,
            public_port: None,
            last_seen: chrono::Utc::now(),
            is_online: true,
        };

        ring.add_device(&device1).await;
        ring.add_device(&device2).await;

        // URL 应该在某个设备上
        let url = "https://example.com/video";
        let owner = ring.get_owner(url).await;
        assert!(owner.is_some());

        // 同一 URL 应该映射到同一设备
        let owner2 = ring.get_owner(url).await;
        assert_eq!(owner, owner2);

        // 移除设备后应该切换到另一个
        ring.remove_device(&device1.id).await;
        let owner3 = ring.get_owner(url).await;
        assert_eq!(owner3, Some(device2.id));
    }

    #[tokio::test]
    async fn test_empty_ring() {
        let ring = HashRing::new();
        let owner = ring.get_owner("test").await;
        assert!(owner.is_none());
    }

    #[tokio::test]
    async fn test_single_device() {
        let ring = HashRing::new();
        let device = Device {
            id: Uuid::new_v4(),
            team_id: Uuid::new_v4(),
            name: "device1".to_string(),
            public_ip: None,
            public_port: None,
            last_seen: chrono::Utc::now(),
            is_online: true,
        };

        ring.add_device(&device).await;

        // 同一 URL 始终映射到唯一设备
        for _ in 0..10 {
            let owner = ring.get_owner("https://example.com/video").await;
            assert_eq!(owner, Some(device.id));
        }
    }

    #[tokio::test]
    async fn test_url_distribution() {
        let ring = HashRing::new();
        let device1 = Device {
            id: Uuid::new_v4(),
            team_id: Uuid::new_v4(),
            name: "device1".to_string(),
            public_ip: None,
            public_port: None,
            last_seen: chrono::Utc::now(),
            is_online: true,
        };
        let device2 = Device {
            id: Uuid::new_v4(),
            team_id: device1.team_id,
            name: "device2".to_string(),
            public_ip: None,
            public_port: None,
            last_seen: chrono::Utc::now(),
            is_online: true,
        };

        ring.add_device(&device1).await;
        ring.add_device(&device2).await;

        // 测试多个 URL 分布
        let mut device1_count = 0u32;
        let mut device2_count = 0u32;
        let urls = (0..100).map(|i| format!("https://example.com/video/{}", i));

        for url in urls {
            match ring.get_owner(&url).await {
                Some(id) if id == device1.id => device1_count += 1,
                Some(id) if id == device2.id => device2_count += 1,
                _ => panic!("Unexpected None owner"),
            }
        }

        // 两个设备都应该收到请求（分布应该大致均匀）
        assert!(device1_count > 0 && device2_count > 0);
    }

    #[tokio::test]
    async fn test_remove_nonexistent_device() {
        let ring = HashRing::new();
        let device_id = Uuid::new_v4();
        // 移除不存在的设备不应 panic
        ring.remove_device(&device_id).await;
    }

    #[tokio::test]
    async fn test_remove_and_readd() {
        let ring = HashRing::new();
        let device = Device {
            id: Uuid::new_v4(),
            team_id: Uuid::new_v4(),
            name: "device1".to_string(),
            public_ip: None,
            public_port: None,
            last_seen: chrono::Utc::now(),
            is_online: true,
        };

        ring.add_device(&device).await;
        let owner1 = ring.get_owner("test_url").await;
        assert_eq!(owner1, Some(device.id));

        ring.remove_device(&device.id).await;
        let owner2 = ring.get_owner("test_url").await;
        assert!(owner2.is_none());

        // 重新添加同一设备
        ring.add_device(&device).await;
        let owner3 = ring.get_owner("test_url").await;
        assert_eq!(owner3, Some(device.id));
    }
}
