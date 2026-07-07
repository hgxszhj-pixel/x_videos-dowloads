//! HLS (m3u8) 解析和下载模块
//!
//! 不依赖 yt-dlp 的原生 HLS 流处理，支持 AES-128 解密和并发分段下载

mod parser;
mod downloader;
mod crypto;

pub use parser::{M3u8Parser, M3u8Segment, M3u8Playlist, EncryptionMethod};
pub use downloader::{HlsDownloader, HlsDownloaderConfig};
pub use crypto::{AesDecryptor, decrypt_aes128_cbc};
