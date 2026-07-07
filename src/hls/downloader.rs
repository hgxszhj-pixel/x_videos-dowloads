//! HLS 分段下载器
//!
//! 支持并发下载、分段解密和进度回调

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use reqwest::Client;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tracing::{info, warn};

use crate::types::DownloadProgress;
use super::parser::{M3u8Parser, M3u8Segment};
use super::crypto::{fetch_key, parse_iv};

/// HLS 下载器配置
#[derive(Debug, Clone)]
pub struct HlsDownloaderConfig {
    /// 最大并发分段数
    pub max_concurrent: usize,
    /// 分段重试次数
    pub retry_count: u32,
    /// 请求超时秒数
    pub timeout_secs: u64,
    /// 是否跳过不可用分段
    pub skip_unavailable: bool,
    /// 重试退避初始延迟（毫秒）
    pub retry_base_delay_ms: u64,
    /// 重试退避最大延迟（毫秒）
    pub retry_max_delay_ms: u64,
    /// 代理 URL (e.g., "socks5://127.0.0.1:1080")
    pub proxy: Option<String>,
    /// HTTP Referer 头（用于绕过防盗链）
    pub referer: Option<String>,
    /// 最大允许失败分段数（超过此数则停止下载）
    pub max_failed_segments: u32,
}

impl Default for HlsDownloaderConfig {
    fn default() -> Self {
        Self {
            max_concurrent: 4,
            retry_count: 10,
            timeout_secs: 60,
            skip_unavailable: false,
            retry_base_delay_ms: 100,  // 初始 100ms
            retry_max_delay_ms: 20000, // 最大 20s
            proxy: None,
            referer: None,
            max_failed_segments: 5,    // 默认 5 次失败后停止
        }
    }
}

/// HLS 下载器
pub struct HlsDownloader {
    client: Client,
    config: HlsDownloaderConfig,
}

impl HlsDownloader {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            config: HlsDownloaderConfig::default(),
        }
    }

    /// 使用配置创建下载器（自动创建带代理的 Client）
    pub async fn with_config(config: HlsDownloaderConfig) -> Result<Self> {
        let client = Self::create_client(&config.proxy)?;
        Ok(Self { client, config })
    }

    pub fn max_concurrent(mut self, n: usize) -> Self {
        self.config.max_concurrent = n;
        self
    }

    pub fn retry_count(mut self, n: u32) -> Self {
        self.config.retry_count = n;
        self
    }

    pub fn skip_unavailable(mut self, skip: bool) -> Self {
        self.config.skip_unavailable = skip;
        self
    }

    pub fn config(mut self, config: HlsDownloaderConfig) -> Self {
        self.config = config;
        self
    }

    pub fn proxy(mut self, proxy_url: &str) -> Self {
        self.config.proxy = Some(proxy_url.to_string());
        self
    }

    pub fn referer(mut self, referer_url: &str) -> Self {
        self.config.referer = Some(referer_url.to_string());
        self
    }

    /// 创建带代理的 Client
    fn create_client(proxy: &Option<String>) -> Result<Client> {
        let mut builder = Client::builder()
            .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36")
            .tcp_keepalive(Duration::from_secs(120))
            .connect_timeout(Duration::from_secs(30))
            .timeout(Duration::from_secs(600));

        if let Some(proxy_url) = proxy {
            // 确保代理 URL 有协议前缀
            let proxy_with_scheme = if proxy_url.starts_with("socks5://")
                || proxy_url.starts_with("http://")
                || proxy_url.starts_with("https://")
            {
                proxy_url.to_string()
            } else {
                format!("http://{}", proxy_url)
            };
            builder = builder.proxy(reqwest::Proxy::all(&proxy_with_scheme)?);
        }

        builder.build().map_err(|e| anyhow!("Failed to build HTTP client: {}", e))
    }

    /// 下载 m3u8 播放列表并合并成分段文件
    pub async fn download(
        &self,
        url: &str,
        output_path: &PathBuf,
        progress_callback: Option<Arc<dyn Fn(DownloadProgress) + Send + Sync>>,
    ) -> Result<u64> {
        // 1. 获取并解析 m3u8
        info!("获取 m3u8: {}", url);
        let playlist = M3u8Parser::fetch_and_parse(&self.client, url).await
            .context("failed to fetch m3u8")?;

        let total_segments = playlist.segments.len();
        let total_duration = playlist.total_duration;
        info!("解析完成: {} 分段, 总时长 {:.1}s", total_segments, total_duration);

        // 2. 创建临时目录存储分段
        let temp_dir = output_path.with_extension("tmp");
        tokio::fs::create_dir_all(&temp_dir).await?;

        // 3. 并发下载分段
        let downloaded = Arc::new(AtomicU64::new(0));
        let total_size = Arc::new(AtomicU64::new(0));
        let cancel_flag = Arc::new(AtomicBool::new(false));
        let failed_count = Arc::new(AtomicU64::new(0));

        let segment_paths: Vec<PathBuf> = (0..total_segments)
            .map(|i| temp_dir.join(format!("segment_{:04}.ts", i)))
            .collect();

        // 并发下载
        let client = &self.client;
        let segments = playlist.segments.clone();
        let downloaded_clone = downloaded.clone();
        let total_size_clone = total_size.clone();
        let failed_count_clone = failed_count.clone();
        let config = self.config.clone();

        let tasks: Vec<_> = segments.into_iter().enumerate().map(|(i, seg)| {
            let client = client.clone();
            let output = segment_paths[i].clone();
            let downloaded = downloaded_clone.clone();
            let total_size = total_size_clone.clone();
            let cancel = cancel_flag.clone();
            let callback = progress_callback.clone();
            let failed_count = failed_count_clone.clone();
            let config = config.clone();

            tokio::spawn(async move {
                Self::download_segment_with_retry(
                    &client,
                    &seg,
                    &output,
                    downloaded,
                    total_size,
                    cancel,
                    callback,
                    &failed_count,
                    &config,
                ).await
            })
        }).collect();

        // 等待所有分段下载完成
        let mut failed_segments = Vec::new();
        for (i, task) in tasks.into_iter().enumerate() {
            match task.await {
                Ok(Ok(_size)) => {
                    info!("分段 {}/{} 下载完成", i + 1, total_segments);
                }
                Ok(Err(e)) => {
                    warn!("分段 {} 下载失败: {}", i + 1, e);
                    failed_segments.push(i + 1);
                }
                Err(e) => {
                    warn!("分段 {} 任务失败: {}", i + 1, e);
                    failed_segments.push(i + 1);
                }
            }

            // 检查是否超过最大失败数
            let current_failed = failed_segments.len() as u32;
            if current_failed >= self.config.max_failed_segments {
                warn!("失败分段数 ({}) 超过限制 ({})，停止下载",
                      current_failed, self.config.max_failed_segments);
                // 设置取消标志
                cancel_flag.store(true, Ordering::Relaxed);
                break;
            }
        }

        // 决定是否失败或继续
        if failed_segments.is_empty() {
            // 成功
        } else if self.config.skip_unavailable {
            warn!("跳过 {} 个不可用分段，继续合并", failed_segments.len());
        } else {
            return Err(anyhow!("下载失败: {} 个分段错误 ({:?})", failed_segments.len(), &failed_segments[..failed_segments.len().min(5)]));
        }

        // 4. 合并分段
        info!("合并 {} 个分段...", total_segments);
        let mut output_file = File::create(output_path).await?;
        let mut total_written = 0u64;
        let failed_set: std::collections::HashSet<_> = failed_segments.iter().collect();

        for (i, path) in segment_paths.iter().enumerate() {
            // 跳过不可用的分段
            if failed_set.contains(&(i + 1)) {
                warn!("跳过不可用分段: {}", i + 1);
                continue;
            }
            let data = tokio::fs::read(path).await?;
            output_file.write_all(&data).await?;
            total_written += data.len() as u64;
            info!("合并进度: {}/{}", i + 1, total_segments);
        }

        // 5. 清理临时文件
        tokio::fs::remove_dir_all(&temp_dir).await.ok();

        info!("下载完成: {} bytes -> {:?}", total_written, output_path);
        Ok(total_written)
    }

    /// 下载单个分段（带指数退避重试）
    async fn download_segment_with_retry(
        client: &Client,
        segment: &M3u8Segment,
        output: &PathBuf,
        _downloaded: Arc<AtomicU64>,
        _total_size: Arc<AtomicU64>,
        cancel_flag: Arc<AtomicBool>,
        _progress_callback: Option<Arc<dyn Fn(DownloadProgress) + Send + Sync>>,
        failed_count: &Arc<AtomicU64>,
        config: &HlsDownloaderConfig,
    ) -> Result<u64> {
        let timeout = Duration::from_secs(config.timeout_secs);
        let base_delay = config.retry_base_delay_ms;
        let max_delay = config.retry_max_delay_ms;

        for attempt in 0..config.retry_count {
            // 检查取消标志
            if cancel_flag.load(Ordering::Relaxed) {
                return Err(anyhow!("下载已取消"));
            }

            match tokio::time::timeout(
                timeout,
                Self::download_single_segment(client, segment, output, &config.referer),
            ).await {
                Ok(Ok(size)) => return Ok(size),
                Ok(Err(e)) => {
                    let err_msg = e.to_string();
                    // 区分可重试和不可重试错误
                    let is_retryable = Self::is_retryable_error(&err_msg);
                    warn!(
                        "分段 {} 下载错误 (attempt {}/{}): {} [{}]",
                        segment.sequence,
                        attempt + 1,
                        config.retry_count,
                        e,
                        if is_retryable { "可重试" } else { "不可重试" }
                    );

                    // 不可重试错误直接失败
                    if !is_retryable {
                        failed_count.fetch_add(1, Ordering::Relaxed);
                        return Err(e);
                    }
                }
                Err(_) => {
                    warn!("分段 {} 下载超时 (attempt {}/{})",
                          segment.sequence, attempt + 1, config.retry_count);
                }
            }

            // 指数退避: delay = min(base * 2^attempt, max_delay)
            if attempt < config.retry_count - 1 {
                let delay_ms = (base_delay * 2u64.pow(attempt)).min(max_delay);
                info!("分段 {} 等待 {:.1}s 后重试...", segment.sequence, delay_ms as f64 / 1000.0);
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
            }
        }

        failed_count.fetch_add(1, Ordering::Relaxed);
        Err(anyhow!("分段 {} 下载失败 (已重试 {} 次)", segment.sequence, config.retry_count))
    }

    /// 判断错误是否可重试
    fn is_retryable_error(err: &str) -> bool {
        let err_lower = err.to_lowercase();

        // 不可重试错误（直接失败）
        if err_lower.contains("connection refused")
            || err_lower.contains("invalid url")
            || err_lower.contains("parse error")
            || err_lower.contains("unauthorized")
            || err_lower.contains("forbidden")
            || err_lower.contains("not found")
            || err_lower.contains("400 bad request")
            || err_lower.contains("404")
        {
            return false;
        }

        // 可重试错误
        true
    }

    /// 下载单个分段
    async fn download_single_segment(
        client: &Client,
        segment: &M3u8Segment,
        output: &PathBuf,
        custom_referer: &Option<String>,
    ) -> Result<u64> {
        let mut data = Vec::new();

        // 使用自定义 Referer 或构造（同源防盗链）
        let referer = custom_referer.clone().unwrap_or_else(|| {
            segment.url.split('?').next()
                .map(|path| {
                    if let Some(idx) = path.rfind('/') {
                        let base = &path[..idx];
                        if base.starts_with("http") {
                            base.to_string()
                        } else if let Some(scheme_end) = segment.url.find("://") {
                            let scheme = &segment.url[..scheme_end + 3];
                            if let Some(host_end) = segment.url[scheme_end + 3..].find('/') {
                                format!("{}{}", scheme, &segment.url[scheme_end + 3..scheme_end + 3 + host_end])
                            } else {
                                segment.url.clone()
                            }
                        } else {
                            segment.url.clone()
                        }
                    } else {
                        segment.url.clone()
                    }
                })
                .unwrap_or_else(|| segment.url.clone())
        });

        // 下载分段（带 Referer 防防盗链）
        let response = client.get(&segment.url)
            .header("Referer", &referer)
            .send()
            .await?;
        if !response.status().is_success() {
            return Err(anyhow!("HTTP {}", response.status()));
        }

        let bytes = response.bytes().await?;
        data.extend_from_slice(&bytes);

        // 如果加密，解密
        if let (Some(key_url), Some(iv_hex)) = (&segment.key_url, &segment.key_iv) {
            let key = fetch_key(client, key_url).await?;
            let iv = parse_iv(iv_hex)?;
            data = Self::decrypt_segment(&data, &key, &iv)?;
        }

        // 写入文件
        let mut file = File::create(output).await?;
        file.write_all(&data).await?;
        file.flush().await?;

        Ok(data.len() as u64)
    }

    /// 解密分段 (AES-128-CBC + PKCS7 padding)
    fn decrypt_segment(data: &[u8], key: &[u8; 16], iv: &[u8; 16]) -> Result<Vec<u8>> {
        use aes::Aes128;
        use aes::cipher::{BlockDecryptMut, KeyInit};
        use generic_array::GenericArray;
        use typenum::U16;

        if data.len() % 16 != 0 {
            return Err(anyhow!("ciphertext must be 16-byte aligned"));
        }

        let iv_arr = GenericArray::from_slice(iv);
        let key_arr = GenericArray::from_slice(key);

        let mut cipher = Aes128::new(&key_arr);

        let mut result = Vec::with_capacity(data.len());
        let mut prev_block = iv_arr.clone();

        for chunk in data.chunks(16) {
            let block = GenericArray::from_slice(chunk);

            // CBC 解密: 解密后 XOR 前一个密文块
            let mut block_copy = block.clone();
            cipher.decrypt_block_mut(&mut block_copy);

            let mut decrypted = [0u8; 16];
            for j in 0..16 {
                decrypted[j] = block_copy[j] ^ prev_block[j];
            }

            prev_block = block.clone();
            result.extend_from_slice(&decrypted);
        }

        // 去除 PKCS7 padding
        let padding = result.last().copied().unwrap_or(0) as usize;
        if padding > 0 && padding <= 16 && result.len() >= padding {
            // 验证 padding 是否正确
            let is_valid = result[result.len() - padding..].iter().all(|&b| b == padding as u8);
            if is_valid {
                result.truncate(result.len() - padding);
            }
        }

        Ok(result)
    }
}
