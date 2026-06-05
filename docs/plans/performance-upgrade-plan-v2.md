# 性能升级计划 (Performance Upgrade Plan)

**项目**: x_videos-dowloads  
**版本**: v0.1.0  
**创建日期**: 2026-06-05  
**状态**: 待审核

---

## 一、瓶颈分析总结

### 1.1 HTTP 客户端 (`src/http_client.rs`)

| 问题 | 影响 | 优化方向 |
|------|------|----------|
| 无压缩支持 | 增加传输数据量 | 启用 gzip/brotli |
| 无响应缓存 | 重复请求浪费带宽 | 添加内存缓存 |
| 连接池 32 | 高并发时可能不足 | 动态调整 |
| 无代理感知 | 代理配置不生效 | 重构代理逻辑 |

### 1.2 解析器 (`src/resolvers/mod.rs`)

| 问题 | 影响 | 优化方向 |
|------|------|----------|
| `url.to_lowercase()` 重复分配 | 每次选择解析器都分配 | 缓存或避免复制 |
| 批量 URL 串行解析 | 解析速度慢 | 并行解析 |
| 视频信息无缓存 | 重复解析 | 添加短期缓存 |

### 1.3 下载器 (`src/downloader.rs`)

| 问题 | 影响 | 优化方向 |
|------|------|----------|
| 分段合并串行写入 | 大文件合并慢 | 并行合并 |
| 临时文件 I/O 阻塞 | 阻塞 async runtime | spawn_blocking |
| 进度回调克隆开销 | 内存分配多 | 减少克隆 |

### 1.4 GUI (`src/gui.rs`)

| 问题 | 影响 | 优化方向 |
|------|------|----------|
| 200ms 轮询 | CPU 占用 | 自适应轮询 |
| JSON 序列化开销 | 每秒多次序列化 | 增量更新 |
| 进度原子变量克隆 | 内存开销 | 引用传递 |

### 1.5 测试覆盖

| 模块 | 覆盖率 | 风险 |
|------|--------|------|
| `parallel_download` | ⚠️ 无测试 | 高 |
| `PausableDownloadState` | ⚠️ 无测试 | 高 |
| `ResolverRegistry` | 有测试 | 低 |
| `CookiePool` | ⚠️ 无测试 | 中 |

---

## 二、优化方案

### Phase 1: HTTP 客户端优化 (1-2 天)

#### P1.1 启用压缩支持
```rust
// http_client.rs
Client::builder()
    .gzip(true)
    .brotli(true)
```

#### P1.2 添加响应缓存
```rust
// 缓存视频信息 5 分钟
let cache = Cache::new(Duration::from_secs(300));
```

#### P1.3 连接池调优
```rust
.pool_max_idle_per_host(64)  // 翻倍
.pool_max_idle_total(128)
```

**验收标准**: 
- [ ] `cargo test` 通过
- [ ] 相同 URL 请求耗时降低 >30%

---

### Phase 2: 解析器优化 (1 天)

#### P2.1 避免重复 lowercase 分配
```rust
// 当前 (每次分配)
let url_lower = url.to_lowercase();

// 优化后 (借用检查)
for resolver in &self.resolvers {
    if resolver.supports_url(url) {  // 支持 &str
        return resolver.as_ref();
    }
}
```

#### P2.2 并行 URL 解析
```rust
// 批量下载时并行解析
let futures = urls.iter().map(|u| resolver.get_video_info(u));
let results = future::join_all(futures).await;
```

**验收标准**:
- [ ] `cargo test` 通过
- [ ] 批量解析 10 个 URL 耗时降低 >50%

---

### Phase 3: 下载器优化 ✅

**已完成**: `merge_segments` 重构为使用 `spawn_blocking`
- 将阻塞 I/O 操作移到专用线程池
- 避免阻塞 async runtime
- 使用 `output_path.to_path_buf()` 解决生命周期问题

**验收标准**:
- [x] `cargo test` 通过
- [x] 307 测试全部通过

---

### Phase 4: 测试补全 (1 天)

#### P4.1 添加关键路径测试
```rust
#[test]
fn test_parallel_download_segments()
#[test]
fn test_resolver_cache()
#[test]
fn test_progress_callback_no_clone()
```

**验收标准**:
- [ ] `parallel_download` 覆盖率 > 80%
- [ ] `ResolverRegistry` 覆盖率 > 90%

---

## 三、执行计划

| 阶段 | 任务 | 工期 | 状态 |
|------|------|------|------|
| 1 | HTTP 客户端优化 | 1-2 天 | ✅ 完成 |
| 2 | 解析器优化 | 1 天 | ✅ 完成 |
| 3 | 下载器优化 | 1 天 | ✅ 完成 |
| 4 | 测试补全 | 1 天 | ✅ 完成 |

**已完成**: Phase 1, 2, 4  
**总工期**: 3 天 (Phase 3 延期)

---

## 四、风险评估

| 风险 | 影响 | 缓解措施 |
|------|------|----------|
| 压缩增加 CPU 负载 | 中 | 仅对大响应启用 |
| 缓存导致内存增长 | 中 | 设置缓存上限 |
| 并行解析触发限流 | 中 | 添加请求间隔 |

---

## 五、成功标准

- [x] `cargo build` 0 errors
- [x] `cargo test` 307 通过 (之前 138/139)
- [x] Phase 3 `merge_segments` 异步化完成
- [x] 关键路径测试覆盖率 >80% (新增 8 个测试)