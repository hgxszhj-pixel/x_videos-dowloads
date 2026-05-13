//! MyLearning 平台解析器
//!
//! 使用 Cookie 池管理认证，支持 cookie 复用和自动重试

use crate::resolvers::{CookieFileManager, CookiePool, PlatformResolver, ResolverError};
use crate::types::VideoInfo;
use crate::history::Platform;
use crate::cloakbrowser_client::CloakBrowserClient;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn};

/// MyLearning 解析器
pub struct MyLearningResolver {
    cookie_pool: Arc<CookiePool>,
    cookie_manager: CookieFileManager,
    browser_client: CloakBrowserClient,
}

impl MyLearningResolver {
    /// 创建新的 MyLearningResolver
    pub fn new(cookie_pool: Arc<CookiePool>) -> Self {
        Self {
            cookie_pool,
            cookie_manager: CookieFileManager::new(),
            browser_client: CloakBrowserClient::new(
                crate::cloakbrowser_client::CloakBrowserConfig::default(),
            ),
        }
    }

    /// 使用 Cookie 池获取视频信息，失败时自动重试
    pub fn fetch_with_pool(&self, url: &str) -> anyhow::Result<VideoInfo> {
        let mut attempt = 0;
        let max_attempts = 3;

        loop {
            attempt += 1;

            // 从 cookie 池获取 cookies
            let cookies = match self.cookie_pool.get() {
                Some(c) => c,
                None => {
                    // 池为空，尝试从浏览器获取
                    info!("Cookie pool empty, fetching from browser");
                    let browser_cookies = tokio::runtime::Handle::current()
                        .block_on(self.browser_client.get_cookies(url))?;
                    let mut cookies: HashMap<String, String> = HashMap::new();
                    for (key, value) in browser_cookies {
                        cookies.insert(key, value);
                    }
                    cookies
                }
            };

            // 使用 CookieFileManager 写入临时 cookie 文件
            let cookie_file = self.cookie_manager.get_cookie_file(&cookies, "mylearning");

            // 调用 yt-dlp 获取视频信息
            let mut ytdlp = crate::yt_dlp::YtDlp::new();
            if let Some(ref cookie_path) = cookie_file {
                if cookie_path.exists() {
                    ytdlp = ytdlp.with_cookie_file(cookie_path.clone());
                }
            }

            match ytdlp.get_video_info(url) {
                Ok(info) => {
                    info!("MyLearning video info fetched successfully");
                    return Ok(info);
                }
                Err(e) => {
                    let error_str = e.to_string().to_lowercase();

                    // 检查是否是 403/401 错误
                    if error_str.contains("403") || error_str.contains("401") {
                        warn!("Cookie authentication failed (attempt {}/{}): {}", attempt, max_attempts, e);

                        // 标记当前 Cookie 无效
                        // 注意：CookiePool 的 get() 方法会自动标记 hit_count 超过限制的 cookie 失效
                        // 这里我们通过等待一小段时间后重试来模拟 cookie 刷新
                        if attempt < max_attempts {
                            // 等待 1 秒后重试
                            std::thread::sleep(Duration::from_secs(1));
                            continue;
                        }
                    }

                    // 其他错误直接返回
                    anyhow::bail!("MyLearning fetch failed: {}", e);
                }
            }
        }
    }
}

impl PlatformResolver for MyLearningResolver {
    fn platform(&self) -> Platform {
        Platform::Other
    }

    fn supported_url_patterns(&self) -> Vec<&'static str> {
        vec!["mylearning.cn"]
    }

    fn get_video_info(&self, url: &str) -> anyhow::Result<VideoInfo, ResolverError> {
        self.fetch_with_pool(url).map_err(ResolverError::from)
    }

    fn requires_auth(&self) -> bool {
        true
    }
}
