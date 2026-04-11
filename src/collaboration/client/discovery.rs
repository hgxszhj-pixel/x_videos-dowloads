//! 设备发现与 NAT 穿透

use anyhow::Result;

/// 发现服务
#[allow(dead_code)]
pub struct DiscoveryService {
    #[allow(unused)]
    stun_server: String,
}

impl DiscoveryService {
    /// 创建发现服务
    #[allow(dead_code)]
    pub fn new(stun_server: Option<String>) -> Self {
        Self {
            stun_server: stun_server.unwrap_or_else(|| "stun.l.google.com:19302".to_string()),
        }
    }

    /// 获取公网端点
    #[allow(dead_code)]
    pub async fn get_public_endpoint(&self) -> Result<(String, u16)> {
        // 使用 STUN 获取公网 IP:Port
        // 简化实现
        let local_ip = local_ip_address::local_ip()?;
        Ok((local_ip.to_string(), 0))
    }

    /// 尝试直连
    #[allow(dead_code)]
    pub async fn try_direct_connect(&self, ip: &str, port: u16) -> Result<bool> {
        // 尝试建立 TCP 连接
        let addr = format!("{}:{}", ip, port);
        Ok(tokio::net::TcpStream::connect(&addr).await.is_ok())
    }
}
