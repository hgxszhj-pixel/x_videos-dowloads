//! HTTP 文件服务器
//!
//! 提供文件下载服务，支持 Range 请求（断点续传）

use anyhow::Result;
use std::collections::HashMap;
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::RwLock;
use uuid::Uuid;

/// 单次请求最大 Range 范围 (100MB)
const MAX_RANGE_SIZE: u64 = 100 * 1024 * 1024;

/// 速率限制器：每个 IP 在窗口时间内最多 N 个请求
struct RateLimiter {
    /// 时间窗口（秒）
    window_secs: u64,
    /// 窗口内最大请求数
    max_requests: u64,
    /// 每个 IP 的请求时间记录
    requests: HashMap<IpAddr, Vec<Instant>>,
}

impl RateLimiter {
    fn new(window_secs: u64, max_requests: u64) -> Self {
        Self {
            window_secs,
            max_requests,
            requests: HashMap::new(),
        }
    }

    /// 检查 IP 是否超过速率限制
    fn is_allowed(&mut self, ip: IpAddr) -> bool {
        let now = Instant::now();
        let window = std::time::Duration::from_secs(self.window_secs);

        let timestamps = self.requests.entry(ip).or_default();

        // 移除过期的请求记录
        timestamps.retain(|t| now.duration_since(*t) < window);

        if timestamps.len() >= self.max_requests as usize {
            return false;
        }

        timestamps.push(now);
        true
    }

    /// 清理过期的 IP 记录（定期调用）
    #[allow(dead_code)]
    fn cleanup(&mut self) {
        let now = Instant::now();
        let window = std::time::Duration::from_secs(self.window_secs);
        self.requests.retain(|_, timestamps| {
            timestamps.retain(|t| now.duration_since(*t) < window);
            !timestamps.is_empty()
        });
    }
}

/// CORS 白名单
#[derive(Clone)]
struct CorsAllowList {
    /// 允许的源列表（包含协议和端口，如 "http://localhost:8080"）
    origins: Vec<String>,
    /// 允许的域名/IP（仅主机部分）
    allowed_hosts: Vec<String>,
}

impl CorsAllowList {
    fn new() -> Self {
        Self {
            origins: Vec::new(),
            allowed_hosts: vec![
                "localhost".to_string(),
                "127.0.0.1".to_string(),
                "::1".to_string(),
                "0.0.0.0".to_string(),
            ],
        }
    }

    /// 添加额外的允许 origin (预留 API)
    #[allow(unknown_lints, dead_code)]
    fn with_extra_origins(mut self, origins: Vec<String>) -> Self {
        for origin in &origins {
            if let Ok(parsed) = origin.parse::<url::Url>() {
                if let Some(host) = parsed.host_str() {
                    if !self.allowed_hosts.contains(&host.to_string()) {
                        self.allowed_hosts.push(host.to_string());
                    }
                }
            }
            self.origins.push(origin.clone());
        }
        self
    }

    /// 检查 Origin 是否被允许
    fn is_allowed(&self, origin: &str) -> bool {
        if origin.is_empty() {
            return true; // 没有 Origin 头视为同源请求
        }

        // 解析 Origin（格式: scheme://host:port）
        if let Ok(parsed) = origin.parse::<url::Url>() {
            let host = parsed.host_str().unwrap_or("");

            // 检查是否是 localhost
            if Self::is_localhost(host) {
                return true;
            }

            // 检查是否在允许的 origin 列表中（精确匹配）
            if self.origins.contains(&origin.to_string()) {
                return true;
            }

            // 检查 host 是否在允许列表中
            if self.allowed_hosts.contains(&host.to_string()) {
                return true;
            }
        }

        false
    }

    /// 检查是否是 localhost
    fn is_localhost(host: &str) -> bool {
        matches!(host, "localhost" | "127.0.0.1" | "::1" | "0.0.0.0")
    }

    /// 从请求中解析 Origin 头
    fn get_origin_from_request(lines: &[&str]) -> Option<String> {
        for line in lines {
            if line.to_lowercase().starts_with("origin:") {
                let origin = line.split(':').nth(1)?.trim();
                return Some(origin.to_string());
            }
        }
        None
    }

    /// 生成 CORS 响应头
    fn build_cors_headers(&self, origin: &str) -> String {
        if origin.is_empty() {
            return String::new();
        }
        format!(
            "Access-Control-Allow-Origin: {}\r\nAccess-Control-Allow-Credentials: true",
            origin
        )
    }
}

/// HTTP 文件服务器
#[allow(dead_code)]
pub struct FileServer {
    port: u16,
    files: Arc<RwLock<HashMap<Uuid, PathBuf>>>, // task_id -> path
    rate_limiter: Arc<RwLock<RateLimiter>>,
    cors_allow_list: CorsAllowList,
}

#[allow(dead_code)]
impl FileServer {
    /// 创建文件服务器
    pub fn new(port: u16) -> Self {
        Self {
            port,
            files: Arc::new(RwLock::new(HashMap::new())),
            // 速率限制：每 60 秒最多 100 个请求（每个 IP）
            rate_limiter: Arc::new(RwLock::new(RateLimiter::new(60, 100))),
            cors_allow_list: CorsAllowList::new(),
        }
    }

    /// 注册文件（路径验证）
    pub async fn register_file(&self, task_id: Uuid, path: PathBuf) -> Result<()> {
        // 验证路径安全性
        let canonical = std::fs::canonicalize(&path)
            .map_err(|_| anyhow::anyhow!("无法获取规范路径"))?;

        let base = std::env::current_dir()
            .map_err(|_| anyhow::anyhow!("无法获取当前目录"))?
            .join("files");

        let canonical_base = std::fs::canonicalize(&base)
            .map_err(|_| anyhow::anyhow!("files 目录不存在"))?;

        if !canonical.starts_with(&canonical_base) {
            anyhow::bail!("路径遍历攻击尝试被阻止");
        }

        self.files.write().await.insert(task_id, canonical);
        Ok(())
    }

    /// 注销文件
    pub async fn unregister_file(&self, task_id: Uuid) {
        self.files.write().await.remove(&task_id);
    }

    /// 启动服务器
    pub async fn start(&self) -> Result<()> {
        let addr = format!("0.0.0.0:{}", self.port);
        let listener = TcpListener::bind(&addr).await?;
        println!("文件服务器监听: {}", addr);

        loop {
            let (mut stream, remote_addr) = listener.accept().await?;
            let files = self.files.clone();
            let rate_limiter = self.rate_limiter.clone();
            let cors_allow_list = self.cors_allow_list.clone();

            tokio::spawn(async move {
                if let Err(e) =
                    Self::handle_request(&mut stream, files, rate_limiter, cors_allow_list, remote_addr)
                        .await
                {
                    eprintln!("请求处理错误 ({}): {}", remote_addr, e);
                }
            });
        }
    }

    async fn handle_request(
        stream: &mut tokio::net::TcpStream,
        files: Arc<RwLock<HashMap<Uuid, PathBuf>>>,
        rate_limiter: Arc<RwLock<RateLimiter>>,
        cors_allow_list: CorsAllowList,
        remote_addr: std::net::SocketAddr,
    ) -> Result<()> {
        // 速率限制检查
        {
            let mut limiter = rate_limiter.write().await;
            if !limiter.is_allowed(remote_addr.ip()) {
                let response = "HTTP/1.1 429 Too Many Requests\r\nContent-Length: 0\r\n\r\n";
                stream.write_all(response.as_bytes()).await?;
                return Ok(());
            }
        }

        // 动态读取请求，支持大请求头和长URL
        // 使用 64KB 初始缓冲区，按需自动扩展
        let mut buffer = vec![0u8; 65536];
        let mut total_read = 0;
        let max_request_size = 10 * 1024 * 1024; // 10MB 最大请求大小

        loop {
            if total_read >= max_request_size {
                anyhow::bail!("请求过大，超过 10MB 限制");
            }
            let n = stream.read(&mut buffer[total_read..]).await?;
            if n == 0 {
                break;
            }
            total_read += n;
            // 如果缓冲区不够用，扩展它
            if total_read >= buffer.len() && buffer.len() < max_request_size {
                let new_size = (buffer.len() * 2).min(max_request_size);
                buffer.resize(new_size, 0);
            }
        }

        if total_read == 0 {
            return Ok(());
        }

        let request = String::from_utf8_lossy(&buffer[..total_read]);
        let lines: Vec<&str> = request.lines().collect();

        // CORS Origin 验证
        let origin = CorsAllowList::get_origin_from_request(&lines).unwrap_or_default();

        if !cors_allow_list.is_allowed(&origin) {
            let response = "HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n";
            stream.write_all(response.as_bytes()).await?;
            return Ok(());
        }

        let first_line = lines.first().unwrap_or(&"");

        // 解析请求行: GET /file/{task_id} HTTP/1.1
        if first_line.starts_with("GET /file/") {
            let parts: Vec<&str> = first_line.split('/').collect();
            if parts.len() >= 4 {
                let task_id_str = parts[3].split_whitespace().next().unwrap_or("");
                if let Ok(task_id) = Uuid::parse_str(task_id_str) {
                    let files_guard = files.read().await;
                    if let Some(path) = files_guard.get(&task_id) {
                        // 检查 Range 请求头
                        let range = Self::parse_range(&lines);
                        Self::serve_file(stream, path, range, &origin, &cors_allow_list).await?;
                        return Ok(());
                    }
                }
            }
        }

        // 404 Not Found
        let cors_headers = cors_allow_list.build_cors_headers(&origin);
        let response = if cors_headers.is_empty() {
            "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_string()
        } else {
            format!("HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n{}\r\n", cors_headers)
        };
        stream.write_all(response.as_bytes()).await?;
        Ok(())
    }

    /// 路径安全验证：确保文件在允许的 files 目录下，防止路径遍历攻击
    fn validate_path(path: &PathBuf) -> Result<()> {
        let canonical_path = std::fs::canonicalize(path)
            .map_err(|e| anyhow::anyhow!("无法获取文件路径: {}", e))?;

        let base_dir = std::env::current_dir()
            .map_err(|e| anyhow::anyhow!("无法获取当前目录: {}", e))?
            .join("files");

        let canonical_base = std::fs::canonicalize(&base_dir)
            .map_err(|e| anyhow::anyhow!("基准目录不存在: {}", e))?;

        if !canonical_path.starts_with(&canonical_base) {
            anyhow::bail!(
                "路径遍历攻击尝试: {:?} 不在允许目录 {:?} 内",
                canonical_path,
                canonical_base
            );
        }

        Ok(())
    }

    /// 解析 Range 请求头
    fn parse_range(lines: &[&str]) -> Option<(u64, Option<u64>)> {
        for line in lines {
            if line.to_lowercase().starts_with("range: bytes=") {
                let range_spec = line.split('=').nth(1)?;
                let parts: Vec<&str> = range_spec.split('-').collect();
                if !parts.is_empty() {
                    let start = parts[0].parse::<u64>().ok()?;
                    let end = if parts.len() >= 2 && !parts[1].is_empty() {
                        Some(parts[1].parse::<u64>().ok()?)
                    } else {
                        None
                    };
                    // 验证 start > end 的情况（会导致 u64 下溢）
                    if let Some(end_val) = end {
                        if start > end_val {
                            return None;
                        }
                    }
                    return Some((start, end));
                }
            }
        }
        None
    }

    /// 服务文件（支持断点续传）
    async fn serve_file(
        stream: &mut tokio::net::TcpStream,
        path: &PathBuf,
        range: Option<(u64, Option<u64>)>,
        origin: &str,
        cors_allow_list: &CorsAllowList,
    ) -> Result<()> {
        // 路径安全验证
        Self::validate_path(path)?;

        let metadata = tokio::fs::metadata(path).await?;
        let file_size = metadata.len();

        let cors_headers = cors_allow_list.build_cors_headers(origin);

        match range {
            Some((start, end)) => {
                // ========== Range 验证增强 ==========

                // 1. 防止 start >= file_size（请求范围超过文件大小）
                if start >= file_size {
                    let response = format!(
                        "HTTP/1.1 416 Range Not Satisfiable\r\n\
                         Content-Range: */\r\n\
                         Content-Length: 0\r\n\
                         {}\r\n",
                        cors_headers
                    );
                    stream.write_all(response.as_bytes()).await?;
                    return Ok(());
                }

                // 2. 计算实际 end 值（处理 open-ended ranges like "bytes=100-"）
                let end = end.unwrap_or(file_size - 1);

                // 3. 防止 start > end
                if start > end {
                    let response = format!(
                        "HTTP/1.1 416 Range Not Satisfiable\r\n\
                         Content-Range: */\r\n\
                         Content-Length: 0\r\n\
                         {}\r\n",
                        cors_headers
                    );
                    stream.write_all(response.as_bytes()).await?;
                    return Ok(());
                }

                // 4. 计算请求范围大小，防止大范围读取
                let range_size = end - start + 1;
                if range_size > MAX_RANGE_SIZE {
                    let response = format!(
                        "HTTP/1.1 416 Range Not Satisfiable\r\n\
                         Content-Range: */\r\n\
                         Content-Length: 0\r\n\
                         {}\r\n",
                        cors_headers
                    );
                    stream.write_all(response.as_bytes()).await?;
                    return Ok(());
                }

                // 限制 end 不超过文件大小
                let end = end.min(file_size - 1);
                let content_length = end - start + 1;

                let response = format!(
                    "HTTP/1.1 206 Partial Content\r\n\
                     Content-Type: application/octet-stream\r\n\
                     Content-Length: {}\r\n\
                     Content-Range: bytes {}-{}/{}\r\n\
                     Accept-Ranges: bytes\r\n\
                     {}\r\n",
                    content_length, start, end, file_size, cors_headers
                );
                stream.write_all(response.as_bytes()).await?;

                // 流式传输指定范围
                let mut file = tokio::fs::File::open(path).await?;
                tokio::io::copy(&mut file, stream).await?;
            }
            None => {
                // 普通请求
                let response = format!(
                    "HTTP/1.1 200 OK\r\n\
                     Content-Type: application/octet-stream\r\n\
                     Content-Length: {}\r\n\
                     Accept-Ranges: bytes\r\n\
                     {}\r\n",
                    file_size, cors_headers
                );
                stream.write_all(response.as_bytes()).await?;

                let mut file = tokio::fs::File::open(path).await?;
                tokio::io::copy(&mut file, stream).await?;
            }
        }
        Ok(())
    }
}
