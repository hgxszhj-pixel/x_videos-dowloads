//! 子命令模块
//!
//! - download: 视频下载主流程
//! - history:  下载历史与书签管理
//! - serve:    协作服务器与团队管理

pub mod analyze;
pub mod clean;
pub mod download;
pub mod history;
pub mod history_stats;
pub mod installer;
pub mod path;
#[cfg(feature = "collaboration")]
pub mod serve;
pub mod status;
