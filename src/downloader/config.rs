//! 下载器配置

/// 下载器配置
#[derive(Debug, Clone)]
pub struct DownloaderConfig {
    /// 连接超时（秒）
    pub connect_timeout: u64,
    /// 请求超时（秒）
    pub request_timeout: u64,
    /// 重试次数
    pub retry_count: u32,
    /// 重试最大间隔（秒）
    pub retry_max_delay: u64,
    /// 块大小
    pub chunk_size: usize,
    /// 自定义 User-Agent
    pub user_agent: Option<String>,
    /// 代理
    pub proxy: Option<String>,
    /// 连接池最大空闲连接数
    pub pool_max_idle_per_host: usize,
    /// 是否启用HTTP/2
    pub http2: bool,
    /// TCP保活时间（秒）
    pub tcp_keepalive: u64,
    /// 分段下载线程数（0表示禁用）
    pub parallel_segments: u32,
    /// 自适应线程控制
    pub adaptive_threads: bool,
    /// 最小线程数
    pub min_threads: u32,
    /// 最大线程数（避免封IP）
    pub max_threads: u32,
    /// 速度阈值（字节/秒），低于此值增加线程
    pub speed_threshold_bps: u64,
}

impl Default for DownloaderConfig {
    fn default() -> Self {
        Self {
            connect_timeout: 30,
            request_timeout: 600, // 大文件需要更长超时
            retry_count: 5,      // 增加重试次数
            retry_max_delay: 30,  // 最大重试间隔30秒
            chunk_size: 32 * 1024 * 1024, // 32MB chunk - 减少HTTP开销
            user_agent: None,
            proxy: None,
            pool_max_idle_per_host: 32,  // 更大连接池
            http2: true,
            tcp_keepalive: 120, // 减少连接断开
            parallel_segments: 4, // 默认4线程
            adaptive_threads: true, // 启用自适应线程
            min_threads: 2,        // 最少2线程
            max_threads: 8,        // 最多8线程（防封IP）
            speed_threshold_bps: 500 * 1024, // 500KB/s 低于此值增加线程
        }
    }
}
