//! 共享 HTTP 客户端
//!
//! 提供全项目统一的 reqwest::Client 实例，支持连接池复用和 HTTP/2

use std::sync::Arc;
use once_cell::sync::Lazy;
use reqwest::Client;

/// 默认 User-Agent
const DEFAULT_USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/148.0.0.0 Safari/537.36";

/// 全局共享 HTTP 客户端（连接池复用，HTTP/2 支持）
pub static SHARED_CLIENT: Lazy<Arc<Client>> = Lazy::new(|| {
    let client = Client::builder()
        .user_agent(DEFAULT_USER_AGENT)
        .tcp_keepalive(std::time::Duration::from_secs(120))
        .connect_timeout(std::time::Duration::from_secs(30))
        .timeout(std::time::Duration::from_secs(600))  // 10分钟请求超时
        .pool_max_idle_per_host(32)  // 支持 8+ 并行分段
        .build()
        .expect("Failed to build shared HTTP client");
    Arc::new(client)
});

/// 全局共享阻塞 HTTP 客户端（用于需要阻塞调用的解析器）
pub static SHARED_BLOCKING_CLIENT: Lazy<Arc<reqwest::blocking::Client>> = Lazy::new(|| {
    let client = reqwest::blocking::Client::builder()
        .user_agent(DEFAULT_USER_AGENT)
        .tcp_keepalive(std::time::Duration::from_secs(120))
        .connect_timeout(std::time::Duration::from_secs(30))
        .timeout(std::time::Duration::from_secs(600))  // 10分钟请求超时
        .pool_max_idle_per_host(32)
        .build()
        .expect("Failed to build shared blocking HTTP client");
    Arc::new(client)
});

/// 获取共享客户端的 Arc 引用
pub fn shared() -> Arc<Client> {
    SHARED_CLIENT.clone()
}

/// 获取共享阻塞客户端的 Arc 引用
pub fn shared_blocking() -> Arc<reqwest::blocking::Client> {
    SHARED_BLOCKING_CLIENT.clone()
}