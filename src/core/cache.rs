//! HEAD 请求缓存
//!
//! 使用 LRU 缓存避免重复 HEAD 请求，提升性能

use lru::LruCache;
use std::sync::LazyLock;
use tokio::sync::Mutex;

/// HEAD 缓存类型
pub type HeadCache = LruCache<String, u64>;

/// 全局 HEAD 缓存实例 - 100 条目，LRU 自动管理
pub static HEAD_CACHE: LazyLock<Mutex<HeadCache>, fn() -> Mutex<HeadCache>> = LazyLock::new(|| {
    Mutex::new(lru::LruCache::new(
        std::num::NonZeroUsize::new(100).expect("常量 100 恒非零"),
    ))
});
