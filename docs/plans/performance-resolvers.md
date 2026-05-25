# Resolver Performance Analysis Report

**Date:** 2026-05-25
**Focus Areas:** URL parsing, Header construction, Token retrieval, Serialization

---

## Finding 1: Redundant to_lowercase() in URL Matching

**Severity:** MEDIUM
**Location:**
- `src/resolvers/url_matcher.rs:63` (detect_platform)
- `src/resolvers/url_matcher.rs:86` (url_matches_platform)
- `src/resolvers/mod.rs:73` (PlatformResolver::supports_url default impl)

**Description:**
Each call to `detect_platform()` or `url_matches_platform()` allocates a new String via `url.to_lowercase()`. The default `supports_url()` implementation in `PlatformResolver` trait (line 73) also calls `to_lowercase()` for every resolver in the registry during `select_resolver()`.

**Impact:**
For a single URL resolution with 4 resolvers registered, `to_lowercase()` is called 5 times (1 for each `supports_url` check + 1 for the final `detect_platform` if used directly).

**Estimated Impact:** ~5-10% overhead on URL resolution latency

**Fix Complexity:** LOW - Use `url.contains()` with case-insensitive pattern or cache the lowercased URL.

---

## Finding 2: HeaderMap Clone Per Request in ZhiXueYunResolver

**Severity:** MEDIUM
**Location:** `src/resolvers/zhixueyun.rs:151-171` (get_auth_headers)

**Description:**
`get_auth_headers()` returns a cloned `HeaderMap` on every call (line 156: `h.clone()`). Even though there's a cache mechanism (`cached_headers`), the cache stores the base headers without auth, so each call with a token still clones.

```rust
let base_headers = {
    let cached = self.cached_headers.read().unwrap();
    if let Some(ref h) = *cached {
        h.clone()  // Clone happens here
    } else {
        // ...
    }
};
let mut headers = base_headers;  // Another allocation
```

**Impact:**
For batch downloads calling `get_auth_headers()` per file, this creates unnecessary HeaderMap allocations.

**Estimated Impact:** ~3-5% overhead per HTTP request

**Fix Complexity:** MEDIUM - Clone only when token is present; consider using `HeaderMap::from_entries` or Cow

---

## Finding 3: Lock Contention in CookiePool::get()

**Severity:** MEDIUM
**Location:** `src/resolvers/cookie_pool.rs:86-143`

**Description:**
`CookiePool::get()` acquires a **write** lock on `entries` (line 87), not a read lock. This serializes all concurrent access even for read-only operations.

```rust
pub fn get(&self) -> Option<HashMap<String, String>> {
    let mut entries = self.entries.write().ok()?;  // WRITE lock, not read!
```

This design choice updates `last_used` and `hit_count` in-place, requiring mutability. However, it means concurrent `get()` calls are serialized.

**Impact:**
High contention under concurrent multi-file downloads from same platform.

**Estimated Impact:** ~10-20% throughput reduction under concurrency

**Fix Complexity:** MEDIUM - Use atomic operations for hit_count and maintain a separate atomic timestamp for LRU

---

## Finding 4: Token Retrieval Unnecessary Clones

**Severity:** LOW
**Location:** `src/resolvers/token_manager.rs:108-117`, `130-142`, `160-166`

**Description:**
Methods like `get_tiktok_tokens()` and `get_bilibili_cookies()` clone strings when inserting into the HashMap:

```rust
pub fn get_tiktok_tokens(&self) -> HashMap<String, String> {
    let mut tokens = HashMap::new();
    if let Some(ref t) = self.tokens.tiktok_ms_token {
        tokens.insert("msToken".to_string(), t.clone());  // Clone here
    }
```

This is intentional (cookies may outlive the source), but creates allocation overhead per call.

**Impact:**
Minimal - only called during resolver initialization or token refresh

**Estimated Impact:** ~1% overall

**Fix Complexity:** LOW - Return references or use `Arc<str>` if tokens are frequently accessed

---

## Finding 5: DouYinApiClient Cookie Construction Overhead

**Severity:** LOW
**Location:** `src/resolvers/douyin_api.rs:81-88`

**Description:**
String capacity pre-allocation is good, but the cookie building is verbose:

```rust
let cookie_len = 8 + token.len(); // "msToken=" (8) + token
let mut cookie = String::with_capacity(cookie_len);
cookie.push_str("msToken=");
cookie.push_str(token);
```

**Impact:**
Negligible - called once per API request, minor allocation

**Estimated Impact:** <1%

**Fix Complexity:** LOW - Use `format!("msToken={}", token)` or `format!("msToken={token}")`

---

## Finding 6: Missing Static Caching in url_matcher

**Severity:** LOW
**Location:** `src/resolvers/url_matcher.rs:16-53`

**Description:**
`URL_PATTERNS` is already a static const, which is good. Pattern matching uses simple `contains()` which is O(n*m) but acceptable for small pattern lists.

**Impact:**
Acceptable for current scale

---

## Summary Table

| Finding | Severity | Location | Est. Impact | Fix Complexity |
|---------|----------|----------|-------------|----------------|
| Redundant to_lowercase() | MEDIUM | url_matcher.rs:63,86; mod.rs:73 | 5-10% | LOW |
| HeaderMap clone per request | MEDIUM | zhixueyun.rs:151-171 | 3-5% | MEDIUM |
| CookiePool write lock contention | MEDIUM | cookie_pool.rs:86-143 | 10-20% | MEDIUM |
| Token retrieval clones | LOW | token_manager.rs:108-166 | 1% | LOW |
| Cookie string building | LOW | douyin_api.rs:81-88 | <1% | LOW |

---

## Recommended Fixes (Priority Order)

1. **P0 - CookiePool lock contention**: Change `get()` to use atomic counters and avoid write lock
2. **P1 - to_lowercase caching**: Cache lowercased URL in `select_resolver()` to avoid repeated calls
3. **P2 - HeaderMap optimization**: Only clone when token differs; use `HeaderMap::from_iter` for efficient construction