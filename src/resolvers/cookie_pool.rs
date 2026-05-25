use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{Duration, Instant};

/// A single cookie entry with metadata for LRU eviction
#[allow(dead_code)]
pub struct CookieEntry {
    /// The cookie key-value pairs
    pub cookies: HashMap<String, String>,
    /// When this entry was created
    pub created_at: Instant,
    /// When this entry was last used
    pub last_used: Instant,
    /// Number of times this entry has been retrieved
    pub hit_count: u32,
    /// Associated account name
    pub account: String,
    /// Whether this entry is still valid
    pub valid: bool,
}

impl CookieEntry {
    pub fn new(cookies: HashMap<String, String>, account: String) -> Self {
        let now = Instant::now();
        Self {
            cookies,
            created_at: now,
            last_used: now,
            hit_count: 0,
            account,
            valid: true,
        }
    }
}

/// Thread-safe cookie pool with LRU eviction and TTL support
#[allow(dead_code)]
pub struct CookiePool {
    entries: RwLock<Vec<CookieEntry>>,
    /// Time-to-live for cookie entries
    ttl: Duration,
    /// Maximum hits before a cookie is considered exhausted
    max_hits_per_cookie: u32,
    /// Index: account -> list of entry indices (for O(1) invalidation)
    account_index: RwLock<HashMap<String, Vec<usize>>>,
}

impl CookiePool {
    /// Create a new CookiePool
    ///
    /// # Arguments
    /// * `ttl_hours` - Time-to-live in hours for cookie entries
    /// * `max_hits` - Maximum number of times a cookie can be retrieved before being invalidated
    #[allow(dead_code)]
    pub fn new(ttl_hours: u64, max_hits: u32) -> Self {
        Self {
            entries: RwLock::new(Vec::new()),
            ttl: Duration::from_secs(ttl_hours * 3600),
            max_hits_per_cookie: max_hits,
            account_index: RwLock::new(HashMap::new()),
        }
    }

    /// Add a new cookie entry to the pool
    ///
    /// # Arguments
    /// * `cookies` - The cookie key-value pairs
    /// * `account` - The account identifier
    #[allow(dead_code)]
    pub fn add(&self, cookies: HashMap<String, String>, account: String) {
        let entry = CookieEntry::new(cookies, account.clone());
        let mut idx = 0usize;
        if let Ok(mut entries) = self.entries.write() {
            idx = entries.len();
            entries.push(entry);
        }
        if let Ok(mut index) = self.account_index.write() {
            index.entry(account).or_insert_with(Vec::new).push(idx);
        }
    }

    /// Get a valid cookie entry using LRU strategy
    ///
    /// Returns the cookies if a valid entry exists, None otherwise.
    /// Updates the last_used timestamp and increments hit_count.
    pub fn get(&self) -> Option<HashMap<String, String>> {
        let mut entries = self.entries.write().ok()?;

        // Find the oldest valid entry
        let now = Instant::now();
        let mut oldest_idx: Option<usize> = None;
        let mut oldest_time: Option<Instant> = None;
        let mut oldest_created: Option<Instant> = None;

        for (i, entry) in entries.iter_mut().enumerate() {
            if !entry.valid {
                continue;
            }

            // Check TTL expiration
            if now.duration_since(entry.created_at) > self.ttl {
                entry.valid = false;
                continue;
            }

            // Check hit count limit
            if entry.hit_count >= self.max_hits_per_cookie {
                entry.valid = false;
                continue;
            }

            // Select the oldest by last_used (LRU)
            // When last_used is equal, prefer the newer entry (created later)
            let dominated = match (oldest_time, oldest_created) {
                (Some(ot), Some(oc)) => {
                    entry.last_used < ot || (entry.last_used == ot && entry.created_at > oc)
                }
                (None, None) => oldest_idx.is_none(),  // Only dominate if we haven't selected yet
                _ => false,
            };
            if dominated {
                oldest_idx = Some(i);
                oldest_time = Some(entry.last_used);
                oldest_created = Some(entry.created_at);
            }
        }

        // Return the oldest valid entry
        if let Some(idx) = oldest_idx {
            let entry = &mut entries[idx];
            entry.last_used = now;
            entry.hit_count += 1;

            // Immediately invalidate if exhausted
            if entry.hit_count >= self.max_hits_per_cookie {
                entry.valid = false;
            }

            Some(entry.cookies.clone())
        } else {
            None
        }
    }

    /// Invalidate all cookie entries for a specific account
    ///
    /// # Arguments
    /// * `account` - The account identifier to invalidate
    #[allow(dead_code)]
    pub fn invalidate_by_account(&self, account: &str) {
        // O(1) lookup via HashMap index
        if let Ok(index) = self.account_index.read() {
            if let Some(indices) = index.get(account) {
                if let Ok(mut entries) = self.entries.write() {
                    for &idx in indices {
                        if idx < entries.len() {
                            entries[idx].valid = false;
                        }
                    }
                }
            }
        }
    }

    /// Returns the number of entries in the pool (including invalid ones)
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.entries.read().map(|e| e.len()).unwrap_or(0)
    }

    /// Returns true if the pool is empty
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the number of valid entries (respecting TTL and hit count limits)
    #[allow(dead_code)]
    pub fn valid_count(&self) -> usize {
        self.entries
            .read()
            .map(|entries| {
                entries
                    .iter()
                    .filter(|e| {
                        e.valid
                            && e.created_at.elapsed() <= self.ttl
                            && e.hit_count < self.max_hits_per_cookie
                    })
                    .count()
            })
            .unwrap_or(0)
    }

    /// Cleanup expired and invalid entries to free memory
    #[allow(dead_code)]
    pub fn cleanup(&self) {
        if let Ok(mut entries) = self.entries.write() {
            entries.retain(|e| e.valid);
        }
        // Rebuild index after cleanup (simpler than incremental update)
        if let Ok(mut index) = self.account_index.write() {
            index.clear();
            if let Ok(entries) = self.entries.read() {
                for (i, entry) in entries.iter().enumerate() {
                    index.entry(entry.account.clone()).or_insert_with(Vec::new).push(i);
                }
            }
        }
    }
}

impl Default for CookiePool {
    fn default() -> Self {
        Self::new(4, 100)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration as StdDuration;

    #[test]
    fn test_cookie_entry_creation() {
        let mut cookies = HashMap::new();
        cookies.insert("session".to_string(), "abc123".to_string());
        cookies.insert("user".to_string(), "test".to_string());

        let entry = CookieEntry::new(cookies.clone(), "test_account".to_string());

        assert_eq!(entry.cookies, cookies);
        assert_eq!(entry.account, "test_account");
        assert!(entry.valid);
        assert_eq!(entry.hit_count, 0);
    }

    #[test]
    fn test_cookie_pool_new() {
        let pool = CookiePool::new(4, 100);
        assert!(pool.is_empty());
        assert_eq!(pool.len(), 0);
    }

    #[test]
    fn test_cookie_pool_add() {
        let pool = CookiePool::new(4, 100);

        let mut cookies = HashMap::new();
        cookies.insert("session".to_string(), "abc123".to_string());

        pool.add(cookies.clone(), "account1".to_string());

        assert!(!pool.is_empty());
        assert_eq!(pool.len(), 1);
        assert_eq!(pool.valid_count(), 1);
    }

    #[test]
    fn test_cookie_pool_get_lru() {
        let pool = CookiePool::new(4, 100);

        let mut cookies1 = HashMap::new();
        cookies1.insert("session".to_string(), "cookie1".to_string());

        let mut cookies2 = HashMap::new();
        cookies2.insert("session".to_string(), "cookie2".to_string());

        pool.add(cookies1.clone(), "account1".to_string());
        thread::sleep(StdDuration::from_millis(10));
        pool.add(cookies2.clone(), "account2".to_string());

        // First get should return the oldest (cookie1)
        let retrieved = pool.get();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().get("session").unwrap(), "cookie1");

        // Second get should return cookie2 (now it's the oldest)
        let retrieved = pool.get();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().get("session").unwrap(), "cookie2");
    }

    #[test]
    fn test_cookie_pool_get_updates_metadata() {
        let pool = CookiePool::new(4, 100);

        let mut cookies = HashMap::new();
        cookies.insert("session".to_string(), "test".to_string());

        pool.add(cookies.clone(), "account1".to_string());

        // Get should update last_used and hit_count
        let retrieved = pool.get();
        assert!(retrieved.is_some());

        let entries = pool.entries.read().unwrap();
        let entry = entries.first().unwrap();
        assert_eq!(entry.hit_count, 1);
    }

    #[test]
    fn test_cookie_pool_get_returns_none_when_empty() {
        let pool = CookiePool::new(4, 100);
        let result = pool.get();
        assert!(result.is_none());
    }

    #[test]
    fn test_cookie_pool_max_hits() {
        let pool = CookiePool::new(4, 3); // max 3 hits

        let mut cookies = HashMap::new();
        cookies.insert("session".to_string(), "test".to_string());

        pool.add(cookies.clone(), "account1".to_string());

        // First 3 gets should succeed
        assert!(pool.get().is_some());
        assert!(pool.get().is_some());
        assert!(pool.get().is_some());

        // 4th get should fail (exhausted)
        assert!(pool.get().is_none());
    }

    #[test]
    fn test_cookie_pool_invalidate_by_account() {
        let pool = CookiePool::new(4, 100);

        let mut cookies1 = HashMap::new();
        cookies1.insert("session".to_string(), "cookie1".to_string());

        let mut cookies2 = HashMap::new();
        cookies2.insert("session".to_string(), "cookie2".to_string());

        pool.add(cookies1.clone(), "account1".to_string());
        pool.add(cookies2.clone(), "account2".to_string());

        assert_eq!(pool.valid_count(), 2);

        pool.invalidate_by_account("account1");

        assert_eq!(pool.valid_count(), 1);

        // account1's cookie should be invalid now
        let entries = pool.entries.read().unwrap();
        assert!(!entries[0].valid);
        assert!(entries[1].valid);
    }

    #[test]
    fn test_cookie_pool_invalidate_nonexistent_account() {
        let pool = CookiePool::new(4, 100);

        let mut cookies = HashMap::new();
        cookies.insert("session".to_string(), "test".to_string());

        pool.add(cookies.clone(), "account1".to_string());

        // Invalidating non-existent account should not affect existing entries
        pool.invalidate_by_account("nonexistent");

        assert_eq!(pool.valid_count(), 1);
    }

    #[test]
    fn test_cookie_pool_default() {
        let pool = CookiePool::default();
        assert!(pool.is_empty());

        let mut cookies = HashMap::new();
        cookies.insert("session".to_string(), "test".to_string());

        pool.add(cookies.clone(), "account1".to_string());
        assert_eq!(pool.len(), 1);
    }

    #[test]
    fn test_cookie_pool_cleanup() {
        let pool = CookiePool::new(4, 1);

        let mut cookies1 = HashMap::new();
        cookies1.insert("session".to_string(), "cookie1".to_string());

        let mut cookies2 = HashMap::new();
        cookies2.insert("session".to_string(), "cookie2".to_string());

        pool.add(cookies1.clone(), "account1".to_string());
        pool.add(cookies2.clone(), "account2".to_string());

        // Exhaust all cookies
        let first = pool.get();
        let second = pool.get();
        assert!(first.is_some(), "first get should succeed");
        assert!(second.is_some(), "second get should succeed");

        let _len_before = pool.len();
        let _valid_before = pool.valid_count();

        pool.cleanup();

        // After cleanup, invalid entries should be removed
        let len_after = pool.len();
        assert_eq!(len_after, 0, "after cleanup len should be 0, got {}", len_after);
    }

    #[test]
    fn test_cookie_pool_concurrent_access() {
        use std::sync::Arc;

        let pool = Arc::new(CookiePool::new(4, 100));

        let mut cookies = HashMap::new();
        cookies.insert("session".to_string(), "test".to_string());

        pool.add(cookies.clone(), "account1".to_string());

        // Multiple threads can read simultaneously
        let pool_clone = Arc::clone(&pool);
        let handles: Vec<_> = (0..4)
            .map(|_| {
                let p = Arc::clone(&pool_clone);
                thread::spawn(move || {
                    let _ = p.get();
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        // Main thread can also access
        let _ = pool.get();
    }

    #[test]
    fn test_cookie_pool_clone_cookies() {
        let pool = CookiePool::new(4, 100);

        let mut cookies = HashMap::new();
        cookies.insert("session".to_string(), "original".to_string());

        pool.add(cookies.clone(), "account1".to_string());

        // Modify original cookies after adding
        cookies.insert("session".to_string(), "modified".to_string());

        // Get should return original, not modified
        let retrieved = pool.get();
        assert_eq!(retrieved.unwrap().get("session").unwrap(), "original");
    }

    #[test]
    fn test_cookie_pool_multiple_accounts_same_session() {
        let pool = CookiePool::new(4, 100);

        let mut cookies1 = HashMap::new();
        cookies1.insert("session".to_string(), "session1".to_string());

        let mut cookies2 = HashMap::new();
        cookies2.insert("session".to_string(), "session2".to_string());

        pool.add(cookies1.clone(), "account1".to_string());
        pool.add(cookies2.clone(), "account1".to_string()); // Same account

        // Both should be retrievable
        let retrieved1 = pool.get();
        let retrieved2 = pool.get();
        assert!(retrieved1.is_some());
        assert!(retrieved2.is_some());
    }
}
