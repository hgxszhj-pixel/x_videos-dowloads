//! 核心功能模块
//!
//! 提供下载、混流、字幕、弹幕等核心功能
//!
//! 架构:
//! - downloader - 视频下载引擎
//! - muxer - 音视频混流 (FFmpeg)
//! - subtitle - 字幕处理
//! - danmaku - 弹幕处理 (Phase 2.2)

// Re-export from main downloader
pub use crate::downloader::{VideoDownloader, DownloaderConfig};
pub use crate::types::DownloadResult;

// Re-export muxer and subtitle
pub use crate::muxer::{mux, mux_with_quality, check_ffmpeg, get_media_info, MediaInfo, MuxerConfig};
pub use crate::subtitle::{Subtitle, SubtitleFormat, SubtitleEntry, vtt_to_srt, download_subtitle, save_subtitle};
