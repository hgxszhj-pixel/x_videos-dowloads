//! URL 匹配工具模块
//!
//! 提供统一的 URL 模式匹配和平台检测功能

use crate::history::Platform;

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
];

/// 从 URL 检测平台
///
/// # Arguments
/// * `url` - 待检测的 URL
///
/// # Returns
/// 检测到的平台，如果都不匹配则返回 `Platform::Other`
pub fn detect_platform(url: &str) -> Platform {
    let url_lower = url.to_lowercase();

    for pattern in URL_PATTERNS {
        for p in pattern.patterns {
            if url_lower.contains(p) {
                return pattern.platform.clone();
            }
        }
    }

    Platform::Other
}

/// 检查 URL 是否匹配给定平台
///
/// # Arguments
/// * `url` - 待检测的 URL
/// * `platform` - 要检测的平台
///
/// # Returns
/// 如果 URL 匹配该平台则返回 `true`
pub fn url_matches_platform(url: &str, platform: Platform) -> bool {
    let url_lower = url.to_lowercase();

    for pattern in URL_PATTERNS {
        if pattern.platform == platform {
            for p in pattern.patterns {
                if url_lower.contains(p) {
                    return true;
                }
            }
        }
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
        assert_eq!(detect_platform("https://x.com/user/status/123"), Platform::X);
        assert_eq!(detect_platform("https://twitter.com/user/status/123"), Platform::X);
        assert_eq!(detect_platform("https://www.x.com/video"), Platform::X);
    }

    #[test]
    fn test_detect_youtube() {
        assert_eq!(detect_platform("https://youtube.com/watch?v=abc"), Platform::YouTube);
        assert_eq!(detect_platform("https://youtu.be/xyz"), Platform::YouTube);
    }

    #[test]
    fn test_detect_tiktok() {
        assert_eq!(detect_platform("https://www.tiktok.com/@user/video/123"), Platform::TikTok);
        assert_eq!(detect_platform("https://vm.tiktok.com/abc"), Platform::TikTok);
        assert_eq!(detect_platform("https://tiktok.com/video/456"), Platform::TikTok);
    }

    #[test]
    fn test_detect_douyin() {
        assert_eq!(detect_platform("https://www.douyin.com/video/7231679494828376333"), Platform::DouYin);
        assert_eq!(detect_platform("https://v.douyin.com/xxxx"), Platform::DouYin);
    }

    #[test]
    fn test_detect_bilibili() {
        assert_eq!(detect_platform("https://www.bilibili.com/video/BV1xx411c7XD"), Platform::Bilibili);
        assert_eq!(detect_platform("https://b23.tv/abc"), Platform::Bilibili);
        assert_eq!(detect_platform("https://www.bilivideo.com/video/bvid"), Platform::Bilibili);
    }

    #[test]
    fn test_detect_instagram() {
        assert_eq!(detect_platform("https://www.instagram.com/p/abc"), Platform::Instagram);
    }

    #[test]
    fn test_detect_facebook() {
        assert_eq!(detect_platform("https://www.facebook.com/video/123"), Platform::Facebook);
        assert_eq!(detect_platform("https://fb.com/video/456"), Platform::Facebook);
    }

    #[test]
    fn test_detect_weibo() {
        assert_eq!(detect_platform("https://weibo.com/user/status/123"), Platform::Weibo);
    }

    #[test]
    fn test_detect_zhixueyun() {
        assert_eq!(detect_platform("https://kc.zhixueyun.com/course/study"), Platform::ZhiXueYun);
    }

    #[test]
    fn test_detect_other() {
        assert_eq!(detect_platform("https://example.com/video"), Platform::Other);
        assert_eq!(detect_platform("https://unknown.site.com"), Platform::Other);
    }

    #[test]
    fn test_url_matches_platform() {
        assert!(url_matches_platform("https://tiktok.com/video/123", Platform::TikTok));
        assert!(!url_matches_platform("https://tiktok.com/video/123", Platform::DouYin));
    }

    #[test]
    fn test_get_platform_patterns() {
        assert_eq!(get_platform_patterns(Platform::TikTok), &["tiktok.com", "vm.tiktok.com"]);
        assert_eq!(get_platform_patterns(Platform::DouYin), &["douyin.com", "v.douyin.com"]);
        assert!(get_platform_patterns(Platform::Other).is_empty());
    }
}