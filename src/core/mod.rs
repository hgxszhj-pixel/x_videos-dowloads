//! 核心功能模块
//!
//! 提供下载、混流、字幕、弹幕、队列等核心功能
//!
//! 架构:
//! - downloader - 视频下载引擎
//! - muxer - 音视频混流 (FFmpeg)
//! - subtitle - 字幕处理
//! - danmaku - 弹幕处理
//! - queue - 下载队列管理

#![allow(unused_imports)]

// Re-export from main downloader
pub use crate::downloader::{VideoDownloader, DownloaderConfig};
pub use crate::types::DownloadResult;

// Re-export muxer, subtitle, danmaku, and queue
pub use crate::muxer::{mux, mux_with_quality, check_ffmpeg, get_media_info, MediaInfo, MuxerConfig};
pub use crate::subtitle::{Subtitle, SubtitleFormat, SubtitleEntry, vtt_to_srt, download_subtitle, save_subtitle};
pub use crate::danmaku::{Danmaku, DanmakuEntry, DanmakuFormat, download_danmaku, parse_xml_danmaku, danmaku_to_ass, save_danmaku};
pub use crate::queue::{DownloadQueue, QueueItem, QueueItemStatus, QueueStats};
