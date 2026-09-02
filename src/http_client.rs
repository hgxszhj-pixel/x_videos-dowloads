//! 共享 HTTP 客户端
//!
//! 提供全项目统一的 reqwest::Client 实例，支持连接池复用和 HTTP/2

use once_cell::sync::Lazy;
use reqwest::Client;
use std::sync::Arc;

/// 默认 User-Agent
const DEFAULT_USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/148.0.0.0 Safari/537.36";

/// 全局共享 HTTP 客户端（连接池复用，HTTP/2，压缩支持）
pub static SHARED_CLIENT: Lazy<Arc<Client>> = Lazy::new(|| {
    let client = Client::builder()
        .user_agent(DEFAULT_USER_AGENT)
        .tcp_keepalive(std::time::Duration::from_secs(120))
        .connect_timeout(std::time::Duration::from_secs(30))
        .timeout(std::time::Duration::from_secs(600)) // 10分钟请求超时
        .pool_max_idle_per_host(64) // 支持 16+ 并行分段 (翻倍)
        .gzip(true) // 启用 Gzip 压缩
        .brotli(true) // 启用 Brotli 压缩
        // HTTP/2 keep-alive：避免长下载期间 CDN 关闭闲置 h2 连接
        .http2_keep_alive_interval(std::time::Duration::from_secs(30))
        .http2_keep_alive_timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap_or_else(|e| panic!("Failed to build shared HTTP client: {}", e));
    Arc::new(client)
});

/// 获取共享客户端的 Arc 引用
pub fn shared() -> Arc<Client> {
    SHARED_CLIENT.clone()
}

/// Resolver 默认 UA(集中维护,改版本只动这一处)
pub const RESOLVER_UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) \
    AppleWebKit/537.36 (KHTML, like Gecko) Chrome/148.0.0.0 Safari/537.36";

/// 给已有 builder 套上 resolver 默认值(供需要追加 proxy/超时 等的 caller)
pub fn apply_resolver_defaults(b: reqwest::ClientBuilder) -> reqwest::ClientBuilder {
    b.user_agent(RESOLVER_UA)
        .tcp_keepalive(std::time::Duration::from_secs(120))
        .connect_timeout(std::time::Duration::from_secs(30))
        .timeout(std::time::Duration::from_secs(600))
}

/// 构建一个独立的 resolver HTTP client(用于一次性短请求,不参与连接池复用)
#[allow(dead_code)] // 仅为未来一次性短请求预留 + test 引用;production 未消费但 test 用了 (test cfg 下 expect 会 unfulfilled)
pub fn build_resolver_client() -> reqwest::Client {
    apply_resolver_defaults(reqwest::Client::builder())
        .build()
        .unwrap_or_else(|e| panic!("Failed to build resolver HTTP client: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolver_ua_constant_format() {
        assert!(RESOLVER_UA.contains("Chrome/"));
        assert!(RESOLVER_UA.contains("Safari/"));
        assert!(RESOLVER_UA.starts_with("Mozilla/5.0"));
    }

    #[test]
    fn test_build_resolver_client_succeeds() {
        let client = build_resolver_client();
        // reqwest::Client 的 UA 不可直接 query,但能 build 不 panic 即证明配置合法
        let _ = client;
    }

    #[test]
    fn test_apply_resolver_defaults_returns_builder() {
        let builder = apply_resolver_defaults(reqwest::Client::builder());
        let client = builder.build().expect("resolver defaults must build");
        let _ = client;
    }
}
