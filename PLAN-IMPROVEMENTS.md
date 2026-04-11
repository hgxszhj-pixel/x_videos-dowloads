# 代码改进计划

## 目标
修复项目中的代码质量问题，提高稳定性、可维护性和安全性。

---

## 任务清单

### Task 1: 修复 GUI 异步问题 (高优先级)

**问题**: `gui.rs` 第 308 行和 `downloader.rs` 第 399 行使用同步 `std::process::Command`

**修改文件**:
- `src/gui.rs`
- `src/downloader.rs`

**修改内容**:

#### 1. gui.rs 修改
```rust
// 修改前 (第 308 行)
// let output = std::process::Command::new(&ytdlp.executable)

// 修改后 - 使用完整路径避免与 iced::Command 冲突
use tokio::process::Command as TokioCommand;
let output = TokioCommand::new(&ytdlp.executable)
    .args(&args)
    .output()
    .await
    .map_err(|e| format!("Failed to run yt-dlp: {}", e))?;
```

#### 2. downloader.rs 修改 (第 399 行)
```rust
// 修改前
use std::process::Command;

// 修改后
use tokio::process::Command;
```

**验证**: `cargo check` 通过

---

### Task 2: 删除未使用代码 (中优先级)

**问题**: 多处 dead code 增加维护负担

#### 2.1 删除 `downloader.rs` 中未使用的 `download_with_ytdlp` 方法
- 位置: 第 392-500 行
- 方法签名: `pub async fn download_with_ytdlp(...)`

#### 2.2 删除 `yt_dlp.rs` 中未使用的 `YtDlpJson` 结构体
- 位置: 第 493-530 行
- 结构体定义

#### 2.3 删除 `main.rs` 中未使用的 `extract_progress` 函数
- 位置: 第 341-345 行
- 函数签名: `fn extract_progress(line: &str) -> Option<f64>`

**验证**: `cargo check` 通过，无警告

---

### Task 3: 修复 sanitize_filename 逻辑 (低优先级)

**问题**: `types.rs` 第 410-414 行路径遍历检测逻辑冗余

**修改文件**: `src/types.rs`

**修改前**:
```rust
let has_path_traversal = name.contains("..")
    || name.starts_with('/')
    || name.starts_with('\\')
    || name.contains("~/")
    || name.starts_with("~");
```

**修改后**:
```rust
let has_path_traversal = name.contains("..")
    || name.starts_with('/')
    || name.starts_with('\\')
    || name.starts_with('~');
```

**验证**: 现有单元测试通过

---

### Task 4: 添加任务验证 (持续)

所有 Task 完成后执行:
```bash
cargo clippy -- -D warnings
cargo test
```

---

## 实施顺序

1. **Phase 1**: Task 1 (GUI 异步修复) - 高优先级，可能影响稳定性
2. **Phase 2**: Task 2 (删除 dead code) - 中优先级
3. **Phase 3**: Task 3 (修复 sanitize_filename) - 低优先级
4. **Phase 4**: Task 4 (验证) - 确保所有修改正确

---

## Git Worktree 策略

- 为每个 Phase 创建独立 worktree
- 分支命名: `improvement/fix-gui-async`, `improvement/remove-dead-code`, `improvement/fix-sanitize`
- 完成后合并到 main 分支

---

## 风险评估

| Task | 风险等级 | 回滚方案 |
|------|----------|----------|
| Task 1 | 中 | git worktree 切换回 main |
| Task 2 | 低 | 删除代码不影响功能 |
| Task 3 | 极低 | 仅优化逻辑，行为不变 |

---

## 预期结果

- 消除所有 compiler warnings
- 修复 GUI 中潜在的异步阻塞问题
- 代码库更简洁，减少维护负担
