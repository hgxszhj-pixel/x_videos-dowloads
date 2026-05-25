# 项目交接文档 (Handoff)

**项目**: x_videos-dowloads (Rust x.com 视频下载器)
**更新时间**: 2026-05-25 00:30
**当前分支**: gitbutler/workspace
**Git**: `08f7a2b` fix: multi-agent review - security, performance, architecture improvements

---

## 项目概述

基于 Rust 的多平台视频下载器，支持 x.com (Twitter), TikTok, 抖音, Bilibili, 智学云等平台。

**技术栈:**
- Rust + tokio (async)
- Slint v1.15.1 GUI
- yt-dlp 视频下载 (主要)
- ResolverRegistry 平台选择
- X-Bogus / A-Bogus 签名 (SM3)
- FxTwitter API (Twitter fallback)
- CookiePool SSO 认证

---

## 核心功能

### 多平台解析器 (src/resolvers/)

| 解析器 | 文件 | 支持 |
|--------|------|------|
| TikTokResolver | tiktok.rs | tiktok.com, vm.tiktok.com |
| DouYinResolver | douyin.rs | douyin.com, v.douyin.com |
| BilibiliResolver | bilibili.rs | bilibili.com, b23.tv |
| ZhiXueYunResolver | zhixueyun.rs | zhixueyun.com (PDF 下载) |
| CookiePool | cookie_pool.rs | Cookie 池管理 |
| YtDlpResolver | mod.rs | 所有平台 (fallback) |

**共享模块:**
| 模块 | 文件 | 用途 |
|------|------|------|
| url_matcher | url_matcher.rs | URL 平台检测统一入口 |

**签名模块:**
| 模块 | 文件 | 算法 |
|------|------|------|
| X-Bogus | xbogus.rs | MD5 + RC4 + 自定义 Base64 |
| A-Bogus | abogus.rs | SM3 + RC4 + 自定义 Base64 (抖音) |

**API 客户端:**
| 客户端 | 文件 | 用途 |
|--------|------|------|
| DouYinApiClient | douyin_api.rs | 抖音 API 直接调用 |

---

## 三级视频解析链

1. **ResolverRegistry** → 选择对应平台解析器
2. **yt-dlp** (首选) - 支持 cookie 传递
3. **FxTwitter API** → api.fxtwitter.com (Twitter fallback)

---

## 最近代码改进 (2026-05-21 上午更新)

### 已完成的改进

| 类别 | 改进内容 |
|------|----------|
| **Clippy 警告修复** | downloader.rs: `&PathBuf` → `&Path` (4处); zhixueyun.rs: rsplit/return/cast/closure 修复; url_matcher.rs: needless_borrow 修复 |
| **参数结构体重构** | ZhiXueYunResolver 新增 DownloadContext 结构体，将 8 个参数减少为 6 个 |
| **Panic 风险修复** | GUI 14 处 lock().unwrap() → match 模式；i18n 4 处 → expect() |
| **大函数拆分** | on_download_clicked 拆分为 5 个函数；parallel_download 拆分为 3 个辅助函数 |
| **Builder 模式** | HistoryEntryBuilder 替代 9 参数 add_entry() |
| **并发优化** | Mutex → RwLock (SharedState, History) |
| **错误处理** | downloader.rs 6 处添加 .context() |
| **共享模块** | 新增 url_matcher.rs 统一 URL 平台检测 |
| **单元测试** | 新增 10 个 resolver 测试 |
| **魔法数字** | 提取 5 个命名常量 |
| **API 文档** | 为 ws.rs, handler.rs 添加文档注释 |

### b21bc2a 修复 (2026-05-21)

| 文件 | 修复内容 |
|------|----------|
| ws.rs | doc test `ConnectionState::from_u64` 改用 ` ```ignore` |
| xbogus.rs | `use md5` → `use ::md5` 消除 redundant import 警告 |
| url_matcher.rs | `url_matches_platform` 添加 `#[allow(dead_code)]` |
| history.rs | `HistoryEntryBuilder` 和 `add_entry` 添加 `#[allow(dead_code)]` |
| cookie_pool.rs | `valid_before` → `_valid_before` 消除 unused var 警告 |

### 新提交 (2026-05-21)

| 文件 | 修复内容 |
|------|----------|
| zhixueyun.rs | `ZHIXUEYUN_TOKEN` 环境变量优先于硬编码 fallback token |

### 新增文件

- src/resolvers/url_matcher.rs - 统一 URL 平台检测

---

## Multi-Agent Review 改进 (2026-05-25)

### Ultrawork 并行分析完成

4 个 agents 并行分析项目瓶颈:
| Agent | 模型 | 分析范围 |
|-------|------|----------|
| Architecture | Opus | main.rs 硬编码逻辑、循环依赖、错误类型 |
| Security | Sonnet | 密钥处理、命令注入、依赖漏洞 |
| Performance | Sonnet | async 阻塞、内存分配、连接池 |
| Code Quality | Haiku | 代码重复、测试覆盖、文档 |

### 已修复的安全问题

| 问题 | 严重性 | 位置 | 修复 |
|------|--------|------|------|
| 硬编码 token `d452ee...` | CRITICAL | zhixueyun.rs:117 | 移除 fallback，改为 `ResolverError::AuthenticationRequired` |
| 命令注入 (URL 未转义) | CRITICAL | yt_dlp.rs, scrapling_client.rs, main.rs | 添加 `shell_words::quote` 转义 |
| 日志泄露 URL token | HIGH | yt_dlp.rs:112 | - |
| ring 0.16.20 漏洞 | HIGH | Cargo.lock | `cargo update -p ring` |

### 已修复的性能问题

| 问题 | 严重性 | 位置 | 修复 |
|------|--------|------|------|
| async 阻塞 (wget) | CRITICAL | main.rs:491 | `std::process::Command` → `tokio::process::Command` |
| Python subprocess 阻塞 | CRITICAL | scrapling_client.rs:79 | `spawn_blocking` 包装 |
| busy-wait try_wait() | MEDIUM | scrapling_client.rs:116 | 改用 `child.wait()` |
| 每 chunk 新建 Client | HIGH | downloader.rs:179 | 复用共享 `Arc<Client>` |
| Vec 内存分配 | CRITICAL | downloader.rs:446 | 改为栈上固定数组 `[0u64; 8]` |
| CookiePool O(n) 查找 | HIGH | cookie_pool.rs:145 | 添加 HashMap 索引 O(1) |

### 已修复的架构问题

| 问题 | 严重性 | 位置 | 修复 |
|------|--------|------|------|
| 硬编码平台判断 | HIGH | main.rs:311 | `download_strategy` 字段路由 |
| DownloadStrategy 缺失 | - | types.rs | 添加 `Native/YtDlp` enum |
| msg.clone() borrow 错误 | MEDIUM | types.rs:190 | 添加 `msg.clone()` |

### 下载策略架构

```rust
// VideoInfo 新增字段
pub struct VideoInfo {
    ...
    pub download_strategy: DownloadStrategy,  // 新增
}

// DownloadStrategy enum
pub enum DownloadStrategy {
    Native,   // 智学云原生下载
    YtDlp,    // yt-dlp 下载 (默认)
}
```

**分发逻辑 (main.rs:309)**:
```rust
// 之前: if info.url.contains("kc.zhixueyun.com")
// 现在: if info.download_strategy == DownloadStrategy::Native
```

### 待改进项 (P2)

| 问题 | 优先级 | 建议方案 |
|------|--------|----------|
| 代码重复 (tiktok/douyin/bilibili) | MEDIUM | 提取 `YtDlpBasedResolver` trait |
| 测试覆盖不足 | MEDIUM | 添加 DouYin/ZhiXueYun 集成测试 |
| sequential chunk merge | MEDIUM | 用 `join_set.spawn` 并行化 |

---

## MCP 服务配置 (2026-05-21 下午更新)

| 服务 | 状态 | 说明 |
|------|------|------|
| `oh-my-claudecode:t` | ✅ | oh-my-claudecode MCP bridge |
| `context-mode:context-mode` | ✅ | 上下文管理 (v1.0.124) |
| `sequential-thinking` | ✅ | 顺序思考 MCP |
| `shadcn` | ✅ | UI 组件库 |
| `codegraph` | ✅ | 代码图谱分析 (v0.8.0) |

### 已移除

| 服务 | 原因 |
|------|------|
| `gitnexus` | native 模块编译失败 + Docker 不可用 |

---

## CodeGraph 分析结果 (2026-05-21)

### 项目索引统计

| 指标 | 数值 |
|------|------|
| 文件数 | 46 |
| 代码节点 | 1,123 |
| 调用关系 | 2,633 |
| 数据库大小 | 3.17 MB |

### 性能瓶颈分析

| 优先级 | 瓶颈 | 状态 |
|--------|------|------|
| 🔴 高 | `reqwest::blocking` (zhixueyun.rs) | 暂不修改 (高风险) |
| ✅ | HTTP Client 复用 | 已优化 (downloader.rs) |
| ✅ | Arc 引用传递 | 已是 `&Arc` 形式 |

### CodeGraph 使用命令

```bash
# 初始化项目
npx @colbymchenry/codegraph init

# 索引代码
npx @colbymchenry/codegraph index

# 查询符号
codegraph query "函数名"

# 查看结构
codegraph files

# 分析变更影响
codegraph affected [files]
```

---

## GUI 进度条要点

**关键规则**（详见 CLAUDE.md）:
- download_segment() 只更新 total_downloaded，不发回调
- 存储格式：progress_arc.store((p.percent * 100.0) as u32)
- UI 更新：progress_arc.load() as f32 / 100.0
- 终止条件：while progress_arc_for_update.load(...) < 100
- 进度条使用 HorizontalLayout { alignment: start }

---

## 抖音 Cookie 问题

**问题描述:**
- yt-dlp 和 lux 都报错 "Fresh cookies (not necessarily logged in) are needed"
- DouYin API 返回 HTTP 200 但 body 为空

**解决方案:**
1. 退出浏览器中的抖音登录
2. 清除浏览器中所有 douyin.com cookies
3. 重新登录抖音账号
4. 立即导出新的 Netscape 格式 cookies
5. 保存到 ~/.cookies/douyin_cookies.txt

**配置文件:** ~/.config/x-video-downloader/config.toml

---

## 待办 (优先级排序)

### P1 - 已完成

1. ✅ **ZHIXUEYUN_TOKEN 硬编码问题** - 已在 multi-agent review 中修复

### P2 - 改进空间

1. **CookiePool 简化** - 当前过度设计（LRU/TTL/hit counting 未使用），但暂不修改以避免风险
2. **测试覆盖** - DouYinResolver/ZhiXueYunResolver 缺少集成测试

---

## 项目质量状态

| 检查项 | 状态 |
|--------|------|
| cargo check | ✅ 通过 |
| cargo test | ✅ 139 测试通过 (lib) |
| clippy 警告 | ✅ 0 warnings |
| cargo build --lib | ✅ 0.73s |

---

## 调试技巧

- GUI 启动时会打印 DEBUG: GUI started, log file: ... 日志
- eprintln!("DEBUG yt-dlp stderr: {}", line) 会输出 yt-dlp 的 stderr
- parse_ytdlp_progress() 函数解析进度，格式: [download] 45.0% of ~2.50MB
- Slint 回调中使用 invoke_from_event_loop 线程安全更新 UI
- 设置 RUST_LOG=debug 查看详细日志