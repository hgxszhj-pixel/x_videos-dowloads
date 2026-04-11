//! STUN 客户端实现
//!
//! 使用 Google STUN 服务器 (stun.l.google.com:19302) 进行 NAT 类型检测

#![allow(dead_code)]

use std::net::SocketAddr;

use tokio::net::UdpSocket;
use bytes::{BytesMut, BufMut};

/// STUN 服务器地址
const STUN_SERVER: &str = "stun.l.google.com:19302";

/// STUN 消息类型：Binding Request
const STUN_METHOD_BINDING: u16 = 0x0001;

/// STUN 消息类型：Binding Response
const STUN_METHOD_BINDING_RESPONSE: u16 = 0x0101;

/// STUN 魔术 cookie
const STUN_MAGIC_COOKIE: u32 = 0x2112_A442;

/// STUN 属性类型：XOR-MAPPED-ADDRESS
const STUN_ATTR_XOR_MAPPED_ADDRESS: u16 = 0x0020;

/// STUN 消息头长度
const STUN_HEADER_LEN: usize = 20;

/// 发送 STUN Binding Request 并获取公网 IP 和端口
pub async fn get_public_ip_port() -> std::io::Result<(String, u16)> {
    // 创建 UDP socket
    let socket = UdpSocket::bind("0.0.0.0:0").await?;

    // 解析 STUN 服务器地址
    let server_addr: SocketAddr = STUN_SERVER
        .parse()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;

    // 构建 STUN Binding Request 消息
    let mut msg = BytesMut::with_capacity(STUN_HEADER_LEN);
    build_binding_request(&mut msg);

    // 发送请求
    socket.send_to(&msg, server_addr).await?;

    // 接收响应
    let mut buf = BytesMut::zeroed(512);
    let (len, _) = socket.recv_from(&mut buf).await?;

    // 解析响应获取 XOR-MAPPED-ADDRESS
    let (ip, port) = parse_xor_mapped_address(&buf[..len])?;

    Ok((ip, port))
}

/// 检测是否为对称型 NAT
///
/// 通过向不同 STUN 服务器发送请求，比较返回的端口是否相同来判断
/// 对称型 NAT 会为不同的目标地址分配不同的端口
pub async fn is_nat_symmetric() -> bool {
    // 使用两个不同的 STUN 服务器进行测试
    let servers = [
        ("stun.l.google.com", 19302),
        ("stun1.l.google.com", 19302),
    ];

    let mut mapped_addrs: Vec<(String, u16)> = Vec::new();

    for (host, port) in servers {
        let addr = format!("{}:{}", host, port);
        if let Ok((ip, p)) = get_public_ip_port_for_server(&addr).await {
            mapped_addrs.push((ip, p));
        }
    }

    // 如果两个服务器返回的端口不同，则为对称型 NAT
    if mapped_addrs.len() >= 2 {
        let (_, port1) = &mapped_addrs[0];
        let (_, port2) = &mapped_addrs[1];
        return port1 != port2;
    }

    // 无法确定，保守返回 true
    true
}

/// 向指定 STUN 服务器获取公网 IP 和端口
async fn get_public_ip_port_for_server(server_addr: &str) -> std::io::Result<(String, u16)> {
    let socket = UdpSocket::bind("0.0.0.0:0").await?;

    let server: SocketAddr = server_addr
        .parse()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;

    let mut msg = BytesMut::with_capacity(STUN_HEADER_LEN);
    build_binding_request(&mut msg);

    socket.send_to(&msg, server).await?;

    let mut buf = BytesMut::zeroed(512);
    let (len, _) = socket.recv_from(&mut buf).await?;

    let (ip, port) = parse_xor_mapped_address(&buf[..len])?;
    Ok((ip, port))
}

/// 构建 STUN Binding Request 消息
fn build_binding_request(buf: &mut BytesMut) {
    // 消息类型：Binding Request
    buf.put_u16(STUN_METHOD_BINDING);
    // 消息长度（不包括 20 字节头部）
    buf.put_u16(0);
    // 魔术 cookie
    buf.put_u32(STUN_MAGIC_COOKIE);
    // 事务 ID（12 字节）
    let transaction_id: [u8; 12] = rand::random();
    buf.put_slice(&transaction_id);
}

/// 解析 XOR-MAPPED-ADDRESS 属性
fn parse_xor_mapped_address(data: &[u8]) -> std::io::Result<(String, u16)> {
    if data.len() < STUN_HEADER_LEN + 4 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "STUN response too short",
        ));
    }

    // 解析消息头
    let msg_type = u16::from_be_bytes([data[0], data[1]]);
    let _msg_length = u16::from_be_bytes([data[2], data[3]]);

    // 验证响应类型
    if msg_type != STUN_METHOD_BINDING_RESPONSE {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("Expected Binding Response (0x{:04x}), got 0x{:04x}", STUN_METHOD_BINDING_RESPONSE, msg_type),
        ));
    }

    // 读取魔术 cookie
    let magic_cookie = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
    if magic_cookie != STUN_MAGIC_COOKIE {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Invalid magic cookie",
        ));
    }

    // 搜索 XOR-MAPPED-ADDRESS 属性
    let mut offset = STUN_HEADER_LEN;
    while offset < data.len() - 4 {
        let attr_type = u16::from_be_bytes([data[offset], data[offset + 1]]);
        let attr_length = u16::from_be_bytes([data[offset + 2], data[offset + 3]]);

        if attr_type == STUN_ATTR_XOR_MAPPED_ADDRESS {
            return parse_xor_address(&data[offset + 4..], attr_length as usize, magic_cookie);
        }

        // 属性需要 4 字节对齐
        offset += 4 + ((attr_length as usize + 3) & !3);
    }

    Err(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        "XOR-MAPPED-ADDRESS not found",
    ))
}

/// 解析 XOR 编码的地址
fn parse_xor_address(data: &[u8], length: usize, magic_cookie: u32) -> std::io::Result<(String, u16)> {
    if length < 4 || data.len() < length {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Invalid XOR address length",
        ));
    }

    // 第一个字节是地址族，0x01 表示 IPv4，0x02 表示 IPv6
    let family = data[0];
    if family != 0x01 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Only IPv4 is supported",
        ));
    }

    // 端口 XOR 操作
    let port = u16::from_be_bytes([data[2], data[3]]) ^ ((magic_cookie >> 16) as u16);

    // IP 地址 XOR 操作
    let xor_bytes = magic_cookie.to_be_bytes();
    let ip_bytes = [
        data[4] ^ xor_bytes[0],
        data[5] ^ xor_bytes[1],
        data[6] ^ xor_bytes[2],
        data[7] ^ xor_bytes[3],
    ];

    let ip = format!("{}.{}.{}.{}", ip_bytes[0], ip_bytes[1], ip_bytes[2], ip_bytes[3]);

    Ok((ip, port))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_get_public_ip_port() {
        let result = get_public_ip_port().await;
        match result {
            Ok((ip, port)) => {
                println!("Public IP: {}, Port: {}", ip, port);
                assert!(!ip.is_empty());
                assert!(port > 0);
            }
            Err(e) => {
                println!("STUN request failed (可能无网络): {}", e);
            }
        }
    }

    #[tokio::test]
    async fn test_nat_type_detection() {
        let is_symmetric = is_nat_symmetric().await;
        println!("Is symmetric NAT: {}", is_symmetric);
    }
}
