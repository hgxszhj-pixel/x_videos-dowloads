//! HLS 分段下载器
//!
//! 支持并发下载、分段解密和进度回调

use std::collections::HashMap;
use std::path::PathBuf;
#[allow(unused_imports)] // AtomicUsize is used in #[cfg(test)] mod tests via `use super::*;`
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use reqwest::Client;
use serde_json::json;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tokio::sync::Semaphore;
use tracing::{info, warn};

use super::crypto::{fetch_key, parse_iv};
use super::parser::{M3u8Parser, M3u8Segment};
use crate::core::trace::TraceStage;
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
            // 并发分段数。bench 实测 c=8 = 281ms / c=4 = 491ms(1.75×),
            // c=8 是 sweet spot(2026-10-12 perf session 决策,当时 commit 漏了,
            // 2026-10-30 补上)。共享 HTTP client 的 pool_max_idle_per_host(64)
            // 足够支撑 8 条并发连接。
            max_concurrent: 8,
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

/// 一个播放列表用到的 AES 密钥表（`key_url` → 16 字节密钥）。
///
/// **为什么需要它**：HLS 的 `#EXT-X-KEY` 在一段 key block 里覆盖**多个**分段，
/// 同一把 key 要重复用几十到上百次。旧实现每下载一个分段都调一次
/// `fetch_key()` —— 也就是**每个分段多一次 HTTP 往返**去取同一把 key。
/// 1000 分段的播放列表 = 1000 次多余的 key 请求。
///
/// 这条路径正是小鹅通（xet.tech / xed.plus）这类加密 HLS 的主路径，
/// 也就是本项目最真实的下载场景 —— 每段多一次 RTT 直接乘在总时长上。
///
/// **语义**：按 `key_url` 缓存 —— HLS 密钥是不可变资源，同一 URL 永远同一把 key；
/// key 轮换（block 切换）时 URL 也会变 → 自然 miss → 重新取。
struct SegmentKeys {
    map: HashMap<String, [u8; 16]>,
}

impl SegmentKeys {
    /// 预取播放列表里用到的**全部不同** key（每个 URL 恰好取一次）。
    ///
    /// **尽力而为**：某个 key 取不到时不中断整个下载 —— 那把 key 的分段
    /// 会在下载时回退到按需 `fetch_key()`（旧行为）。这样「key 服务器临时抽风」
    /// 不会把整个播放列表拖死。
    async fn prefetch(client: &Client, segments: &[M3u8Segment]) -> Self {
        let mut urls: Vec<&str> = segments
            .iter()
            .filter_map(|s| s.key_url.as_deref())
            .collect();
        urls.sort_unstable();
        urls.dedup();

        let mut map = HashMap::with_capacity(urls.len());
        for url in urls {
            match fetch_key(client, url).await {
                Ok(key) => {
                    map.insert(url.to_string(), key);
                }
                Err(e) => {
                    // 不中断:受影响的分段会走按需取 key 的回退路径
                    warn!("预取 AES 密钥失败({}),相关分段将按需重取: {}", url, e);
                }
            }
        }

        if !map.is_empty() {
            info!(
                "预取 {} 个 AES 密钥(旧实现:每分段取一次,共 {} 次)",
                map.len(),
                segments.iter().filter(|s| s.key_url.is_some()).count()
            );
        }

        Self { map }
    }

    /// 取已缓存的 key；没有则返回 None（调用方回退到按需 fetch）。
    fn get(&self, key_url: &str) -> Option<[u8; 16]> {
        self.map.get(key_url).copied()
    }

    /// 已成功缓存的 key 个数（埋点用：区分「预取到 N 把」与「清单里引用了 M 把」）。
    fn len(&self) -> usize {
        self.map.len()
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

        // 埋点:清单阶段单独计时。CLI 侧此前只有 run 级聚合,
        // 「下载慢」无法拆成 清单 / 取 key / 传输 / 合并 四段。
        let manifest_stage = TraceStage::start(
            "hls_manifest",
            json!({"host": trace_url_host(url)}),
            json!({}),
        );
        let playlist = M3u8Parser::fetch_and_parse(&self.client, url)
            .await
            .context("failed to fetch m3u8")?;

        let total_segments = playlist.segments.len();
        let total_duration = playlist.total_duration;
        manifest_stage.finish(json!({
            "segments": total_segments,
            "total_duration": total_duration,
        }));
        info!(
            "解析完成: {} 分段, 总时长 {:.1}s",
            total_segments, total_duration
        );

        // 预取 AES 密钥:加密 HLS 的一把 key 覆盖整段 key block,
        // 旧实现每个分段都去取一次(每段多一次 HTTP RTT)。
        let keys_stage = TraceStage::start("hls_keys", json!({}), json!({}));
        let segment_keys = Arc::new(SegmentKeys::prefetch(&self.client, &playlist.segments).await);
        keys_stage.finish(json!({"cached_keys": segment_keys.len()}));

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

        // 埋点:分段传输阶段。**不**进 stall 白名单 —— 一门课的整体传输本来就
        // 可能 >30s(1.3GB 的课 26–130s),把它算作 stall 就是永久误报。
        let segments_stage = TraceStage::start(
            "hls_segments",
            json!({
                "segments": total_segments,
                "max_concurrent": self.config.max_concurrent,
            }),
            json!({}),
        );

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
                let segment_keys = segment_keys.clone();

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
                        Some(segment_keys),
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

        // 传输阶段结束(成功或部分失败都算结束 —— 失败信息在 output 里)
        segments_stage.finish(json!({
            "failed": failed_segments.len(),
            "bytes": progress_bytes.load(Ordering::Relaxed),
        }));

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
        // 埋点:合并阶段(全文件 2× 磁盘 IO)。此前只能靠估算(「占 1–5%」),
        // 埋上之后这个占比是**实测**的。
        let merge_stage =
            TraceStage::start("hls_merge", json!({"segments": total_segments}), json!({}));
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

        merge_stage.finish(json!({"bytes_written": total_written}));

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
        segment_keys: Option<Arc<SegmentKeys>>,
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
                    segment_keys.as_deref(),
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
        segment_keys: Option<&SegmentKeys>,
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
            // 优先用预取好的 key（同一把 key 覆盖整个 key block）。
            // 没有缓存 → 回退到按需取（旧行为:预取失败 / 该 key 未预取到）。
            let key = match segment_keys.and_then(|k| k.get(key_url)) {
                Some(key) => key,
                None => fetch_key(client, key_url).await?,
            };
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

/// 埋点用:只取 URL 的 host。
///
/// 完整 m3u8 URL 常带鉴权 token(小鹅通 CDN 的 `?auth=...`),
/// NDJSON 是落盘文件,不该把 token 写进去 —— 只留 host 足够定位「哪个 CDN 慢」。
fn trace_url_host(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let host = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    // 去掉 userinfo(user:pass@host)
    host.rsplit('@').next().unwrap_or(host)
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
    use serial_test::serial;

    /// 验证默认配置的关键值
    #[test]
    fn test_default_config() {
        let config = HlsDownloaderConfig::default();
        assert_eq!(config.max_concurrent, 8);
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

    /// 端到端：**加密 HLS 播放列表里,同一把 key 只取一次**(不是每分段一次)。
    ///
    /// **这条用例必须能红**:旧实现每个分段都调一次 `fetch_key()`,
    /// 6 个分段 = 6 次 key 请求;新实现预取后 = **1 次**。
    /// 少掉的 5 次 RTT 直接从总下载时长里减去 ——
    /// 加密 HLS(小鹅通 xet.tech)是本项目最真实的下载路径。
    ///
    /// 同时验证解密结果正确(用测试内的 CBC 加密器造密文)。
    #[tokio::test]
    async fn test_aes_key_fetched_once_per_distinct_key_url() {
        use aes::cipher::{BlockEncryptMut, KeyInit};
        use aes::Aes128;
        use generic_array::GenericArray;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        const SEGMENT_COUNT: usize = 6;
        const KEY: [u8; 16] = [0x2b; 16];
        const IV_HEX: &str = "0x000102030405060708090a0b0c0d0e0f";
        const PLAINTEXT: &[u8] = b"hello hls segment payload!!"; // 24 bytes

        // CBC 加密 + PKCS7 填充(= decrypt_segment 的精确逆运算)
        fn encrypt_cbc(plain: &[u8], key: &[u8; 16], iv: &[u8; 16]) -> Vec<u8> {
            let mut cipher = Aes128::new(GenericArray::from_slice(key));
            let mut data = plain.to_vec();
            let pad = 16 - (data.len() % 16);
            data.extend(std::iter::repeat_n(pad as u8, pad));
            let mut prev = *iv;
            let mut out = Vec::with_capacity(data.len());
            for chunk in data.chunks(16) {
                let mut block = *GenericArray::from_slice(chunk);
                for j in 0..16 {
                    block[j] ^= prev[j];
                }
                cipher.encrypt_block_mut(&mut block);
                prev = block.into();
                out.extend_from_slice(&block);
            }
            out
        }

        let iv_bytes: [u8; 16] = {
            let mut iv = [0u8; 16];
            for (i, b) in iv.iter_mut().enumerate() {
                *b = i as u8;
            }
            iv
        };
        let ciphertext = encrypt_cbc(PLAINTEXT, &KEY, &iv_bytes);

        let key_requests = Arc::new(AtomicUsize::new(0));
        let segment_requests = Arc::new(AtomicUsize::new(0));

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        let key_req_clone = key_requests.clone();
        let seg_req_clone = segment_requests.clone();
        let ciphertext = Arc::new(ciphertext);
        tokio::spawn(async move {
            loop {
                let (mut socket, _) = match listener.accept().await {
                    Ok(p) => p,
                    Err(_) => break,
                };
                let key_req = key_req_clone.clone();
                let seg_req = seg_req_clone.clone();
                let ciphertext = ciphertext.clone();

                tokio::spawn(async move {
                    let mut buf = vec![0u8; 4096];
                    let n = match socket.read(&mut buf).await {
                        Ok(n) if n > 0 => n,
                        _ => return,
                    };
                    let req = String::from_utf8_lossy(&buf[..n]);

                    let response: Vec<u8> = if req.starts_with("GET /manifest") {
                        let mut m3u8 =
                            String::from("#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-TARGETDURATION:10\n");
                        m3u8.push_str(&format!(
                            "#EXT-X-KEY:METHOD=AES-128,URI=\"http://127.0.0.1:{}/key.bin\",IV={}\n",
                            port, IV_HEX
                        ));
                        for i in 0..SEGMENT_COUNT {
                            m3u8.push_str(&format!("#EXTINF:5.0,\nsegment_{:04}.ts\n", i));
                        }
                        m3u8.push_str("#EXT-X-ENDLIST\n");
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/vnd.apple.mpegurl\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            m3u8.len(), m3u8
                        )
                        .into_bytes()
                    } else if req.starts_with("GET /key.bin") {
                        key_req.fetch_add(1, Ordering::SeqCst);
                        let mut resp = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            KEY.len()
                        )
                        .into_bytes();
                        resp.extend_from_slice(&KEY);
                        resp
                    } else {
                        seg_req.fetch_add(1, Ordering::SeqCst);
                        let mut resp = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: video/mp2t\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            ciphertext.len()
                        )
                        .into_bytes();
                        resp.extend_from_slice(&ciphertext);
                        resp
                    };

                    let _ = socket.write_all(&response).await;
                    let _ = socket.shutdown().await;
                });
            }
        });

        tokio::time::sleep(Duration::from_millis(50)).await;

        let temp_dir = tempfile::tempdir().unwrap();
        let output_path = temp_dir.path().join("output.ts");

        let config = HlsDownloaderConfig {
            max_concurrent: 4,
            retry_count: 2,
            timeout_secs: 5,
            ..Default::default()
        };
        let downloader = HlsDownloader::with_config(config).await.unwrap();
        let manifest_url = format!("http://127.0.0.1:{}/manifest.m3u8", port);

        let result = downloader.download(&manifest_url, &output_path, None).await;
        assert!(
            result.is_ok(),
            "download should succeed: {:?}",
            result.err()
        );

        tokio::time::sleep(Duration::from_millis(200)).await;

        let observed_keys = key_requests.load(Ordering::SeqCst);
        let observed_segments = segment_requests.load(Ordering::SeqCst);

        assert_eq!(observed_segments, SEGMENT_COUNT, "每个分段都该下载一次");
        assert_eq!(
            observed_keys, 1,
            "同一把 key 只该取一次(旧实现每分段取一次 = {} 次)",
            SEGMENT_COUNT
        );

        // 解密结果必须正确:合并后是 6 段明文拼接
        let merged = std::fs::read(&output_path).unwrap();
        assert_eq!(
            merged.len(),
            PLAINTEXT.len() * SEGMENT_COUNT,
            "合并后的长度应等于 6 段明文"
        );
        for i in 0..SEGMENT_COUNT {
            let start = i * PLAINTEXT.len();
            assert_eq!(
                &merged[start..start + PLAINTEXT.len()],
                PLAINTEXT,
                "第 {} 段解密后内容不对",
                i
            );
        }
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

    /// 埋点回归:一次 HLS 下载必须产出 **4 个叶子阶段**的 done 事件。
    ///
    /// 缺了它们,「下载慢」就还是一坨黑盒 —— 管线分析只能给出「下载占 95%」这种
    /// 没法行动的结论。拆成 清单 / 取 key / 分段传输 / 合并 之后,
    /// 才能回答「合并阶段到底占几个百分点」(此前只是估算的 1–5%)。
    ///
    /// **这条用例必须能红**:删掉 `src/hls/downloader.rs` 里任一
    /// `TraceStage::start("hls_*")`,对应的 `*_done` 事件就消失 → 必红。
    // 属性顺序必须是 `#[serial]` 在外、`#[tokio::test]` 在内(与 src/core/trace.rs 一致)。
    // 反过来写时 `#[tokio::test]` 先展开,`#[serial]` 落到生成函数的**内层**,
    // 拿不到全局串行锁 ⇒ 本用例会与 `core::trace::tests::*` 并发跑,
    // 两者都写全局 `X_VIDEOS_TRACE_DIR` ⇒ trace 文件互相污染
    // (实测症状:`test_trace_step_raii` 读到 `hls_manifest` 而不是 `sleep_step`)。
    #[serial]
    #[tokio::test]
    async fn test_hls_download_emits_four_leaf_stage_events() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        const SEGMENT_COUNT: usize = 2;
        const SEGMENT_BODY: &[u8] = b"0123456789abcdef"; // 16 bytes,未加密

        // mock HLS server:manifest + 2 个分段(明文,不用 key)
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            loop {
                let (mut socket, _) = match listener.accept().await {
                    Ok(p) => p,
                    Err(_) => break,
                };
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 4096];
                    let n = match socket.read(&mut buf).await {
                        Ok(n) if n > 0 => n,
                        _ => return,
                    };
                    let req = String::from_utf8_lossy(&buf[..n]);
                    let response: Vec<u8> = if req.starts_with("GET /manifest") {
                        let mut m3u8 =
                            String::from("#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-TARGETDURATION:10\n");
                        for i in 0..SEGMENT_COUNT {
                            m3u8.push_str(&format!("#EXTINF:5.0,\nsegment_{:04}.ts\n", i));
                        }
                        m3u8.push_str("#EXT-X-ENDLIST\n");
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/vnd.apple.mpegurl\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            m3u8.len(), m3u8
                        )
                        .into_bytes()
                    } else {
                        let mut resp = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: video/mp2t\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            SEGMENT_BODY.len()
                        )
                        .into_bytes();
                        resp.extend_from_slice(SEGMENT_BODY);
                        resp
                    };
                    let _ = socket.write_all(&response).await;
                    let _ = socket.shutdown().await;
                });
            }
        });
        tokio::time::sleep(Duration::from_millis(50)).await;

        // 埋点落盘到独立 tmpdir(trace_root 每次读 env,不受其他测试影响)
        let trace_dir = std::env::temp_dir().join(format!(
            "hls-trace-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&trace_dir).unwrap();
        std::env::set_var("X_VIDEOS_TRACE_DIR", &trace_dir);
        let session_id = crate::core::trace::trace_session_open("hls-stage").await;

        let out_dir = tempfile::tempdir().unwrap();
        let output_path = out_dir.path().join("output.ts");
        let config = HlsDownloaderConfig {
            max_concurrent: 4,
            retry_count: 2,
            timeout_secs: 5,
            ..Default::default()
        };
        let downloader = HlsDownloader::with_config(config).await.unwrap();
        let manifest_url = format!("http://127.0.0.1:{}/manifest.m3u8", port);

        let result = downloader.download(&manifest_url, &output_path, None).await;
        assert!(
            result.is_ok(),
            "download should succeed: {:?}",
            result.err()
        );

        crate::core::trace::trace_session_close(json!({"status": "ok"})).await;
        tokio::time::sleep(Duration::from_millis(100)).await;

        let path = trace_dir.join(format!("{session_id}.ndjson"));
        let content = std::fs::read_to_string(&path).unwrap();
        let events: Vec<serde_json::Value> = content
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str(l).expect("每行必须是合法 JSON"))
            .collect();

        fn find<'a>(events: &'a [serde_json::Value], step: &str) -> &'a serde_json::Value {
            events
                .iter()
                .find(|e| e["step"] == step)
                .unwrap_or_else(|| {
                    panic!(
                        "缺少埋点事件 {step}(实际有: {:?})",
                        events
                            .iter()
                            .map(|e| e["step"].as_str().unwrap_or("?"))
                            .collect::<Vec<_>>()
                    )
                })
        }

        // 4 个阶段各一对 marker + done(marker 立刻落盘 = 卡死现场的第一手证据)
        for stage in ["hls_manifest", "hls_keys", "hls_segments", "hls_merge"] {
            assert!(
                find(&events, stage)["output"]["phase"] == "start",
                "{stage} 缺少开始标记"
            );
            assert!(
                find(&events, &format!("{stage}_done"))["duration_ms"].is_u64(),
                "{stage}_done 必须带真实耗时"
            );
        }

        let expected_bytes = (SEGMENT_BODY.len() * SEGMENT_COUNT) as u64;
        assert_eq!(find(&events, "hls_manifest_done")["output"]["segments"], 2);
        // 明文流没有 key —— 阶段仍必须出现(cached_keys=0),否则「无 key 的流」会缺一段计时
        assert_eq!(find(&events, "hls_keys_done")["output"]["cached_keys"], 0);
        assert_eq!(find(&events, "hls_segments_done")["output"]["failed"], 0);
        assert_eq!(
            find(&events, "hls_segments_done")["output"]["bytes"],
            expected_bytes
        );
        assert_eq!(
            find(&events, "hls_merge_done")["output"]["bytes_written"],
            expected_bytes
        );
        // 埋点里不许出现完整 URL(可能带鉴权 token)—— 只留 host
        assert_eq!(
            find(&events, "hls_manifest_done")["input"]["host"],
            format!("127.0.0.1:{port}")
        );

        let _ = std::fs::remove_dir_all(&trace_dir);
        std::env::remove_var("X_VIDEOS_TRACE_DIR");
    }
}
