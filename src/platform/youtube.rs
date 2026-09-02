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
