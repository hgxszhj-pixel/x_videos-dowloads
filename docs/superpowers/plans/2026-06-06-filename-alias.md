# Filename Alias Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 当下载视频 title 相同时，自动生成唯一的文件名别名，避免覆盖已存在的文件。

**Architecture:** 在 `types.rs` 添加 `generate_unique_filename` 函数，检查文件系统寻找可用文件名。修改 `main.rs` 和 `gui.rs` 的路径构建逻辑使用该函数。

**Tech Stack:** Rust

---

## Task 1: Add `generate_unique_filename` function to types.rs

**Files:**
- Modify: `src/types.rs` (在 `sanitize_filename` 函数附近添加)

- [ ] **Step 1: 添加函数和测试**

在 `src/types.rs` 的 `sanitize_filename` 函数下方添加:

```rust
/// 生成唯一的文件名，避免覆盖已存在的文件
/// 如果 `base_name.ext` 已存在，尝试 `base_name (1).ext`、`base_name (2).ext` 等
pub fn generate_unique_filename(
    output_dir: &std::path::Path,
    base_name: &str,
    ext: &str,
) -> std::path::PathBuf {
    let mut counter = 0;
    loop {
        let filename = if counter == 0 {
            format!("{}.{}", base_name, ext)
        } else {
            format!("{} ({}).{}", base_name, counter, ext)
        };
        let path = output_dir.join(&filename);
        if !path.exists() {
            return path;
        }
        counter += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_unique_filename_first_available() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = generate_unique_filename(temp_dir.path(), "video", "mp4");
        assert_eq!(path.file_name().unwrap().to_str().unwrap(), "video.mp4");
    }

    #[test]
    fn test_generate_unique_filename_avoids_conflict() {
        let temp_dir = tempfile::tempdir().unwrap();
        // 创建 "video.mp4"
        std::fs::write(temp_dir.path().join("video.mp4"), b"existing").unwrap();
        let path = generate_unique_filename(temp_dir.path(), "video", "mp4");
        assert_eq!(path.file_name().unwrap().to_str().unwrap(), "video (1).mp4");
    }

    #[test]
    fn test_generate_unique_filename_multiple_conflicts() {
        let temp_dir = tempfile::tempdir().unwrap();
        // 创建 "video.mp4", "video (1).mp4"
        std::fs::write(temp_dir.path().join("video.mp4"), b"1").unwrap();
        std::fs::write(temp_dir.path().join("video (1).mp4"), b"2").unwrap();
        let path = generate_unique_filename(temp_dir.path(), "video", "mp4");
        assert_eq!(path.file_name().unwrap().to_str().unwrap(), "video (2).mp4");
    }

    #[test]
    fn test_generate_unique_filename_gap_reuse() {
        let temp_dir = tempfile::tempdir().unwrap();
        // 创建 "video.mp4", "video (1).mp4", "video (2).mp4"
        std::fs::write(temp_dir.path().join("video.mp4"), b"1").unwrap();
        std::fs::write(temp_dir.path().join("video (1).mp4"), b"2").unwrap();
        std::fs::write(temp_dir.path().join("video (2).mp4"), b"3").unwrap();
        // 删除 "video (1).mp4"，下次应该生成 "video (1).mp4"（填坑）
        std::fs::remove_file(temp_dir.path().join("video (1).mp4")).unwrap();
        let path = generate_unique_filename(temp_dir.path(), "video", "mp4");
        assert_eq!(path.file_name().unwrap().to_str().unwrap(), "video (1).mp4");
    }
}
```

- [ ] **Step 2: 运行测试验证**

Run: `cargo test test_generate_unique_filename -- --nocapture`
Expected: 4 tests PASS

- [ ] **Step 3: 提交**

```bash
git add src/types.rs
git commit -m "feat: add generate_unique_filename for filename aliasing"
```

---

## Task 2: Modify `download_with_ytdlp` in main.rs

**Files:**
- Modify: `src/main.rs:562-595` (文件名构建逻辑)

- [ ] **Step 1: 修改 filename 构建逻辑**

找到 `download_with_ytdlp` 函数中以下代码 (约 line 562-595):

```rust
// 确定文件名
let filename = if let Some(ref name) = cli.filename {
    sanitize_filename(name)
} else {
    sanitize_filename(&info.title)
};

// 确定格式
// ...

// 构建 yt-dlp 命令
let output_path = output_dir.join(format!("{}.%(ext)s", filename));
```

修改为:

```rust
// 确定文件名（sanitized）
let base_filename = if let Some(ref name) = cli.filename {
    sanitize_filename(name)
} else {
    sanitize_filename(&info.title)
};

// 确定格式
// ...

// 从 URL 或格式中提取扩展名（用于生成唯一文件名）
let ext = if is_direct_url {
    info.url.split('?').next()
        .and_then(|s| s.rsplit('/').next()?.split('.').next_back())
        .unwrap_or("mp4")
} else {
    "mp4"  // yt-dlp 会根据格式自动选择扩展名
};

// 生成唯一路径（避免同名覆盖）
let output_path = generate_unique_filename(&output_dir, &base_filename, ext);
```

同时，yt-dlp 的 output_path 模板需要修改为固定扩展名（因为我们已经处理了扩展名）:

```rust
// 构建 yt-dlp 命令（使用固定扩展名）
let output_path = output_dir.join(format!("{}.{}", base_filename, ext));
```

注意：对于 yt-dlp 下载（line 585 附近的 `output_path = output_dir.join(format!("{}.%(ext)s", filename))`），需要保留 `%(ext)s` 让 yt-dlp 决定扩展名，但文件名部分使用 sanitize 后的 base_filename。

**关键修改点**:
1. line 562-567: 提取 base_filename（不包含扩展名）
2. line 585: `output_path = output_dir.join(format!("{}.%(ext)s", filename))` 改为使用 `generate_unique_filename`
3. 对于 video.twimg.com 直接下载（line 595-630），同样使用 `generate_unique_filename`

- [ ] **Step 2: 验证编译**

Run: `cargo build 2>&1 | head -50`
Expected: 无编译错误

- [ ] **Step 3: 提交**

```bash
git add src/main.rs
git commit -m "feat: use generate_unique_filename in download_with_ytdlp"
```

---

## Task 3: Modify `download_direct` in main.rs

**Files:**
- Modify: `src/main.rs` (搜索 `download_direct` 函数)

- [ ] **Step 1: 找到并修改 `download_direct` 函数**

搜索 `download_direct` 函数，找到文件名构建逻辑并使用 `generate_unique_filename`。

典型的下载逻辑类似:

```rust
let filename = sanitize_filename(&title);
let output_path = output_dir.join(format!("{}.{}", filename, ext));
```

修改为:

```rust
let base_filename = sanitize_filename(&title);
let output_path = generate_unique_filename(&output_dir, &base_filename, ext);
```

- [ ] **Step 2: 验证编译**

Run: `cargo build 2>&1 | head -50`
Expected: 无编译错误

- [ ] **Step 3: 提交**

```bash
git add src/main.rs
git commit -m "feat: use generate_unique_filename in download_direct"
```

---

## Task 4: Modify GUI download in gui.rs

**Files:**
- Modify: `src/gui.rs` (搜索文件名构建逻辑)

- [ ] **Step 1: 搜索 gui.rs 中的路径构建**

搜索 `output_dir.join` 或 `sanitize_filename` 在 gui.rs 中的使用位置。

关键位置可能在:
- 下载按钮的点击处理函数
- `start_download` 或类似函数

- [ ] **Step 2: 修改为使用 `generate_unique_filename`**

类似 main.rs 的修改，将:

```rust
let filename = sanitize_filename(&title);
let output_path = output_dir.join(format!("{}.{}", filename, ext));
```

修改为:

```rust
let base_filename = sanitize_filename(&title);
let output_path = generate_unique_filename(&output_dir, &base_filename, ext);
```

- [ ] **Step 3: 验证编译**

Run: `cargo build 2>&1 | head -50`
Expected: 无编译错误

- [ ] **Step 4: 提交**

```bash
git add src/gui.rs
git commit -m "feat: use generate_unique_filename in GUI download"
```

---

## Task 5: Final Verification

- [ ] **Step 1: 运行所有测试**

Run: `cargo test 2>&1 | tail -20`
Expected: 所有测试通过

- [ ] **Step 2: 运行 clippy**

Run: `cargo clippy 2>&1 | head -30`
Expected: 无警告

---

## Summary

| Task | Files | Status |
|------|-------|--------|
| 1 | `src/types.rs` | Add `generate_unique_filename` + tests |
| 2 | `src/main.rs` | Modify `download_with_ytdlp` |
| 3 | `src/main.rs` | Modify `download_direct` |
| 4 | `src/gui.rs` | Modify GUI download |
| 5 | All | Final verification |

---

**Plan complete and saved to `docs/superpowers/plans/2026-06-06-filename-alias.md`**.

**Two execution options:**

**1. Subagent-Driven (recommended)** - I dispatch a fresh subagent per task, review between tasks, fast iteration

**2. Inline Execution** - Execute tasks in this session using executing-plans, batch execution with checkpoints

**Which approach?**