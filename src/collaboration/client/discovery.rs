//! 设备发现与 NAT 穿透

/// 发现服务
#[expect(dead_code)]
pub struct DiscoveryService {
    #[allow(unused)]
    stun_server: String,
}

impl DiscoveryService {
    /// 创建发现服务
    #[expect(dead_code)]
    pub fn new(stun_server: Option<String>) -> Self {
        Self {
            stun_server: stun_server.unwrap_or_else(|| "stun.l.google.com:19302".to_string()),
        }
    }
}
