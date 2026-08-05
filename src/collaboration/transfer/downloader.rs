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
    client: Arc<reqwest::Client>,
}

#[allow(dead_code)]
impl ChunkedDownloader {
    /// 创建下载器
    pub fn new(chunk_size: u64) -> Self {
        Self {
            chunk_size,
            connect_timeout: Duration::from_secs(10),
            client: crate::http_client::shared(),
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
        let head_resp = self.client.head(&url).send().await?;
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
        let response = self
            .client
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

    /// 下载单个分块（使用共享client）
    async fn download_chunk_with_client<F>(
        client: &reqwest::Client,
        url: &str,
        path: &Path,
        start: u64,
        end: u64,
        progress_callback: F,
    ) -> Result<u64>
    where
        F: Fn(u64, u64) + Send + Sync,
    {
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

    /// 下载单个分块（Arc回调版本，使用共享client）
    async fn download_chunk_arc_with_client<F>(
        client: &reqwest::Client,
        url: &str,
        path: &Path,
        start: u64,
        end: u64,
        progress_callback: Arc<F>,
    ) -> Result<u64>
    where
        F: Fn(u64, u64) + Send + Sync + 'static,
    {
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
        let response = self
            .client
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
