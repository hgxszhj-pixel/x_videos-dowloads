//! URL 解析链路
//!
//! 统一调度 ResolverRegistry / FxTwitter / yt-dlp / Scrapling 完成视频信息解析。

use std::sync::{Arc, Mutex};

use anyhow::Result;
use tracing::info;

use crate::cli::Cli;
use crate::config::AppConfig;
use crate::fxtwitter::FxTwitterResolver;
use crate::platform::{
    BilibiliResolver, CookiePool, DouYinResolver, NnyyResolver, ResolverError, ResolverRegistry,
    TikTokResolver, TulingPythonResolver, YouTubeResolver, ZhiXueYunResolver,
};
use crate::types::VideoInfo;
use crate::yt_dlp::YtDlp;

/// 全局 ResolverRegistry 单例（Mutex<Option> 模式，支持测试 reset）。
///
/// 第一次调用时根据 `AppConfig` 一次性构建 7 个 resolver 并冻结为 Arc。
/// 后续调用直接返回 Arc clone，避免每次 `get_video_info` 都重新实例化
/// （包括 ZhiXueYunResolver 的 Arc<CookiePool> 分配、TulingPython 初始化等）。
///
/// 用 Mutex 而非 OnceLock 是因为测试需要 reset 单例验证不同 config 行为；
/// 生产路径只在首次调用时 lock（contention 极低）。
///
/// **已知限制**：config 一旦改变（如 msToken 热更新），registry 不会重建。
/// 当前 config 在 main.rs 启动时 load-once，不热重载，故无影响。
/// 将来若加 config hot-reload，可改用 `RwLock<Option<Arc<ResolverRegistry>>>` + reload 时 `take()`。
static GLOBAL_REGISTRY: Mutex<Option<Arc<ResolverRegistry>>> = Mutex::new(None);

/// 获取全局 ResolverRegistry（首次调用时构建）
///
/// 参数 `config` 仅在首次调用时生效（用于注入 DouYinResolver 的 msToken）。
/// 后续调用忽略 config 参数，直接返回已缓存的 Arc clone。
pub fn global_registry(config: &AppConfig) -> Arc<ResolverRegistry> {
    let mut guard = GLOBAL_REGISTRY.lock().unwrap_or_else(|err| {
        tracing::warn!("GLOBAL_REGISTRY poisoned，自动重建: {}", err);
        err.into_inner()
    });
    if let Some(ref arc) = *guard {
        return arc.clone();
    }

    let mut r = ResolverRegistry::new();

    // 创建 DouYinResolver 并注入 msToken
    let mut douyin_resolver = DouYinResolver::new();
    if let Some(ref ms_token) = config.douyin_ms_token {
        let mut tokens = std::collections::HashMap::new();
        tokens.insert("msToken".to_string(), ms_token.clone());
        douyin_resolver = douyin_resolver.with_tokens(tokens);
    }
    r.register(douyin_resolver);

    r.register(TikTokResolver::new());
    r.register(BilibiliResolver::new());
    r.register(YouTubeResolver::new());
    r.register(NnyyResolver::new());
    r.register(ZhiXueYunResolver::new(Arc::new(CookiePool::new(24, 100))));
    if let Ok(tp) = TulingPythonResolver::new() {
        r.register(tp);
    } else {
        tracing::warn!("TulingPythonResolver 初始化失败，跳过注册");
    }

    let arc = Arc::new(r);
    *guard = Some(arc.clone());
    arc
}

/// 测试用：重置全局单例（仅在 `#[cfg(test)]` 下暴露）。
#[cfg(test)]
fn reset_global_registry_for_tests() {
    let mut guard = GLOBAL_REGISTRY.lock().unwrap_or_else(|err| {
        tracing::warn!("GLOBAL_REGISTRY poisoned，自动重建: {}", err);
        err.into_inner()
    });
    *guard = None;
}

/// 检测是否为 x.com (Twitter) URL
pub fn is_x_com_url(url: &str) -> bool {
    url.contains("x.com/") || url.contains("twitter.com/")
}

/// 获取视频信息（使用 ResolverRegistry 统一调度）
pub async fn get_video_info(
    ytdlp: &YtDlp,
    url: &str,
    cli: &Cli,
    config: &AppConfig,
) -> Result<VideoInfo> {
    // x.com 专用解析器链: FxTwitter > yt-dlp > Scrapling
    if is_x_com_url(url) {
        info!("检测到 x.com URL，使用专用解析链");

        // 1. 尝试 FxTwitter (最佳选择)
        let fxtwitter = if let Some(ref proxy) = cli.proxy {
            FxTwitterResolver::with_proxy(proxy)?
        } else {
            FxTwitterResolver::new()
        };

        if let Ok(info) = fxtwitter.get_video_info(url).await {
            info!("FxTwitter 解析成功!");
            return Ok(info);
        }
        info!("FxTwitter 解析失败，尝试 yt-dlp...");

        // 2. yt-dlp fallback
        return fetch_with_ytdlp(ytdlp, url, cli, config).await;
    }

    // 其他平台: ResolverRegistry > yt-dlp > FxTwitter > Scrapling
    // 使用全局 singleton 避免每次调用重建 7 个 resolver 实例
    let registry = global_registry(config);

    let resolver = registry.select_resolver(url);
    info!("使用解析器: {:?}", resolver.platform());

    // 尝试解析器
    match resolver.get_video_info(url).await {
        Ok(info) => {
            info!("Resolver 解析成功!");
            return Ok(info);
        }
        Err(ResolverError::Detailed {
            platform,
            url,
            message,
            action,
        }) => {
            // Detailed 错误包含用户可操作指引，直接展示
            let err = ResolverError::Detailed {
                platform,
                url,
                message,
                action,
            };
            return Err(err.into());
        }
        Err(e) => {
            info!("Resolver 解析失败: {}，尝试 yt-dlp...", e);
        }
    }

    // yt-dlp fallback
    if let Ok(info) = fetch_with_ytdlp(ytdlp, url, cli, config).await {
        return Ok(info);
    }
    info!("yt-dlp 也失败，尝试 FxTwitter...");

    // FxTwitter
    let fxtwitter = if let Some(ref proxy) = cli.proxy {
        FxTwitterResolver::with_proxy(proxy)?
    } else {
        FxTwitterResolver::new()
    };

    if let Ok(info) = fxtwitter.get_video_info(url).await {
        info!("FxTwitter 解析成功!");
        return Ok(info);
    }
    info!("FxTwitter 也失败，尝试 Scrapling...");

    // Scrapling 最后尝试
    if let Ok(client) = crate::scrapling_client::ScraplingClient::new() {
        if let Ok(info) = client.get_video_info(url).await {
            info!("Scrapling 解析成功!");
            return Ok(info);
        }
    }

    anyhow::bail!("所有解析器均失败");
}

/// 使用 yt-dlp 获取视频信息
pub async fn fetch_with_ytdlp(
    ytdlp: &YtDlp,
    url: &str,
    cli: &Cli,
    config: &AppConfig,
) -> Result<VideoInfo> {
    let mut yt = ytdlp.clone();

    if let Some(ref ua) = cli.user_agent {
        yt = yt.with_user_agent(ua.clone());
    }

    if let Some(ref cookie) = cli.cookies {
        yt = yt.with_cookie_file(cookie.clone());
    }

    // 抖音 URL 使用 DouYin cookie 文件
    if url.contains("douyin.com") {
        if let Some(ref dy_cookie) = config.douyin_cookie_file {
            if dy_cookie.exists() {
                info!("使用 DouYin cookie 文件: {:?}", dy_cookie);
                yt = yt.with_cookie_file(dy_cookie.clone());
            }
        }
    }

    if let Some(ref proxy) = cli.proxy {
        yt = yt.with_proxy(proxy.clone());
    }

    yt.get_video_info(url).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;
    use std::path::PathBuf;

    /// 用空 config 创建 AppConfig 用于测试
    fn test_config() -> AppConfig {
        AppConfig {
            output_dir: PathBuf::from("/tmp"),
            douyin_ms_token: None,
            douyin_cookie_file: None,
            ..Default::default()
        }
    }

    /// 用 msToken 创建 AppConfig 用于测试
    fn test_config_with_ms_token() -> AppConfig {
        AppConfig {
            output_dir: PathBuf::from("/tmp"),
            douyin_ms_token: Some("test_ms_token_xyz".to_string()),
            douyin_cookie_file: None,
            ..Default::default()
        }
    }

    #[test]
    fn test_global_registry_returns_same_instance() {
        // 重置单例（避免被其他测试污染）
        reset_global_registry_for_tests();

        let config = test_config();
        let r1 = global_registry(&config);
        let r2 = global_registry(&config);

        // 两次调用必须返回同一个 Arc 指针（singleton 契约）
        assert!(Arc::ptr_eq(&r1, &r2), "global_registry must return same Arc");
    }

    #[test]
    fn test_global_registry_init_with_ms_token() {
        // 重置单例，验证 msToken 注入生效
        reset_global_registry_for_tests();

        let config = test_config_with_ms_token();
        let registry = global_registry(&config);

        // 验证 DouYinResolver 已注册：douyin URL 应被路由到 DouYin（不是默认 Other）
        let douyin_resolver = registry.select_resolver("https://www.douyin.com/video/123");
        assert_eq!(
            douyin_resolver.platform(),
            crate::history::Platform::DouYin,
            "DouYinResolver must be registered for msToken injection"
        );

        // TikTok / Bilibili 也应注册
        assert_eq!(
            registry.select_resolver("https://tiktok.com/video/1").platform(),
            crate::history::Platform::TikTok
        );
        assert_eq!(
            registry
                .select_resolver("https://bilibili.com/video/BV1")
                .platform(),
            crate::history::Platform::Bilibili
        );

        // 未知平台应回退到默认 resolver (Other)
        assert_eq!(
            registry.select_resolver("https://example.com/video").platform(),
            crate::history::Platform::Other
        );
    }
}
