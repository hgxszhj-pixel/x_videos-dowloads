//! P2P 分块下载器
//!
//! 支持从对等设备下载文件，使用 Range 请求实现断点续传

use anyhow::{Context, Result};
use futures::StreamExt;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncWriteExt;

/// 分块下载器
#[allow(dead_code)]
pub struct ChunkedDownloader {
    chunk_size: u64,
    connect_timeout: Duration,
}

#[allow(dead_code)]
impl ChunkedDownloader {
    /// 创建下载器
    pub fn new(chunk_size: u64) -> Self {
        Self {
            chunk_size,
            connect_timeout: Duration::from_secs(10),
        }
    }

    /// 从对等设备下载文件（支持断点续传）
    pub async fn download_from_peer<F>(
        &self,
        ip: &str,
        port: u16,
        task_id: &str,
        output_path: &Path,
        progress_callback: F,
    ) -> Result<u64>
    where
        F: Fn(u64, u64),
    {
        let url = format!("http://{}:{}/file/{}", ip, port, task_id);

        // 首先获取文件大小
        let client = reqwest::Client::new();
        let head_resp = client.head(&url).send().await?;
        let total_size = head_resp
            .content_length()
            .context("服务器未返回 Content-Length")?;

        // 检查已下载的部分（断点续传）
        let existing_size = if output_path.exists() {
            let metadata = tokio::fs::metadata(output_path).await?;
            metadata.len()
        } else {
            0
        };

        if existing_size >= total_size {
            // 文件已完整
            progress_callback(total_size, total_size);
            return Ok(total_size);
        }

        // 追加模式打开文件
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(output_path)
            .await?;

        // 从断点开始下载
        let response = client
            .get(&url)
            .header("Range", format!("bytes={}-", existing_size))
            .send()
            .await?;

        let mut downloaded = existing_size;
        let mut stream = response.bytes_stream();

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result?;
            file.write_all(&chunk).await?;
            downloaded += chunk.len() as u64;
            progress_callback(downloaded, total_size);
        }

        Ok(downloaded)
    }

    /// 并行多连接下载（用于大文件加速）
    #[allow(dead_code)]
    pub async fn download_parallel<F>(
        &self,
        ip: &str,
        port: u16,
        task_id: &str,
        output_path: &Path,
        num_connections: usize,
        progress_callback: F,
    ) -> Result<u64>
    where
        F: Fn(u64, u64) + Send + Sync + Clone + 'static,
    {
        let url = format!("http://{}:{}/file/{}", ip, port, task_id);

        // 获取文件大小
        let client = reqwest::Client::new();
        let head_resp = client.head(&url).send().await?;
        let total_size = head_resp
            .content_length()
            .context("服务器未返回 Content-Length")?;

        // 如果文件小于 chunk_size * 2，使用普通下载
        if total_size <= self.chunk_size * 2 {
            return self.download_from_peer(ip, port, task_id, output_path, progress_callback).await;
        }

        // 创建临时目录存储分块
        let temp_dir = output_path
            .parent()
            .map(|p| p.join(format!(".tmp_{}", task_id)))
            .unwrap_or_else(|| Path::new(".").join(format!(".tmp_{}", task_id)));
        tokio::fs::create_dir_all(&temp_dir).await?;

        // 计算分块
        let chunk_size = self.chunk_size;
        let num_chunks = total_size.div_ceil(chunk_size) as usize;

        // 并行下载各分块
        use tokio::task::JoinSet;
        let mut join_set = JoinSet::new();

        for (i, chunk_idx) in (0..num_chunks).enumerate() {
            if i >= num_connections {
                break;
            }
            let url = url.clone();
            let temp_path = temp_dir.join(format!("chunk_{}", chunk_idx));
            let start = (chunk_idx as u64) * chunk_size;
            let end = std::cmp::min(start + chunk_size - 1, total_size - 1);
            let cb = Arc::new(progress_callback.clone());

            join_set.spawn(async move {
                Self::download_chunk_arc(&url, &temp_path, start, end, cb).await
            });
        }

        // 收集结果
        let mut total_downloaded: u64 = 0;
        while let Some(result) = join_set.join_next().await {
            match result {
                Ok(Ok(size)) => total_downloaded += size,
                Ok(Err(e)) => eprintln!("分块下载失败: {}", e),
                Err(e) => eprintln!("任务 join 失败: {}", e),
            }
        }

        // 合并分块
        let mut final_file = tokio::fs::File::create(output_path).await?;
        for i in 0..num_chunks {
            let chunk_path = temp_dir.join(format!("chunk_{}", i));
            if chunk_path.exists() {
                let mut chunk_file = tokio::fs::File::open(&chunk_path).await?;
                tokio::io::copy(&mut chunk_file, &mut final_file).await?;
                let _ = tokio::fs::remove_file(&chunk_path).await;
            }
        }
        let _ = tokio::fs::remove_dir_all(&temp_dir).await;

        Ok(total_downloaded)
    }

    /// 下载单个分块
    async fn download_chunk<F>(url: &str, path: &Path, start: u64, end: u64, progress_callback: F) -> Result<u64>
    where
        F: Fn(u64, u64) + Send + Sync,
    {
        let client = reqwest::Client::new();
        let response = client
            .get(url)
            .header("Range", format!("bytes={}-{}", start, end))
            .send()
            .await?;

        let content_length = end - start + 1;
        let mut file = tokio::fs::File::create(path).await?;
        let mut downloaded: u64 = 0;

        let mut stream = response.bytes_stream();
        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result?;
            file.write_all(&chunk).await?;
            downloaded += chunk.len() as u64;
            progress_callback(downloaded, content_length);
        }

        Ok(downloaded)
    }

    /// 下载单个分块（Arc回调版本）
    async fn download_chunk_arc<F>(url: &str, path: &Path, start: u64, end: u64, progress_callback: Arc<F>) -> Result<u64>
    where
        F: Fn(u64, u64) + Send + Sync + 'static,
    {
        let client = reqwest::Client::new();
        let response = client
            .get(url)
            .header("Range", format!("bytes={}-{}", start, end))
            .send()
            .await?;

        let content_length = end - start + 1;
        let mut file = tokio::fs::File::create(path).await?;
        let mut downloaded: u64 = 0;

        let mut stream = response.bytes_stream();
        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result?;
            file.write_all(&chunk).await?;
            downloaded += chunk.len() as u64;
            progress_callback(downloaded, content_length);
        }

        Ok(downloaded)
    }

    /// 下载文件（兼容旧接口）
    pub async fn download_file<F>(
        &self,
        url: &str,
        output_path: &Path,
        from_byte: u64,
        progress_callback: F,
    ) -> Result<u64>
    where
        F: Fn(u64, u64),
    {
        let client = reqwest::Client::new();
        let response = client
            .get(url)
            .header("Range", format!("bytes={}-", from_byte))
            .send()
            .await?;

        let total_size = response.content_length().unwrap_or(0);
        let mut file = tokio::fs::File::create(output_path).await?;
        let mut downloaded: u64 = 0;

        let mut stream = response.bytes_stream();
        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result?;
            file.write_all(&chunk).await?;
            downloaded += chunk.len() as u64;
            progress_callback(downloaded, total_size);
        }

        Ok(downloaded)
    }
}
