---
title: Platform Resolver Skeleton — YtdlpBacked Base + HTTP Client Factory
date: 2026-09-02
status: design-approved-pending-spec-review
intent: DRY 4 个 yt-dlp 包装型 resolver (YouTube/TikTok/Bilibili/DouYin) 的字段/构造/调用样板,顺手把 3+ 处 reqwest::Client 重复构造收敛到 http_client 工厂。下次新加 yt-dlp 平台从 ~100 行降到 ~30 行。
---

# Platform Resolver Skeleton — Design

## 1. Context

8 个 `PlatformResolver` impl 各自维护相同样板代码:

**Ytdlp 包装型**(YouTube/TikTok/Bilibili/DouYin 4 个,共 ~550 行)
- 字段 `tokens: HashMap<String,String>` + `cookie_manager: CookieFileManager`
- `new()` / `with_tokens()` 构造器(几乎一字不差)
- `get_cookie_file()` 内联 helper
- `get_video_info()` 体内:`build ytdlp → 注入 cookie → ytdlp.get_video_info → log + wrap错误`(YouTube:54-77、TikTok:67-87、Bilibili:56-75、DouYin:175-202+207-220)

**reqwest::Client 构造**(Nnyy:21、FxTwitter:140、ZhiXueYun:95/113 共 3+ 处)
- UA 字符串 `"Mozilla/5.0 ... Chrome/120.0.0.0 Safari/537.36"` 散落多处
- `tcp_keepalive / connect_timeout / timeout` 三个 Duration 重复设置
- 改浏览器版本要全局 grep,容易漏

每次新加 yt-dlp 包装平台要复制 ~100 行,YAGNI 红线。

## 2. Goals / Non-Goals

### Goals

1. **杠杆**:新加 yt-dlp 包装平台从 ~100 行降到 ~30 行
2. **DRY**:4 个 yt-dlp 包装 resolver 净减少 ~200 行重复
3. **集中 UA**:改浏览器版本只动 `http_client::RESOLVER_UA` 一处
4. **零回归**:既有 256/256 测试不动,新增 4-6 个 base 单测

### Non-Goals

1. 不改 `PlatformResolver` trait 接口
2. 不改 `ResolverRegistry` 调度逻辑
3. 不重构 Nnyy/TulingPython/ZhiXueYun/FxTwitter 的非重复部分
4. 不引入新依赖(macro 不参与本期)
5. 不做平台错误信息国际化(沿用中文模板)

## 3. Architecture

### 3.1 Module Layout

```
src/
├── platform/
│   ├── base.rs           [新增]   YtdlpBacked + 单测
│   ├── mod.rs            [改]     pub mod base; pub use base::YtdlpBacked;
│   ├── youtube.rs        [改]     字段 → { base: YtdlpBacked }
│   ├── tiktok.rs         [改]     同上
│   ├── bilibili.rs       [改]     同上
│   └── douyin.rs         [改]     yt-dlp 分支改用 base; API 分支保留
├── http_client.rs        [改]     + RESOLVER_UA / build_resolver_client()
└── fxtwitter.rs          [改]     with_proxy 改用 build_resolver_client()
```

### 3.2 Component Diagram

```
┌────────────────────────────────────────────────────────┐
│ PlatformResolver (trait, 已有, 不动)                   │
│   get_video_info(url) -> Result<VideoInfo, ResolverError>
└──────────────────▲─────────────────────────────────────┘
                   │
   ┌───────────────┼───────────────┬─────────────────┐
   │               │               │                 │
YouTubeResolver TikTokResolver BilibiliResolver DouYinResolver
{ base }        { base }        { base }          { base } + (API 分支原样)
   │               │               │                 │
   └───────────────┴───────────────┴─────────────────┘
                   │
                   ▼
        ┌─────────────────────┐
        │ YtdlpBacked (新)    │
        │  tokens             │
        │  cookie_manager     │
        │  platform_tag       │
        │  fetch_via_ytdlp()  │──► http_client::build_ytdlp()
        └─────────────────────┘                       │
                                                     ▼
                                          reqwest::Client (复用)
```

## 4. Components

### 4.1 `YtdlpBacked` (新, `src/platform/base.rs`)

```rust
//! yt-dlp 包装型 resolver 的共享基类
//!
//! 收纳 YouTube / TikTok / Bilibili / DouYin(yt-dlp 分支) 4 个 resolver
//! 的字段 / 构造器 / 调用样板。新加 yt-dlp 包装平台只需:
//! ```ignore
//! struct XResolver { base: YtdlpBacked }
//! // impl PlatformResolver { ... self.base.fetch_via_ytdlp(url).await ... }
//! ```

use crate::history::Platform;
use crate::platform::{CookieFileManager, PlatformResolver, ResolverError};
use crate::types::VideoInfo;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub struct YtdlpBacked {
    tokens: HashMap<String, String>,
    cookie_manager: CookieFileManager,
    platform_tag: Platform,
    token_manager: Option<Arc<Mutex<crate::platform::TokenManager>>>,
}

impl YtdlpBacked {
    /// 创建指定平台 tag 的 base。platform_tag 用于错误日志前缀。
    pub fn new(platform: Platform) -> Self {
        Self {
            tokens: HashMap::new(),
            cookie_manager: CookieFileManager::new(),
            platform_tag: platform,
            token_manager: None,
        }
    }

    /// 设置 tokens (builder 风格, 沿用现有 TikTok/DouYin 接口)
    pub fn with_tokens(mut self, tokens: HashMap<String, String>) -> Self {
        self.tokens = tokens;
        self
    }

    /// 设置自动 token 生成器 (TikTok/DouYin 用)
    #[expect(dead_code)]
    pub fn with_token_manager(mut self, tm: Arc<Mutex<crate::platform::TokenManager>>) -> Self {
        self.token_manager = Some(tm);
        self
    }

    /// 检查是否包含指定 token (用于 `requires_auth()`)
    pub fn has_token(&self, key: &str) -> bool {
        self.tokens.contains_key(key)
    }

    /// 生成 cookie 文件路径 (来自 tokens),无 tokens 返回 None
    pub fn cookie_file(&self) -> Option<PathBuf> {
        // 用 platform_tag 的小写作为 namespace
        let ns = match self.platform_tag {
            Platform::YouTube => "youtube",
            Platform::TikTok => "tiktok",
            Platform::Bilibili => "bilibili",
            Platform::DouYin => "douyin",
            _ => "default",
        };
        self.cookie_manager.get_cookie_file(&self.tokens, ns)
    }

    /// 核心: build ytdlp → 注入 cookie → 调 yt-dlp → log + wrap 错误
    pub async fn fetch_via_ytdlp(&self, url: &str) -> Result<VideoInfo, ResolverError> {
        let mut ytdlp = crate::yt_dlp::YtDlp::new();
        if let Some(cookie_file) = self.cookie_file() {
            tracing::debug!("{} using cookie file for authentication", self.platform_tag);
            ytdlp = ytdlp.with_cookie_file(cookie_file);
        }
        match ytdlp.get_video_info(url).await {
            Ok(info) => {
                tracing::info!("{} video info via yt-dlp: {}", self.platform_tag, info.title);
                Ok(info)
            }
            Err(e) => {
                tracing::warn!("{} yt-dlp failed: {}", self.platform_tag, e);
                Err(ResolverError::ParseFailed(format!(
                    "{} 解析失败: {}",
                    self.platform_tag,
                    e
                )))
            }
        }
    }
}
```

### 4.2 `http_client` 扩展 (改, `src/http_client.rs`)

```rust
// 新增常量 (pub 供其他模块复用)
pub const RESOLVER_UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) \
    AppleWebKit/537.36 (KHTML, like Gecko) Chrome/148.0.0.0 Safari/537.36";

// 新增工厂函数
pub fn build_resolver_client() -> reqwest::Client {
    apply_resolver_defaults(Client::builder())
        .build()
        .unwrap_or_else(|e| panic!("Failed to build resolver HTTP client: {}", e))
}

// 给已有 builder 套上 resolver 默认值 (供需要追加 proxy 等的 caller)
// 例: build_resolver_client_with(|b| b.proxy(...))
pub fn apply_resolver_defaults(b: reqwest::ClientBuilder) -> reqwest::ClientBuilder {
    b.user_agent(RESOLVER_UA)
        .tcp_keepalive(std::time::Duration::from_secs(120))
        .connect_timeout(std::time::Duration::from_secs(30))
        .timeout(std::time::Duration::from_secs(600))
}
```

**保留** `shared()` 和 `SHARED_CLIENT`(连接池复用价值仍在,主下载路径用 `shared()`,resolver 偶发 HTTP 用 `build_resolver_client()`)。

### 4.3 Resolver 改造样板

以 TikTok 为例(其他 3 个套同模式):

**改前** 109 行,**改后** 47 行(节选):
```rust
pub struct TikTokResolver {
    base: YtdlpBacked,
}

impl TikTokResolver {
    pub fn new() -> Self {
        Self { base: YtdlpBacked::new(Platform::TikTok) }
    }
    pub fn with_tokens(mut self, tokens: HashMap<String, String>) -> Self {
        self.base = self.base.with_tokens(tokens);
        self
    }
    #[expect(dead_code)]
    pub fn with_token_manager(mut self, tm: Arc<Mutex<TokenManager>>) -> Self {
        self.base = self.base.with_token_manager(tm);
        self
    }
}

#[async_trait]
impl PlatformResolver for TikTokResolver {
    fn platform(&self) -> Platform { Platform::TikTok }
    fn supported_url_patterns(&self) -> Vec<&'static str> {
        url_matcher::get_platform_patterns(Platform::TikTok).to_vec()
    }
    async fn get_video_info(&self, url: &str) -> Result<VideoInfo, ResolverError> {
        self.base.fetch_via_ytdlp(url).await
    }
    fn requires_auth(&self) -> bool { self.base.has_token("msToken") }
    fn required_tokens(&self) -> Vec<&'static str> { vec!["msToken", "ttwid"] }
}
```

**DouYin 例外**:保留 API 分支原样,仅 yt-dlp fallback 分支用 `base.fetch_via_ytdlp`:
```rust
// 已有 API 成功路径不动
// API 失败 / 无 modal_id 时:
let info = self.base.fetch_via_ytdlp(&download_url).await?;
```

### 4.4 `fxtwitter.rs` 改造

`with_proxy()` 中 `reqwest::Client::builder()...` 改用 `apply_resolver_defaults()` + 追加 `.proxy(...)`:

```rust
let client = crate::http_client::apply_resolver_defaults(reqwest::Client::builder())
    .proxy(reqwest::Proxy::all(&proxy_with_scheme)?)
    .build()?;
```

## 5. Data Flow

新加 yt-dlp 包装平台的标准流程(以假想 `TwitterFallbackResolver` 为例):

```
1. impl Default::new()  → base = YtdlpBacked::new(Platform::X)
2. impl with_tokens()    → base.with_tokens(tokens)
3. trait platform()      → Platform::X (常量)
4. trait supports_url()  → 复用默认实现(url_matcher 已有)
5. trait get_video_info  → self.base.fetch_via_ytdlp(url).await
6. trait requires_auth   → self.base.has_token("xxx")
7. trait required_tokens → vec![...]
```

总计 ~30 行(原 ~100 行),新平台**零样板**。

## 6. Error Handling

**不引入** 新 `ResolverError` variant。`fetch_via_ytdlp` 沿用现有模式:

| 场景 | 处理 |
|---|---|
| ytdlp Ok | `tracing::info!`,return Ok |
| ytdlp Err(e: anyhow::Error) | `tracing::warn!`,return `Err(ResolverError::ParseFailed(format!("{platform} 解析失败: {e}")))` |

与现有 YouTube:70-77、TikTok:82-86、Bilibili:69-74 完全一致。错误日志前缀用 `self.platform_tag`(`Platform` 已实现 `Display`)。

## 7. Testing Strategy

### 7.1 新增单测 (`src/platform/base.rs` 内 `#[cfg(test)] mod tests`)

| 测试 | 覆盖 |
|---|---|
| `test_ytdlp_backed_new_initializes_empty` | `new()` 后 tokens 空 / has_token 都 false |
| `test_ytdlp_backed_with_tokens_sets_tokens` | builder 返回新实例,旧实例未变(不可变性) |
| `test_ytdlp_backed_cookie_file_none_when_empty` | 无 tokens → None |
| `test_ytdlp_backed_cookie_file_some_when_present` | 有 tokens → Some(PathBuf 存在) |
| `test_ytdlp_backed_cookie_file_namespace_per_platform` | TikTok 平台 cookie_manager ns == "tiktok",DouYin == "douyin",Bilibili == "bilibili" |

### 7.2 新增单测 (`src/http_client.rs` 内 `#[cfg(test)] mod tests`)

| 测试 | 覆盖 |
|---|---|
| `test_resolver_ua_constant` | `RESOLVER_UA` 包含 "Chrome/" |
| `test_build_resolver_client_succeeds` | 调用成功,返回的 client UA 等于 RESOLVER_UA |

### 7.3 回归检查

- `cargo test --lib` 期望:既有 ~256 tests + 新 7 = ~263 全过
- `cargo clippy -- -D warnings` 无警告(注意 `unwrap_used = "warn"` 在 base.rs 中需谨慎)
- `cargo build` 编译通过
- 手工 smoke(可选):`cargo run -- --help` 启动,检查 resolver 注册顺序

### 7.4 不在范围内

- `YtdlpBacked::fetch_via_ytdlp` 的端到端测试(需 yt-dlp 二进制 + 网络,留给 `#[ignore]` 集成测试)
- 4 个 resolver 自己的 `test_supports_url` 不变

## 8. Migration / Rollout

按依赖顺序改动,每个 commit 独立可编译:

1. **Commit 1**: `feat(platform): add YtdlpBacked base struct + unit tests`
   - 仅新增 `src/platform/base.rs` + `mod.rs` 注册
   - 零回归风险
2. **Commit 2**: `feat(http_client): add RESOLVER_UA + build_resolver_client() + tests`
   - 仅扩展 `src/http_client.rs`
   - 零回归风险
3. **Commit 3**: `refactor(youtube): use YtdlpBacked`
   - YouTube 单文件净 -60 行
4. **Commit 4**: `refactor(tiktok): use YtdlpBacked`
   - TikTok 单文件净 -65 行
5. **Commit 5**: `refactor(bilibili): use YtdlpBacked`
   - Bilibili 单文件净 -55 行
6. **Commit 6**: `refactor(douyin): use YtdlpBacked for yt-dlp branch`
   - DouYin 改动较细,保留 API 分支
7. **Commit 7**: `refactor(fxtwitter): use build_resolver_client()`
   - FxTwitter with_proxy 收缩

每步独立运行 `cargo test --lib` 验证,任意步骤失败立即停止。

## 9. Risks & Mitigations

| 风险 | 概率 | 缓解 |
|---|---|---|
| 平台 tag Display 输出与现有日志不一致 | 中 | Commit 1 加测试 `assert_eq!(format!("{}", Platform::TikTok), "TikTok")` 锁定格式 |
| `build_resolver_client()` 不带连接池,FxTwitter 高频调用降级 | 低 | FxTwitter 是 fallback 路径,频率低;若需可改回 `shared()` + `apply_resolver_defaults` |
| CookieFileManager namespace 改 platform-based 后,旧 cookie 缓存失效 | 中 | §7.1 `test_ytdlp_backed_cookie_file_namespace_per_platform` 锁定 ns == 原值 ("tiktok"/"douyin"/"bilibili") |
| `unwrap_used = "warn"` 在新代码触发警告 | 低 | 用 `unwrap_or_else(\|e\| panic!(...))` 模式,沿用 ZhiXueYun:100 |
| DouYin 改动复杂,API 分支保留引入 bug | 中 | Commit 6 单独,出问题 revert 单 commit |
| YtdlpBacked 内部 log 信息丢失细节(原 YouTube 有额外 tracing) | 低 | fetch_via_ytdlp 内 tracing 覆盖所有现有 log 等级 |
| YouTube 当前用 user-supplied cookie_path,迁到 base 后改用 CookieFileManager 生成临时文件 | 中 | YouTube 行为微调在 §10 out-of-scope 记录;若需保留 user-path 行为,可在 YouTube impl 单独覆盖 cookie_file() 方法 |

## 10. Out of Scope (记录 TODO)

- C. `m3u8_extractor::extract_from_html` 共享(见 brainstorming §4 C,留待下期)
- D. `wrap_resolver_call!` 宏(见 brainstorming §4 D,价值低不做)
- Nnyy/ZhiXueYun 内部 m3u8 / CookiePool 抽取
- FxTwitter JSON 解析抽取(只共享 client 部分)
- TulingPython Drission 抽象

## 11. Open Questions

无。brainstorming 已澄清所有设计选择。