//! 网站特征学习模块
//!
//! 记录每个网站的速度特征、线程限制等，实现智能下载

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::RwLock;
use tracing::{info, warn};

/// 防爬设置（bundled to reduce boilerplate）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct AntiCrawlerSettings {
    rotate_user_agent: bool,
    use_accept_language: bool,
    use_referer: bool,
    referer_domain: Option<String>,
    use_chrome_headers: bool,
    tls_fingerprint: String,
    requires_js_rendering: bool,
    detect_js_fingerprint: bool,
    clear_cookies_on_failure: bool,
}

impl AntiCrawlerSettings {
    fn chrome() -> Self {
        Self {
            rotate_user_agent: true,
            use_accept_language: true,
            use_referer: true,
            referer_domain: Some("https://x.com".to_string()),
            use_chrome_headers: true,
            tls_fingerprint: "chrome".to_string(),
            requires_js_rendering: false,
            detect_js_fingerprint: false,
            clear_cookies_on_failure: true,
        }
    }

    fn none() -> Self {
        Self {
            rotate_user_agent: false,
            use_accept_language: false,
            use_referer: false,
            referer_domain: None,
            use_chrome_headers: false,
            tls_fingerprint: "normal".to_string(),
            requires_js_rendering: false,
            detect_js_fingerprint: false,
            clear_cookies_on_failure: false,
        }
    }

    fn js_aware() -> Self {
        Self {
            rotate_user_agent: true,
            use_accept_language: true,
            use_referer: false,
            referer_domain: None,
            use_chrome_headers: true,
            tls_fingerprint: "chrome".to_string(),
            requires_js_rendering: true,
            detect_js_fingerprint: true,
            clear_cookies_on_failure: true,
        }
    }
}

/// 网站配置模板
enum ProfileTemplate {
    Cloudflare { delay_ms: u32, thread_limit: bool },
    None,
    RateLimit { delay_ms: u32 },
    Auth { delay_ms: u32 },
    JavaScript,
}

impl ProfileTemplate {
    fn build(&self, domain: &str) -> WebsiteProfile {
        let (anti_crawler_type, adaptive_threads, max_threads, speed_threshold_bps, anti_settings, requires_auth, recommended_delay_ms) = match self {
            Self::Cloudflare { delay_ms, thread_limit } => (
                AntiCrawlerType::Cloudflare,
                true,
                if *thread_limit { 4 } else { 6 },
                500 * 1024,
                AntiCrawlerSettings::chrome(),
                false,
                *delay_ms,
            ),
            Self::None => (
                AntiCrawlerType::None,
                true,
                6,
                600 * 1024,
                AntiCrawlerSettings::none(),
                false,
                0,
            ),
            Self::RateLimit { delay_ms } => (
                AntiCrawlerType::RateLimit,
                true,
                6,
                800 * 1024,
                AntiCrawlerSettings::none(),
                false,
                *delay_ms,
            ),
            Self::Auth { delay_ms } => (
                AntiCrawlerType::Cloudflare,
                true,
                4,
                500 * 1024,
                AntiCrawlerSettings::chrome(),
                true,
                *delay_ms,
            ),
            Self::JavaScript => (
                AntiCrawlerType::JavaScript,
                true,
                4,
                500 * 1024,
                AntiCrawlerSettings::js_aware(),
                false,
                500,
            ),
        };

        let anti = anti_settings;
        WebsiteProfile {
            domain: domain.to_string(),
            adaptive_threads,
            min_threads: 2,
            max_threads,
            speed_threshold_bps,
            avg_speed_bps: 0,
            success_count: 0,
            failure_count: 0,
            last_visited: 0,
            has_thread_limit: matches!(self, Self::Cloudflare { thread_limit: true, .. })
                || matches!(self, Self::Auth { .. }),
            has_speed_limit: matches!(self, Self::Cloudflare { thread_limit: true, .. })
                || matches!(self, Self::Auth { .. }),
            anti_crawler_type,
            requires_auth,
            recommended_delay_ms,
            rotate_user_agent: anti.rotate_user_agent,
            use_accept_language: anti.use_accept_language,
            use_referer: anti.use_referer,
            referer_domain: anti.referer_domain,
            use_chrome_headers: anti.use_chrome_headers,
            tls_fingerprint: anti.tls_fingerprint,
            use_proxy_rotation: false,
            proxy_rotation_interval_s: 60,
            requires_js_rendering: anti.requires_js_rendering,
            detect_js_fingerprint: anti.detect_js_fingerprint,
            clear_cookies_on_failure: anti.clear_cookies_on_failure,
        }
    }
}

/// 网站特征
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WebsiteProfile {
    /// 网站域名
    pub domain: String,
    /// 是否启用自适应线程
    pub adaptive_threads: bool,
    /// 推荐的最小线程数
    pub min_threads: u32,
    /// 推荐的 最大线程数
    pub max_threads: u32,
    /// 速度阈值（字节/秒）
    pub speed_threshold_bps: u64,
    /// 平均下载速度（字节/秒）
    pub avg_speed_bps: u64,
    /// 成功下载次数
    pub success_count: u32,
    /// 失败次数
    pub failure_count: u32,
    /// 最后访问时间
    pub last_visited: i64,
    /// 已知线程限制标志
    pub has_thread_limit: bool,
    /// 已知速度限制标志
    pub has_speed_limit: bool,
    /// 防爬机制类型
    pub anti_crawler_type: AntiCrawlerType,
    /// 是否需要Cookie认证
    pub requires_auth: bool,
    /// 建议的请求间隔（毫秒）
    pub recommended_delay_ms: u32,
    /// 需要User-Agent轮换
    pub rotate_user_agent: bool,

    // ===== 反爬虫规避字段 =====
    /// 需要 Accept-Language 头
    pub use_accept_language: bool,
    /// 需要 Referer 头
    pub use_referer: bool,
    /// 防盗链目标域名
    pub referer_domain: Option<String>,
    /// 需要额外的浏览器指纹头（Sec-CH-UA 等）
    pub use_chrome_headers: bool,
    /// TLS JA3 指纹类型
    pub tls_fingerprint: String,
    /// 是否使用代理轮换
    pub use_proxy_rotation: bool,
    /// 代理轮换间隔（秒）
    pub proxy_rotation_interval_s: u32,
    /// 需要 JavaScript 渲染
    pub requires_js_rendering: bool,
    /// 检测 JS 指纹
    pub detect_js_fingerprint: bool,
    /// 失败后清除 Cookie 重试
    pub clear_cookies_on_failure: bool,
}

/// 防爬机制类型
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AntiCrawlerType {
    #[default]
    /// 无已知防爬
    None,
    /// Cloudflare保护
    Cloudflare,
    /// 验证码
    Captcha,
    /// 速率限制
    RateLimit,
    /// User-Agent检测
    UserAgent,
    /// Cookie/Session要求
    CookieAuth,
    /// JS挑战
    JavaScript,
    /// IP封禁
    IPBan,
}

impl WebsiteProfile {
    /// 从域名创建设置
    pub fn from_domain(domain: &str) -> Self {
        Self {
            domain: domain.to_string(),
            adaptive_threads: true,
            min_threads: 2,
            max_threads: 8,
            speed_threshold_bps: 500 * 1024,
            avg_speed_bps: 0,
            success_count: 0,
            failure_count: 0,
            last_visited: chrono_now(),
            has_thread_limit: false,
            has_speed_limit: false,
            anti_crawler_type: AntiCrawlerType::None,
            requires_auth: false,
            recommended_delay_ms: 0,
            rotate_user_agent: false,
            use_accept_language: false,
            use_referer: false,
            referer_domain: None,
            use_chrome_headers: false,
            tls_fingerprint: "normal".to_string(),
            use_proxy_rotation: false,
            proxy_rotation_interval_s: 60,
            requires_js_rendering: false,
            detect_js_fingerprint: false,
            clear_cookies_on_failure: false,
        }
    }

    /// 更新成功下载
    pub fn record_success(&mut self, speed_bps: u64) {
        self.success_count += 1;
        self.last_visited = chrono_now();

        // 更新平均速度（指数移动平均，50/50 权重：让网络环境变化在 2 次下载内反映到 profile）
        if self.avg_speed_bps == 0 {
            self.avg_speed_bps = speed_bps;
        } else {
            self.avg_speed_bps = ((self.avg_speed_bps as u128 + speed_bps as u128) / 2) as u64;
        }

        // 如果速度很快，降低线程需求
        if speed_bps > 2 * 1024 * 1024 { // > 2MB/s
            self.has_thread_limit = false;
        }

        info!("网站 {} 记录成功，速度: {:.1} KB/s，平均: {:.1} KB/s",
            self.domain,
            speed_bps as f64 / 1024.0,
            self.avg_speed_bps as f64 / 1024.0
        );
    }

    /// 更新失败
    pub fn record_failure(&mut self) {
        self.failure_count += 1;
        self.last_visited = chrono_now();

        // 频繁失败，认为有线程限制
        if self.failure_count >= 3 && self.success_count < self.failure_count {
            self.has_thread_limit = true;
            self.max_threads = (self.max_threads / 2).max(2);
            info!("网站 {} 检测到线程限制，降低最大线程到 {}", self.domain, self.max_threads);
        }
    }

    /// 获取推荐配置
    pub fn recommended_config(&self) -> (u32, u32, u64) {
        let threads = if self.has_thread_limit {
            self.max_threads.min(4) // 有线程限制，最多4线程
        } else {
            self.max_threads
        };

        (self.min_threads, threads, self.speed_threshold_bps)
    }

    /// 根据预期文件大小推荐分段数（自适应按文件大小调整并行度）
    ///
    /// - 小文件 (< 50MB): 1 段（合并开销 > 并行收益）
    /// - 中等 (50-500MB): 4 段（受限 max_threads）
    /// - 大文件 (> 500MB): max_threads 段（最饱和）
    pub fn recommended_segments_for_size(&self, total_size: u64) -> u32 {
        let max = self.max_threads.max(1);

        if total_size < 50 * 1024 * 1024 {
            1.min(max)
        } else if total_size < 500 * 1024 * 1024 {
            4.min(max)
        } else {
            max
        }
    }

    /// 可信度（基于成功次数）
    pub fn confidence(&self) -> f64 {
        let total = self.success_count + self.failure_count;
        if total == 0 {
            return 0.0;
        }
        (self.success_count as f64 / total as f64).min(1.0)
    }
}

/// 根据域名获取默认配置（懒加载）
fn get_profile_for_domain(domain: &str) -> Option<WebsiteProfile> {
    let template = match domain {
        // ===== 社交媒体视频 =====
        // video.twimg.com 不支持并发 Range 分片请求 (实测 4 路并发下
        // "Got error: Downloaded X bytes, expected Y" 重试 10 次仍失败);
        // 单连接直下可达 7 MiB/s。绕开 Cloudflare 模板直接定 max_threads=1。
        "video.twimg.com" => return Some(WebsiteProfile {
            domain: domain.to_string(),
            adaptive_threads: false,
            min_threads: 1,
            max_threads: 1,
            speed_threshold_bps: 500 * 1024,
            ..Default::default()
        }),
        "api.fxtwitter.com" => return Some(WebsiteProfile {
            domain: domain.to_string(),
            adaptive_threads: false,
            min_threads: 1,
            max_threads: 2,
            speed_threshold_bps: 1024 * 1024,
            ..Default::default()
        }),
        "v.redd.it" | "preview.redd.it" => ProfileTemplate::Cloudflare { delay_ms: 500, thread_limit: true },

        // ===== 代码托管社区 =====
        "github.com" => ProfileTemplate::RateLimit { delay_ms: 1000 },
        "githubusercontent.com" => ProfileTemplate::None,

        // ===== 博客平台 =====
        "medium.com" => ProfileTemplate::Cloudflare { delay_ms: 500, thread_limit: true },
        "substack.com" | "wordpress.com" | "blogspot.com" => ProfileTemplate::None,
        "tumblr.com" => ProfileTemplate::Auth { delay_ms: 1000 },

        // ===== 视频平台 =====
        "youtube.com" | "youtu.be" => ProfileTemplate::RateLimit { delay_ms: 500 },
        "twitch.tv" => ProfileTemplate::Cloudflare { delay_ms: 1000, thread_limit: true },
        "vimeo.com" | "dailymotion.com" => ProfileTemplate::None,

        // ===== 社区论坛 =====
        "reddit.com" => ProfileTemplate::Auth { delay_ms: 1000 },
        "stackoverflow.com" => ProfileTemplate::None,
        "disqus.com" => ProfileTemplate::Cloudflare { delay_ms: 500, thread_limit: false },

        // ===== 新闻媒体 =====
        "bbc.com" => ProfileTemplate::None,
        "cnn.com" | "theguardian.com" | "washingtonpost.com" => {
            ProfileTemplate::Cloudflare { delay_ms: 500, thread_limit: false }
        }
        "nytimes.com" => ProfileTemplate::Auth { delay_ms: 1000 },

        // ===== 社交平台（通配符匹配） =====
        s if s.ends_with(".facebook.com") => ProfileTemplate::Auth { delay_ms: 1000 },
        s if s.ends_with(".instagram.com") => ProfileTemplate::Auth { delay_ms: 1000 },
        s if s.ends_with(".youtube.com") => ProfileTemplate::RateLimit { delay_ms: 500 },
        s if s.ends_with(".tiktok.com") => ProfileTemplate::JavaScript,
        s if s.ends_with(".pinterest.com") => {
            ProfileTemplate::Cloudflare { delay_ms: 500, thread_limit: false }
        }
        s if s.ends_with(".linkedin.com") => ProfileTemplate::Auth { delay_ms: 1000 },
        s if s.ends_with(".x.com") => ProfileTemplate::Cloudflare { delay_ms: 500, thread_limit: false },

        // ===== 通配符（默认配置） =====
        s if s.ends_with(".com") || s.ends_with(".org") || s.ends_with(".net") => {
            ProfileTemplate::Cloudflare { delay_ms: 500, thread_limit: false }
        }

        // 未知域名，返回 None（将使用默认配置）
        _ => return None,
    };
    Some(template.build(domain))
}

/// 获取当前时间戳
fn chrono_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// 网站特征存储器
pub struct WebsiteProfileStore {
    profiles: RwLock<HashMap<String, WebsiteProfile>>,
    store_path: PathBuf,
}

impl WebsiteProfileStore {
    /// 创建新的存储器
    pub fn new(store_path: PathBuf) -> Self {
        let mut store = Self {
            profiles: RwLock::new(HashMap::new()),
            store_path,
        };
        store.load();
        store
    }

    /// 获取网站特征
    pub fn get(&self, domain: &str) -> Option<WebsiteProfile> {
        self.profiles.read().unwrap().get(domain).cloned()
    }

    /// 获取或创建特征（懒加载方式）- 使用双检查锁定避免 TOCTOU 竞态
    pub fn get_or_create(&self, domain: &str) -> WebsiteProfile {
        // 先精确匹配（检查内存中是否已有）- 第一次检查
        if let Some(profile) = self.profiles.read().unwrap().get(domain) {
            return profile.clone();
        }

        // 获取默认配置
        let new_profile = get_profile_for_domain(domain)
            .unwrap_or_else(|| WebsiteProfile::from_domain(domain));

        // 使用写锁插入 - 第二次检查（在写锁内）
        let mut profiles = self.profiles.write().unwrap();
        if let Some(existing) = profiles.get(domain) {
            return existing.clone();
        }
        profiles.insert(domain.to_string(), new_profile.clone());
        new_profile
    }

    /// 更新特征
    pub fn update(&self, mut profile: WebsiteProfile) {
        let domain = profile.domain.clone();
        profile.last_visited = chrono_now();

        let mut profiles = self.profiles.write().unwrap();
        profiles.insert(domain, profile);

        drop(profiles);
        self.save();
    }

    /// 记录下载成功
    pub fn record_success(&self, domain: &str, speed_bps: u64) {
        let mut profile = self.get_or_create(domain);
        profile.record_success(speed_bps);
        self.update(profile);
    }

    /// 记录下载失败
    pub fn record_failure(&self, domain: &str) {
        let mut profile = self.get_or_create(domain);
        profile.record_failure();
        self.update(profile);
    }

    /// 从URL提取域名
    pub fn extract_domain(url: &str) -> String {
        // 简单提取域名
        if let Some(start) = url.find("://") {
            let after = &url[start + 3..];
            if let Some(end) = after.find('/') {
                return after[..end].to_string();
            }
            return after.to_string();
        }
        url.to_string()
    }

    /// 持久化到磁盘
    fn save(&self) {
        let profiles = self.profiles.read().unwrap();
        let json = serde_json::to_string_pretty(&*profiles).unwrap_or_default();

        if let Err(e) = std::fs::write(&self.store_path, json) {
            warn!("保存网站特征失败: {}", e);
        }
    }

    /// 从磁盘加载
    fn load(&mut self) {
        // 初始化空 HashMap，配置将在访问时懒加载
        *self.profiles.write().unwrap() = HashMap::new();

        if !self.store_path.exists() {
            info!("网站特征库为空，首次访问时将自动加载默认配置");
            return;
        }

        if let Ok(json) = std::fs::read_to_string(&self.store_path) {
            if let Ok(profiles) = serde_json::from_str::<HashMap<String, WebsiteProfile>>(&json) {
                let mut store = self.profiles.write().unwrap();
                for (domain, profile) in profiles {
                    store.insert(domain, profile);
                }
                info!("已加载网站特征: {} 个网站", store.len());
            }
        }
    }
}

impl Default for WebsiteProfileStore {
    fn default() -> Self {
        Self::new(PathBuf::from("website_profiles.json"))
    }
}

/// 从全局配置获取存储器（懒加载单例）
use std::sync::OnceLock;
static PROFILE_STORE: OnceLock<RwLock<WebsiteProfileStore>> = OnceLock::new();

/// 获取全局网站特征存储器
pub fn get_profile_store() -> &'static RwLock<WebsiteProfileStore> {
    PROFILE_STORE.get_or_init(|| {
        let config_dir = directories::ProjectDirs::from("com", "x-video-downloader", "x-video-downloader")
            .map(|dirs| dirs.config_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));

        let store_path = config_dir.join("website_profiles.json");

        // 确保目录存在
        let _ = std::fs::create_dir_all(&config_dir);

        RwLock::new(WebsiteProfileStore::new(store_path))
    })
}

/// 根据URL获取推荐配置
pub fn get_recommended_config(url: &str) -> (bool, u32, u32, u64) {
    let domain = WebsiteProfileStore::extract_domain(url);
    let store = get_profile_store();

    // 使用 get_or_create 实现懒加载
    let profile = store.read().unwrap().get(&domain)
        .or_else(|| get_profile_for_domain(&domain))
        .unwrap_or_else(|| WebsiteProfile::from_domain(&domain));

    if profile.confidence() > 0.5 {
        let (min, max, threshold) = profile.recommended_config();
        info!("使用已知配置 {}: min={}, max={}, threshold={:.1}KB/s, 可信度={:.1}%",
            domain, min, max, threshold as f64 / 1024.0, profile.confidence() * 100.0);
        (profile.adaptive_threads, min, max, threshold)
    } else {
        info!("使用默认配置: {}", domain);
        (true, 2, 8, 500 * 1024)
    }
}

/// 记录下载结果
pub fn record_download_result(url: &str, success: bool, speed_bps: u64) {
    let domain = WebsiteProfileStore::extract_domain(url);
    let store = get_profile_store();

    if success {
        store.write().unwrap().record_success(&domain, speed_bps);
    } else {
        store.write().unwrap().record_failure(&domain);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_domain() {
        assert_eq!(
            WebsiteProfileStore::extract_domain("https://video.twimg.com/abc"),
            "video.twimg.com"
        );
        assert_eq!(
            WebsiteProfileStore::extract_domain("https://x.com/user/status/123"),
            "x.com"
        );
    }

    #[test]
    fn test_profile_record() {
        let mut profile = WebsiteProfile::from_domain("test.com");

        profile.record_success(1024 * 1024); // 1MB/s
        assert_eq!(profile.success_count, 1);
        assert_eq!(profile.avg_speed_bps, 1024 * 1024);

        profile.record_success(2 * 1024 * 1024); // 2MB/s
        // 移动平均: (1 + 2) / 2 = 1.5MB/s
        assert!(profile.avg_speed_bps > 1024 * 1024);
    }

    #[test]
    fn test_thread_limit_detection() {
        let mut profile = WebsiteProfile::from_domain("test.com");

        // 多次失败应该触发线程限制
        for _ in 0..3 {
            profile.record_failure();
        }

        assert!(profile.has_thread_limit);
        assert!(profile.max_threads <= 4);
    }

    /// 三个文件大小分桶：小 (< 50MB) / 中 (50-500MB) / 大 (> 500MB)
    #[test]
    fn test_recommended_segments_for_size() {
        // from_domain 默认 max_threads=8
        let profile = WebsiteProfile::from_domain("test-cdn.com");
        assert_eq!(profile.max_threads, 8);

        // 小文件：1 段（省合并开销）
        assert_eq!(profile.recommended_segments_for_size(10 * 1024 * 1024), 1);
        assert_eq!(profile.recommended_segments_for_size(49 * 1024 * 1024), 1);

        // 中等：4 段（受限 max）
        assert_eq!(profile.recommended_segments_for_size(50 * 1024 * 1024), 4);
        assert_eq!(profile.recommended_segments_for_size(499 * 1024 * 1024), 4);

        // 大文件：8 段（饱和）
        assert_eq!(profile.recommended_segments_for_size(500 * 1024 * 1024), 8);
        assert_eq!(profile.recommended_segments_for_size(2 * 1024 * 1024 * 1024), 8);

        // max_threads=1 的域（video.twimg.com 风格）所有分桶都退化到 1
        let low = WebsiteProfile {
            domain: "limited.com".to_string(),
            max_threads: 1,
            ..Default::default()
        };
        assert_eq!(low.recommended_segments_for_size(10 * 1024 * 1024), 1);
        assert_eq!(low.recommended_segments_for_size(200 * 1024 * 1024), 1);
        assert_eq!(low.recommended_segments_for_size(2 * 1024 * 1024 * 1024), 1);
    }
}
