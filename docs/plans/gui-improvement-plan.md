# GUI Improvement Plan

**Project**: x_videos-dowloads (Rust Video Downloader)
**Created**: 2026-05-29
**Updated**: 2026-05-29 (after code-review)
**Status**: Reviewed - Ready for Execution

---

## 已修复的 CRITICAL 问题 (基于 code-review)

1. **[FIXED] tokio runtime panic**: 根因已定位在 `src/gui.rs:518`，修复方案为使用 `tokio::process::Command`
2. **[FIXED] 进度条循环语义不清晰**: 存储格式 `percent * 100.0` 正确，终止条件应为 `progress < 10000`
3. **[ADDED] cancel_flag 未使用**: 将在迭代 3 中实现暂停/恢复功能

---

## 1. 项目概述

本计划针对 GUI 模块进行系统性改进，基于 `.understand-anything/knowledge-graph.json` 图谱分析和 `HANDOFF.md` 中记录的已知问题。

### 1.1 GUI 模块组成

| 文件 | 大小 | 职责 |
|------|------|------|
| `src/gui.rs` | 43,052 bytes | Slint GUI 主实现，MainWindow，SharedState |
| `src/theme.rs` | 4,455 bytes | 主题/颜色管理 |
| `src/i18n.rs` | 7,951 bytes | 国际化支持 |
| `src/types.rs` | 20,842 bytes | DownloadProgress 等类型定义 |
| `src/history.rs` | 12,669 bytes | 下载历史管理 |

### 1.2 已知问题 (HANDOFF.md 记录)

1. **tokio runtime panic**: `spawn_blocking` 在 async context 中被调用导致 panic
   - **根因**: `src/gui.rs:518` 使用 `std::process::Command::spawn()` 在 `rt.spawn(async move {` 内阻塞
   - **修复方案**: 使用 `tokio::process::Command` 替代 `std::process::Command`
2. **进度条循环语义不清晰**: 存储 `percent * 100.0` (范围 0-10000)，UI 显示时除以 100
   - **问题**: 终止条件 `progress < 100` 与存储格式不匹配
   - **修复方案**: 统一存储格式为 `percent * 100.0` (0-10000)，终止条件改为 `progress < 10000`
3. **GUI 卡顿**: Slint 在 macOS 上渲染性能限制
4. **cancel_flag 未实际使用**: 在 line 663 被设置，但下载逻辑中未检查

---

## 2. 改进领域

### 2.1 代码结构优化

**问题**: `src/gui.rs` 文件过大 (43KB)，难以维护，且 Slint 组件通过 `slint::include_modules!()` 生成

**改进方向**:
- 提取 `SharedState` 到独立模块
- 提取 UI 组件定义到单独文件
- 提取进度解析逻辑到 `progress.rs`
- 分离事件处理和业务逻辑
- 将 Slint 组件保持在主文件或迁移到 `.slint` 源文件

**目标文件结构**:
```
src/gui/
├── mod.rs          # 主入口，run_gui()
├── state.rs        # SharedState 定义
├── window.rs       # MainWindow 实现
├── progress.rs     # 进度解析和更新
├── handlers.rs     # 事件处理器
├── theme.rs        # 主题管理 (迁移)
└── i18n.rs         # 国际化 (迁移)
```

> 注意: Slint 组件定义需保留在 `window.rs` 或通过 `slint::include_modules!()` 保持现状

### 2.2 进度条系统改进

**当前架构**:
```rust
struct SharedState {
    download_progress_arc: Option<Arc<AtomicU32>>,
    cancel_flag: Arc<AtomicU32>,
}
```

**问题**:
- 进度值存储格式 `percent * 100.0` (范围 0-10000)，语义不清晰
- UI 线程和下载线程的同步逻辑分散
- 终止条件 `progress < 100` 与存储格式不匹配

**改进方向**:
1. 统一进度值存储格式为 `percent * 100.0` (0-10000)，终止条件改为 `progress < 10000`
2. 提取 `ProgressUpdater` 统一管理进度更新
3. 添加暂停/恢复功能到 `cancel_flag` (当前未使用)
4. 改进进度解析 (支持更多 yt-dlp 输出格式)

### 2.3 错误处理改进

**当前问题**: 错误信息通过 `error_message` + `error_action` 字符串传递

**改进方向**:
1. 定义 `GuiError` 枚举，包含错误类型和可 localized 的消息 key
2. 使用 `Result<T, GuiError>` 替代字符串错误
3. 错误恢复操作的标准化处理

### 2.4 国际化 (i18n) 改进

**当前**: `src/i18n.rs` 使用简单的翻译 map

**改进方向**:
1. 支持复数形式
2. 支持占位符替换
3. 支持 RTL 语言（未来扩展）
4. 提取 UI 字符串到独立文件

### 2.5 主题系统改进

**当前**: `src/theme.rs` 管理颜色

**改进方向**:
1. 支持明暗主题切换
2. 支持用户自定义颜色
3. 统一颜色变量命名

---

## 3. 迭代计划 (10 轮)

| 迭代 | 重点领域 | 交付物 |
|------|----------|--------|
| 1 | 代码结构分析 + 重构计划 | `docs/plans/gui-refactor-plan.md` |
| 2 | SharedState 提取 | `src/gui/state.rs` |
| 3 | 进度系统重构 | `src/gui/progress.rs` (修复终止条件 `progress < 10000`) |
| 4 | 错误处理标准化 | `src/gui/error.rs` |
| 5 | 事件处理器分离 | `src/gui/handlers.rs` |
| 6 | i18n 系统增强 | `src/gui/i18n.rs` |
| 7 | 主题系统增强 | `src/gui/theme.rs` |
| 8 | UI 组件优化 | `src/gui/window.rs` |
| 9 | 集成测试 + 修复 | 通过 `cargo test` |
| 10 | 最终审查 + 清理 | 通过 `cargo clippy` |

> 迭代 3 优先级: 修复 tokio runtime panic (使用 `tokio::process::Command`) 和进度条终止条件

---

## 4. 约束条件

1. **向后兼容**: CLI 参数和配置文件格式不变
2. **性能**: GUI 启动时间 < 2s，响应延迟 < 100ms
3. **代码质量**: `cargo clippy -- -D warnings`
4. **测试覆盖**: 关键路径 80%+ 覆盖率

---

## 5. 风险评估

| 风险 | 影响 | 缓解措施 |
|------|------|----------|
| Slint 渲染性能 | 高 | 避免不必要的高频 UI 更新 |
| 线程安全 | 高 | 使用 Arc<AtomicU32> 替代锁 |
| 破坏现有功能 | 高 | 每轮迭代后运行完整测试 |
| Slint 版本兼容性 | 高 | 重构时锁定 Slint 版本，避免 API 变化 |
| 历史记录锁竞争 | 中 | `history.write()` 在多个回调中连续调用，可能造成 UI 阻塞 |
| cancel_flag 未实际使用 | 中 | 下载逻辑中未检查此标志，需实现暂停/恢复功能 |
| 进度循环终止逻辑错误 | 高 | 当前使用 `progress < 100`，应为 `progress < 10000` |

---

## 6. 验证标准

每轮迭代必须通过:
```bash
cargo check
cargo test
cargo clippy -- -D warnings
cargo build --release  # 确保发布构建通过
```

GUI 功能验证:
```bash
cargo run -- --gui
# 测试: URL 输入，进度条，下载完成
```