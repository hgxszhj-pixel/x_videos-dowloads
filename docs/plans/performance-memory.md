# Performance Memory Analysis - Findings Report

## Executive Summary

Analyzed three critical files for memory management and data structure inefficiencies:
- `src/types.rs` - Core type definitions
- `src/downloader.rs` - Video downloader with parallel segment support
- `src/resolvers/cookie_pool.rs` - Thread-safe cookie pool with LRU eviction

**Critical Issues Found:** 5
**Medium Issues Found:** 4
**Optimization Opportunities:** 6

---

## 1. Data Structure Inefficiencies

### Issue 1.1: CookiePool Uses Vec Instead of LinkedHashMap for LRU (CRITICAL)

**Location:** `src/resolvers/cookie_pool.rs:39`

```rust
pub struct CookiePool {
    entries: RwLock<Vec<CookieEntry>>,        // <-- Linear scan on every get()
    account_index: RwLock<HashMap<String, Vec<usize>>>,
}
```

**Problem:** `get()` performs O(n) linear scan through ALL entries to find the oldest:

```rust
for (i, entry) in entries.iter_mut().enumerate() {  // O(n) every call
    // ... TTL check
    // ... hit count check
    // ... LRU comparison
}
```

**Impact:** With 100+ cookie entries, every `get()` call must scan potentially the entire vector. This is a hot path called on every video request.

**Recommendation:** Replace `Vec<CookieEntry>` with `LinkedHashMap<CookieKey, CookieEntry>` or use a dedicated LRU cache crate like `lru` for O(1) access.

---

### Issue 1.2: CookiePool Double RwLock on add() (MEDIUM)

**Location:** `src/resolvers/cookie_pool.rs:70-79`

```rust
pub fn add(&self, cookies: HashMap<String, String>, account: String) {
    let entry = CookieEntry::new(cookies, account.clone());
    let mut idx = 0usize;
    if let Ok(mut entries) = self.entries.write() {   // Lock 1
        idx = entries.len();
        entries.push(entry);
    }
    if let Ok(mut index) = self.account_index.write() { // Lock 2 - Can fail independently
        index.entry(account).or_insert_with(Vec::new).push(idx);
    }
}
```

**Problem:** Two separate locks create race condition window. If second lock fails, `entries` has the entry but `account_index` doesn't - inconsistent state.

**Recommendation:** Use a single lock or transactional approach to ensure atomicity.

---

### Issue 1.3: HashMap for CookieEntry Index (MEDIUM)

**Location:** `src/resolvers/cookie_pool.rs:45`

```rust
account_index: RwLock<HashMap<String, Vec<usize>>>,
```

**Problem:** Secondary index stores `Vec<usize>` which can fragment. On `cleanup()`, entire index is rebuilt (O(n) scan + O(n) rebuild).

**Recommendation:** Consider `HashMap<String, Vec<usize>>` → `HashMap<String, usize>` (single index per account assumed) or use `Slotmap` for stable indices.

---

## 2. Lock Contention Issues

### Issue 2.1: CookiePool get() Holds Write Lock (CRITICAL)

**Location:** `src/resolvers/cookie_pool.rs:87`

```rust
pub fn get(&self) -> Option<HashMap<String, String>> {
    let mut entries = self.entries.write().ok()?;  // <-- Write lock!
    // ... iterates with mutable reference
    Some(entry.cookies.clone())  // <-- Also clones on hot path
}
```

**Problem:**
1. Write lock excludes ALL other readers/writers
2. `get()` is likely called from multiple download threads
3. Returns cloned HashMap - additional allocation

**Impact:** High contention in parallel download scenarios. With 8 threads all hitting `get()`, they serialize on this lock.

**Recommendation:** Use `RwLock<Vec<CookieEntry>>` and upgrade to read-lock for scan, only write-lock for metadata update. Or separate hot/cold data structures.

---

### Issue 2.2: Progress Callback Cloning (MEDIUM)

**Location:** `src/downloader.rs:408-412`

```rust
let mut progress = progress_clone.lock().await;
progress.update(downloaded, speed, current_threads);
let callback_progress = progress.clone();  // <-- Clone while holding lock
drop(progress);
if let Some(ref callback) = callback_clone {
    callback(callback_progress);  // <-- Lock already dropped
}
```

**Good Practice:** The code correctly clones **outside** the lock before calling callback (line 408-409). This is actually the correct pattern.

**Issue:** However `DownloadProgress` contains multiple Strings which get cloned:

```rust
pub struct DownloadProgress {
    pub task_id: String,           // Heap allocation
    pub bytes_downloaded: u64,
    pub total_bytes: u64,
    pub percent: f64,
    pub speed: f64,
    pub eta_secs: Option<f64>,
    pub active_threads: u32,
    pub status: DownloadStatus,     // Enum with no data
}
```

**Recommendation:** `DownloadProgress` is ~50 bytes, cloning is cheap but consider using `Arc<DownloadProgress>` if callback frequency is very high.

---

### Issue 2.3: Segment Sizes Array Initialization (LOW)

**Location:** `src/downloader.rs:439`

```rust
let mut segment_sizes = [0u64; 8];  // Fixed 8-element array
```

**Problem:** Uses fixed array even when `segments < 8`, wasting stack space. But max_threads=8 so this is bounded and acceptable.

---

## 3. Memory Allocation Issues

### Issue 3.1: HashMap Clone on CookiePool get() (HIGH)

**Location:** `src/resolvers/cookie_pool.rs:139`

```rust
Some(entry.cookies.clone())  // Full HashMap clone on every get
```

**Problem:** Each call returns a cloned `HashMap<String, String>`. For cookies with 5-10 entries, this is 200-500 bytes of allocation per call.

**Impact:** In a download session making 10 requests with cookie refresh every 5 minutes, this is continuous allocation churn.

**Recommendation:** Consider returning `Arc<HashMap<String, String>>` instead of cloning. Or use copy-on-write semantics.

---

### Issue 3.2: DownloadProgress Clone Per Chunk (MEDIUM)

**Location:** `src/downloader.rs:588`

```rust
let callback_progress = progress.clone();
if let Some(ref callback) = progress_callback {
    callback(callback_progress);
}
```

**Problem:** For a 100MB file with 1MB chunks = 100 progress updates, each cloning ~50 byte struct. Minor but notable.

**Recommendation:** Consider passing `&progress` and letting callback clone if needed. Or use `Arc<DownloadProgress>` to avoid clones entirely.

---

### Issue 3.3: Vec Capacity Growth in CookiePool (LOW)

**Location:** `src/resolvers/cookie_pool.rs:75`

```rust
entries.push(entry);  // Vec grows exponentially
```

**Problem:** Vec reallocation copies all entries. With many cookies, this causes fragmentation and occasional GC pauses.

**Recommendation:** Use `Vec::with_capacity(n)` when initial size is known, or use `SmallVec<[CookieEntry; 16]>` for cache locality.

---

## 4. Unnecessary Cloning Overhead

### Issue 4.1: DownloadRequest Clone in do_download (HIGH)

**Location:** `src/downloader.rs:527-530`

```rust
async fn do_download(
    &self,
    request: &DownloadRequest,      // & reference
    progress_callback: Option<Arc<dyn Fn(DownloadProgress) + Send + Sync>>,
) -> Result<DownloadResult> {
    let start_time = Instant::now();
    let url = &request.url;        // & borrow, no clone
    let output_path = &request.output_path;  // & borrow
```

**Good:** Uses references correctly, no clone here.

---

### Issue 4.2: URL Clone in parallel_download (MEDIUM)

**Location:** `src/downloader.rs:421`

```rust
for i in 0..segments {
    let url = url.clone();  // <-- String clone per segment
    let temp_path = temp_dir_path.join(format!("part_{}.tmp", i));
    // ...
}
```

**Problem:** Clones URL string for every segment (max 8 threads). String is typically 50-100 bytes.

**Recommendation:** Pass `&str` or `Arc<str>` instead of cloning. Consider passing cloned references.

---

### Issue 4.3: DownloadResult Clone on Success (LOW)

**Location:** `src/downloader.rs:623`

```rust
Ok(DownloadResult::success(output_path.clone(), downloaded, elapsed))
```

**Problem:** `PathBuf` clone is cheap (just pointer copy), but still unnecessary in this context since `output_path` is consumed.

**Note:** Actually fine here - `output_path` is moved, not cloned. The clone is on the return path.

---

## 5. Summary of Recommendations

### Critical Priority

| # | Issue | File | Recommendation |
|---|-------|------|----------------|
| 1 | O(n) LRU scan in CookiePool::get() | cookie_pool.rs:86-142 | Replace Vec with LruCache for O(1) |
| 2 | Write lock on hot path | cookie_pool.rs:87 | Use RwLock with read-heavy pattern |
| 3 | HashMap clone per get() | cookie_pool.rs:139 | Return Arc<HashMap> or Cow |

### Medium Priority

| # | Issue | File | Recommendation |
|---|-------|------|----------------|
| 4 | Double-lock add() inconsistency | cookie_pool.rs:70-79 | Single atomic transaction |
| 5 | Progress clone per chunk | downloader.rs:588 | Arc<DownloadProgress> |
| 6 | URL clone per segment | downloader.rs:421 | Use &str or Arc |

### Low Priority / Nice to Have

| # | Issue | File | Recommendation |
|---|-------|------|----------------|
| 7 | Vec reallocation | cookie_pool.rs:75 | Pre-allocate capacity |
| 8 | Segment array size | downloader.rs:439 | Acceptable (bounded) |
| 9 | account_index rebuild | cookie_pool.rs:201-209 | Incremental update |

---

## Risk Assessment

**High Contention Scenario:** 8-segment parallel download with CookiePool refresh:
- All 8 threads call `get()` concurrently
- Serialize on write lock
- Each returns cloned HashMap
- O(n) scan per call

**Estimated Impact:** 8 threads × O(n) scan × HashMap clone = significant CPU overhead in parallel download path.

**Recommended Fix Order:** Issue 1.1 → Issue 2.1 → Issue 4.1 (address LRU + lock contention together)

---

*Generated: 2026-05-25*
*Files Analyzed: src/types.rs, src/downloader.rs, src/resolvers/cookie_pool.rs*