//! 历史记录写入
//!
//! 将 `DownloadResult` + `VideoInfo` + URL 转换为 `HistoryEntry` 并写入磁盘。
//! 隔离 9 参数 `add_entry_with_all` 调用，让上层 orchestrator 只关心业务字段。

use anyhow::Result;
use url::Url;

use crate::history::{DownloadStatus, History};
use crate::types::{DownloadResult, VideoInfo};
use crate::utils::url_matcher;

/// 历史写入上下文（轻量，替代 9 个 positional 参数）
pub struct HistoryCtx<'a> {
    pub url: &'a str,
    pub title: Option<&'a str>,
    pub video_info: Option<&'a VideoInfo>,
    pub result: &'a DownloadResult,
}

/// 写入一条历史记录（仅在 `result.success == true` 时写）
///
/// 调用方约定：失败下载也记录，但 `status = Failed`。
/// 我们尊重调用方的 status 选择，只补全 platform 字段。
pub fn record(history: &mut History, ctx: HistoryCtx<'_>, status: DownloadStatus) -> Result<()> {
    let file_size = if ctx.result.file_size > 0 {
        Some(ctx.result.file_size)
    } else {
        None
    };
    let error_message = if status == DownloadStatus::Failed {
        ctx.result.error_message.clone()
    } else {
        None
    };
    let duration_secs = ctx.video_info.and_then(|v| v.duration).map(|d| d as f64);

    history.add_entry_with_all(
        ctx.url.to_string(),
        ctx.title.map(|t| t.to_string()),
        if ctx.result.success {
            Some(ctx.result.output_path.clone())
        } else {
            None
        },
        file_size,
        duration_secs,
        status,
        error_message,
    )
}

/// URL → Platform 转换的 thin wrapper（便于测试覆盖）
#[allow(dead_code)]
pub fn platform_from_url(url: &str) -> crate::types::Platform {
    // 兜底：如果 URL 解析失败，返回 Other（与 url_matcher::detect_platform 行为一致）
    if Url::parse(url).is_err() {
        return crate::types::Platform::Other;
    }
    url_matcher::detect_platform(url)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{DownloadResult, VideoInfo};
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn make_result(success: bool, file_size: u64) -> DownloadResult {
        DownloadResult {
            success,
            output_path: PathBuf::from("/tmp/test.mp4"),
            file_size,
            duration_secs: 1.0,
            error_message: if success {
                None
            } else {
                Some("test failure".to_string())
            },
        }
    }

    fn make_info() -> VideoInfo {
        VideoInfo {
            url: "https://tiktok.com/v/123".to_string(),
            title: "test".to_string(),
            description: None,
            uploader: None,
            duration: Some(60),
            thumbnail: None,
            formats: vec![],
            created_at: None,
            download_strategy: crate::types::DownloadStrategy::default(),
            audio_url: None,
        }
    }

    #[test]
    fn record_success_writes_completed_entry() {
        let dir = tempdir().unwrap();
        let mut history = History::new(dir.path()).unwrap();
        let result = make_result(true, 1024);

        record(
            &mut history,
            HistoryCtx {
                url: "https://tiktok.com/v/123",
                title: Some("video"),
                video_info: Some(&make_info()),
                result: &result,
            },
            DownloadStatus::Completed,
        )
        .unwrap();

        let entries = history.get_entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].status, DownloadStatus::Completed);
        assert_eq!(entries[0].platform, crate::types::Platform::TikTok);
        assert_eq!(entries[0].file_size, Some(1024));
        assert_eq!(entries[0].title, Some("video".to_string()));
    }

    #[test]
    fn record_failure_writes_failed_entry_with_error() {
        let dir = tempdir().unwrap();
        let mut history = History::new(dir.path()).unwrap();
        let result = make_result(false, 0);

        record(
            &mut history,
            HistoryCtx {
                url: "https://example.com/video",
                title: None,
                video_info: None,
                result: &result,
            },
            DownloadStatus::Failed,
        )
        .unwrap();

        let entries = history.get_entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].status, DownloadStatus::Failed);
        assert_eq!(entries[0].error_message, Some("test failure".to_string()));
        // file_size=0 → None
        assert_eq!(entries[0].file_size, None);
        // file_path 失败时不写入
        assert_eq!(entries[0].file_path, None);
    }

    #[test]
    fn platform_from_url_returns_correct_platform() {
        assert_eq!(
            platform_from_url("https://tiktok.com/v/1"),
            crate::types::Platform::TikTok
        );
        assert_eq!(
            platform_from_url("https://bilibili.com/video/BV1"),
            crate::types::Platform::Bilibili
        );
        assert_eq!(
            platform_from_url("not a url"),
            crate::types::Platform::Other
        );
    }
}
