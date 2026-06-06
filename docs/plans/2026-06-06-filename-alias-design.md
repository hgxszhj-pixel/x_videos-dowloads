# 文件名别名设计文档

**日期**: 2026-06-06
**状态**: 已批准

---

## 1. 目标

当下载的视频 title 相同时，自动生成唯一的文件名别名，避免覆盖已存在的文件。

---

## 2. 解决方案

### 别名格式

```
原始文件名.mp4
原始文件名 (1).mp4
原始文件名 (2).mp4
...
```

### 生成规则

1. 下载前检查目标路径是否已存在
2. 如存在，尝试递增后缀 `(n)`，找到第一个可用的文件名
3. 仅检查文件系统，不持久化任何状态

---

## 3. 实现方案

### 新增函数

```rust
// src/types.rs 或新建 utils 模块
pub fn generate_unique_filename(
    output_dir: &Path,
    base_name: &str,
    ext: &str,
) -> PathBuf {
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
```

### 修改位置

| 文件 | 修改内容 |
|------|----------|
| `src/main.rs` | `download_with_ytdlp` 和 `download_direct` 使用新函数 |
| `src/gui.rs` | GUI 下载时使用新函数 |

### 修改示例 (download_with_ytdlp)

```rust
// 确定文件名
let base_filename = if let Some(ref name) = cli.filename {
    sanitize_filename(name)
} else {
    sanitize_filename(&info.title)
};

// 生成唯一路径（避免同名覆盖）
let ext = ...; // 从 URL 或格式中提取扩展名
let output_path = generate_unique_filename(&output_dir, &base_filename, ext);
```

---

## 4. 行为示例

| 场景 | 结果 |
|------|------|
| 下载第一个 "视频.mp4" | `视频.mp4` |
| 下载第二个同名视频 | `视频 (1).mp4` |
| 用户删除 `视频 (1).mp4`，再下载 | `视频 (1).mp4` |

---

## 5. 验证方式

- [ ] cargo build 通过
- [ ] cargo test 通过
- [ ] 手动测试：连续下载两个相同 title 的视频，确认文件名不冲突