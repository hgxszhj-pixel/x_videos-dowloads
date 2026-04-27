//! 网站特征学习模块
//!
//! 记录每个网站的速度特征、线程限制等，实现智能下载

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::RwLock;
use tracing::{info, warn};

/// 网站特征
#[derive(Debug, Clone, Serialize, Deserialize)]
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
}

/// 防爬机制类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AntiCrawlerType {
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
            speed_threshold_bps: 500 * 1024, // 500KB/s
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
        }
    }

    /// 更新成功下载
    pub fn record_success(&mut self, speed_bps: u64) {
        self.success_count += 1;
        self.last_visited = chrono_now();

        // 更新平均速度（指数移动平均）
        if self.avg_speed_bps == 0 {
            self.avg_speed_bps = speed_bps;
        } else {
            self.avg_speed_bps = ((self.avg_speed_bps as u128 * 3 + speed_bps as u128) / 4) as u64;
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

    /// 检测速度限制
    pub fn detect_speed_limit(&mut self, measured_bps: u64) {
        // 如果测量速度远低于平均速度，可能被限速
        if self.avg_speed_bps > 0 && measured_bps < self.avg_speed_bps / 3 {
            self.has_speed_limit = true;
            self.speed_threshold_bps = ((measured_bps as f64 * 0.8) as u64).max(1024);
            info!("网站 {} 检测到速度限制，阈值调整为 {:.1} KB/s",
                self.domain,
                self.speed_threshold_bps as f64 / 1024.0
            );
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

    /// 可信度（基于成功次数）
    pub fn confidence(&self) -> f64 {
        let total = self.success_count + self.failure_count;
        if total == 0 {
            return 0.0;
        }
        (self.success_count as f64 / total as f64).min(1.0)
    }
}

/// 默认网站特征表（已知网站的推荐配置）
fn get_default_profiles() -> HashMap<String, WebsiteProfile> {
    let mut defaults = HashMap::new();

    // ===== 社交媒体视频 =====
    // Twitter/X video CDN - Cloudflare保护
    defaults.insert("video.twimg.com".to_string(), WebsiteProfile {
        domain: "video.twimg.com".to_string(),
        adaptive_threads: true,
        min_threads: 2,
        max_threads: 4,
        speed_threshold_bps: 500 * 1024,
        avg_speed_bps: 0,
        success_count: 0,
        failure_count: 0,
        last_visited: 0,
        has_thread_limit: true,
        has_speed_limit: true,
        anti_crawler_type: AntiCrawlerType::Cloudflare,
        requires_auth: false,
        recommended_delay_ms: 500,
        rotate_user_agent: true,
    });

    // FxTwitter API - 无防爬
    defaults.insert("api.fxtwitter.com".to_string(), WebsiteProfile {
        domain: "api.fxtwitter.com".to_string(),
        adaptive_threads: false,
        min_threads: 1,
        max_threads: 2,
        speed_threshold_bps: 1024 * 1024,
        avg_speed_bps: 0,
        success_count: 0,
        failure_count: 0,
        last_visited: 0,
        has_thread_limit: false,
        has_speed_limit: false,
        anti_crawler_type: AntiCrawlerType::None,
        requires_auth: false,
        recommended_delay_ms: 0,
        rotate_user_agent: false,
    });

    // Reddit视频 - Cloudflare + RateLimit
    defaults.insert("v.redd.it".to_string(), WebsiteProfile {
        domain: "v.redd.it".to_string(),
        adaptive_threads: true,
        min_threads: 2,
        max_threads: 4,
        speed_threshold_bps: 500 * 1024,
        avg_speed_bps: 0,
        success_count: 0,
        failure_count: 0,
        last_visited: 0,
        has_thread_limit: true,
        has_speed_limit: true,
        anti_crawler_type: AntiCrawlerType::Cloudflare,
        requires_auth: false,
        recommended_delay_ms: 500,
        rotate_user_agent: true,
    });

    defaults.insert("preview.redd.it".to_string(), WebsiteProfile {
        domain: "preview.redd.it".to_string(),
        adaptive_threads: true,
        min_threads: 2,
        max_threads: 4,
        speed_threshold_bps: 500 * 1024,
        avg_speed_bps: 0,
        success_count: 0,
        failure_count: 0,
        last_visited: 0,
        has_thread_limit: true,
        has_speed_limit: true,
        anti_crawler_type: AntiCrawlerType::Cloudflare,
        requires_auth: false,
        recommended_delay_ms: 500,
        rotate_user_agent: true,
    });

    // ===== 代码托管社区 =====
    // GitHub - 基本无防爬，但有RateLimit
    defaults.insert("github.com".to_string(), WebsiteProfile {
        domain: "github.com".to_string(),
        adaptive_threads: true,
        min_threads: 2,
        max_threads: 6,
        speed_threshold_bps: 800 * 1024,
        avg_speed_bps: 0,
        success_count: 0,
        failure_count: 0,
        last_visited: 0,
        has_thread_limit: false,
        has_speed_limit: false,
        anti_crawler_type: AntiCrawlerType::RateLimit,
        requires_auth: false,
        recommended_delay_ms: 1000,
        rotate_user_agent: false,
    });

    defaults.insert("githubusercontent.com".to_string(), WebsiteProfile {
        domain: "githubusercontent.com".to_string(),
        adaptive_threads: true,
        min_threads: 2,
        max_threads: 6,
        speed_threshold_bps: 800 * 1024,
        avg_speed_bps: 0,
        success_count: 0,
        failure_count: 0,
        last_visited: 0,
        has_thread_limit: false,
        has_speed_limit: false,
        anti_crawler_type: AntiCrawlerType::None,
        requires_auth: false,
        recommended_delay_ms: 0,
        rotate_user_agent: false,
    });

    // ===== 博客平台 =====
    // Medium - Cloudflare + JS挑战
    defaults.insert("medium.com".to_string(), WebsiteProfile {
        domain: "medium.com".to_string(),
        adaptive_threads: true,
        min_threads: 2,
        max_threads: 4,
        speed_threshold_bps: 500 * 1024,
        avg_speed_bps: 0,
        success_count: 0,
        failure_count: 0,
        last_visited: 0,
        has_thread_limit: true,
        has_speed_limit: true,
        anti_crawler_type: AntiCrawlerType::Cloudflare,
        requires_auth: false,
        recommended_delay_ms: 500,
        rotate_user_agent: true,
    });

    defaults.insert("substack.com".to_string(), WebsiteProfile {
        domain: "substack.com".to_string(),
        adaptive_threads: true,
        min_threads: 2,
        max_threads: 4,
        speed_threshold_bps: 500 * 1024,
        avg_speed_bps: 0,
        success_count: 0,
        failure_count: 0,
        last_visited: 0,
        has_thread_limit: false,
        has_speed_limit: false,
        anti_crawler_type: AntiCrawlerType::None,
        requires_auth: false,
        recommended_delay_ms: 0,
        rotate_user_agent: false,
    });

    defaults.insert("wordpress.com".to_string(), WebsiteProfile {
        domain: "wordpress.com".to_string(),
        adaptive_threads: true,
        min_threads: 2,
        max_threads: 4,
        speed_threshold_bps: 400 * 1024,
        avg_speed_bps: 0,
        success_count: 0,
        failure_count: 0,
        last_visited: 0,
        has_thread_limit: false,
        has_speed_limit: false,
        anti_crawler_type: AntiCrawlerType::None,
        requires_auth: false,
        recommended_delay_ms: 0,
        rotate_user_agent: false,
    });

    defaults.insert("blogspot.com".to_string(), WebsiteProfile {
        domain: "blogspot.com".to_string(),
        adaptive_threads: true,
        min_threads: 2,
        max_threads: 4,
        speed_threshold_bps: 400 * 1024,
        avg_speed_bps: 0,
        success_count: 0,
        failure_count: 0,
        last_visited: 0,
        has_thread_limit: false,
        has_speed_limit: false,
        anti_crawler_type: AntiCrawlerType::None,
        requires_auth: false,
        recommended_delay_ms: 0,
        rotate_user_agent: false,
    });

    defaults.insert("tumblr.com".to_string(), WebsiteProfile {
        domain: "tumblr.com".to_string(),
        adaptive_threads: true,
        min_threads: 2,
        max_threads: 4,
        speed_threshold_bps: 400 * 1024,
        avg_speed_bps: 0,
        success_count: 0,
        failure_count: 0,
        last_visited: 0,
        has_thread_limit: false,
        has_speed_limit: false,
        anti_crawler_type: AntiCrawlerType::Cloudflare,
        requires_auth: true,
        recommended_delay_ms: 1000,
        rotate_user_agent: true,
    });

    // ===== 视频平台 =====
    // YouTube - 无防爬但有RateLimit
    defaults.insert("youtube.com".to_string(), WebsiteProfile {
        domain: "youtube.com".to_string(),
        adaptive_threads: true,
        min_threads: 2,
        max_threads: 8,
        speed_threshold_bps: 1024 * 1024,
        avg_speed_bps: 0,
        success_count: 0,
        failure_count: 0,
        last_visited: 0,
        has_thread_limit: false,
        has_speed_limit: false,
        anti_crawler_type: AntiCrawlerType::RateLimit,
        requires_auth: false,
        recommended_delay_ms: 500,
        rotate_user_agent: false,
    });

    defaults.insert("youtu.be".to_string(), WebsiteProfile {
        domain: "youtu.be".to_string(),
        adaptive_threads: true,
        min_threads: 2,
        max_threads: 8,
        speed_threshold_bps: 1024 * 1024,
        avg_speed_bps: 0,
        success_count: 0,
        failure_count: 0,
        last_visited: 0,
        has_thread_limit: false,
        has_speed_limit: false,
        anti_crawler_type: AntiCrawlerType::RateLimit,
        requires_auth: false,
        recommended_delay_ms: 500,
        rotate_user_agent: false,
    });

    // Twitch - Cloudflare + RateLimit
    defaults.insert("twitch.tv".to_string(), WebsiteProfile {
        domain: "twitch.tv".to_string(),
        adaptive_threads: true,
        min_threads: 2,
        max_threads: 6,
        speed_threshold_bps: 800 * 1024,
        avg_speed_bps: 0,
        success_count: 0,
        failure_count: 0,
        last_visited: 0,
        has_thread_limit: true,
        has_speed_limit: true,
        anti_crawler_type: AntiCrawlerType::Cloudflare,
        requires_auth: false,
        recommended_delay_ms: 1000,
        rotate_user_agent: true,
    });

    defaults.insert("vimeo.com".to_string(), WebsiteProfile {
        domain: "vimeo.com".to_string(),
        adaptive_threads: true,
        min_threads: 2,
        max_threads: 6,
        speed_threshold_bps: 800 * 1024,
        avg_speed_bps: 0,
        success_count: 0,
        failure_count: 0,
        last_visited: 0,
        has_thread_limit: false,
        has_speed_limit: false,
        anti_crawler_type: AntiCrawlerType::None,
        requires_auth: false,
        recommended_delay_ms: 0,
        rotate_user_agent: false,
    });

    defaults.insert("dailymotion.com".to_string(), WebsiteProfile {
        domain: "dailymotion.com".to_string(),
        adaptive_threads: true,
        min_threads: 2,
        max_threads: 4,
        speed_threshold_bps: 500 * 1024,
        avg_speed_bps: 0,
        success_count: 0,
        failure_count: 0,
        last_visited: 0,
        has_thread_limit: false,
        has_speed_limit: false,
        anti_crawler_type: AntiCrawlerType::Cloudflare,
        requires_auth: false,
        recommended_delay_ms: 500,
        rotate_user_agent: true,
    });

    // ===== 社区论坛 =====
    // Reddit - Cloudflare + RateLimit + 需要Cookie
    defaults.insert("reddit.com".to_string(), WebsiteProfile {
        domain: "reddit.com".to_string(),
        adaptive_threads: true,
        min_threads: 2,
        max_threads: 4,
        speed_threshold_bps: 500 * 1024,
        avg_speed_bps: 0,
        success_count: 0,
        failure_count: 0,
        last_visited: 0,
        has_thread_limit: true,
        has_speed_limit: true,
        anti_crawler_type: AntiCrawlerType::Cloudflare,
        requires_auth: true,
        recommended_delay_ms: 1000,
        rotate_user_agent: true,
    });

    // StackOverflow - 基本无防爬
    defaults.insert("stackoverflow.com".to_string(), WebsiteProfile {
        domain: "stackoverflow.com".to_string(),
        adaptive_threads: true,
        min_threads: 2,
        max_threads: 4,
        speed_threshold_bps: 400 * 1024,
        avg_speed_bps: 0,
        success_count: 0,
        failure_count: 0,
        last_visited: 0,
        has_thread_limit: false,
        has_speed_limit: false,
        anti_crawler_type: AntiCrawlerType::None,
        requires_auth: false,
        recommended_delay_ms: 0,
        rotate_user_agent: false,
    });

    // Disqus - Cloudflare
    defaults.insert("disqus.com".to_string(), WebsiteProfile {
        domain: "disqus.com".to_string(),
        adaptive_threads: true,
        min_threads: 2,
        max_threads: 4,
        speed_threshold_bps: 400 * 1024,
        avg_speed_bps: 0,
        success_count: 0,
        failure_count: 0,
        last_visited: 0,
        has_thread_limit: false,
        has_speed_limit: false,
        anti_crawler_type: AntiCrawlerType::Cloudflare,
        requires_auth: false,
        recommended_delay_ms: 500,
        rotate_user_agent: true,
    });

    // ===== 新闻媒体 =====
    // BBC - 基本无防爬
    defaults.insert("bbc.com".to_string(), WebsiteProfile {
        domain: "bbc.com".to_string(),
        adaptive_threads: true,
        min_threads: 2,
        max_threads: 4,
        speed_threshold_bps: 600 * 1024,
        avg_speed_bps: 0,
        success_count: 0,
        failure_count: 0,
        last_visited: 0,
        has_thread_limit: false,
        has_speed_limit: false,
        anti_crawler_type: AntiCrawlerType::None,
        requires_auth: false,
        recommended_delay_ms: 0,
        rotate_user_agent: false,
    });

    // CNN - Cloudflare
    defaults.insert("cnn.com".to_string(), WebsiteProfile {
        domain: "cnn.com".to_string(),
        adaptive_threads: true,
        min_threads: 2,
        max_threads: 4,
        speed_threshold_bps: 600 * 1024,
        avg_speed_bps: 0,
        success_count: 0,
        failure_count: 0,
        last_visited: 0,
        has_thread_limit: false,
        has_speed_limit: false,
        anti_crawler_type: AntiCrawlerType::Cloudflare,
        requires_auth: false,
        recommended_delay_ms: 500,
        rotate_user_agent: true,
    });

    // NYTimes - Cloudflare + 需要Cookie
    defaults.insert("nytimes.com".to_string(), WebsiteProfile {
        domain: "nytimes.com".to_string(),
        adaptive_threads: true,
        min_threads: 2,
        max_threads: 4,
        speed_threshold_bps: 500 * 1024,
        avg_speed_bps: 0,
        success_count: 0,
        failure_count: 0,
        last_visited: 0,
        has_thread_limit: true,
        has_speed_limit: true,
        anti_crawler_type: AntiCrawlerType::Cloudflare,
        requires_auth: true,
        recommended_delay_ms: 1000,
        rotate_user_agent: true,
    });

    // The Guardian - Cloudflare
    defaults.insert("theguardian.com".to_string(), WebsiteProfile {
        domain: "theguardian.com".to_string(),
        adaptive_threads: true,
        min_threads: 2,
        max_threads: 4,
        speed_threshold_bps: 500 * 1024,
        avg_speed_bps: 0,
        success_count: 0,
        failure_count: 0,
        last_visited: 0,
        has_thread_limit: false,
        has_speed_limit: false,
        anti_crawler_type: AntiCrawlerType::Cloudflare,
        requires_auth: false,
        recommended_delay_ms: 500,
        rotate_user_agent: true,
    });

    // Washington Post - Cloudflare
    defaults.insert("washingtonpost.com".to_string(), WebsiteProfile {
        domain: "washingtonpost.com".to_string(),
        adaptive_threads: true,
        min_threads: 2,
        max_threads: 4,
        speed_threshold_bps: 500 * 1024,
        avg_speed_bps: 0,
        success_count: 0,
        failure_count: 0,
        last_visited: 0,
        has_thread_limit: false,
        has_speed_limit: false,
        anti_crawler_type: AntiCrawlerType::Cloudflare,
        requires_auth: false,
        recommended_delay_ms: 500,
        rotate_user_agent: true,
    });

    // ===== 通配符（默认配置） =====
    defaults.insert("*.com".to_string(), WebsiteProfile {
        domain: "*.com".to_string(),
        adaptive_threads: true,
        min_threads: 2,
        max_threads: 4,
        speed_threshold_bps: 300 * 1024,
        avg_speed_bps: 0,
        success_count: 0,
        failure_count: 0,
        last_visited: 0,
        has_thread_limit: false,
        has_speed_limit: false,
        anti_crawler_type: AntiCrawlerType::Cloudflare,
        requires_auth: false,
        recommended_delay_ms: 500,
        rotate_user_agent: true,
    });

    defaults.insert("*.org".to_string(), WebsiteProfile {
        domain: "*.org".to_string(),
        adaptive_threads: true,
        min_threads: 2,
        max_threads: 4,
        speed_threshold_bps: 300 * 1024,
        avg_speed_bps: 0,
        success_count: 0,
        failure_count: 0,
        last_visited: 0,
        has_thread_limit: false,
        has_speed_limit: false,
        anti_crawler_type: AntiCrawlerType::Cloudflare,
        requires_auth: false,
        recommended_delay_ms: 500,
        rotate_user_agent: true,
    });

    defaults.insert("*.net".to_string(), WebsiteProfile {
        domain: "*.net".to_string(),
        adaptive_threads: true,
        min_threads: 2,
        max_threads: 4,
        speed_threshold_bps: 300 * 1024,
        avg_speed_bps: 0,
        success_count: 0,
        failure_count: 0,
        last_visited: 0,
        has_thread_limit: false,
        has_speed_limit: false,
        anti_crawler_type: AntiCrawlerType::Cloudflare,
        requires_auth: false,
        recommended_delay_ms: 500,
        rotate_user_agent: true,
    });

    defaults
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

    /// 获取或创建特征
    pub fn get_or_create(&self, domain: &str) -> WebsiteProfile {
        // 先精确匹配
        if let Some(profile) = self.profiles.read().unwrap().get(domain) {
            return profile.clone();
        }

        // 再尝试通配符匹配
        let profiles = self.profiles.read().unwrap();
        for (key, profile) in profiles.iter() {
            if key.starts_with("*.") {
                let suffix = &key[1..];
                if domain.ends_with(suffix) {
                    return profile.clone();
                }
            }
        }

        // 没有匹配，创建新特征
        drop(profiles);
        WebsiteProfile::from_domain(domain)
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

    /// 检测速度限制
    pub fn check_speed_limit(&self, domain: &str, measured_bps: u64) {
        let mut profile = self.get_or_create(domain);
        profile.detect_speed_limit(measured_bps);
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
        // 先加载默认配置
        let defaults = get_default_profiles();
        *self.profiles.write().unwrap() = defaults;

        if !self.store_path.exists() {
            info!("使用默认网站特征配置");
            return;
        }

        if let Ok(json) = std::fs::read_to_string(&self.store_path) {
            if let Ok(profiles) = serde_json::from_str::<HashMap<String, WebsiteProfile>>(&json) {
                // 合并：用户配置覆盖默认配置
                let mut store = self.profiles.write().unwrap();
                for (domain, profile) in profiles {
                    store.insert(domain, profile);
                }
                info!("已加载网站特征: {} 个网站", store.len());
            }
        }
    }

    /// 列出所有已知网站
    pub fn list_domains(&self) -> Vec<String> {
        self.profiles.read().unwrap().keys().cloned().collect()
    }

    /// 获取统计信息
    pub fn stats(&self) -> (usize, usize) {
        let profiles = self.profiles.read().unwrap();
        let known = profiles.len();
        let with_limits = profiles.values()
            .filter(|p| p.has_thread_limit || p.has_speed_limit)
            .count();
        (known, with_limits)
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

    let profile = store.read().unwrap().get(&domain);

    match profile {
        Some(p) if p.confidence() > 0.5 => {
            let (min, max, threshold) = p.recommended_config();
            info!("使用已知配置 {}: min={}, max={}, threshold={:.1}KB/s, 可信度={:.1}%",
                domain, min, max, threshold as f64 / 1024.0, p.confidence() * 100.0);
            (p.adaptive_threads, min, max, threshold)
        }
        _ => {
            info!("使用默认配置: {}", domain);
            (true, 2, 8, 500 * 1024)
        }
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
}
