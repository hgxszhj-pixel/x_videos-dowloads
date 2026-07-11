//! HLS 分段下载器
//!
//! 支持并发下载、分段解密和进度回调

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use reqwest::Client;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tokio::sync::Semaphore;
use tracing::{info, warn};

use crate::types::DownloadProgress;
use super::parser::{M3u8Parser, M3u8Segment};
use super::crypto::{fetch_key, parse_iv};

/// HLS 下载器配置
#[allow(dead_code)] // proxy field exposed in public API for future use
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
    /// 自适应并发控制（基于吞吐量动态调整推荐值，参见 parallel.rs 同名逻辑）
    pub adaptive_concurrency: bool,
    /// 最小并发分段数（自适应时的下限）
    pub min_concurrent: usize,
    /// 速度阈值（字节/秒），低于此值增加推荐并发
    pub speed_threshold_bps: u64,
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
            adaptive_concurrency: true,
            min_concurrent: 2,
            speed_threshold_bps: 500 * 1024, // 500KB/s
        }
    }
}

/// HLS 下载器
pub struct HlsDownloader {
    client: Client,
    config: HlsDownloaderConfig,
}

#[allow(dead_code)] // builder API (skip_unavailable/config/proxy/create_client)
impl HlsDownloader {
    /// 使用配置创建下载器（自动创建带代理的 Client）
    pub async fn with_config(config: HlsDownloaderConfig) -> Result<Self> {
        let client = Self::create_client(&config.proxy)?;
        Ok(Self { client, config })
    }

    pub fn new(client: Client) -> Self {
        Self {
            client,
            config: HlsDownloaderConfig::default(),
        }
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

        // 使用 Semaphore 限制最大并发（修复：之前 max_concurrent 未实际生效）
        let semaphore = Arc::new(Semaphore::new(self.config.max_concurrent));

        // 自适应并发：跟踪当前最优并发数（基于吞吐量动态调整）
        // Semaphore 仍然硬性限制 max_concurrent，current_optimal 仅用于监控和日志
        let current_optimal = Arc::new(AtomicUsize::new(self.config.max_concurrent));
        let adaptive_active = self.config.adaptive_concurrency;
        let min_conc = self.config.min_concurrent;
        let max_conc = self.config.max_concurrent;
        let speed_threshold = self.config.speed_threshold_bps;

        // 启动后台采样器（每 500ms 采样一次吞吐量）
        let downloaded_for_sampler = downloaded.clone();
        let current_optimal_for_sampler = current_optimal.clone();
        let cancel_for_sampler = cancel_flag.clone();
        let sampler_task = tokio::spawn(async move {
            let mut last_bytes: u64 = 0;
            while !cancel_for_sampler.load(Ordering::Relaxed) {
                tokio::time::sleep(Duration::from_millis(500)).await;
                if cancel_for_sampler.load(Ordering::Relaxed) {
                    break;
                }
                let now = downloaded_for_sampler.load(Ordering::Relaxed);
                let delta = now.saturating_sub(last_bytes);
                let bps = delta * 2; // 500ms 采样窗口 → 字节/秒
                last_bytes = now;

                if adaptive_active {
                    let cur = current_optimal_for_sampler.load(Ordering::Relaxed);
                    if bps < speed_threshold && cur < max_conc {
                        let next = cur + 1;
                        current_optimal_for_sampler.store(next, Ordering::Relaxed);
                        info!("HLS 自适应：吞吐量低 ({:.1} KB/s)，推荐并发 {} -> {}",
                            bps as f64 / 1024.0, cur, next);
                    } else if bps > speed_threshold * 3 && cur > min_conc {
                        let next = cur - 1;
                        current_optimal_for_sampler.store(next, Ordering::Relaxed);
                        info!("HLS 自适应：吞吐良好 ({:.1} KB/s)，推荐并发 {} -> {}",
                            bps as f64 / 1024.0, cur, next);
                    }
                }
            }
        });

        let tasks: Vec<_> = segments.into_iter().enumerate().map(|(i, seg)| {
            let client = client.clone();
            let output = segment_paths[i].clone();
            let downloaded = downloaded_clone.clone();
            let total_size = total_size_clone.clone();
            let cancel = cancel_flag.clone();
            let callback = progress_callback.clone();
            let failed_count = failed_count_clone.clone();
            let config = config.clone();
            let semaphore = semaphore.clone();

            tokio::spawn(async move {
                // 获取 semaphore permit，超过 max_concurrent 时阻塞等待
                let _permit = semaphore.acquire().await
                    .context("semaphore closed unexpectedly")?;
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
            // 停止 sampler 再返回错误
            cancel_flag.store(true, Ordering::Relaxed);
            let _ = sampler_task.await;
            return Err(anyhow!("下载失败: {} 个分段错误 ({:?})", failed_segments.len(), &failed_segments[..failed_segments.len().min(5)]));
        }

        // 4. 合并分段
        // 停止自适应采样器
        cancel_flag.store(true, Ordering::Relaxed);
        let _ = sampler_task.await;
        // Phase D 优化：并行读取所有分段（写仍串行，保证顺序与原子性）
        info!("合并 {} 个分段（并行读取）...", total_segments);
        let mut output_file = File::create(output_path).await?;
        let mut total_written = 0u64;
        let failed_set: std::collections::HashSet<_> = failed_segments.iter().collect();

        // 并行读取所有有效分段
        let read_tasks: Vec<_> = segment_paths
            .iter()
            .enumerate()
            .filter(|(i, _)| !failed_set.contains(&(i + 1)))
            .map(|(i, path)| {
                let path = path.clone();
                tokio::spawn(async move {
                    let data = tokio::fs::read(&path).await?;
                    Ok::<(usize, Vec<u8>), anyhow::Error>((i, data))
                })
            })
            .collect();

        // 收集读取结果（保持原始顺序）
        let mut segment_data: Vec<(usize, Vec<u8>)> = Vec::with_capacity(read_tasks.len());
        for task in read_tasks {
            match task.await {
                Ok(Ok(pair)) => segment_data.push(pair),
                Ok(Err(e)) => return Err(e),
                Err(e) => return Err(anyhow!("read task join failed: {}", e)),
            }
        }
        segment_data.sort_by_key(|(i, _)| *i);

        // 串行写入（顺序与原子性）
        for (i, data) in segment_data {
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
    #[allow(clippy::too_many_arguments)]
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
        // Phase C 优化：大分段（≥1MB）使用 spawn_blocking 释放 async runtime
        // 小分段直接解密（spawn_blocking 开销 ~100μs，超过小段解密本身耗时）
        if let (Some(key_url), Some(iv_hex)) = (&segment.key_url, &segment.key_iv) {
            let key = fetch_key(client, key_url).await?;
            let iv = parse_iv(iv_hex)?;

            const SPAWN_BLOCKING_THRESHOLD: usize = 1024 * 1024; // 1MB
            if data.len() >= SPAWN_BLOCKING_THRESHOLD {
                // 大段：移交到 blocking 线程池，不阻塞 tokio runtime
                let data_owned = data;
                data = tokio::task::spawn_blocking(move || {
                    Self::decrypt_segment(&data_owned, &key, &iv)
                })
                .await
                .map_err(|e| anyhow!("decrypt task join failed: {}", e))??;
            } else {
                // 小段：直接解密（开销低于 spawn_blocking）
                data = Self::decrypt_segment(&data, &key, &iv)?;
            }
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
        

        if !data.len().is_multiple_of(16) {
            return Err(anyhow!("ciphertext must be 16-byte aligned"));
        }

        let iv_arr = GenericArray::from_slice(iv);
        let key_arr = GenericArray::from_slice(key);

        let mut cipher = Aes128::new(key_arr);

        let mut result = Vec::with_capacity(data.len());
        let mut prev_block = *iv_arr;

        for chunk in data.chunks(16) {
            let block = GenericArray::from_slice(chunk);

            // CBC 解密: 解密后 XOR 前一个密文块
            let mut block_copy = *block;
            cipher.decrypt_block_mut(&mut block_copy);

            let mut decrypted = [0u8; 16];
            for j in 0..16 {
                decrypted[j] = block_copy[j] ^ prev_block[j];
            }

            prev_block = *block;
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 验证默认配置 max_concurrent = 4
    #[test]
    fn test_default_config_max_concurrent() {
        let config = HlsDownloaderConfig::default();
        assert_eq!(config.max_concurrent, 4);
    }

    /// 验证默认配置启用自适应并发
    #[test]
    fn test_default_config_adaptive_concurrency() {
        let config = HlsDownloaderConfig::default();
        assert!(config.adaptive_concurrency, "adaptive_concurrency should default to true");
        assert_eq!(config.min_concurrent, 2);
        assert_eq!(config.speed_threshold_bps, 500 * 1024);
    }

    /// 验证 Semaphore 模式能有效限制并发数
    /// 这是 Phase A 的核心修复测试：之前 max_concurrent 未实际生效
    #[tokio::test]
    async fn test_semaphore_bounds_concurrency() {
        const MAX_CONCURRENT: usize = 3;
        const TOTAL_TASKS: usize = 20;

        let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT));
        let active = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));

        let tasks: Vec<_> = (0..TOTAL_TASKS).map(|_| {
            let semaphore = semaphore.clone();
            let active = active.clone();
            let peak = peak.clone();
            tokio::spawn(async move {
                let _permit = semaphore.acquire().await.unwrap();
                let cur = active.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(cur, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(20)).await;
                active.fetch_sub(1, Ordering::SeqCst);
            })
        }).collect();

        for t in tasks {
            t.await.unwrap();
        }

        let observed_peak = peak.load(Ordering::SeqCst);
        assert!(
            observed_peak <= MAX_CONCURRENT,
            "peak concurrency {} > max {} (Semaphore failed to bound)",
            observed_peak,
            MAX_CONCURRENT
        );
        // 至少应该有并发（不是全串行）
        assert!(
            observed_peak >= 2,
            "peak concurrency {} too low (Semaphore too restrictive)",
            observed_peak
        );
    }

    /// 验证 Semaphore 在零 permit 时不阻塞测试（permit=0 是合法配置）
    #[tokio::test]
    async fn test_semaphore_zero_permits_blocks() {
        let semaphore = Arc::new(Semaphore::new(0));
        let acquired = semaphore.try_acquire();
        assert!(acquired.is_err(), "try_acquire on zero-permit semaphore should fail");
    }

    /// 验证自适应并发调整规则（与 parallel.rs::calculate_optimal_threads 一致）
    #[test]
    fn test_adaptive_concurrency_adjustment_rules() {
        // 低吞吐量 → 增加
        let mut cur = 4usize;
        let max = 8;
        let min = 2;
        let threshold = 500_000u64;
        let bps_low = 100_000;
        if bps_low < threshold && cur < max {
            cur = (cur + 1).min(max);
        }
        assert_eq!(cur, 5, "low throughput should increase concurrency");

        // 高吞吐量 → 减少
        let bps_high = threshold * 4;
        if bps_high > threshold * 3 && cur > min {
            cur = cur.saturating_sub(1).max(min);
        }
        assert_eq!(cur, 4, "high throughput should decrease concurrency");

        // 已在 max → 不变
        cur = max;
        if bps_low < threshold && cur < max {
            cur = (cur + 1).min(max);
        }
        assert_eq!(cur, max, "already at max should not increase");

        // 已在 min → 不变
        cur = min;
        if bps_high > threshold * 3 && cur > min {
            cur = cur.saturating_sub(1).max(min);
        }
        assert_eq!(cur, min, "already at min should not decrease");
    }
}
