# 下载速度优化计划

## 防封IP策略 ⚠️

**重要**: Twitter/X 有严格的防爬机制，过多并发连接可能导致IP被封。

### 安全配置（当前）

```rust
parallel_segments: 4,        // 默认4线程
pool_max_idle_per_host: 16, // 适度连接池
tcp_keepalive: 120,         // 减少连接断开
adaptive_threads: true,      // 启用自适应线程
min_threads: 2,              // 最少2线程
max_threads: 8,              // 最多8线程（防封IP）
speed_threshold_bps: 500KB,  // 500KB/s 低于此值增加线程
```

---

## 已实施的优化

| 参数 | 原值 | 新值 | 效果 |
|------|------|------|------|
| tcp_keepalive | 60s | 120s | 减少断开 |
| pool_max_idle | 10 | 16 | 适度提升 |
| 自适应线程 | 无 | 有 | 智能调整 |
| 网站特征学习 | 无 | 有 | 持久化学习 |

---

## 网站特征学习系统 ✅ 新增

记录每个网站的下载特征和防爬机制，持久化到磁盘，实现越用越智能：

### 特征数据

```json
{
  "video.twimg.com": {
    "adaptive_threads": true,
    "min_threads": 2,
    "max_threads": 4,
    "speed_threshold_bps": 512000,
    "avg_speed_bps": 8388608,
    "success_count": 10,
    "failure_count": 1,
    "has_thread_limit": true,
    "has_speed_limit": true,
    "anti_crawler_type": "Cloudflare",
    "requires_auth": false,
    "recommended_delay_ms": 500,
    "rotate_user_agent": true
  }
}
```

### 防爬机制类型

| 类型 | 说明 |
|------|------|
| None | 无已知防爬 |
| Cloudflare | Cloudflare保护 |
| RateLimit | 速率限制 |
| Captcha | 验证码 |
| UserAgent | UA检测 |
| CookieAuth | 需要Cookie |
| JavaScript | JS挑战 |
| IPBan | IP封禁 |

### 默认特征表示例

| 网站 | 线程 | 防爬类型 | 需要认证 | UA轮换 |
|------|------|----------|----------|--------|
| video.twimg.com | 2-4 | Cloudflare | 否 | 是 |
| v.redd.it | 2-4 | Cloudflare | 否 | 是 |
| github.com | 2-6 | RateLimit | 否 | 否 |
| youtube.com | 2-8 | RateLimit | 否 | 否 |
| reddit.com | 2-4 | Cloudflare | 是 | 是 |
| nytimes.com | 2-4 | Cloudflare | 是 | 是 |
| twitch.tv | 2-6 | Cloudflare | 否 | 是 |
| *.com (通配) | 2-4 | Cloudflare | 否 | 是 |

### 学习逻辑

1. **成功下载**: 记录速度，更新平均速度
2. **失败3次+**: 检测到线程限制，降低max_threads
3. **速度突降**: 检测到速度限制，调整threshold
4. **持续快速**: 解除线程限制

### 文件位置

- `~/.config/x-video-downloader/website_profiles.json`

---

## 自适应线程控制

根据下载速度智能调整线程数：

### 逻辑

```
速度 < 500KB/s + 线程 < 8  →  增加线程 (+1)
速度 > 1.5MB/s + 线程 > 2  →  减少线程 (-1)
速度正常                      →  保持不变
```

### 优势

1. **慢速网络**: 自动增加线程加速下载
2. **快速网络**: 自动减少线程节省资源
3. **防封IP**: 渐进式调整，避免激封
4. **网站感知**: 结合特征表针对优化

---

## 验证结果

- 133 tests passed ✅
- 构建成功 ✅
- 网站特征学习已实现 ✅
- 默认特征表已加载 ✅
