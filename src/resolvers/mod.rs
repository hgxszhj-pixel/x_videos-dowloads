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
mod mylearning;

pub use token_manager::TokenManager;
pub use tiktok::TikTokResolver;
pub use douyin::DouYinResolver;
pub use bilibili::BilibiliResolver;
pub use xbogus::build_cookie_string;
pub use douyin_api::{DouYinApiClient, extract_modal_id};
pub use cookie_utils::CookieFileManager;
pub use cookie_pool::CookiePool;
pub use mylearning::MyLearningResolver;

use crate::types::VideoInfo;
use crate::history::Platform;

/// 解析器错误类型
#[derive(Debug, Clone)]
pub enum ResolverError {
    /// URL 不支持
    UnsupportedUrl(String),
    /// 需要认证
    AuthenticationRequired,
    /// 内容不可用
    ContentUnavailable(String),
    /// 网络错误
    Network(String),
    /// 解析失败
    ParseFailed(String),
    /// Token 缺失
    MissingToken(String),
}

impl std::fmt::Display for ResolverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolverError::UnsupportedUrl(url) => write!(f, "URL 不支持: {}", url),
            ResolverError::AuthenticationRequired => write!(f, "需要登录认证"),
            ResolverError::ContentUnavailable(msg) => write!(f, "内容不可用: {}", msg),
            ResolverError::Network(msg) => write!(f, "网络错误: {}", msg),
            ResolverError::ParseFailed(msg) => write!(f, "解析失败: {}", msg),
            ResolverError::MissingToken(token) => write!(f, "缺少 Token: {}", token),
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
pub trait PlatformResolver: Send + Sync {
    /// 获取解析器对应的平台
    fn platform(&self) -> Platform;

    /// 获取支持的 URL 模式列表
    fn supported_url_patterns(&self) -> Vec<&'static str>;

    /// 检查是否支持此 URL
    fn supports_url(&self, url: &str) -> bool {
        let url_lower = url.to_lowercase();
        self.supported_url_patterns()
            .iter()
            .any(|pattern| url_lower.contains(&pattern.to_lowercase()))
    }

    /// 获取视频信息
    fn get_video_info(&self, url: &str) -> anyhow::Result<VideoInfo, ResolverError>;

    /// 检查是否需要认证
    fn requires_auth(&self) -> bool {
        false
    }

    /// 获取所需 Token 名称列表
    fn required_tokens(&self) -> Vec<&'static str> {
        vec![]
    }
}

/// YtDlp 解析器（包装现有实现）
pub struct YtDlpResolver {
    cookie_file: Option<std::path::PathBuf>,
}

impl YtDlpResolver {
    pub fn new() -> Self {
        Self { cookie_file: None }
    }

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

impl PlatformResolver for YtDlpResolver {
    fn platform(&self) -> Platform {
        Platform::Other
    }

    fn supported_url_patterns(&self) -> Vec<&'static str> {
        vec![] // yt-dlp 支持所有平台
    }

    fn get_video_info(&self, url: &str) -> anyhow::Result<VideoInfo, ResolverError> {
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

    /// 根据 URL 选择合适的解析器
    pub fn select_resolver(&self, url: &str) -> &dyn PlatformResolver {
        for resolver in &self.resolvers {
            if resolver.supports_url(url) {
                return resolver.as_ref();
            }
        }
        &*self.default_resolver
    }

    /// 尝试使用主解析器，失败时使用回退列表
    pub fn try_resolve(
        &self,
        url: &str,
        fallback_order: &[Platform],
    ) -> Result<VideoInfo, ResolverError> {
        // 首先尝试 URL 匹配的解析器
        let primary = self.select_resolver(url);
        match primary.get_video_info(url) {
            Ok(info) => Ok(info),
            Err(e) => {
                tracing::warn!("Primary resolver failed: {}, trying fallbacks", e);
                // 如果失败，尝试 fallback 列表中的解析器
                for platform in fallback_order {
                    for resolver in &self.resolvers {
                        if resolver.platform() == *platform {
                            match resolver.get_video_info(url) {
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
        let err = ResolverError::UnsupportedUrl("test".to_string());
        assert_eq!(format!("{}", err), "URL 不支持: test");
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
}