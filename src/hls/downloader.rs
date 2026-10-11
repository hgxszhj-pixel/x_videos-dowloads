//! HLS 分段下载器
//!
//! 支持并发下载、分段解密和进度回调

use std::path::PathBuf;
#[allow(unused_imports)] // AtomicUsize is used in #[cfg(test)] mod tests via `use super::*;`
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use reqwest::Client;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tokio::sync::Semaphore;
use tracing::{info, warn};

use super::crypto::{fetch_key, parse_iv};
use super::parser::{M3u8Parser, M3u8Segment};
use crate::types::{DownloadProgress, DownloadStatus};

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
    /// HTTP Cookie 头（用于登录平台鉴权 CDN，如 tulingpyton 的 xet.tech / xed.plus）
    pub cookie_header: Option<String>,
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
            cookie_header: None,
            max_failed_segments: 5, // 默认 5 次失败后停止
        }
    }
}

/// HLS 下载器
pub struct HlsDownloader {
    client: Client,
    config: Arc<HlsDownloaderConfig>,
}

#[expect(dead_code)] // builder API (skip_unavailable/config/proxy/create_client)
impl HlsDownloader {
    /// 使用配置创建下载器（自动创建带代理的 Client）
    pub async fn with_config(config: HlsDownloaderConfig) -> Result<Self> {
        let client = Self::create_client(&config.proxy)?;
        Ok(Self {
            client,
            config: Arc::new(config),
        })
    }

    pub fn new(client: Client) -> Self {
        Self {
            client,
            config: Arc::new(HlsDownloaderConfig::default()),
        }
    }

    pub fn max_concurrent(mut self, n: usize) -> Self {
        Arc::make_mut(&mut self.config).max_concurrent = n;
        self
    }

    pub fn retry_count(mut self, n: u32) -> Self {
        Arc::make_mut(&mut self.config).retry_count = n;
        self
    }

    pub fn skip_unavailable(mut self, skip: bool) -> Self {
        Arc::make_mut(&mut self.config).skip_unavailable = skip;
        self
    }

    pub fn config(mut self, config: HlsDownloaderConfig) -> Self {
        self.config = Arc::new(config);
        self
    }

    pub fn proxy(mut self, proxy_url: &str) -> Self {
        Arc::make_mut(&mut self.config).proxy = Some(proxy_url.to_string());
        self
    }

    pub fn referer(mut self, referer_url: &str) -> Self {
        Arc::make_mut(&mut self.config).referer = Some(referer_url.to_string());
        self
    }

    /// 设置 Cookie 头(用于登录平台鉴权 CDN,如 tulingpyton 的 xet.tech / xed.plus)
    pub fn cookie_header(mut self, cookie: String) -> Self {
        Arc::make_mut(&mut self.config).cookie_header = Some(cookie);
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

        builder
            .build()
            .map_err(|e| anyhow!("Failed to build HTTP client: {}", e))
    }

    /// 下载 m3u8 播放列表并合并成分段文件
    ///
    /// `progress_callback` 在每个分段下载完成后被回调一次（per-segment 粒度）。
    /// 由于 m3u8 总字节数通常未知（无 Content-Length），按"已下载分段 / 总分段"估算 percent，
    /// 并以累计字节数计算瞬时速度。
    pub async fn download(
        &self,
        url: &str,
        output_path: &PathBuf,
        progress_callback: Option<Arc<dyn Fn(DownloadProgress) + Send + Sync>>,
    ) -> Result<u64> {
        // 1. 获取并解析 m3u8
        info!("获取 m3u8: {}", url);

        // 自动探测防盗链 CDN（零调用方改动）：
        // 小鹅通 CDN（xet.tech / xed.plus / xiaoe-materials.com）要求 Referer = vip.tulingpyton.cn
        // 且需登录 Cookie（host-filtered ego-lite）。未显式配置时自动填默认值。
        let config: Arc<HlsDownloaderConfig> = if needs_xiaoe_cdn_headers(url)
            && (self.config.referer.is_none() || self.config.cookie_header.is_none())
        {
            let mut cfg = (*self.config).clone();
            if cfg.referer.is_none() {
                cfg.referer = Some("https://vip.tulingpyton.cn/".to_string());
            }
            #[cfg(target_os = "macos")]
            if cfg.cookie_header.is_none() {
                if let Ok(cookie) = crate::drission_client::decrypt_ego_lite_cookies_for_host(
                    "vip.tulingpyton.cn",
                    &["tulingpyton.cn", "xiaoeknow.com"],
                ) {
                    cfg.cookie_header = Some(cookie);
                }
            }
            Arc::new(cfg)
        } else {
            self.config.clone()
        };

        let playlist = M3u8Parser::fetch_and_parse(&self.client, url)
            .await
            .context("failed to fetch m3u8")?;

        let total_segments = playlist.segments.len();
        let total_duration = playlist.total_duration;
        info!(
            "解析完成: {} 分段, 总时长 {:.1}s",
            total_segments, total_duration
        );

        // 2. 创建临时目录存储分段
        let temp_dir = output_path.with_extension("tmp");
        tokio::fs::create_dir_all(&temp_dir).await?;

        // 3. 并发下载分段
        let cancel_flag = Arc::new(AtomicBool::new(false));
        let failed_count = Arc::new(AtomicU64::new(0));

        let segment_paths: Vec<PathBuf> = (0..total_segments)
            .map(|i| temp_dir.join(format!("segment_{:04}.ts", i)))
            .collect();

        // 并发下载
        let client = &self.client;
        let segments = playlist.segments.clone();
        let failed_count_clone = failed_count.clone();

        // Per-segment 进度跟踪：累计下载字节 + 起始时间（用于算 speed）
        let progress_bytes = Arc::new(AtomicU64::new(0));
        let start_time = std::time::Instant::now();
        // 节流：已上报分段数（避免每段都回调；目标 ≤ 50 次总回调）
        let last_reported = Arc::new(AtomicU64::new(0));
        let report_every = std::cmp::max(1, total_segments / 50);

        // 使用 Semaphore 限制最大并发（修复：之前 max_concurrent 未实际生效）
        let semaphore = Arc::new(Semaphore::new(self.config.max_concurrent));

        let tasks: Vec<_> = segments
            .into_iter()
            .enumerate()
            .map(|(i, seg)| {
                let client = client.clone();
                let output = segment_paths[i].clone();
                let cancel = cancel_flag.clone();
                let failed_count = failed_count_clone.clone();
                let config = config.clone();
                let semaphore = semaphore.clone();
                let progress_bytes = progress_bytes.clone();
                let last_reported = last_reported.clone();
                let progress_cb = progress_callback.clone();

                tokio::spawn(async move {
                    // 获取 semaphore permit，超过 max_concurrent 时阻塞等待
                    let _permit = semaphore
                        .acquire()
                        .await
                        .context("semaphore closed unexpectedly")?;
                    let result = Self::download_segment_with_retry(
                        &client,
                        &seg,
                        &output,
                        cancel,
                        &failed_count,
                        &config,
                    )
                    .await;

                    // Per-segment 进度回调（仅 Ok 时上报；节流至 ≤ 50 次）
                    if let Ok(size) = &result {
                        let bytes = progress_bytes.fetch_add(*size, Ordering::Relaxed) + size;
                        let prev = last_reported.load(Ordering::Relaxed);
                        let should_report = prev + 1 >= total_segments as u64
                            || (i as u64 + 1).is_multiple_of(report_every as u64);
                        if should_report
                            && last_reported.swap(i as u64 + 1, Ordering::Relaxed) == prev
                        {
                            if let Some(ref cb) = progress_cb {
                                let elapsed = start_time.elapsed().as_secs_f64();
                                let speed = if elapsed > 0.0 {
                                    bytes as f64 / elapsed
                                } else {
                                    0.0
                                };
                                let mut p = DownloadProgress::new("hls".to_string())
                                    .with_status(DownloadStatus::Downloading);
                                p.bytes_downloaded = bytes;
                                p.speed = speed;
                                // 按"已下载分段序号 / 总分段"估算（i+1 是当前完成的最高序号近似）
                                p.percent = if total_segments > 0 {
                                    (i as f64 + 1.0) / total_segments as f64
                                } else {
                                    0.0
                                };
                                cb(p);
                            }
                        }
                    }

                    result
                })
            })
            .collect();

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
                warn!(
                    "失败分段数 ({}) 超过限制 ({})，停止下载",
                    current_failed, self.config.max_failed_segments
                );
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
            return Err(anyhow!(
                "下载失败: {} 个分段错误 ({:?})",
                failed_segments.len(),
                &failed_segments[..failed_segments.len().min(5)]
            ));
        }

        // 4. 合并分段
        // 修复: 原实现并行读所有分段到 Vec<u8> → 4K 视频 1000+ 分段时 OOM
        // 改为顺序读取 + 立即写入，内存占用恒定为单个分段大小
        info!("合并 {} 个分段（顺序读取）...", total_segments);
        let mut output_file = File::create(output_path).await?;
        let mut total_written = 0u64;
        let failed_set: std::collections::HashSet<_> = failed_segments.iter().collect();

        for (i, path) in segment_paths.iter().enumerate() {
            if failed_set.contains(&(i + 1)) {
                continue; // 跳过失败的
            }
            let data = tokio::fs::read(path).await?;
            output_file.write_all(&data).await?;
            total_written += data.len() as u64;
            drop(data); // 显式释放（对大文件场景降低峰值内存）
            info!("合并进度: {}/{}", i + 1, total_segments);
        }

        // 5. 清理临时文件
        tokio::fs::remove_dir_all(&temp_dir).await.ok();

        info!("下载完成: {} bytes -> {:?}", total_written, output_path);
        Ok(total_written)
    }

    /// 下载单个分段（带指数退避重试 + Retry-After 头解析）
    #[allow(clippy::too_many_arguments)]
    async fn download_segment_with_retry(
        client: &Client,
        segment: &M3u8Segment,
        output: &PathBuf,
        cancel_flag: Arc<AtomicBool>,
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

            // 本轮响应的 Retry-After 秒数（仅 Ok(Err) 时设置;超时归零）
            let mut last_retry_after_secs: Option<u64> = None;

            match tokio::time::timeout(
                timeout,
                Self::download_single_segment(
                    client,
                    segment,
                    output,
                    &config.referer,
                    &config.cookie_header,
                ),
            )
            .await
            {
                Ok(Ok(size)) => return Ok(size),
                Ok(Err(e)) => {
                    let err_msg = e.to_string();
                    // 区分可重试和不可重试错误
                    let is_retryable = Self::is_retryable_error(&err_msg);
                    // 优先采用服务端 Retry-After 头指示的等待时间（429/503 常见）
                    last_retry_after_secs = Self::extract_retry_after_from_err(&err_msg);
                    warn!(
                        "分段 {} 下载错误 (attempt {}/{}): {} [{}]",
                        segment.sequence,
                        attempt + 1,
                        config.retry_count,
                        e,
                        if is_retryable {
                            "可重试"
                        } else {
                            "不可重试"
                        }
                    );

                    // 不可重试错误直接失败
                    if !is_retryable {
                        failed_count.fetch_add(1, Ordering::Relaxed);
                        return Err(e);
                    }
                }
                Err(_) => {
                    warn!(
                        "分段 {} 下载超时 (attempt {}/{})",
                        segment.sequence,
                        attempt + 1,
                        config.retry_count
                    );
                    // last_retry_after_secs 保持 None → 走指数退避
                }
            }

            // 退避策略:Retry-After > 指数退避(都受 max_delay 约束)
            if attempt < config.retry_count - 1 {
                let delay_ms = match last_retry_after_secs {
                    Some(secs) => secs.saturating_mul(1000).min(max_delay),
                    None => (base_delay * 2u64.pow(attempt)).min(max_delay),
                };
                let reason = if last_retry_after_secs.is_some() {
                    "retry-after"
                } else {
                    "exp-backoff"
                };
                info!(
                    "分段 {} 等待 {:.1}s 后重试 ({})...",
                    segment.sequence,
                    delay_ms as f64 / 1000.0,
                    reason
                );
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
            }
        }

        failed_count.fetch_add(1, Ordering::Relaxed);
        Err(anyhow!(
            "分段 {} 下载失败 (已重试 {} 次)",
            segment.sequence,
            config.retry_count
        ))
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

    /// 解析 HTTP `Retry-After` 头的秒数（仅支持纯数字格式，date 格式需 chrono 不引入）。
    /// 返回 `None` 表示无值/格式不识别。
    fn parse_retry_after_secs(value: &str) -> Option<u64> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return None;
        }
        trimmed.parse::<u64>().ok()
    }

    /// 从 `download_single_segment` 生成的错误消息中提取 Retry-After 秒数。
    /// 消息格式：`"HTTP 429 (retry-after: 5s)"`。其他格式返回 `None`。
    fn extract_retry_after_from_err(err_msg: &str) -> Option<u64> {
        let start = err_msg.find("retry-after:")? + "retry-after:".len();
        let rest = &err_msg[start..];
        let end = rest.find('s').unwrap_or(rest.len());
        let num_str = rest[..end].trim().trim_end_matches(')');
        num_str.parse::<u64>().ok()
    }

    /// 下载单个分段
    async fn download_single_segment(
        client: &Client,
        segment: &M3u8Segment,
        output: &PathBuf,
        custom_referer: &Option<String>,
        custom_cookie: &Option<String>,
    ) -> Result<u64> {
        let mut data = Vec::new();

        // 使用自定义 Referer 或构造（同源防盗链）
        let referer = custom_referer.clone().unwrap_or_else(|| {
            segment
                .url
                .split('?')
                .next()
                .map(|path| {
                    if let Some(idx) = path.rfind('/') {
                        let base = &path[..idx];
                        if base.starts_with("http") {
                            base.to_string()
                        } else if let Some(scheme_end) = segment.url.find("://") {
                            let scheme = &segment.url[..scheme_end + 3];
                            if let Some(host_end) = segment.url[scheme_end + 3..].find('/') {
                                format!(
                                    "{}{}",
                                    scheme,
                                    &segment.url[scheme_end + 3..scheme_end + 3 + host_end]
                                )
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

        // 下载分段（带 Referer + Cookie 防防盗链/鉴权 CDN）
        let mut req = client.get(&segment.url).header("Referer", &referer);
        if let Some(cookie) = custom_cookie {
            req = req.header("Cookie", cookie);
        }
        let response = req.send().await?;
        if !response.status().is_success() {
            let status = response.status();
            // 429/503 通常携带 Retry-After 头（秒数或 HTTP date）。
            // 仅支持秒数格式（date 格式需 chrono，避免引入重依赖）。
            let retry_after = response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(Self::parse_retry_after_secs);
            return match retry_after {
                Some(secs) => Err(anyhow!("HTTP {} (retry-after: {}s)", status, secs)),
                None => Err(anyhow!("HTTP {}", status)),
            };
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
        use aes::cipher::{BlockDecryptMut, KeyInit};
        use aes::Aes128;
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
            let is_valid = result[result.len() - padding..]
                .iter()
                .all(|&b| b == padding as u8);
            if is_valid {
                result.truncate(result.len() - padding);
            }
        }

        Ok(result)
    }
}

/// 检测 URL host 是否属于需要 tulingpyton 防盗链头的 CDN。
/// 小鹅通 CDN（xet.tech / xed.plus / xiaoe-materials.com）必须带 Referer + 登录 Cookie，
/// 否则返回 HTTP 403（2026-08-29 实测）。
fn needs_xiaoe_cdn_headers(url: &str) -> bool {
    url.contains("xet.tech") || url.contains("xed.plus") || url.contains("xiaoe-materials.com")
}

#[cfg(test)]
#[allow(unused_imports)] // bench 编译上下文看不到 super::* 调用方，test 实际使用
mod tests {
    use super::*;

    /// 验证默认配置的关键值
    #[test]
    fn test_default_config() {
        let config = HlsDownloaderConfig::default();
        assert_eq!(config.max_concurrent, 4);
        assert_eq!(config.retry_count, 10);
        assert_eq!(config.max_failed_segments, 5);
        assert_eq!(config.retry_base_delay_ms, 100);
        assert_eq!(config.retry_max_delay_ms, 20000);
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

        let tasks: Vec<_> = (0..TOTAL_TASKS)
            .map(|_| {
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
            })
            .collect();

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
        assert!(
            acquired.is_err(),
            "try_acquire on zero-permit semaphore should fail"
        );
    }

    /// 端到端集成测试：用 mock HTTP server 验证 max_concurrent 真实限制并发
    ///
    /// 这是 Phase A 的关键验证：之前 max_concurrent 配置是死代码（无 semaphore）
    /// 修复后，N 个分段并发下载时，server 端观察到的并发连接数 ≤ max_concurrent
    #[tokio::test]
    async fn test_max_concurrent_e2e_mock_server() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        const SEGMENT_COUNT: usize = 16;
        const MAX_CONCURRENT: usize = 3;
        const SEGMENT_LATENCY_MS: u64 = 80;
        const SEGMENT_PAYLOAD: &[u8] = b"fake-segment-data-1234567890";

        let active = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let total_requests = Arc::new(AtomicUsize::new(0));

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        let active_clone = active.clone();
        let peak_clone = peak.clone();
        let total_clone = total_requests.clone();
        tokio::spawn(async move {
            loop {
                let (mut socket, _) = match listener.accept().await {
                    Ok(p) => p,
                    Err(_) => break,
                };
                let active = active_clone.clone();
                let peak = peak_clone.clone();
                let total = total_clone.clone();

                tokio::spawn(async move {
                    let mut buf = vec![0u8; 4096];
                    let n = match socket.read(&mut buf).await {
                        Ok(n) if n > 0 => n,
                        _ => return,
                    };
                    let req = String::from_utf8_lossy(&buf[..n]);

                    let cur = active.fetch_add(1, Ordering::SeqCst) + 1;
                    peak.fetch_max(cur, Ordering::SeqCst);
                    total.fetch_add(1, Ordering::SeqCst);

                    tokio::time::sleep(Duration::from_millis(SEGMENT_LATENCY_MS)).await;

                    let response = if req.starts_with("GET /manifest") {
                        let mut m3u8 = String::from("#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-TARGETDURATION:10\n#EXT-X-MEDIA-SEQUENCE:0\n");
                        for i in 0..SEGMENT_COUNT {
                            m3u8.push_str(&format!("#EXTINF:5.0,\nsegment_{:04}.ts\n", i));
                        }
                        m3u8.push_str("#EXT-X-ENDLIST\n");
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/vnd.apple.mpegurl\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            m3u8.len(), m3u8
                        )
                    } else {
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: video/mp2t\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            SEGMENT_PAYLOAD.len()
                        ) + std::str::from_utf8(SEGMENT_PAYLOAD).unwrap()
                    };

                    let _ = socket.write_all(response.as_bytes()).await;
                    let _ = socket.shutdown().await;

                    active.fetch_sub(1, Ordering::SeqCst);
                });
            }
        });

        tokio::time::sleep(Duration::from_millis(50)).await;

        let temp_dir = tempfile::tempdir().unwrap();
        let output_path = temp_dir.path().join("output.ts");

        let config = HlsDownloaderConfig {
            max_concurrent: MAX_CONCURRENT,
            retry_count: 1,
            timeout_secs: 5,
            ..Default::default()
        };
        let downloader = HlsDownloader::with_config(config).await.unwrap();
        let manifest_url = format!("http://127.0.0.1:{}/manifest.m3u8", port);

        let result = downloader.download(&manifest_url, &output_path, None).await;

        tokio::time::sleep(Duration::from_millis(200)).await;

        let observed_peak = peak.load(Ordering::SeqCst);
        let observed_total = total_requests.load(Ordering::SeqCst);

        assert!(
            result.is_ok(),
            "download should succeed: {:?}",
            result.err()
        );
        assert!(
            observed_total > SEGMENT_COUNT,
            "expected >{} requests, got {}",
            SEGMENT_COUNT,
            observed_total
        );
        assert!(
            observed_peak <= MAX_CONCURRENT,
            "PEAK {} > max {} — Semaphore FAILED to bound",
            observed_peak,
            MAX_CONCURRENT
        );
        assert!(
            observed_peak >= 2,
            "peak {} too low — semaphore too restrictive",
            observed_peak
        );
    }

    /// Retry-After 头解析：纯数字格式
    #[test]
    fn parse_retry_after_secs_parses_positive_integer() {
        assert_eq!(HlsDownloader::parse_retry_after_secs("120"), Some(120));
        assert_eq!(HlsDownloader::parse_retry_after_secs("0"), Some(0));
        assert_eq!(HlsDownloader::parse_retry_after_secs("1"), Some(1));
    }

    #[test]
    fn parse_retry_after_secs_handles_whitespace() {
        assert_eq!(HlsDownloader::parse_retry_after_secs("  120  "), Some(120));
        assert_eq!(HlsDownloader::parse_retry_after_secs("\t5\n"), Some(5));
    }

    #[test]
    fn parse_retry_after_secs_rejects_empty_and_invalid() {
        assert_eq!(HlsDownloader::parse_retry_after_secs(""), None);
        assert_eq!(HlsDownloader::parse_retry_after_secs("   "), None);
        // 负数、浮点、date 格式 → 不支持,返回 None
        assert_eq!(HlsDownloader::parse_retry_after_secs("-5"), None);
        assert_eq!(HlsDownloader::parse_retry_after_secs("1.5"), None);
        assert_eq!(
            HlsDownloader::parse_retry_after_secs("Wed, 21 Oct 2015 07:28:00 GMT"),
            None
        );
        assert_eq!(HlsDownloader::parse_retry_after_secs("abc"), None);
    }

    /// 从 download_single_segment 错误消息提取 Retry-After
    #[test]
    fn extract_retry_after_from_err_parses_standard_format() {
        assert_eq!(
            HlsDownloader::extract_retry_after_from_err(
                "HTTP 429 Too Many Requests (retry-after: 5s)"
            ),
            Some(5)
        );
        assert_eq!(
            HlsDownloader::extract_retry_after_from_err("HTTP 503 (retry-after: 120s)"),
            Some(120)
        );
        assert_eq!(
            HlsDownloader::extract_retry_after_from_err("HTTP 429 (retry-after:0s)"),
            Some(0)
        );
    }

    #[test]
    fn extract_retry_after_from_err_returns_none_for_plain_error() {
        // 无 retry-after 标记
        assert_eq!(
            HlsDownloader::extract_retry_after_from_err("HTTP 500 Internal Server Error"),
            None
        );
        // 格式损坏
        assert_eq!(
            HlsDownloader::extract_retry_after_from_err("HTTP 429 (retry-after: not_a_number)"),
            None
        );
    }
}
