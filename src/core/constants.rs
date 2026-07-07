//! 下载器常量配置
//!
//! 定义分段下载、重试策略等核心常量

/// 并行下载常量
/// 启用并行下载的最小文件大小 (10MB)
pub const SEGMENT_SIZE_THRESHOLD: u64 = 10 * 1024 * 1024;

/// 分段下载独立超时时间 (5分钟)
pub const SEGMENT_TIMEOUT_SECS: u64 = 300;

/// 分段大小验证比例 (90%)
/// 分段实际大小应达到期望大小的至少此比例
pub const SEGMENT_VALIDATION_RATIO: u64 = 90;

/// 断点续传最小下载比例 (80%)
/// 已下载部分达到此比例时才尝试断点续传
pub const RESUME_MIN_DOWNLOAD_RATIO: u64 = 80;

/// 最终文件大小验证比例 (90%)
/// 合并后文件大小应达到期望大小的至少此比例
pub const FILE_SIZE_VALIDATION_RATIO: u64 = 90;
