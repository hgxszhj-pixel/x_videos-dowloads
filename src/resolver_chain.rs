//! URL 解析链路
//!
//! 统一调度 ResolverRegistry / FxTwitter / yt-dlp / Scrapling 完成视频信息解析。

use std::sync::Arc;

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
    let mut registry = ResolverRegistry::new();

    // 创建 DouYinResolver 并注入 msToken
    let mut douyin_resolver = DouYinResolver::new();
    if let Some(ref ms_token) = config.douyin_ms_token {
        let mut tokens = std::collections::HashMap::new();
        tokens.insert("msToken".to_string(), ms_token.clone());
        douyin_resolver = douyin_resolver.with_tokens(tokens);
    }
    registry.register(douyin_resolver);

    registry.register(TikTokResolver::new());
    registry.register(BilibiliResolver::new());
    registry.register(YouTubeResolver::new());
    registry.register(NnyyResolver::new());
    registry.register(ZhiXueYunResolver::new(Arc::new(CookiePool::new(24, 100))));
    registry.register(TulingPythonResolver::new().map_err(|e| {
        anyhow::anyhow!("TulingPythonResolver 初始化失败: {}", e)
    })?);

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
