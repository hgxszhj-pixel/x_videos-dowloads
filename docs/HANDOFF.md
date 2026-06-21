# Handoff Document

## Session: 2026-06-17

---

## 1. Codex DeepSeek Configuration Fix

### Problem
Codex was failing with: `Missing environment variable: ***REDACTED_DEEPSEEK_API_KEY***`

### Root Cause
`config.toml` had `env_key` set to the actual API key value instead of an environment variable name.

### Solution
1. Changed `env_key` from `***REDACTED_DEEPSEEK_API_KEY***` to `DEEPSEEK_API_KEY`
2. Added `export DEEPSEEK_API_KEY="***REDACTED_DEEPSEEK_API_KEY***"` to `~/.zshrc`
3. Changed `deepseek-chat` provider from local proxy (`http://127.0.0.1:15721/v1`) to official API (`https://api.deepseek.com/v1`)

### Files Modified
- `~/.codex/config.toml`

---

## 2. GUI History Clear Button

### Problem
Large `history.json` file causing slow GUI startup due to JSON parsing overhead.

### Solution
Added "Clear History" button to the history panel in Dioxus GUI.

### Features
- Red button with trash icon: `🗑️ 清空历史`
- Clears both UI state and physical `history.json` file (writes `[]`)
- Located in the history panel header, next to the close button

### Files Modified
- `src/gui_dioxus.rs` (lines ~895-910)

### Verification
- `cargo check` passes with 0 errors

---

## Recent Commits (for reference)

| Commit | Description |
|--------|-------------|
| `227b20b` | docs: update HANDOFF.md with GUI optimization and complete summary |
| `49b2efd` | perf: cache yt-dlp availability and version checks |
| `0d64257` | feat: pass format_id to download request for format selection |
| `79494f9` | fix: resolve tokio runtime panic from blocking client initialization |
| `de4347a` | perf: add progress threshold filtering to reduce UI updates |

---

## Key Architecture Notes

### GUI Startup Flow
```
run_gui()
  ├─ YtDlp::new().is_available()  → Already cached via OnceLock ✅
  ├─ History::new()               → Parses history.json (potential bottleneck)
  └─ launch(AppComponent)         → Dioxus framework init
```

### yt-dlp Check (Already Optimized)
```rust
// src/yt_dlp.rs:347 - Uses OnceLock, no repeated process spawns
pub fn is_available(&self) -> bool {
    static CACHED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *CACHED.get_or_init(|| { ... })
}
```

### History Clear Method (Already Exists)
```rust
// src/history.rs:275
pub fn clear(&mut self) -> Result<()> {
    self.entries.clear();
    self.save()
}
```

---

## Current Status

- **GUI**: Functional, with clear history button added
- **CLI**: Working
- **Codex**: DeepSeek API configured and working
- **Build**: `cargo check` passes

---

## 4. Video Merge Speed Optimization (2026-06-18)

### Problem
- Video merging after download was slow due to default ffmpeg re-encoding

### Solution
Added `--ffmpeg-args` parameter to yt-dlp for fast stream copy:
```rust
"--ffmpeg-args".to_string(),
"-c:v copy -c:a copy -map 0:v:0 -map 0:a:0 -movflags +faststart".to_string(),
```

### Key Changes
- `-c:v copy -c:a copy`: Stream copy (no re-encoding, instant)
- `-map 0:v:0 -map 0:a:0`: Explicitly map first video/audio tracks
- `-movflags +faststart`: Enable fast start for web playback

### Files Modified
- `src/yt_dlp.rs`: Added ffmpeg fast copy arguments

---

## Pending / Future Work

1. Monitor if `history.json` growth causes startup delays (100 entry limit added)
2. Consider adding progress bar during merge phase

---

## 3. History Panel Fix (2026-06-18)

### Problem
- GUI history panel was always empty despite downloads completing
- `_history` variable in `run_gui()` was created but never used (dropped immediately)
- `AppState.history` was initialized as empty `Vec::new()` and never populated
- Download completion callback didn't add entries to history

### Solution
1. **Global History Instance**: Added `GLOBAL_HISTORY: std::sync::Mutex<Option<History>>` with `OnceLock`-style pattern
2. **Initialization**: Modified `run_gui()` to call `init_global_history(history)` instead of dropping the History
3. **Download Completion**: `add_completed_download()` called in download success callback
4. **UI Sync**: `use_effect()` loads history from global state on mount
5. **Clear History**: Updated to use `clear_global_history()` function
6. **Auto Cleanup**: Added `MAX_HISTORY_ENTRIES = 100` limit, auto-prunes old entries

### Files Modified
- `src/gui_dioxus.rs`:
  - Added global history management functions
  - Modified download completion callback
  - Updated clear history button
  - Added auto cleanup with 100 entry limit
- `src/history.rs`:
  - Added `get_entries()` method for thread-safe access
  - Added `add_entry_with_auto_cleanup()` for automatic pruning
  - Added `count()` method

### Key Code Additions
```rust
static GLOBAL_HISTORY: std::sync::Mutex<Option<History>> = std::sync::Mutex::new(None);
const MAX_HISTORY_ENTRIES: usize = 100;

fn add_completed_download(title: String, url: String, path: Option<PathBuf>) {
    if let Ok(mut h) = GLOBAL_HISTORY.lock() {
        if let Some(history) = h.as_mut() {
            let entry = HistoryEntry { ... };
            let _ = history.add_entry_with_auto_cleanup(entry, MAX_HISTORY_ENTRIES);
        }
    }
}
```

### Verification
- `cargo check` passes with 0 errors

---

_Generated: 2026-06-18_
