//! X Video Downloader Library
//!
//! 提供 x.com 视频下载的核心功能

pub mod collaboration;
pub mod downloader;
pub mod gui;
pub mod types;
pub mod yt_dlp;

// ============================================================================
// 代码质量报告 - collaboration 模块分析
// ============================================================================
//
// 1. CLIPPY 检查
//    ✓ 无警告 - cargo clippy -- -W warnings 通过
//
// 2. #[allow(dead_code)] 分析
//    ✗ 问题严重：40+ 处 #[allow(dead_code)] 标记
//
//    高优先级可移除（功能已完整实现）:
//    - src/history.rs:108,151,164,170 - save(), load(), remove(), clear_history()
//    - src/config.rs:137 - reload()
//    - src/types.rs:151,158,165,178,213 - VideoFormat 字段, DownloadRequest 构造
//    - src/collaboration/client/queue.rs:19,33,47,67,81 - LocalQueue 方法
//    - src/collaboration/server/db.rs:233 - Database 方法
//
//    需确认后再移除（可能为预留 API）:
//    - src/collaboration/types.rs:10 - Team 结构体
//    - src/collaboration/client/ws.rs:15,27 - CollaborationClient/WithFileHandler
//    - src/collaboration/client/discovery.rs:6 - DiscoveryService
//    - src/collaboration/transfer/http_server.rs:18 - FileServer
//    - src/collaboration/transfer/downloader.rs:13 - ChunkedDownloader
//
// 3. 错误处理一致性
//    ✓ 良好：统一使用 anyhow::Result
//    - 主要模块: lib.rs, history.rs, config.rs, collaboration/*
//    - 协作模块使用 thiserror 定义特定错误类型 + anyhow::Result
//
// 4. 公共 API 文档
//    △ 一般：模块级文档良好，但子模块公共 API 文档不足
//    - ✓ collaboration/mod.rs: 完整的模块文档
//    - ✗ collaboration/client/ws.rs: CollaborationClient 缺少文档注释
//    - ✗ collaboration/server/handler.rs: MessageHandler 缺少文档注释
//    - △ types.rs: 大部分结构体有文档，但内部字段注释不完整
//
// 5. 内存安全
//    ✓ 优秀：无 unsafe 代码，未使用原始指针
//    ✓ Arc<Mutex<>> 正确用于并发场景
//    ✓ 所有 WebSocket 消息反序列化使用 serde 安全验证
//
// 优化建议:
//
// [高优先级]
// 1. 逐步移除 history.rs 和 config.rs 中已实现但仍标记 dead_code 的方法
// 2. 为 collaboration/client 和 collaboration/server 主要结构体添加文档注释
// 3. 确认 Team, DiscoveryService, FileServer, ChunkedDownloader 是否为预留 API
//
// [中优先级]
// 4. 考虑为 VideoFormat, DownloadRequest 等类型实现 Builder 模式替代多个可选字段
// 5. collaboration/types.rs 中的 ClientMessage/ServerMessage 应添加变体文档
//
// [建议]
// 6. GUI 模块 (gui.rs) 有大量未使用的 Message 变体，建议定期清理
// ============================================================================
