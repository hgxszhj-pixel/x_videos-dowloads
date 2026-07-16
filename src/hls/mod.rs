//! HLS (m3u8) 解析和下载模块
//!
//! 不依赖 yt-dlp 的原生 HLS 流处理，支持 AES-128 解密和并发分段下载

mod parser;
mod downloader;
mod crypto;

#[allow(unused_imports)] // bench 编译上下文看不到 nnyy.rs 消费方，lib 实际使用
pub use parser::M3u8Parser;
pub use downloader::HlsDownloader;
