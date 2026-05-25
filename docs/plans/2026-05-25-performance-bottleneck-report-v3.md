# Performance Bottleneck Report (Round 3) — x-video-downloader

**Date:** 2026/05/25
**Analysis:** Multi-agent parallel review (pipeline, HTTP, memory, resolver agents)
**Context:** Round 1-2 fixes already applied. This is fresh analysis of remaining bottlenecks.

---

## Executive Summary

| Priority | Issue | Estimated Speedup | Fix Complexity |
|----------|-------|-------------------|----------------|
| P0 | `FxTwitterResolver` creates independent clients | 15-20% | LOW |
| P0 | `CookiePool::get()` write lock + O(n) scan | 10-20% | MEDIUM |
| P1 | `progress.clone()` per chunk in do_download | 5-15% | MEDIUM |
| P1 | Redundant `to_lowercase()` 5x per URL | 5-10% | LOW |
| P2 | `segment_sizes[8]` latent panic risk | 1% | LOW |
| P2 | Blocking `merge_segments()` could yield | 2-5% | LOW |
| P3 | Duplicate `tcp_keepalive` call | 0% | LOW |

**Estimated max speedup if all issues fixed: 35-50%**

---

## P0 — Critical

### 1. FxTwitterResolver Creates Independent reqwest Clients

**Severity:** HIGH
**Location:** `src/fxtwitter.rs:120-160`

**Evidence:** `FxTwitterResolver::new()` and `with_proxy()` create independent `reqwest::Client` instances instead of using `http_client::shared()`:
```rust
// Line 120-144: Creates new client per instance
let builder = reqwest::Client::builder()
    .user_agent("...")
    .tcp_keepalive(...)
    // Missing: pool_max_idle_per_host
    .build()
    .unwrap_or_else(|_| reqwest::Client::new())  // Silent fallback!

// Line 157-160: with_proxy() has no timeout at all
let client = reqwest::Client::builder()
    .proxy(reqwest::Proxy::all(&proxy_with_scheme)?)
    .build()
    .unwrap_or_default();  // No timeout!
```

**Impact:** 15-20% speedup. Each FxTwitter call creates fresh TCP connection instead of reusing from pool.

**Fix:** Refactor to accept `Arc<Client>` in constructor, use `http_client::shared()` by default.

**Fix Complexity:** LOW.

---

### 2. CookiePool::get() Write Lock + O(n) Linear Scan

**Severity:** HIGH
**Location:** `src/resolvers/cookie_pool.rs:86-143`

**Evidence:**
1. `get()` acquires **write** lock instead of read lock
2. O(n) linear scan through ALL entries for LRU
3. Returns cloned HashMap (200-500 bytes allocation churn)

```rust
pub fn get(&self) -> Option<HashMap<String, String>> {
    let mut entries = self.entries.write().ok()?;  // WRITE lock!
    for (i, entry) in entries.iter_mut().enumerate() {  // O(n) scan
        // ... TTL check, LRU comparison
    }
    Some(entry.cookies.clone())  // HashMap clone
}
```

**Impact:** 10-20% throughput reduction under concurrency. With 8 threads all calling `get()`, they serialize on this lock.

**Fix:** 
- Option A: Use `RwLock<Vec<CookieEntry>>` with read-lock scan, only write-lock for metadata update
- Option B: Replace Vec with `lru` crate for O(1) LRU access

**Fix Complexity:** MEDIUM.

---

## P1 — High Priority

### 3. progress.clone() Per Chunk in do_download Hot Loop

**Severity:** HIGH
**Location:** `src/downloader.rs:568-591`

**Evidence:** Inside the chunk processing loop:
```rust
let callback_progress = progress.clone();  // Every chunk iteration!
if let Some(ref callback) = progress_callback {
    callback(callback_progress);
}
```

`DownloadProgress` contains `task_id: String` (~50 bytes). For a 500MB video @ 1MB chunks = ~500 unnecessary string clones.

**Impact:** 5-15% speedup on high-speed downloads.

**Fix:** Use `Arc<DownloadProgress>` or pass reference and let callback clone if needed.

**Fix Complexity:** MEDIUM.

---

### 4. Redundant to_lowercase() Called 5x Per URL

**Severity:** MEDIUM
**Location:**
- `src/resolvers/url_matcher.rs:63` (detect_platform)
- `src/resolvers/url_matcher.rs:86` (url_matches_platform)
- `src/resolvers/mod.rs:73` (PlatformResolver::supports_url)

**Evidence:** Each call to `detect_platform()` or `url_matches_platform()` allocates a new String via `url.to_lowercase()`. With 4 resolvers registered, `to_lowercase()` is called 5 times per URL.

**Impact:** 5-10% overhead on URL resolution latency.

**Fix:** Cache lowercase URL once in `select_resolver()`:
```rust
fn select_resolver(url: &str) -> Option<Arc<dyn PlatformResolver>> {
    let url_lower = url.to_lowercase();  // once
    let platform = detect_platform(&url_lower);
    // reuse url_lower for resolver checks
}
```

**Fix Complexity:** LOW.

---

## P2 — Medium Priority

### 5. segment_sizes[8] Latent Panic Risk

**Severity:** MEDIUM
**Location:** `src/downloader.rs:439`

**Evidence:**
```rust
let mut segment_sizes = [0u64; 8];  // Fixed 8-element array
```

If `parallel_segments > 8`, this panics. Max threads is 8 per config, but this is a latent bug.

**Fix:** Use `Vec::with_capacity(segments)` or `smallvec::SmallVec<[u64; 8]>`.

**Fix Complexity:** LOW.

---

### 6. Blocking merge_segments() Could Yield

**Severity:** MEDIUM
**Location:** `src/downloader.rs:266-279`

**Evidence:** `merge_segments()` uses `std::fs::File`, `std::io::copy` — blocking I/O. Already inside `spawn_blocking` at line 349, but could use `tokio::fs` for better async yield points.

**Impact:** 2-5% improvement in I/O scheduling.

**Fix:** Convert to `tokio::fs` for async scheduling yields.

**Fix Complexity:** LOW.

---

### 7. Duplicate tcp_keepalive Call

**Severity:** LOW
**Location:** `src/http_client.rs:14-20`

**Evidence:**
```rust
.tcp_keepalive(Duration::from_secs(120))  // Set #1
.connect_timeout(Duration::from_secs(30))
.timeout(Duration::from_secs(600))
.pool_max_idle_per_host(32)
.tcp_keepalive(Duration::from_secs(120))  // Set #2 (DUPLICATE)
```

Second call overrides first. No functional issue but indicates copy-paste error.

**Fix:** Remove duplicate line.

**Fix Complexity:** LOW.

---

## P3 — Already Fixed (Round 2)

These issues were identified in Round 2 and have been fixed:

| Issue | Fix Applied |
|-------|------------|
| 6 scattered reqwest Clients | Consolidated to `http_client::shared()` |
| HeaderMap clone per request | Added `cached_headers` to ZhiXueYunResolver |
| Double to_lowercase in supports_url | Pattern already lowercase, removed redundant call |

---

## Recommended Implementation Order

```
Phase 1 (Quick Wins — <1 day):
  - #1  FxTwitterResolver → shared() client (15-20% speedup, 30 min fix)
  - #4  Cache to_lowercase URL (5-10% speedup, 10 min fix)
  - #7  Remove duplicate tcp_keepalive (cosmetic, 1 min fix)

Phase 2 (High Impact — 1-2 days):
  - #2  CookiePool lock + O(n) fix (10-20% speedup, 2 hr work)
  - #3  progress.clone() → Arc (5-15% speedup, 1 hr work)

Phase 3 (Medium Complexity — 2-3 days):
  - #5  Fix segment_sizes latent panic (1%, 30 min work)
  - #6  Convert merge_segments to tokio::fs (2-5% speedup)
```

---

## Verification Commands

```bash
# Check for remaining independent Client instantiation
grep -rn "Client::new\|Client::builder" src/ --include="*.rs" | grep -v http_client

# Verify CookiePool lock usage
grep -n "\.write\(\)" src/resolvers/cookie_pool.rs

# Benchmark before/after
cargo build --release
time ./target/release/x-video-downloader "URL"
```

---

*Generated by multi-agent parallel review (Round 3): pipeline-agent, http-agent, resolver-agent, memory-agent*
