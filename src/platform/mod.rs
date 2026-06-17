//! 平台解析器模块
//!
//! 提供统一接口 `PlatformResolver`，各平台实现自己的解析逻辑
mod token_manager;
mod tiktok;
mod douyin;
mod bilibili;
mod xbogus;
mod douyin_api;
mod cookie_utils;
mod cookie_pool;
mod zhixueyun;
mod youtube;
pub mod url_matcher;
pub use token_manager::TokenManager;
pub use tiktok::TikTokResolver;
pub use douyin::DouYinResolver;
pub use bilibili::BilibiliResolver;
pub use douyin_api::{DouYinApiClient, extract_modal_id};
pub use cookie_utils::CookieFileManager;
pub use cookie_pool::CookiePool;
pub use zhixueyun::ZhiXueYunResolver;
pub use youtube::YouTubeResolver;
pub mod zhixueyun_token;
use crate::types::VideoInfo;
use crate::history::Platform;
use async_trait::async_trait;
use serde::{Serialize, Deserialize};
/// 解析器错误类型
#[derive(Debug, Clone)]
pub enum ResolverError {
    /// 需要认证
    AuthenticationRequired,
    /// 内容不可用
    ContentUnavailable(String),
    /// 网络错误
    Network(String),
    /// 解析失败
    ParseFailed(String),
    /// 详细错误 (含平台/URL/操作提示)
    Detailed {
        /// 失败的平台
        platform: Platform,
        /// 失败的 URL
        url: String,
        /// 错误消息
        message: String,
        /// 建议的操作
        action: String,
    },
}

impl ResolverError {
    /// 净化 URL 用于用户显示 - 移除查询参数中的敏感信息
    fn sanitize_url_for_display(url: &str) -> String {
        // 解析 URL 并移除敏感查询参数
        if let Ok(mut parsed) = url::Url::parse(url) {
            // 移除可能包含 token/session 的参数
            let sensitive_params = ["token", "session", "auth", "msToken", "csrf", "sig"];
            let query_pairs: Vec<(String, String)> = parsed.query_pairs().to_owned()
                .into_iter()
                .filter(|(key, _)| !sensitive_params.iter().any(|s| key.to_lowercase().contains(s)))
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect();

            // 如果所有参数都被移除，返回不含查询的 URL
            if query_pairs.is_empty() {
                parsed.set_query(None);
            } else {
                let query_string = query_pairs.into_iter().map(|(k, v)| format!("{}={}", k, v)).collect::<Vec<_>>().join("&");
                parsed.set_query(Some(&query_string));
            }

            parsed.to_string()
        } else {
            // 如果解析失败，至少移除明显的 token 模式
            let sanitized = regex::Regex::new(r"(\?|&)(token|session|auth|msToken|csrf|sig)=[^&]+")
                .map(|re| re.replace_all(url, "").to_string())
                .unwrap_or_else(|_| url.to_string());
            // 清理多余的 ? 或 &
            sanitized.trim_end_matches(['?', '&']).to_string()
        }
    }

    /// 将当前错误转换为详细错误 (添加上下文)
    pub fn with_context(self, url: String, platform: Platform, action: String) -> Self {
        let (message, existing_platform) = match self {
            ResolverError::AuthenticationRequired => ("需要登录认证".to_string(), platform),
            ResolverError::ContentUnavailable(msg) => (msg, platform),
            ResolverError::Network(msg) => (msg, platform),
            ResolverError::ParseFailed(msg) => (msg, platform),
            ResolverError::Detailed { message, platform: p, .. } => (message, p),
        };
        ResolverError::Detailed {
            platform: existing_platform,
            url,
            message,
            action,
        }
    }

    /// 获取建议的操作提示
    pub fn suggested_action(&self) -> Option<String> {
        match self {
            ResolverError::AuthenticationRequired => {
                Some("请更新 Cookie 后重试 (抖音/哔哩哔哩需要 Fresh cookies)".to_string())
            }
            ResolverError::ContentUnavailable(msg) => {
                if msg.contains(" login") || msg.contains("登录") || msg.contains("权限") {
                    Some("请检查账号是否登录，或尝试更新 Cookie".to_string())
                } else {
                    Some("视频可能已下架或仅限特定地区观看".to_string())
                }
            }
            ResolverError::Network(msg) => {
                if msg.contains("rate") || msg.contains("限流") || msg.contains("429") {
                    Some("请求过于频繁，请稍后再试或使用代理".to_string())
                } else if msg.contains("timeout") || msg.contains("超时") {
                    Some("网络超时，请检查网络连接或使用代理".to_string())
                } else {
                    Some("请检查网络连接，或尝试使用 --proxy 参数".to_string())
                }
            }
            ResolverError::ParseFailed(_) => {
                Some("视频格式可能不支持，尝试使用 yt-dlp fallback".to_string())
            }
            ResolverError::Detailed { action, .. } => {
                if action.is_empty() { None } else { Some(action.clone()) }
            }
        }
    }

    /// 获取错误消息的简短描述
    pub fn brief(&self) -> String {
        match self {
            ResolverError::AuthenticationRequired => "需要登录认证".to_string(),
            ResolverError::ContentUnavailable(msg) => format!("内容不可用: {}", msg),
            ResolverError::Network(msg) => format!("网络错误: {}", msg),
            ResolverError::ParseFailed(msg) => format!("解析失败: {}", msg),
            ResolverError::Detailed { platform, message, .. } => {
                format!("[{}] {}", platform.display_name(), message)
            }
        }
    }
}

impl std::fmt::Display for ResolverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolverError::AuthenticationRequired => write!(f, "需要登录认证"),
            ResolverError::ContentUnavailable(msg) => write!(f, "内容不可用: {}", msg),
            ResolverError::Network(msg) => write!(f, "网络错误: {}", msg),
            ResolverError::ParseFailed(msg) => write!(f, "解析失败: {}", msg),
            ResolverError::Detailed { platform, url, message, action } => {
                let sanitized_url = Self::sanitize_url_for_display(url);
                write!(f, "[{}] {}\n提示: {}", platform.display_name(), message, action)?;
                if !sanitized_url.is_empty() && sanitized_url != *url {
                    write!(f, "\nURL: {}", sanitized_url)?;
                }
                Ok(())
            }
        }
    }
}
impl std::error::Error for ResolverError {}
impl From<crate::types::DownloadError> for ResolverError {
    fn from(e: crate::types::DownloadError) -> Self {
        match e.error_type {
            crate::types::DownloadErrorType::Authentication => ResolverError::AuthenticationRequired,
            crate::types::DownloadErrorType::ContentUnavailable => ResolverError::ContentUnavailable(e.message),
            crate::types::DownloadErrorType::RateLimited => ResolverError::Network(e.message),
            _ => ResolverError::Network(e.to_string()),
        }
    }
}
impl From<anyhow::Error> for ResolverError {
    fn from(e: anyhow::Error) -> Self {
        ResolverError::Network(e.to_string())
    }
}
/// 平台解析器 trait
/// 所有平台解析器必须实现此接口
#[async_trait]
#[allow(dead_code)]
pub trait PlatformResolver: Send + Sync {
    /// 获取解析器对应的平台
    fn platform(&self) -> Platform;
    /// 获取支持的 URL 模式列表
    fn supported_url_patterns(&self) -> Vec<&'static str>;
    /// 检查是否支持此 URL (精确域名匹配)
    fn supports_url(&self, url: &str) -> bool {
        url_matcher::url_matches_platform(url, self.platform())
    }
    /// 检查是否支持此 URL 的 hostname（小写化版本，避免重复 to_lowercase）
    /// 默认实现复用 supports_url
    fn supports_url_by_host(&self, host_lower: &str) -> bool {
        url_matcher::url_matches_platform_by_host(host_lower, self.platform())
    }
    /// 获取视频信息
    async fn get_video_info(&self, url: &str) -> Result<VideoInfo, ResolverError>;
    /// 获取字幕列表（可选实现）
    async fn get_subtitles(&self, _video_id: &str) -> Result<Vec<SubtitleInfo>, ResolverError> {
        Ok(vec![]) // 默认返回空
    }
    /// 获取弹幕列表（可选实现）
    async fn get_danmaku(&self, _video_id: &str) -> Result<Vec<DanmakuInfo>, ResolverError> {
        Ok(vec![]) // 默认返回空
    }
    /// 检查是否需要认证
    fn requires_auth(&self) -> bool {
        false
    }
    /// 获取所需 Token 名称列表
    fn required_tokens(&self) -> Vec<&'static str> {
        vec![]
    }
    /// 获取下载策略
    fn download_strategy(&self) -> crate::types::DownloadStrategy {
        crate::types::DownloadStrategy::YtDlp
    }
}

/// 字幕信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubtitleInfo {
    /// 语言代码
    pub lang: String,
    /// 语言名称
    pub lang_name: String,
    /// 字幕下载 URL
    pub url: String,
    /// 字幕格式 (srt, vtt, ass)
    pub format: String,
}

/// 弹幕信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DanmakuInfo {
    /// 弹幕文本
    pub text: String,
    /// 出现时间（秒）
    pub time: f64,
    /// 弹幕颜色 (RGB)
    pub color: u32,
    /// 发送者 ID
    pub user_id: Option<u64>,
}
/// YtDlp 解析器（包装现有实现）
pub struct YtDlpResolver {
    cookie_file: Option<std::path::PathBuf>,
}
impl YtDlpResolver {
    pub fn new() -> Self {
        Self { cookie_file: None }
    }
    #[allow(dead_code)]
    pub fn with_cookie_file(mut self, path: std::path::PathBuf) -> Self {
        self.cookie_file = Some(path);
        self
    }
}
impl Default for YtDlpResolver {
    fn default() -> Self {
        Self::new()
    }
}
#[async_trait]
impl PlatformResolver for YtDlpResolver {
    fn platform(&self) -> Platform {
        Platform::Other
    }
    fn supported_url_patterns(&self) -> Vec<&'static str> {
        vec![] // yt-dlp 支持所有平台
    }
    async fn get_video_info(&self, url: &str) -> Result<VideoInfo, ResolverError> {
        let mut ytdlp = crate::yt_dlp::YtDlp::new();
        if let Some(ref cookie) = self.cookie_file {
            if cookie.exists() {
                ytdlp = ytdlp.with_cookie_file(cookie.clone());
            }
        }
        ytdlp.get_video_info(url).map_err(ResolverError::from)
    }
    fn requires_auth(&self) -> bool {
        self.cookie_file.is_some()
    }
}
/// 解析器注册表
pub struct ResolverRegistry {
    resolvers: Vec<Box<dyn PlatformResolver>>,
    default_resolver: Box<dyn PlatformResolver>,
}
impl ResolverRegistry {
    /// 创建新的注册表
    pub fn new() -> Self {
        Self {
            resolvers: Vec::new(),
            default_resolver: Box::new(YtDlpResolver::new()),
        }
    }
    /// 注册解析器
    pub fn register<R: PlatformResolver + 'static>(&mut self, resolver: R) {
        self.resolvers.push(Box::new(resolver));
    }
    /// 根据 URL 选择合适的解析器（缓存 hostname 避免重复 to_lowercase）
    pub fn select_resolver(&self, url: &str) -> &dyn PlatformResolver {
        // 一次性提取并小写化 hostname，避免每个 resolver 重复调用 to_lowercase
        let host_lower = match url_matcher::extract_hostname_and_lower(url) {
            Some(h) => h,
            None => return &*self.default_resolver,
        };

        for resolver in &self.resolvers {
            if resolver.supports_url_by_host(&host_lower) {
                return resolver.as_ref();
            }
        }
        &*self.default_resolver
    }
    /// 尝试使用主解析器，失败时使用回退列表
    #[allow(dead_code)]
    pub async fn try_resolve(
        &self,
        url: &str,
        fallback_order: &[Platform],
    ) -> Result<VideoInfo, ResolverError> {
        // 首先尝试 URL 匹配的解析器
        let primary = self.select_resolver(url);
        match primary.get_video_info(url).await {
            Ok(info) => Ok(info),
            Err(e) => {
                tracing::warn!("Primary resolver failed: {}, trying fallbacks", e);
                // 如果失败，尝试 fallback 列表中的解析器
                for platform in fallback_order {
                    for resolver in &self.resolvers {
                        if resolver.platform() == *platform {
                            match resolver.get_video_info(url).await {
                                Ok(info) => return Ok(info),
                                Err(_) => continue,
                            }
                        }
                    }
                }
                Err(e)
            }
        }
    }

    /// 尝试所有已注册的解析器，返回第一个成功的结果
    #[allow(dead_code)]
    pub async fn try_resolve_all(&self, url: &str) -> Result<VideoInfo, ResolverError> {
        // 首先尝试 URL 匹配的解析器
        let primary = self.select_resolver(url);
        if let Ok(info) = primary.get_video_info(url).await {
            return Ok(info);
        }
        tracing::warn!("Primary resolver failed, trying all registered resolvers");

        // 尝试所有已注册的解析器
        for resolver in &self.resolvers {
            if resolver.platform() == primary.platform() {
                continue; // 跳过已尝试的主解析器
            }
            match resolver.get_video_info(url).await {
                Ok(info) => return Ok(info),
                Err(_) => continue,
            }
        }

        Err(ResolverError::ParseFailed(format!("所有解析器均失败 for URL: {}", url)))
    }
}
impl Default for ResolverRegistry {
    fn default() -> Self {
        Self::new()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_resolver_error_display() {
        let err = ResolverError::ParseFailed("test".to_string());
        assert_eq!(format!("{}", err), "解析失败: test");
    }
    #[test]
    fn test_tiktok_supports_url() {
        let resolver = TikTokResolver::new();
        assert!(resolver.supports_url("https://www.tiktok.com/@user/video/123"));
        assert!(resolver.supports_url("https://vm.tiktok.com/abc"));
        assert!(!resolver.supports_url("https://youtube.com/watch"));
    }
    #[test]
    fn test_bilibili_supports_url() {
        let resolver = BilibiliResolver::new();
        assert!(resolver.supports_url("https://www.bilibili.com/video/BV1xx411c7XD"));
        assert!(resolver.supports_url("https://b23.tv/abc"));
        assert!(!resolver.supports_url("https://tiktok.com/video"));
    }
    #[test]
    fn test_registry_select() {
        let mut registry = ResolverRegistry::new();
        registry.register(TikTokResolver::new());
        registry.register(BilibiliResolver::new());
        let tiktok_resolver = registry.select_resolver("https://www.tiktok.com/video/123");
        assert_eq!(tiktok_resolver.platform(), Platform::TikTok);
        let bili_resolver = registry.select_resolver("https://www.bilibili.com/video/BV123");
        assert_eq!(bili_resolver.platform(), Platform::Bilibili);
        let default_resolver = registry.select_resolver("https://youtube.com/watch");
        assert_eq!(default_resolver.platform(), Platform::Other); // YtDlp as default
    }

    #[test]
    fn test_registry_default_resolver() {
        let registry = ResolverRegistry::new();
        // Empty registry should use default YtDlpResolver
        let resolver = registry.select_resolver("https://any.url.com/test");
        assert_eq!(resolver.platform(), Platform::Other);
    }

    #[test]
    fn test_registry_multiple_resolvers_same_platform() {
        // Register multiple resolvers - first matching one wins
        let mut registry = ResolverRegistry::new();
        registry.register(TikTokResolver::new());
        registry.register(TikTokResolver::new()); // duplicate
        let resolver = registry.select_resolver("https://www.tiktok.com/video/123");
        assert_eq!(resolver.platform(), Platform::TikTok);
    }

    #[test]
    fn test_platform_resolver_default_requires_auth() {
        struct TestResolver;
        #[async_trait]
        impl PlatformResolver for TestResolver {
            fn platform(&self) -> Platform { Platform::Other }
            fn supported_url_patterns(&self) -> Vec<&'static str> { vec![] }
            async fn get_video_info(&self, _url: &str) -> Result<VideoInfo, ResolverError> {
                unreachable!()
            }
        }
        let resolver = TestResolver;
        // Default implementation returns false
        assert!(!resolver.requires_auth());
    }

    #[test]
    fn test_platform_resolver_default_required_tokens() {
        struct TestResolver;
        #[async_trait]
        impl PlatformResolver for TestResolver {
            fn platform(&self) -> Platform { Platform::Other }
            fn supported_url_patterns(&self) -> Vec<&'static str> { vec![] }
            async fn get_video_info(&self, _url: &str) -> Result<VideoInfo, ResolverError> {
                unreachable!()
            }
        }
        let resolver = TestResolver;
        // Default implementation returns empty vec
        assert!(resolver.required_tokens().is_empty());
    }

    #[test]
    fn test_ytdlp_resolver_basic() {
        let resolver = YtDlpResolver::new();
        assert_eq!(resolver.platform(), Platform::Other);
        assert!(!resolver.requires_auth()); // No cookie file by default
    }

    #[test]
    fn test_ytdlp_resolver_with_cookie() {
        let resolver = YtDlpResolver::new().with_cookie_file(std::path::PathBuf::from("/tmp/cookies.txt"));
        assert!(resolver.requires_auth());
    }

    #[test]
    fn test_resolver_error_from_download_error() {
        use crate::types::{DownloadError, DownloadErrorType};

        // Authentication error
        let download_err = DownloadError {
            error_type: DownloadErrorType::Authentication,
            message: "auth required".to_string(),
            action: None,
        };
        let resolver_err: ResolverError = download_err.into();
        assert!(matches!(resolver_err, ResolverError::AuthenticationRequired));

        // Content unavailable error
        let download_err = DownloadError {
            error_type: DownloadErrorType::ContentUnavailable,
            message: "video not found".to_string(),
            action: None,
        };
        let resolver_err: ResolverError = download_err.into();
        assert!(matches!(resolver_err, ResolverError::ContentUnavailable(msg) if msg == "video not found"));

        // Rate limited error
        let download_err = DownloadError {
            error_type: DownloadErrorType::RateLimited,
            message: "rate limited".to_string(),
            action: None,
        };
        let resolver_err: ResolverError = download_err.into();
        assert!(matches!(resolver_err, ResolverError::Network(_)));

        // Network error
        let download_err = DownloadError {
            error_type: DownloadErrorType::Network,
            message: "network error".to_string(),
            action: None,
        };
        let resolver_err: ResolverError = download_err.into();
        assert!(matches!(resolver_err, ResolverError::Network(_)));
    }

    #[test]
    fn test_resolver_error_from_anyhow() {
        let anyhow_err = anyhow::anyhow!("test error");
        let resolver_err: ResolverError = anyhow_err.into();
        assert!(matches!(resolver_err, ResolverError::Network(msg) if msg.contains("test error")));
    }

    #[test]
    fn test_resolver_error_display_all_variants() {
        assert_eq!(format!("{}", ResolverError::AuthenticationRequired), "需要登录认证");
        assert_eq!(format!("{}", ResolverError::ContentUnavailable("test".to_string())), "内容不可用: test");
        assert_eq!(format!("{}", ResolverError::Network("net error".to_string())), "网络错误: net error");
        assert_eq!(format!("{}", ResolverError::ParseFailed("parse error".to_string())), "解析失败: parse error");
    }

    #[test]
    fn test_registry_register_preserves_order() {
        let mut registry = ResolverRegistry::new();
        registry.register(DouYinResolver::new());
        registry.register(BilibiliResolver::new());

        // Bilibili should be selected for bilibili URL (last registered)
        let resolver = registry.select_resolver("https://www.bilibili.com/video/BV123");
        assert_eq!(resolver.platform(), Platform::Bilibili);
    }
}