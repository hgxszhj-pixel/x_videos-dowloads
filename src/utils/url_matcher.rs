//! URL 匹配工具模块
//!
//! 提供统一的 URL 模式匹配和平台检测功能

use crate::types::Platform;

/// URL 模式定义
pub struct UrlPattern {
    /// 平台
    pub platform: Platform,
    /// URL 模式列表（按优先级排序，第一个匹配的生效）
    pub patterns: &'static [&'static str],
}

/// 所有支持的 URL 模式
const URL_PATTERNS: &[UrlPattern] = &[
    UrlPattern {
        platform: Platform::X,
        patterns: &["x.com", "twitter.com"],
    },
    UrlPattern {
        platform: Platform::YouTube,
        patterns: &["youtube.com", "youtu.be"],
    },
    UrlPattern {
        platform: Platform::TikTok,
        patterns: &["tiktok.com", "vm.tiktok.com"],
    },
    UrlPattern {
        platform: Platform::DouYin,
        patterns: &["douyin.com", "v.douyin.com"],
    },
    UrlPattern {
        platform: Platform::Bilibili,
        patterns: &["bilibili.com", "bilivideo.com", "b23.tv"],
    },
    UrlPattern {
        platform: Platform::Instagram,
        patterns: &["instagram.com"],
    },
    UrlPattern {
        platform: Platform::Facebook,
        patterns: &["facebook.com", "fb.com"],
    },
    UrlPattern {
        platform: Platform::Weibo,
        patterns: &["weibo.com"],
    },
    UrlPattern {
        platform: Platform::ZhiXueYun,
        patterns: &["kc.zhixueyun.com"],
    },
    UrlPattern {
        platform: Platform::TulingPython,
        patterns: &["vip.tulingpyton.cn", "tulingpyton.cn"],
    },
];

/// 从 URL 提取 hostname 并转换为小写（一次性完成，避免重复分配）
///
/// # Arguments
/// * `url` - 待处理的 URL
///
/// # Returns
/// 小写的 hostname，或 None
pub fn extract_hostname_and_lower(url: &str) -> Option<String> {
    extract_hostname(url).map(|h| h.to_lowercase())
}

/// 从已小写化的 hostname 检测平台 (精确域名匹配)
///
/// # Arguments
/// * `host_lower` - 已小写化的 hostname
///
/// # Returns
/// 检测到的平台，如果都不匹配则返回 `Platform::Other`
pub fn detect_platform_by_host(host_lower: &str) -> Platform {
    for pattern in URL_PATTERNS {
        for p in pattern.patterns {
            if host_lower == *p || host_lower.ends_with(&format!(".{}", *p)) {
                return pattern.platform.clone();
            }
        }
    }
    Platform::Other
}

/// 从 URL 检测平台 (精确域名匹配)
///
/// # Arguments
/// * `url` - 待检测的 URL
///
/// # Returns
/// 检测到的平台，如果都不匹配则返回 `Platform::Other`
pub fn detect_platform(url: &str) -> Platform {
    if let Some(host_lower) = extract_hostname_and_lower(url) {
        return detect_platform_by_host(&host_lower);
    }
    Platform::Other
}

/// 从 URL 提取 hostname
fn extract_hostname(url: &str) -> Option<String> {
    // 尝试解析为 URL
    if let Ok(parsed) = url::Url::parse(url) {
        if let Some(host) = parsed.host_str() {
            return Some(host.to_string());
        }
    }
    // 回退：手动提取 "://" 和 "/" 之间的部分
    if let Some(host_start) = url.find("://") {
        let after_scheme = &url[host_start + 3..];
        if let Some(host_end) = after_scheme.find(['/', '?', '#']) {
            return Some(after_scheme[..host_end].to_string());
        } else {
            return Some(after_scheme.to_string());
        }
    }
    None
}

/// 检查已小写化的 hostname 是否匹配给定平台 (精确域名匹配)
///
/// # Arguments
/// * `host_lower` - 已小写化的 hostname
/// * `platform` - 要检测的平台
///
/// # Returns
/// 如果 hostname 匹配该平台则返回 `true`
pub fn url_matches_platform_by_host(host_lower: &str, platform: Platform) -> bool {
    for pattern in URL_PATTERNS {
        if pattern.platform == platform {
            for p in pattern.patterns {
                if host_lower == *p || host_lower.ends_with(&format!(".{}", *p)) {
                    return true;
                }
            }
        }
    }
    false
}

/// 检查 URL hostname 是否匹配给定平台 (精确域名匹配)
/// # Arguments
/// * `url` - 待检测的 URL
/// * `platform` - 要检测的平台
///
/// # Returns
/// 如果 URL 匹配该平台则返回 `true`
#[allow(dead_code)]
pub fn url_matches_platform(url: &str, platform: Platform) -> bool {
    if let Some(host_lower) = extract_hostname_and_lower(url) {
        return url_matches_platform_by_host(&host_lower, platform);
    }
    false
}

/// 获取平台的 URL 模式列表
///
/// # Arguments
/// * `platform` - 目标平台
///
/// # Returns
/// 该平台对应的 URL 模式列表，如果平台不支持则返回空数组
pub fn get_platform_patterns(platform: Platform) -> &'static [&'static str] {
    for pattern in URL_PATTERNS {
        if pattern.platform == platform {
            return pattern.patterns;
        }
    }
    &[]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_x() {
        assert_eq!(
            detect_platform("https://x.com/user/status/123"),
            Platform::X
        );
        assert_eq!(
            detect_platform("https://twitter.com/user/status/123"),
            Platform::X
        );
        assert_eq!(detect_platform("https://www.x.com/video"), Platform::X);
    }

    #[test]
    fn test_detect_youtube() {
        assert_eq!(
            detect_platform("https://youtube.com/watch?v=abc"),
            Platform::YouTube
        );
        assert_eq!(detect_platform("https://youtu.be/xyz"), Platform::YouTube);
    }

    #[test]
    fn test_detect_tiktok() {
        assert_eq!(
            detect_platform("https://www.tiktok.com/@user/video/123"),
            Platform::TikTok
        );
        assert_eq!(
            detect_platform("https://vm.tiktok.com/abc"),
            Platform::TikTok
        );
        assert_eq!(
            detect_platform("https://tiktok.com/video/456"),
            Platform::TikTok
        );
    }

    #[test]
    fn test_detect_douyin() {
        assert_eq!(
            detect_platform("https://www.douyin.com/video/7231679494828376333"),
            Platform::DouYin
        );
        assert_eq!(
            detect_platform("https://v.douyin.com/xxxx"),
            Platform::DouYin
        );
    }

    #[test]
    fn test_detect_bilibili() {
        assert_eq!(
            detect_platform("https://www.bilibili.com/video/BV1xx411c7XD"),
            Platform::Bilibili
        );
        assert_eq!(detect_platform("https://b23.tv/abc"), Platform::Bilibili);
        assert_eq!(
            detect_platform("https://www.bilivideo.com/video/bvid"),
            Platform::Bilibili
        );
    }

    #[test]
    fn test_detect_instagram() {
        assert_eq!(
            detect_platform("https://www.instagram.com/p/abc"),
            Platform::Instagram
        );
    }

    #[test]
    fn test_detect_facebook() {
        assert_eq!(
            detect_platform("https://www.facebook.com/video/123"),
            Platform::Facebook
        );
        assert_eq!(
            detect_platform("https://fb.com/video/456"),
            Platform::Facebook
        );
    }

    #[test]
    fn test_detect_weibo() {
        assert_eq!(
            detect_platform("https://weibo.com/user/status/123"),
            Platform::Weibo
        );
    }

    #[test]
    fn test_detect_zhixueyun() {
        assert_eq!(
            detect_platform("https://kc.zhixueyun.com/course/study"),
            Platform::ZhiXueYun
        );
    }

    #[test]
    fn test_detect_tulingpython() {
        assert_eq!(detect_platform("https://vip.tulingpyton.cn/p/t_pc/course_pc_detail/column/p_6981db02e4b0694c34d6c8dc"), Platform::TulingPython);
        assert_eq!(
            detect_platform("https://tulingpyton.cn/p/course/member/p_628ef20ee4b0cedf38b6e660"),
            Platform::TulingPython
        );
    }

    #[test]
    fn test_detect_other() {
        assert_eq!(
            detect_platform("https://example.com/video"),
            Platform::Other
        );
        assert_eq!(detect_platform("https://unknown.site.com"), Platform::Other);
    }

    #[test]
    fn test_url_matches_platform() {
        assert!(url_matches_platform(
            "https://tiktok.com/video/123",
            Platform::TikTok
        ));
        assert!(!url_matches_platform(
            "https://tiktok.com/video/123",
            Platform::DouYin
        ));
    }

    #[test]
    fn test_get_platform_patterns() {
        assert_eq!(
            get_platform_patterns(Platform::TikTok),
            &["tiktok.com", "vm.tiktok.com"]
        );
        assert_eq!(
            get_platform_patterns(Platform::DouYin),
            &["douyin.com", "v.douyin.com"]
        );
        assert!(get_platform_patterns(Platform::Other).is_empty());
    }
}
