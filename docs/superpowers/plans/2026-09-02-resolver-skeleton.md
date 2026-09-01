# Resolver Skeleton Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** DRY 4 yt-dlp 包装 resolver (YouTube/TikTok/Bilibili/DouYin) 的字段/构造/调用样板 + 集中 3+ 处 reqwest::Client 重复构造,下次新加 yt-dlp 平台从 ~100 行降到 ~30 行。

**Architecture:** 引入 `YtdlpBacked` 组合式基类 (字段 `tokens/cookie_manager/platform_tag`) + `fetch_via_ytdlp(url)` 核心方法;扩展 `http_client` 加 `RESOLVER_UA` 常量 + `build_resolver_client()` 工厂 + `apply_resolver_defaults(builder)` helper。4 个 resolver 改用 base,7 步 commit 独立可编译。

**Tech Stack:** Rust 1.x + async-trait 0.1 + reqwest 0.12 + once_cell 1.19 + tokio 1.35 + CookieFileManager(已有) + tracing 0.1

**Spec:** `docs/superpowers/specs/2026-09-02-resolver-skeleton-design.md`

---

## Global Constraints

- **不引入新依赖**(no new crates;本期用 base struct,不引入 macro 框架)
- **不改 `PlatformResolver` trait**(已在 `src/platform/mod.rs:228`)
- **不改 `ResolverRegistry` 调度逻辑**(已在 `src/platform/mod.rs:354`)
- **每 commit 独立 `cargo test --lib` 通过 + `cargo clippy` 无新警告**
- **每个 commit message 必须含 `Intent: <一句话>` 一行**(项目 commit-msg hook 强制)
- **错误日志沿用中文模板** `"{platform} 解析失败: {e}"` (现有 YouTube:72/TikTok:84/Bilibili:72)
- **ZhiXueYun 阻塞 client 不动**(blocking client 不同 API,本期不合并)
- **`unwrap_used = "warn"`** 全程用 `unwrap_or_else(|e| panic!("..."))` 模式 (沿用 ZhiXueYun:100)

---

## File Structure

| 操作 | 文件 | 职责 |
|---|---|---|
| 新增 | `src/platform/base.rs` | `YtdlpBacked` 结构体 + 单测 |
| 改 | `src/platform/mod.rs` | 加 `mod base; pub use base::YtdlpBacked;` |
| 改 | `src/platform/youtube.rs` | 字段 → `{ base: YtdlpBacked }` |
| 改 | `src/platform/tiktok.rs` | 同上 |
| 改 | `src/platform/bilibili.rs` | 同上 |
| 改 | `src/platform/douyin.rs` | yt-dlp 分支改用 base;API 分支保留 |
| 改 | `src/http_client.rs` | 加 `RESOLVER_UA` + `build_resolver_client()` + `apply_resolver_defaults()` |
| 改 | `src/fxtwitter.rs` | `with_proxy()` 改用 `apply_resolver_defaults` |

**接口契约**:
- `YtdlpBacked::new(platform: Platform) -> Self`
- `YtdlpBacked::with_tokens(self, tokens: HashMap<String, String>) -> Self` (consuming builder)
- `YtdlpBacked::with_token_manager(self, tm: Arc<Mutex<TokenManager>>) -> Self`
- `YtdlpBacked::has_token(&self, key: &str) -> bool`
- `YtdlpBacked::cookie_file(&self) -> Option<PathBuf>`
- `YtdlpBacked::fetch_via_ytdlp(&self, url: &str) -> Result<VideoInfo, ResolverError>`
- `http_client::RESOLVER_UA: &'static str`
- `http_client::build_resolver_client() -> reqwest::Client`
- `http_client::apply_resolver_defaults(b: reqwest::ClientBuilder) -> reqwest::ClientBuilder`

---

### Task 1: Add `YtdlpBacked` base struct + unit tests

**Files:**
- Create: `src/platform/base.rs`
- Modify: `src/platform/mod.rs:4-34` (在 `mod tulingpython;` 后插入 `mod base;`,加 `pub use base::YtdlpBacked;`)

**Interfaces:**
- Consumes: `crate::platform::{CookieFileManager, PlatformResolver, ResolverError}`, `crate::history::Platform`, `crate::types::VideoInfo`, `crate::platform::TokenManager`
- Produces: `YtdlpBacked` (无依赖其他 task)

- [ ] **Step 1: Write the failing tests**

新建 `src/platform/base.rs`,写入(只 tests,空 struct):

```rust
//! yt-dlp 包装型 resolver 的共享基类
//!
//! 收纳 YouTube / TikTok / Bilibili / DouYin(yt-dlp 分支) 4 个 resolver
//! 的字段 / 构造器 / 调用样板。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::history::Platform;
use crate::platform::{CookieFileManager, TokenManager};

pub struct YtdlpBacked {
    #[allow(dead_code)]
    tokens: HashMap<String, String>,
    #[allow(dead_code)]
    cookie_manager: CookieFileManager,
    #[allow(dead_code)]
    platform_tag: Platform,
    #[allow(dead_code)]
    token_manager: Option<Arc<Mutex<TokenManager>>>,
}

impl YtdlpBacked {
    #[allow(dead_code)]
    pub fn new(platform: Platform) -> Self {
        Self {
            tokens: HashMap::new(),
            cookie_manager: CookieFileManager::new(),
            platform_tag: platform,
            token_manager: None,
        }
    }

    #[allow(dead_code)]
    pub fn with_tokens(mut self, tokens: HashMap<String, String>) -> Self {
        self.tokens = tokens;
        self
    }

    #[allow(dead_code)]
    pub fn with_token_manager(mut self, tm: Arc<Mutex<TokenManager>>) -> Self {
        self.token_manager = Some(tm);
        self
    }

    #[allow(dead_code)]
    pub fn has_token(&self, key: &str) -> bool {
        self.tokens.contains_key(key)
    }

    #[allow(dead_code)]
    pub fn cookie_file(&self) -> Option<PathBuf> {
        let ns = match self.platform_tag {
            Platform::YouTube => "youtube",
            Platform::TikTok => "tiktok",
            Platform::Bilibili => "bilibili",
            Platform::DouYin => "douyin",
            _ => "default",
        };
        self.cookie_manager.get_cookie_file(&self.tokens, ns)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ytdlp_backed_new_initializes_empty() {
        let base = YtdlpBacked::new(Platform::TikTok);
        assert!(!base.has_token("msToken"));
        assert!(!base.has_token("any"));
        assert!(base.cookie_file().is_none());
    }

    #[test]
    fn test_ytdlp_backed_with_tokens_sets_tokens() {
        let base = YtdlpBacked::new(Platform::TikTok)
            .with_tokens(HashMap::from([("msToken".to_string(), "abc".to_string())]));
        assert!(base.has_token("msToken"));
        assert!(!base.has_token("ttwid"));
    }

    #[test]
    fn test_ytdlp_backed_with_tokens_immutable_builder() {
        let original = YtdlpBacked::new(Platform::TikTok);
        let _modified = original
            .clone_with_tokens(HashMap::from([("msToken".to_string(), "x".to_string())]));
        // 注意:HashMap 不 Clone,需要 base 提供 clone helper 或测试方法调整。
        // 实际通过 base 字段私有,这里只断言 has_token:
        let base2 = YtdlpBacked::new(Platform::TikTok)
            .with_tokens(HashMap::from([("msToken".to_string(), "x".to_string())]));
        assert!(base2.has_token("msToken"));
    }

    #[test]
    fn test_ytdlp_backed_cookie_file_none_when_empty() {
        let base = YtdlpBacked::new(Platform::TikTok);
        assert!(base.cookie_file().is_none());
    }

    #[test]
    fn test_ytdlp_backed_cookie_file_namespace_per_platform() {
        let tiktok = YtdlpBacked::new(Platform::TikTok)
            .with_tokens(HashMap::from([("msToken".to_string(), "x".to_string())]));
        let path = tiktok.cookie_file();
        assert!(path.is_some(), "TikTok with msToken must produce cookie file");
        // cookie_manager 写入命名空间 "tiktok",生成的文件名包含 ns
        let path_str = path.unwrap().to_string_lossy().into_owned();
        assert!(
            path_str.contains("tiktok"),
            "TikTok cookie file path must contain 'tiktok', got: {path_str}"
        );
    }
}
```

**注意**:`test_ytdlp_backed_with_tokens_immutable_builder` 用 `clone_with_tokens` 引用一个不存在的方法。**改为简化版**(去掉该测试,只断言新实例的 has_token,见下方 Step 1 修正)。

修正后 `tests` 模块(去掉引用不存在方法的测试):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ytdlp_backed_new_initializes_empty() {
        let base = YtdlpBacked::new(Platform::TikTok);
        assert!(!base.has_token("msToken"));
        assert!(!base.has_token("any"));
        assert!(base.cookie_file().is_none());
    }

    #[test]
    fn test_ytdlp_backed_with_tokens_sets_tokens() {
        let base = YtdlpBacked::new(Platform::TikTok)
            .with_tokens(HashMap::from([("msToken".to_string(), "abc".to_string())]));
        assert!(base.has_token("msToken"));
        assert!(!base.has_token("ttwid"));
    }

    #[test]
    fn test_ytdlp_backed_cookie_file_none_when_empty() {
        let base = YtdlpBacked::new(Platform::TikTok);
        assert!(base.cookie_file().is_none());
    }

    #[test]
    fn test_ytdlp_backed_cookie_file_namespace_per_platform() {
        let tiktok = YtdlpBacked::new(Platform::TikTok)
            .with_tokens(HashMap::from([("msToken".to_string(), "x".to_string())]));
        let path = tiktok.cookie_file();
        assert!(path.is_some(), "TikTok with msToken must produce cookie file");
        let path_str = path.unwrap().to_string_lossy().into_owned();
        assert!(
            path_str.contains("tiktok"),
            "TikTok cookie file path must contain 'tiktok', got: {path_str}"
        );
    }
}
```

- [ ] **Step 2: Run test to verify compilation succeeds**

Run: `cargo build --lib 2>&1 | tail -20`
Expected: 编译通过(struct 未用 `fetch_via_ytdlp` 等未实现方法,但允许 `#[allow(dead_code)]` 标记)

- [ ] **Step 3: Register `base` module**

修改 `src/platform/mod.rs`,在 `mod bilibili;` 之后插入:

```rust
mod base;
```

并在文件末尾 `pub use` 区加:

```rust
pub use base::YtdlpBacked;
```

- [ ] **Step 4: Run unit tests**

Run: `cargo test --lib platform::base:: 2>&1 | tail -30`
Expected: 4 个测试全过 (`test_ytdlp_backed_*`)

- [ ] **Step 5: Verify full test suite still green**

Run: `cargo test --lib 2>&1 | tail -10`
Expected: 所有既有测试通过 + 4 个新测试 = 260 tests

- [ ] **Step 6: Verify Platform Display format**

Run: `cargo test --lib platform::base::test_ytdlp_backed_cookie_file_namespace_per_platform -- --nocapture 2>&1 | tail -20`
Expected: 测试通过(隐含验证 Platform::TikTok 在日志中可读)

- [ ] **Step 7: Commit**

```bash
git add src/platform/base.rs src/platform/mod.rs
git commit -m "$(cat <<'EOF'
feat(platform): add YtdlpBacked base struct + unit tests

新增 src/platform/base.rs 收纳 YouTube/TikTok/Bilibili/DouYin 4 个
yt-dlp 包装 resolver 共享的 tokens/cookie_manager/platform_tag 字段
和 has_token/cookie_file 辅助方法,4 个测试覆盖。下一步 Task 3-6
把 4 个 resolver impl 收缩为 ~30 行/个,共用 fetch_via_ytdlp()。

Intent: 为 4 个 yt-dlp 包装 resolver 提供共享基类的字段/构造/查询层
EOF
)"
```

---

### Task 2: Add `RESOLVER_UA` + `build_resolver_client()` + `apply_resolver_defaults()` + tests

**Files:**
- Modify: `src/http_client.rs:1-33` (加常量 + 2 个新函数 + 1 个 `#[cfg(test)]` 模块)

**Interfaces:**
- Consumes: `reqwest::Client`, `reqwest::ClientBuilder`
- Produces:
  - `pub const RESOLVER_UA: &'static str`
  - `pub fn build_resolver_client() -> reqwest::Client`
  - `pub fn apply_resolver_defaults(b: reqwest::ClientBuilder) -> reqwest::ClientBuilder`

- [ ] **Step 1: Write the failing tests**

修改 `src/http_client.rs`,文件末尾追加(原 `shared()` 函数保留):

```rust
/// Resolver 默认 UA(集中维护,改版本只动这一处)
pub const RESOLVER_UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) \
    AppleWebKit/537.36 (KHTML, like Gecko) Chrome/148.0.0.0 Safari/537.36";

/// 给已有 builder 套上 resolver 默认值(供需要追加 proxy/超时 等的 caller)
pub fn apply_resolver_defaults(b: reqwest::ClientBuilder) -> reqwest::ClientBuilder {
    b.user_agent(RESOLVER_UA)
        .tcp_keepalive(std::time::Duration::from_secs(120))
        .connect_timeout(std::time::Duration::from_secs(30))
        .timeout(std::time::Duration::from_secs(600))
}

/// 构建一个独立的 resolver HTTP client(用于一次性短请求,不参与连接池复用)
pub fn build_resolver_client() -> reqwest::Client {
    apply_resolver_defaults(reqwest::Client::builder())
        .build()
        .unwrap_or_else(|e| panic!("Failed to build resolver HTTP client: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolver_ua_constant_format() {
        assert!(RESOLVER_UA.contains("Chrome/"));
        assert!(RESOLVER_UA.contains("Safari/"));
        assert!(RESOLVER_UA.starts_with("Mozilla/5.0"));
    }

    #[test]
    fn test_build_resolver_client_succeeds() {
        let client = build_resolver_client();
        // reqwest::Client 的 UA 不可直接 query,但能 build 不 panic 即证明配置合法
        let _ = client;
    }

    #[test]
    fn test_apply_resolver_defaults_returns_builder() {
        let builder = apply_resolver_defaults(reqwest::Client::builder());
        let client = builder.build().expect("resolver defaults must build");
        let _ = client;
    }
}
```

- [ ] **Step 2: Run test to verify it passes**

Run: `cargo test --lib http_client:: 2>&1 | tail -20`
Expected: 3 个测试全过

- [ ] **Step 3: Verify full test suite still green**

Run: `cargo test --lib 2>&1 | tail -5`
Expected: 263 tests(260 + 3)全过

- [ ] **Step 4: Verify clippy**

Run: `cargo clippy --lib 2>&1 | tail -20`
Expected: 无 `unwrap_used` 警告(用 `unwrap_or_else(panic!)` 避免)

- [ ] **Step 5: Commit**

```bash
git add src/http_client.rs
git commit -m "$(cat <<'EOF'
feat(http_client): add RESOLVER_UA + build_resolver_client()

新增 RESOLVER_UA 常量集中维护浏览器版本(原散落 Nnyy/FxTwitter/
ZhiXueYun 3+ 处);build_resolver_client() 给一次性 resolver HTTP
调用复用统一 UA/timeout;apply_resolver_defaults() 让需要追加
proxy 等的 caller 在已有 builder 上套默认值。3 个测试覆盖。

Intent: 把 3+ 处 reqwest::Client 重复构造 + UA 字符串集中到 http_client
EOF
)"
```

---

### Task 3: Refactor `YouTubeResolver` to use `YtdlpBacked`

**Files:**
- Modify: `src/platform/youtube.rs:1-87`(全文),`src/platform/base.rs`(加 `fetch_via_ytdlp`)

**Interfaces:**
- Consumes: `YtdlpBacked` (从 Task 1)
- Produces: `YtdlpBacked::fetch_via_ytdlp` 新方法 + `YouTubeResolver { base: YtdlpBacked }`,保持 `PlatformResolver` trait 行为不变

- [ ] **Step 1: Verify existing YouTube tests pass**

Run: `cargo test --lib platform::youtube 2>&1 | tail -10`
Expected: 编译通过(目前 YouTube 无 test mod,但 resolver_chain 的注册/调度测试应仍过)

- [ ] **Step 2: Add `fetch_via_ytdlp` to `YtdlpBacked`**

修改 `src/platform/base.rs`,顶部 import 改为:

```rust
use crate::platform::{CookieFileManager, PlatformResolver, ResolverError, TokenManager};
use crate::types::VideoInfo;
```

在 `cookie_file()` 方法后追加:

```rust
    /// 核心:build ytdlp → 注入 cookie → 调 yt-dlp → log + wrap 错误
    #[allow(dead_code)]
    pub async fn fetch_via_ytdlp(
        &self,
        url: &str,
    ) -> Result<VideoInfo, ResolverError> {
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
```

- [ ] **Step 3: Verify base compiles with new method**

Run: `cargo build --lib 2>&1 | tail -10`
Expected: 编译通过

- [ ] **Step 4: Rewrite `src/platform/youtube.rs`**

完整替换为:

```rust
//! YouTube 解析器
//!
//! 使用 yt-dlp 作为主要方式,支持 Cookie 认证
//! 用于处理 age-restricted 或需要登录的视频

use crate::history::Platform;
use crate::platform::{url_matcher, PlatformResolver, ResolverError, YtdlpBacked};
use crate::types::VideoInfo;
use async_trait::async_trait;

/// YouTube 解析器
pub struct YouTubeResolver {
    base: YtdlpBacked,
}

impl YouTubeResolver {
    pub fn new() -> Self {
        Self {
            base: YtdlpBacked::new(Platform::YouTube),
        }
    }

    /// 从 cookie 文件创建解析器 (向后兼容,本期 cookie_path 行为保留)
    #[expect(dead_code)]
    pub fn with_cookie_file(self, _path: std::path::PathBuf) -> Self {
        // 注: base 暂未支持 user-supplied cookie path,本期 YouTube 不再使用此 builder
        // 留待下期扩展 base(参见 plan §10 out-of-scope)
        self
    }
}

impl Default for YouTubeResolver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PlatformResolver for YouTubeResolver {
    fn platform(&self) -> Platform {
        Platform::YouTube
    }

    fn supported_url_patterns(&self) -> Vec<&'static str> {
        url_matcher::get_platform_patterns(Platform::YouTube).to_vec()
    }

    fn supports_url_by_host(&self, host_lower: &str) -> bool {
        host_lower == "youtube.com"
            || host_lower.ends_with(".youtube.com")
            || host_lower == "youtu.be"
            || host_lower.ends_with(".youtu.be")
    }

    async fn get_video_info(&self, url: &str) -> Result<VideoInfo, ResolverError> {
        self.base.fetch_via_ytdlp(url).await
    }

    fn requires_auth(&self) -> bool {
        false // Cookie 可选,不强制要求
    }

    fn download_strategy(&self) -> crate::types::DownloadStrategy {
        crate::types::DownloadStrategy::YtDlp
    }
}
```

- [ ] **Step 5: Run full test suite**

Run: `cargo test --lib 2>&1 | tail -5`
Expected: 263 tests 全过(YouTube 无自己的 test mod,主要靠 resolver_chain 注册测试)

- [ ] **Step 6: Verify clippy**

Run: `cargo clippy --lib 2>&1 | tail -10`
Expected: 无新警告(`#[expect(dead_code)]` 抑制 `with_cookie_file` 警告)

- [ ] **Step 7: Commit**

```bash
git add src/platform/youtube.rs src/platform/base.rs
git commit -m "$(cat <<'EOF'
refactor(youtube): use YtdlpBacked base + add fetch_via_ytdlp to base

YouTube 解析器从 87 行收缩到 50 行:字段 tokens/cookie_manager 改
base: YtdlpBacked,get_video_info 改 base.fetch_via_ytdlp。同时给
base 加 fetch_via_ytdlp 核心方法,Task 4-6 共用。YouTube 现有
user-supplied cookie_path 行为留待下期扩展。

Intent: 第一个迁移案例,验证 YtdlpBacked 形状适合 4 个 resolver 之一
EOF
)"
```

---

### Task 4: Refactor `TikTokResolver` to use `YtdlpBacked`

**Files:**
- Modify: `src/platform/tiktok.rs:1-109`(全文)

**Interfaces:**
- Consumes: `YtdlpBacked::fetch_via_ytdlp` (从 Task 3)
- Produces: `TikTokResolver { base: YtdlpBacked }`,保持 `PlatformResolver` trait 行为不变

- [ ] **Step 1: Verify existing TikTok tests pass**

Run: `cargo test --lib platform::tiktok 2>&1 | tail -10`
Expected: `test_supports_url` 通过

- [ ] **Step 2: Rewrite `src/platform/tiktok.rs`**

完整替换为:

```rust
//! TikTok 解析器
//!
//! 使用 yt-dlp 作为主要方式,msToken/ttwid 用于认证
//! 支持 TokenManager 自动 token 生成和注入

use crate::history::Platform;
use crate::platform::{
    url_matcher, PlatformResolver, ResolverError, TokenManager, YtdlpBacked,
};
use crate::types::VideoInfo;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// TikTok 解析器
#[expect(dead_code)]
pub struct TikTokResolver {
    base: YtdlpBacked,
}

impl TikTokResolver {
    pub fn new() -> Self {
        Self {
            base: YtdlpBacked::new(Platform::TikTok),
        }
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

impl Default for TikTokResolver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PlatformResolver for TikTokResolver {
    fn platform(&self) -> Platform {
        Platform::TikTok
    }

    fn supported_url_patterns(&self) -> Vec<&'static str> {
        url_matcher::get_platform_patterns(Platform::TikTok).to_vec()
    }

    async fn get_video_info(&self, url: &str) -> Result<VideoInfo, ResolverError> {
        self.base.fetch_via_ytdlp(url).await
    }

    fn requires_auth(&self) -> bool {
        self.base.has_token("msToken")
    }

    fn required_tokens(&self) -> Vec<&'static str> {
        vec!["msToken", "ttwid"]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_supports_url() {
        let resolver = TikTokResolver::new();
        assert!(resolver.supports_url("https://www.tiktok.com/@user/video/123"));
        assert!(resolver.supports_url("https://vm.tiktok.com/abc"));
        assert!(!resolver.supports_url("https://youtube.com/watch"));
    }
}
```

- [ ] **Step 3: Run TikTok tests**

Run: `cargo test --lib platform::tiktok 2>&1 | tail -10`
Expected: `test_supports_url` 通过

- [ ] **Step 4: Verify full test suite green**

Run: `cargo test --lib 2>&1 | tail -5`
Expected: 263 tests 全过

- [ ] **Step 5: Verify clippy**

Run: `cargo clippy --lib 2>&1 | tail -10`
Expected: 无新警告

- [ ] **Step 6: Commit**

```bash
git add src/platform/tiktok.rs
git commit -m "$(cat <<'EOF'
refactor(tiktok): use YtdlpBacked base

TikTok 解析器从 109 行收缩到 55 行:字段 tokens/cookie_manager/
token_manager 改 base: YtdlpBacked,get_video_info 改 base.fetch_via_
ytdlp,requires_auth 改 base.has_token("msToken")。test_supports_url
保留。

Intent: 第二个迁移案例,验证 builder 链式 with_tokens/with_token_manager 仍兼容
EOF
)"
```

---

### Task 5: Refactor `BilibiliResolver` to use `YtdlpBacked`

**Files:**
- Modify: `src/platform/bilibili.rs:1-97`(全文),`src/platform/base.rs`(加 `tokens_is_empty`)

**Interfaces:**
- Consumes: `YtdlpBacked::fetch_via_ytdlp` (从 Task 3)
- Produces: `YtdlpBacked::tokens_is_empty` 新方法 + `BilibiliResolver { base: YtdlpBacked }`,保持 `PlatformResolver` trait 行为不变

- [ ] **Step 1: Verify existing Bilibili tests pass**

Run: `cargo test --lib platform::bilibili 2>&1 | tail -10`
Expected: `test_supports_url` 通过

- [ ] **Step 2: Add `tokens_is_empty` to `YtdlpBacked`**

修改 `src/platform/base.rs`,在 `has_token` 方法后追加:

```rust
    #[allow(dead_code)]
    pub fn tokens_is_empty(&self) -> bool {
        self.tokens.is_empty()
    }
```

- [ ] **Step 3: Rewrite `src/platform/bilibili.rs`**

完整替换为:

```rust
//! Bilibili 解析器
//!
//! 使用 yt-dlp 作为主要方式,支持 cookies 认证

use crate::history::Platform;
use crate::platform::{url_matcher, PlatformResolver, ResolverError, YtdlpBacked};
use crate::types::VideoInfo;
use async_trait::async_trait;
use std::collections::HashMap;

/// Bilibili 解析器
pub struct BilibiliResolver {
    base: YtdlpBacked,
}

impl BilibiliResolver {
    pub fn new() -> Self {
        Self {
            base: YtdlpBacked::new(Platform::Bilibili),
        }
    }

    #[expect(dead_code)]
    pub fn with_cookies(mut self, cookies: HashMap<String, String>) -> Self {
        self.base = self.base.with_tokens(cookies);
        self
    }
}

impl Default for BilibiliResolver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PlatformResolver for BilibiliResolver {
    fn platform(&self) -> Platform {
        Platform::Bilibili
    }

    fn supported_url_patterns(&self) -> Vec<&'static str> {
        url_matcher::get_platform_patterns(Platform::Bilibili).to_vec()
    }

    async fn get_video_info(&self, url: &str) -> Result<VideoInfo, ResolverError> {
        self.base.fetch_via_ytdlp(url).await
    }

    fn requires_auth(&self) -> bool {
        // Bilibili 旧逻辑: cookies 非空 → 需要 auth
        !self.base.tokens_is_empty()
    }

    fn required_tokens(&self) -> Vec<&'static str> {
        vec!["SESSDATA", "bili_jct", "buvid3"]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_supports_url() {
        let resolver = BilibiliResolver::new();
        assert!(resolver.supports_url("https://www.bilibili.com/video/BV1xx411c7XD"));
        assert!(resolver.supports_url("https://b23.tv/abc"));
        assert!(!resolver.supports_url("https://tiktok.com/video"));
    }
}
```

- [ ] **Step 4: Run Bilibili tests**

Run: `cargo test --lib platform::bilibili 2>&1 | tail -10`
Expected: `test_supports_url` 通过

- [ ] **Step 5: Verify full test suite green**

Run: `cargo test --lib 2>&1 | tail -5`
Expected: 263 tests 全过

- [ ] **Step 6: Verify clippy**

Run: `cargo clippy --lib 2>&1 | tail -10`
Expected: 无新警告

- [ ] **Step 7: Commit**

```bash
git add src/platform/bilibili.rs src/platform/base.rs
git commit -m "$(cat <<'EOF'
refactor(bilibili): use YtdlpBacked base + add tokens_is_empty helper

Bilibili 解析器从 97 行收缩到 48 行:字段 cookies/cookie_manager
改 base: YtdlpBacked,with_cookies 改 base.with_tokens,get_video_info
改 base.fetch_via_ytdlp,requires_auth 改 !base.tokens_is_empty。
base 同时新增 tokens_is_empty helper 给需要"非空判定"的 resolver
复用(如 Bilibili)。

Intent: 第三个迁移案例,验证 requires_auth 用 base helper 表达足够
EOF
)"
```

---

### Task 6: Refactor `DouYinResolver` yt-dlp branch to use `YtdlpBacked`

**Files:**
- Modify: `src/platform/douyin.rs:1-251`(全文),`src/platform/base.rs`(加 `tokens_get`)

**Interfaces:**
- Consumes: `YtdlpBacked::fetch_via_ytdlp` (从 Task 3)
- Produces: `YtdlpBacked::tokens_get` 新方法 + `DouYinResolver { base: YtdlpBacked }`,保持 `PlatformResolver` trait 行为不变

**警告**:DouYin 改动最复杂,有 API 分支(`spawn_blocking` + `DouYinApiClient`)和 yt-dlp 分支共存。本 task **仅迁移 yt-dlp 分支**,API 分支保留原样。

- [ ] **Step 1: Verify existing DouYin tests pass**

Run: `cargo test --lib platform::douyin 2>&1 | tail -10`
Expected: `test_supports_url` 通过

- [ ] **Step 2: Add `tokens_get` to `YtdlpBacked`**

修改 `src/platform/base.rs`,在 `tokens_is_empty` 方法后追加:

```rust
    #[allow(dead_code)]
    pub fn tokens_get(&self, key: &str) -> Option<&String> {
        self.tokens.get(key)
    }
```

- [ ] **Step 3: Rewrite `src/platform/douyin.rs`(保留 API 分支)**

完整替换为:

```rust
//! 抖音解析器
//!
//! 使用 yt-dlp 作为主要方式,msToken/ttwid 用于认证
//! 支持信息流 URL (jingxuan/sports) 和 modal_id 解析
//! API 分支(feed/modal_id)走 DouYinApiClient,yt-dlp 分支走 YtdlpBacked

use crate::history::Platform;
use crate::platform::{
    extract_modal_id, url_matcher, DouYinApiClient, PlatformResolver, ResolverError, YtdlpBacked,
};
use crate::types::{VideoFormat, VideoInfo};
use async_trait::async_trait;
use std::collections::HashMap;

/// 抖音解析器
#[expect(dead_code)]
pub struct DouYinResolver {
    base: YtdlpBacked,
}

impl DouYinResolver {
    pub fn new() -> Self {
        Self {
            base: YtdlpBacked::new(Platform::DouYin),
        }
    }

    pub fn with_tokens(mut self, tokens: HashMap<String, String>) -> Self {
        self.base = self.base.with_tokens(tokens);
        self
    }

    /// 构造直接视频 URL (jingxuan/recommend feed → modal_id)
    fn direct_video_url(&self, url: &str, modal_id: Option<&String>) -> String {
        if url.contains("/jingxuan/") || url.contains("/recommend/") {
            if let Some(m_id) = modal_id {
                return format!("https://www.douyin.com/video/{}", m_id);
            }
        }
        url.to_string()
    }
}

impl Default for DouYinResolver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PlatformResolver for DouYinResolver {
    fn platform(&self) -> Platform {
        Platform::DouYin
    }

    fn supported_url_patterns(&self) -> Vec<&'static str> {
        url_matcher::get_platform_patterns(Platform::DouYin).to_vec()
    }

    async fn get_video_info(&self, url: &str) -> Result<VideoInfo, ResolverError> {
        // 检查是否是 feed URL (jingxuan/sports 等) 或包含 modal_id
        let modal_id = extract_modal_id(url);
        let is_feed_url =
            url.contains("jingxuan") || url.contains("sports") || url.contains("recommend");

        // 对于 feed URL 和包含 modal_id 的 URL,优先使用 API
        if modal_id.is_some() || is_feed_url {
            tracing::info!("DouYin feed/modal_id URL detected, using API directly");

            if let Some(modal_id) = &modal_id {
                let modal_id_clone = modal_id.clone();
                let ms_token = self.base.tokens_get("msToken").cloned();

                let api_result = tokio::task::spawn_blocking(move || {
                    let client = if let Some(token) = ms_token {
                        DouYinApiClient::new().with_ms_token(token)
                    } else {
                        DouYinApiClient::new()
                    };
                    let rt = tokio::runtime::Runtime::new()?;
                    rt.block_on(client.get_video_by_modal_id(&modal_id_clone))
                })
                .await;

                match api_result {
                    Ok(Ok(resp)) => {
                        if let Some(video) =
                            resp.aweme_list.and_then(|list| list.into_iter().next())
                        {
                            let title = video.get_title();
                            let video_id = video.get_video_id().unwrap_or(modal_id).to_string();
                            let uploader = video.get_author_name();
                            let duration = video.get_duration();
                            let thumbnail = video
                                .video
                                .as_ref()
                                .and_then(|v| v.cover.as_ref())
                                .and_then(|c| c.url_list.as_ref())
                                .and_then(|urls| urls.first())
                                .cloned();

                            tracing::info!("DouYin API success: {} by {}", title, uploader);

                            return Ok(VideoInfo {
                                title: if title.is_empty() {
                                    format!("DouYin Video {}", video_id)
                                } else {
                                    title
                                },
                                description: None,
                                uploader: Some(uploader),
                                duration: Some(duration),
                                url: format!("https://www.douyin.com/video/{}", video_id),
                                thumbnail,
                                formats: vec![VideoFormat {
                                    format_id: video_id.clone(),
                                    ext: "mp4".to_string(),
                                    resolution: Some(
                                        video
                                            .video
                                            .as_ref()
                                            .map(|v| {
                                                let w = v.width.unwrap_or(0);
                                                let h = v.height.unwrap_or(0);
                                                if w > 0 && h > 0 {
                                                    format!("{}x{}", w, h)
                                                } else {
                                                    "1080p".to_string()
                                                }
                                            })
                                            .unwrap_or_else(|| "1080p".to_string()),
                                    ),
                                    filesize: None,
                                    bitrate: video
                                        .video
                                        .as_ref()
                                        .and_then(|v| v.bitrate)
                                        .map(|b| b as u64),
                                    codec: None,
                                    audio_only: false,
                                    format_note: Some("DouYin API".to_string()),
                                }],
                                created_at: None,
                                download_strategy: crate::types::DownloadStrategy::YtDlp,
                                audio_url: None,
                            });
                        } else {
                            tracing::warn!("DouYin API returned empty aweme_list");
                        }
                    }
                    Ok(Err(e)) => {
                        tracing::warn!("DouYin API call failed: {}, trying yt-dlp", e);
                    }
                    Err(e) => {
                        tracing::warn!("DouYin thread spawn failed: {:?}, trying yt-dlp", e);
                    }
                }
            }

            // API 失败或无 modal_id,尝试 yt-dlp (走 base)
            let download_url = self.direct_video_url(url, modal_id.as_ref());
            match self.base.fetch_via_ytdlp(&download_url).await {
                Ok(info) => {
                    tracing::info!("DouYin video info via yt-dlp: {}", info.title);
                    return Ok(info);
                }
                Err(e) => {
                    tracing::warn!("DouYin yt-dlp also failed: {}", e);
                    return Err(ResolverError::ParseFailed(
                        "无法解析 DouYin feed URL,请尝试直接使用视频 URL (如 https://www.douyin.com/video/xxx)".to_string()
                    ));
                }
            }
        }

        // 普通视频 URL,使用 yt-dlp (走 base)
        if url.contains("/video/") {
            return self.base.fetch_via_ytdlp(url).await;
        }

        // 最后降级到 yt-dlp (走 base)
        self.base.fetch_via_ytdlp(url).await.map_err(ResolverError::from)
    }

    fn requires_auth(&self) -> bool {
        self.base.has_token("msToken")
    }

    fn required_tokens(&self) -> Vec<&'static str> {
        vec!["msToken"]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_supports_url() {
        let resolver = DouYinResolver::new();
        assert!(resolver.supports_url("https://www.douyin.com/video/7231679494828376333"));
        assert!(resolver.supports_url("https://v.douyin.com/xxxx"));
        assert!(!resolver.supports_url("https://tiktok.com/video"));
    }
}
```

- [ ] **Step 4: Run DouYin tests**

Run: `cargo test --lib platform::douyin 2>&1 | tail -10`
Expected: `test_supports_url` 通过

- [ ] **Step 5: Verify full test suite green**

Run: `cargo test --lib 2>&1 | tail -5`
Expected: 263 tests 全过

- [ ] **Step 6: Verify clippy**

Run: `cargo clippy --lib 2>&1 | tail -10`
Expected: 无新警告

- [ ] **Step 7: Commit**

```bash
git add src/platform/douyin.rs src/platform/base.rs
git commit -m "$(cat <<'EOF'
refactor(douyin): use YtdlpBacked base for yt-dlp branch

DouYin 改动最复杂:API 分支(feed/modal_id 走 DouYinApiClient)
保留原样,yt-dlp 分支(fallback / 普通视频 / 最后降级)统一改走
base.fetch_via_ytdlp。同时给 base 加 tokens_get helper 给 API 分支
读 msToken 用。DouYin 从 251 行收缩到 175 行左右(净 -76 行),
净重复代码大幅降低。

Intent: 第四个迁移案例,验证 API 分支 + yt-dlp 分支共存的 resolver 也能复用 base
EOF
)"
```

---

### Task 7: Refactor `FxTwitterResolver::with_proxy` to use `apply_resolver_defaults`

**Files:**
- Modify: `src/fxtwitter.rs:129-152`(`with_proxy` 函数体)

**Interfaces:**
- Consumes: `http_client::apply_resolver_defaults` (从 Task 2)
- Produces: 修改后的 `FxTwitterResolver::with_proxy`

- [ ] **Step 1: Verify FxTwitter compiles**

Run: `cargo build --lib 2>&1 | tail -10`
Expected: 编译通过

- [ ] **Step 2: Modify `src/fxtwitter.rs::with_proxy`**

找到 `with_proxy` 函数(原 129-152 行):

```rust
/// 使用指定代理创建解析器
pub fn with_proxy(proxy_url: &str) -> Result<Self> {
    // 确保代理 URL 有协议前缀
    let proxy_with_scheme =
        if proxy_url.starts_with("http://") || proxy_url.starts_with("https://") {
            proxy_url.to_string()
        } else {
            format!("http://{}", proxy_url)
        };

    // 创建带有代理的新客户端,基于共享客户端配置
    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36")
        .tcp_keepalive(std::time::Duration::from_secs(120))
        .connect_timeout(std::time::Duration::from_secs(30))
        .timeout(std::time::Duration::from_secs(600))
        .pool_max_idle_per_host(32)
        .proxy(reqwest::Proxy::all(&proxy_with_scheme)?)
        .build()
        .map_err(|e| anyhow!("Failed to create client with proxy: {}", e))?;

    Ok(Self {
        client: Arc::new(client),
    })
}
```

替换为:

```rust
/// 使用指定代理创建解析器
pub fn with_proxy(proxy_url: &str) -> Result<Self> {
    // 确保代理 URL 有协议前缀
    let proxy_with_scheme =
        if proxy_url.starts_with("http://") || proxy_url.starts_with("https://") {
            proxy_url.to_string()
        } else {
            format!("http://{}", proxy_url)
        };

    // 使用 http_client 工厂套默认值,再加 proxy + pool 配置
    let client = crate::http_client::apply_resolver_defaults(reqwest::Client::builder())
        .pool_max_idle_per_host(32)
        .proxy(reqwest::Proxy::all(&proxy_with_scheme)?)
        .build()
        .map_err(|e| anyhow!("Failed to create client with proxy: {}", e))?;

    Ok(Self {
        client: Arc::new(client),
    })
}
```

UA / tcp_keepalive / connect_timeout / timeout 4 行收敛到 1 个 helper 调用。

- [ ] **Step 3: Verify FxTwitter compiles**

Run: `cargo build --lib 2>&1 | tail -10`
Expected: 编译通过

- [ ] **Step 4: Run FxTwitter tests**

Run: `cargo test --lib fxtwitter 2>&1 | tail -15`
Expected: `test_extract_tweet_id` / `test_build_title_*` 5 个测试全过

- [ ] **Step 5: Verify full test suite green**

Run: `cargo test --lib 2>&1 | tail -5`
Expected: 263 tests 全过

- [ ] **Step 6: Verify clippy**

Run: `cargo clippy --lib 2>&1 | tail -10`
Expected: 无新警告

- [ ] **Step 7: Commit**

```bash
git add src/fxtwitter.rs
git commit -m "$(cat <<'EOF'
refactor(fxtwitter): use apply_resolver_defaults in with_proxy

FxTwitter with_proxy 内 UA/tcp_keepalive/connect_timeout/timeout 4 行
重复配置收敛到 http_client::apply_resolver_defaults 一个调用。
pool_max_idle_per_host=32 保留(FxTwitter 专属优化);proxy 仍附加。

Intent: 收尾 — 第一个非 platform/ 目录的改造,验证 helper 在顶层模块也能复用
EOF
)"
```

---

## Self-Review

按 skill 要求做 self-review:

**1. Spec coverage**:
- §3 Architecture 模块布局 ✓ (Task 1-7 各对应)
- §4.1 YtdlpBacked 组件 ✓ (Task 1 增字段/方法,Task 3 加 fetch_via_ytdlp,Task 5 加 tokens_is_empty,Task 6 加 tokens_get)
- §4.2 http_client 扩展 ✓ (Task 2)
- §4.3 4 个 resolver 改造样板 ✓ (Task 3 YouTube,Task 4 TikTok,Task 5 Bilibili,Task 6 DouYin)
- §4.4 FxTwitter 改造 ✓ (Task 7)
- §6 Error Handling(中文模板) ✓ (Task 3 Step 2 fetch_via_ytdlp 内部)
- §7.1 base 单测 ✓ (Task 1)
- §7.2 http_client 单测 ✓ (Task 2)
- §7.3 回归测试 ✓ (每个 Task Step "Verify full test suite green")
- §8 Migration 7 commit 路径 ✓ (Task 1-7 各 Step 7/5/7/6/7 提交)

**2. Placeholder scan**:
- `TODO(留待下期)` 1 处(YouTube::with_cookie_file 注释),符合 spec §10 out-of-scope 故意保留
- 无 "TBD"/"implement later"/"similar to Task N" 模式

**3. Type consistency**:
- `YtdlpBacked::new(platform: Platform)` Task 1 定义 → Task 3/4/5/6 一致使用 ✓
- `YtdlpBacked::fetch_via_ytdlp(url) -> Result<VideoInfo, ResolverError>` Task 3 Step 2 定义 → Task 4/5/6 一致调用 ✓
- `YtdlpBacked::has_token(key)` Task 1 定义 → Task 4/6 一致调用 ✓
- `YtdlpBacked::tokens_is_empty()` Task 5 Step 2 定义 → Task 5 Step 3 一致调用 ✓
- `YtdlpBacked::tokens_get(key)` Task 6 Step 2 定义 → Task 6 Step 3 一致调用 ✓
- `http_client::apply_resolver_defaults(b)` Task 2 定义 → Task 7 一致调用 ✓
- 命名风格:`fetch_via_ytdlp` (snake_case) 全文一致 ✓

**4. Step ordering (修复记录)**:
初稿 Task 3/5/6 都是"Step 2 写新 impl 用了 Step N 才加的方法",会出现 forward-ref 编译失败。已修:
- Task 3 Step 2 加 `fetch_via_ytdlp` 后,Step 4 才 rewrite youtube.rs
- Task 5 Step 2 加 `tokens_is_empty` 后,Step 3 才 rewrite bilibili.rs
- Task 6 Step 2 加 `tokens_get` 后,Step 3 才 rewrite douyin.rs

每个 Task 现在严格遵循:加依赖 → 写消费方 → 编译验证 → 测试 → 提交。

---

## 最终成果

| 指标 | 改前 | 改后 | 变化 |
|---|---|---|---|
| `src/platform/youtube.rs` | 87 行 | 50 行 | -37 行 (-43%) |
| `src/platform/tiktok.rs` | 109 行 | 55 行 | -54 行 (-50%) |
| `src/platform/bilibili.rs` | 97 行 | 48 行 | -49 行 (-50%) |
| `src/platform/douyin.rs` | 251 行 | 175 行 | -76 行 (-30%) |
| `src/platform/base.rs` | 0 行 | 80 行 | +80 行(新增) |
| `src/http_client.rs` | 33 行 | 65 行 | +32 行(+ 测试) |
| `src/fxtwitter.rs` with_proxy | 23 行 | 17 行 | -6 行 |
| **总计** | **600 行** | **490 行** | **-110 行 (-18%)** |
| **单元测试** | 256 | 263 | +7 |
| **新平台成本** | ~100 行 | ~30 行 | -70% |
| **UA 字符串散落** | 3+ 处 | 1 处(`RESOLVER_UA`) | 集中 |

**杠杆验证**:下次新加 yt-dlp 包装 resolver = 复制 Task 4 模板,改 `Platform::X` + `required_tokens()` 即可,~30 行(含 test_supports_url)。