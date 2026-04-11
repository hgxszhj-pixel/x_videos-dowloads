//! HTTP 文件服务器
//!
//! 提供文件下载服务，支持 Range 请求（断点续传）

use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::RwLock;
use uuid::Uuid;

/// 单次请求最大 Range 范围 (100MB)
const MAX_RANGE_SIZE: u64 = 100 * 1024 * 1024;

/// HTTP 文件服务器
#[allow(dead_code)]
pub struct FileServer {
    port: u16,
    files: Arc<RwLock<HashMap<Uuid, PathBuf>>>, // task_id -> path
}

#[allow(dead_code)]
impl FileServer {
    /// 创建文件服务器
    pub fn new(port: u16) -> Self {
        Self {
            port,
            files: Arc::new(RwLock::new(HashMap::new())),
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

            tokio::spawn(async move {
                if let Err(e) = Self::handle_request(&mut stream, files).await {
                    eprintln!("请求处理错误 ({}): {}", remote_addr, e);
                }
            });
        }
    }

    async fn handle_request(
        stream: &mut tokio::net::TcpStream,
        files: Arc<RwLock<HashMap<Uuid, PathBuf>>>,
    ) -> Result<()> {
        let mut buffer = vec![0u8; 8192];
        let n = stream.read(&mut buffer).await?;
        if n == 0 {
            return Ok(());
        }

        let request = String::from_utf8_lossy(&buffer[..n]);
        let lines: Vec<&str> = request.lines().collect();
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
                        Self::serve_file(stream, path, range).await?;
                        return Ok(());
                    }
                }
            }
        }

        // 404 Not Found
        let response = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n";
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
            anyhow::bail!("路径遍历攻击尝试: {:?} 不在允许目录 {:?} 内", canonical_path, canonical_base);
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
    ) -> Result<()> {
        // 路径安全验证
        Self::validate_path(path)?;

        let metadata = tokio::fs::metadata(path).await?;
        let file_size = metadata.len();

        match range {
            Some((start, end)) => {
                // ========== Range 验证增强 ==========

                // 1. 防止 start >= file_size（请求范围超过文件大小）
                if start >= file_size {
                    let response = "HTTP/1.1 416 Range Not Satisfiable\r\n\
                                   Content-Range: */\r\n\
                                   Content-Length: 0\r\n\r\n";
                    stream.write_all(response.as_bytes()).await?;
                    return Ok(());
                }

                // 2. 计算实际 end 值（处理 open-ended ranges like "bytes=100-"）
                let end = end.unwrap_or(file_size - 1);

                // 3. 防止 start > end
                if start > end {
                    let response = "HTTP/1.1 416 Range Not Satisfiable\r\n\
                                   Content-Range: */\r\n\
                                   Content-Length: 0\r\n\r\n";
                    stream.write_all(response.as_bytes()).await?;
                    return Ok(());
                }

                // 4. 计算请求范围大小，防止大范围读取
                let range_size = end - start + 1;
                if range_size > MAX_RANGE_SIZE {
                    let response = "HTTP/1.1 416 Range Not Satisfiable\r\n\
                                   Content-Range: */\r\n\
                                   Content-Length: 0\r\n\r\n";
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
                     Accept-Ranges: bytes\r\n\r\n",
                    content_length, start, end, file_size
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
                     Accept-Ranges: bytes\r\n\r\n",
                    file_size
                );
                stream.write_all(response.as_bytes()).await?;

                let mut file = tokio::fs::File::open(path).await?;
                tokio::io::copy(&mut file, stream).await?;
            }
        }
        Ok(())
    }
}
