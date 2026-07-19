//! 下载策略派发
//!
//! 纯函数：根据 `VideoInfo` 或 `--direct` 标志决定走哪个 strategy。
//! 无副作用、无 I/O，便于单元测试覆盖所有分支。

use crate::types::{DownloadStrategy, VideoInfo};

/// 下载路径决策结果
#[derive(Debug, Clone, PartialEq)]
pub enum DownloadPath {
    /// 智学云/原生 (reqwest + Authorization header)
    Native { url: String, title: String },
    /// yt-dlp 子进程
    YtDlp { url: String, title: String, strategy: DownloadStrategy },
    /// 直接 reqwest (含 m3u8 自动检测)
    Direct { url: String },
    /// HLS (从 .m3u8 URL 自动检测)
    Hls { url: String },
}

impl DownloadPath {
    /// 提取 URL（所有变体都有 url 字段）
    pub fn url(&self) -> &str {
        match self {
            DownloadPath::Native { url, .. } => url,
            DownloadPath::YtDlp { url, .. } => url,
            DownloadPath::Direct { url } => url,
            DownloadPath::Hls { url } => url,
        }
    }
}

/// 根据 `cli.direct` 标志和 `video_info` 决定下载路径
///
/// **纯函数**：无副作用，不发起 I/O，不修改全局状态。
pub fn decide_path(
    direct_mode: bool,
    video_info: Option<&VideoInfo>,
    cli_direct_url: Option<&str>,
) -> Option<DownloadPath> {
    // CLI --direct 标志：跳过 video_info 解析，直接下载 URL
    if direct_mode {
        let url = cli_direct_url?.to_string();
        // m3u8 URL 走 HLS
        if url.contains(".m3u8") {
            return Some(DownloadPath::Hls { url });
        }
        return Some(DownloadPath::Direct { url });
    }

    // 正常模式：根据解析器决定的 download_strategy 选择 strategy
    let info = video_info?;
    let title = info.title.clone();
    let url = info.url.clone();

    match info.download_strategy {
        DownloadStrategy::Native => Some(DownloadPath::Native { url, title }),
        DownloadStrategy::Hls => Some(DownloadPath::Hls { url }),
        // Default + YtDlp 都走 yt-dlp 子进程
        _ => Some(DownloadPath::YtDlp {
            url,
            title,
            strategy: info.download_strategy,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{DownloadStrategy, VideoInfo};

    fn make_info(strategy: DownloadStrategy, url: &str, title: &str) -> VideoInfo {
        VideoInfo {
            url: url.to_string(),
            title: title.to_string(),
            description: None,
            uploader: None,
            duration: Some(120),
            thumbnail: None,
            formats: vec![],
            created_at: None,
            download_strategy: strategy,
            audio_url: None,
        }
    }

    #[test]
    fn direct_mode_with_m3u8_returns_hls_path() {
        let path = decide_path(true, None, Some("https://example.com/video.m3u8")).unwrap();
        assert_eq!(path, DownloadPath::Hls { url: "https://example.com/video.m3u8".to_string() });
    }

    #[test]
    fn direct_mode_with_plain_url_returns_direct_path() {
        let path = decide_path(true, None, Some("https://example.com/video.mp4")).unwrap();
        assert_eq!(path, DownloadPath::Direct { url: "https://example.com/video.mp4".to_string() });
    }

    #[test]
    fn direct_mode_without_url_returns_none() {
        assert!(decide_path(true, None, None).is_none());
    }

    #[test]
    fn normal_mode_native_strategy_returns_native_path() {
        let info = make_info(DownloadStrategy::Native, "https://x.com/v/1", "title");
        let path = decide_path(false, Some(&info), None).unwrap();
        assert_eq!(path, DownloadPath::Native {
            url: "https://x.com/v/1".to_string(),
            title: "title".to_string(),
        });
    }

    #[test]
    fn normal_mode_hls_strategy_returns_hls_path() {
        let info = make_info(DownloadStrategy::Hls, "https://example.com/stream.m3u8", "title");
        let path = decide_path(false, Some(&info), None).unwrap();
        assert!(matches!(path, DownloadPath::Hls { .. }));
    }

    #[test]
    fn normal_mode_default_returns_ytdlp_path() {
        let info = make_info(DownloadStrategy::default(), "https://yt.com/v/1", "title");
        let path = decide_path(false, Some(&info), None).unwrap();
        assert!(matches!(path, DownloadPath::YtDlp { .. }));
    }

    #[test]
    fn normal_mode_without_info_returns_none() {
        assert!(decide_path(false, None, None).is_none());
    }
}