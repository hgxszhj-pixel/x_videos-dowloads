//! 通用工具模块
//!
//! 跨模块共享的纯工具（无业务逻辑依赖）。
//! - `url_matcher` - URL/hostname → Platform 检测（避免 platform ↔ history 循环）

pub mod url_matcher;
