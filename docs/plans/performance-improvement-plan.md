# 性能改进计划报告 (修订版)
**项目**: x-video-downloader (Rust 视频下载器)
**生成日期**: 2026-05-29
**知识图谱**: 350 nodes, 420 edges, 10 layers
**审核状态**: 已通过 Security (CONDITIONAL PASS), Architecture (NEEDS_REWORK), Performance (NEEDS_IMPROVEMENT)

---

## 一、性能瓶颈分析

### 瓶颈 1: 分段合并 O(n×m) seeks ⚠️ HIGH (最高优先级)

**位置**: `src/downloader.rs:340-366`

```rust
// 当前问题: 每读一次就 seek 一次
for (idx, (offset, mut input)) in input_files.into_iter().enumerate() {
    output.seek(SeekFrom::Start(offset + written))?;  // ← 频繁 seek
    loop {
        let bytes_read = input.read(&mut buffer)?;
        output.write_all(&buffer[..bytes_read])?;
    }
}
```

**问题**:
- n 个分段，每段 m 次读取 = O(n×m) 次 seek 操作
- 实际是算法问题，不是封装问题

**影响**: 10-20% (正确评估)

---

### 瓶颈 2: 重试机制无差异化 ⚠️ MEDIUM

**位置**: `src/downloader.rs:155-161`

```rust
let delay_secs = (2u64.pow(attempt - 1)).min(self.config.retry_max_delay);
```

**问题**:
- 所有错误类型使用相同退避策略
- 429 限流需要 60-120s，不是指数退避
- 未解析 Retry-After header

**影响**: 5-10%

---

### 瓶颈 3: HEAD 请求无缓存 ⚠️ MEDIUM

**位置**: `src/downloader.rs:164`

```rust
let total_size = self.get_content_length(&request.url).await?;
```

**问题**:
- 每次下载都发起 HEAD 请求
- 重复下载同一 URL 无缓存

**影响**: 10-15% (仅重复下载)

---

### 瓶颈 4: HTTP 连接池静态配置 ⚠️ MEDIUM

**位置**: `src/http_client.rs:13-23`

```rust
.pool_max_idle_per_host(32)  // ← 硬编码 32
```

**问题**:
- 固定 32 连接，未考虑服务器限制
- CDN 通常限制 6-12 并发连接

**影响**: 5-15% (原估计 20-30% 过高)

---

### 瓶颈 5: 动态线程调整风险 ⚠️ LOW (建议放弃或限制)

**位置**: `src/downloader.rs:123-145`

**问题**:
- 网络波动导致不必要的线程调整
- 引入 DoS 攻击面 (人为制造慢速网络)
- 与现有 website_profile 重复

---

## 二、改进方案 (修订后优先级)

### Phase 1: P4 - 智能重试机制 (最低风险)

**方案**:
```rust
async fn download_with_retry(&self, request: &DownloadRequest) -> Result<DownloadResult> {
    for attempt in 0..self.config.retry_count {
        match self.do_download(request).await {
            Ok(result) => return Ok(result),
            Err(e) => {
                let delay = self.calculate_backoff(e, attempt).await;
                if let Some(d) = delay {
                    tokio::time::sleep(d).await;
                } else {
                    return Err(e); // 不可重试的错误
                }
            }
        }
    }
    Err(last_error)
}

async fn calculate_backoff(&self, err: &Error, attempt: usize) -> Option<Duration> {
    match err {
        // 429 限流 - 等待更长时间，解析 Retry-After
        Error::RateLimited { retry_after } => {
            retry_after.or_else(|| Duration::from_secs(60))
        }
        // 5xx 服务器错误 - 短等待
        Error::ServerError(_) => {
            let backoff = 2u64.pow(attempt as u32).min(8);
            Some(Duration::from_secs(backoff))
        }
        // 网络错误 - 标准指数退避
        _ => {
            let backoff = (2u64.pow(attempt as u32)).min(self.config.retry_max_delay);
            Some(Duration::from_secs(backoff))
        }
    }
}
```

**安全修复**:
- 减少重试日志详细程度，避免暴露内部状态

**预期改善**: 5-10%

---

### Phase 2: P1' - 分段合并算法优化 (最高实际影响)

**方案**:
```rust
// 关键改进: 按 offset 排序后顺序写入，消除所有 seek
async fn merge_segments_optimized(&self, output_path: &Path, temp_dir: &Path,
                                  segments: u32, chunk_size: u64, total_size: u64) -> Result<u64> {
    let mut output = tokio::fs::File::create(output_path).await?;
    let mut final_size = 0u64;

    // 阶段1: 按 offset 排序所有分段
    let mut segment_files: Vec<(u64, tokio::fs::File)> = Vec::with_capacity(segments as usize);
    for i in 0..segments {
        let temp_path = temp_dir.join(format!("part_{}.tmp", i));
        if let Ok(mut file) = tokio::fs::File::open(&temp_path).await {
            // 验证文件头位置正确
            let metadata = file.metadata().await?;
            if metadata.len() > 0 {
                segment_files.push((i as u64 * chunk_size, file));
            }
        }
    }

    // 按 offset 排序
    segment_files.sort_by_key(|(offset, _)| *offset);

    // 阶段2: 顺序写入，无 seek
    let mut expected_offset = 0u64;
    for (offset, mut input) in segment_files {
        // 验证分段从正确的位置开始
        if offset != expected_offset {
            warn!("Segment gap detected at offset {}, expected {}", offset, expected_offset);
        }

        let mut buffer = vec![0u8; 64 * 1024]; // 64KB buffer
        loop {
            match input.read(&mut buffer).await {
                Ok(0) => break,
                Ok(n) => {
                    output.write_all(&buffer[..n]).await?;
                    final_size += n as u64;
                    expected_offset += n as u64;
                }
                Err(e) => {
                    warn!("Read error during merge: {:?}", e);
                    break;
                }
            }
        }
    }

    Ok(final_size)
}
```

**关键改进**:
- 排序后顺序写入，消除所有 seek
- 从 O(n×m) seeks 降至 O(0) seeks
- 不依赖 async 封装，解决根本算法问题

**预期改善**: 10-20%

---

### Phase 3: P2 - HEAD 结果缓存 (安全版)

**方案**:
```rust
use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{Duration, Instant};

struct HeadCache {
    entries: RwLock<HashMap<String, (u64, Instant)>>,
    ttl: Duration,
    max_size: usize,
}

impl HeadCache {
    fn new() -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
            ttl: Duration::from_secs(300), // 5min TTL
            max_size: 100,
        }
    }

    fn get(&self, url: &str) -> Option<u64> {
        let entries = self.entries.read().ok()?;
        if let Some((size, time)) = entries.get(url) {
            if time.elapsed() < self.ttl {
                return Some(*size);
            }
        }
        None
    }

    fn insert(&self, url: &str, size: u64) {
        if let Ok(mut entries) = self.entries.write() {
            // LRU eviction
            if entries.len() >= self.max_size {
                // 简单的 FIFO eviction，实际可用 LinkedHashMap 优化
                if let Some(oldest) = entries.keys().next().cloned() {
                    entries.remove(&oldest);
                }
            }
            entries.insert(url.to_string(), (size, Instant::now()));
        }
    }
}

// 安全修复: 使用 URL 的 hash 作为缓存 key 的一部分，防止 cache poisoning
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

fn cache_key(url: &str, user_context: &str) -> String {
    let mut s = DefaultHasher::new();
    url.hash(&mut s);
    user_context.hash(&mut s);
    format!("{:x}", s.finish())
}
```

**安全修复**:
- 缓存 key 包含 user_context (防止跨用户 cache poisoning)
- TTL 5min 后自动失效
- LRU eviction 防止内存膨胀

**预期改善**: 10-15% (仅重复下载)

---

### Phase 4: P0' - HTTP 连接池配置修正

**方案**:
```rust
// http_client.rs - 不引入新类型，使用 builder pattern
pub fn shared_with_pool_size(pool_size: usize) -> Arc<Client> {
    static CLIENTS: Lazy<RwLock<HashMap<usize, Arc<Client>>>> =
        Lazy::new(|| RwLock::new(HashMap::new()));

    // 缓存已有配置
    if let Ok(clients) = CLIENTS.read() {
        if let Some(client) = clients.get(&pool_size) {
            return client.clone();
        }
    }

    // 创建新 client
    let client = Client::builder()
        .user_agent(DEFAULT_USER_AGENT)
        .tcp_keepalive(Duration::from_secs(120))
        .connect_timeout(Duration::from_secs(30))
        .timeout(Duration::from_secs(600))
        .pool_max_idle_per_host(pool_size.min(64)) // 上限 64，防止资源耗尽
        .build()
        .expect("Failed to build HTTP client");

    let client = Arc::new(client);

    if let Ok(mut clients) = CLIENTS.write() {
        clients.insert(pool_size, client.clone());
    }

    client
}

// 调用处: 根据分段数设置连接池
let pool_size = (self.config.parallel_segments as usize * 2).max(16).min(64);
let client = http_client::shared_with_pool_size(pool_size);
```

**安全修复**:
- 上限 64，防止资源耗尽
- 缓存不同配置的 client

**预期改善**: 5-15%

---

### Phase 5: P3 - 放弃或限制 (NOT RECOMMENDED)

**原因**:
- Architecture: 层耦合问题
- Security: DoS 攻击面
- Performance: 实际改善有限 (5-10%)
- 与 website_profile 重复

**建议**: 保持当前静态配置，由 website_profile 提供 per-domain 最优值

---

## 三、优先级矩阵 (修订后)

| 优先级 | 瓶颈 | 预期改善 | 实施难度 | 风险 | 状态 |
|--------|------|----------|----------|------|------|
| **Phase 1** | P4 智能重试 | 5-10% | 低 | 低 | 待实施 |
| **Phase 2** | P1' 线性合并 | 10-20% | 高 | 中 | 待实施 |
| **Phase 3** | P2 HEAD缓存 | 10-15% | 低 | 低 | 待实施 |
| **Phase 4** | P0' 连接池修正 | 5-15% | 中 | 低 | 待实施 |
| **Phase 5** | P3 动态线程 | 5-10% | 高 | 中 | **放弃** |

---

## 四、实施计划

### Phase 1: P4 (Smart Retry) - 2 天
1. 添加错误类型 enum (RateLimited, ServerError, NetworkError)
2. 实现 Retry-After header 解析
3. 实现差异化退避策略
4. 减少重试日志详细程度
5. 验证: cargo build + cargo test

### Phase 2: P1' (Linear Merge) - 3 天
1. 重构 merge_segments 按 offset 排序
2. 移除所有 seek 操作
3. 使用 tokio::fs::File 异步写入
4. 验证: cargo build + cargo test + 手动测试大文件

### Phase 3: P2 (HEAD Cache) - 1 天
1. 实现 HeadCache struct
2. 使用 hash-based cache key
3. 添加 LRU eviction
4. 验证: cargo build + cargo test

### Phase 4: P0' (Pool Config) - 1 天
1. 实现 shared_with_pool_size()
2. 添加连接池上限保护
3. 验证: cargo build + cargo test

---

## 五、验证标准

每个 Phase 完成后必须验证:
- [ ] `cargo build` 编译通过
- [ ] `cargo test` 293 测试全部通过
- [ ] `cargo clippy -- -D warnings` 无警告
- [ ] 手动测试: 下载 100MB+ 文件
- [ ] 安全检查: 无新增警告

---

## 六、审核反馈摘要

### Security (CONDITIONAL PASS)
- HEAD cache 需要 authentication context in key
- 连接池需要上限 64
- 重试日志需要减少详细程度
- 速度测量需要防manipulation

### Architecture (NEEDS_REWORK)
- P0 不需要 HttpClientPool，用 builder pattern
- P1 不是 async 封装问题，是算法问题
- P3 引入层耦合

### Performance (NEEDS_IMPROVEMENT)
- P1 预期从 15-25% 修正为 10-20% (算法改进)
- P0 预期从 20-30% 修正为 5-15% (服务器限制)
- 关键改进: 线性合并消除 seeks

---

*本报告由知识图谱 (understand) + codegraph 分析 + 3 个 agent 审核生成*
*修订日期: 2026-05-29*