//! 核心下载原语模块
//!
//! 只包含下载引擎依赖的底层原语，不 re-export 上层模块，
//! 避免与 `downloader` / `muxer` / `subtitle` / `danmaku` / `queue`
//! 形成隐式循环依赖。需要这些类型时直接从各自模块导入。
//!
//! - constants - 下载器常量配置
//! - cache     - HEAD 请求缓存
//! - progress  - 进度跟踪
//! - segment   - 分段下载实现

pub mod cache;
pub mod constants;
pub mod progress;
pub mod segment;
pub mod trace;
