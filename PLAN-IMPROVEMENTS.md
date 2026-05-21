# 项目改进计划 (PLAN-IMPROVEMENTS)

**项目**: Rust x.com 视频下载器
**生成时间**: 2026-05-21
**分析范围**: ~28,000 行 Rust 代码

---

## 问题概览

| 维度 | 状态 | 问题数量 |
|------|------|----------|
| **代码质量** | MEDIUM | 90+ `#[allow(dead_code)]`, 60+ `.unwrap()` 调用 |
| **架构** | GOOD | 模块化设计清晰 |
| **性能** | MEDIUM | GUI 中锁竞争 |
| **可维护性** | LOW | 死代码积累、函数过大 |
| **测试** | GOOD | 核心模块有测试 |
| **安全** | GOOD | 有路径遍历保护和输入验证 |

---

## PRIORITY 1: 关键问题 (风险: HIGH)

### 1.1 GUI 模块中的 panic 风险

**文件**: `src/gui.rs` (约 15 处 `lock().unwrap()`)

**问题**: GUI 模块大量使用 `lock().unwrap()`，如果锁被 poisoned 会导致 panic。在异步上下文中尤其危险。

**修复方案**: 替换为适当的错误处理：
```rust
// Before (risky)
let mut s = shared.lock().unwrap();

// After (safe)
let mut s = shared.lock().map_err(|e| format!("Lock poisoned: {}", e))?;
```

**预估工作量**: 2-3 小时

### 1.2 i18n 模块中的 panic 风险

**文件**: `src/i18n.rs` (4 处 `CURRENT_LANG.lock().unwrap()`)

**问题**: 全局状态使用 `Mutex.lock().unwrap()` 没有容错处理。

**修复方案**: 创建包装器处理 poisoning，或使用 `parking_lot` crate。

**预估工作量**: 1-2 小时

### 1.3 gui.rs 中大函数

**文件**: `src/gui.rs` - `on_download_clicked` (~200 行)

**问题**: 函数处理多个职责：验证、进度回调设置、历史管理、下载协调。

**修复方案**: 提取子函数：
- `validate_download_request()`
- `setup_progress_callback()`
- `execute_download()`

**预估工作量**: 3-4 小时

### 1.4 History::add_entry 参数过多

**文件**: `src/history.rs:150`

**问题**: 函数有 9 个参数，难以使用且容易出错。

**修复方案**: 使用 Builder 模式或接受结构体参数。

**预估工作量**: 1 小时

---

## PRIORITY 2: 高优先级问题 (风险: MEDIUM)

### 2.1 死代码积累 (90+ 处)

**主要文件**:
- `src/resolvers/token_manager.rs`: 7 处
- `src/collaboration/client/queue.rs`: 10 处
- `src/resolvers/douyin_api.rs`: 8 处
- `src/resolvers/zhixueyun.rs`: 7 处

**问题**: 90+ `#[allow(dead_code)]` 属性表明实现不完整或已放弃的功能。

**修复方案**:
1. 审计所有 dead_code 项
2. 分类：删除 / 实现 / 文档化
3. 系统性清理

**预估工作量**: 4-6 小时（仅审计），2-3 天（完全清理）

### 2.2 Downloader 缺少错误上下文

**文件**: `src/downloader.rs`

**问题**: 多处错误点使用 `anyhow!("message")` 但没有上下文链。

**修复方案**: 在所有错误传播点添加 context。

**预估工作量**: 1-2 小时

### 2.3 ResolverError 变体标记为 dead_code

**文件**: `src/resolvers/mod.rs:26-38`

**问题**: `UnsupportedUrl` 和 `MissingToken` 变体标记为 dead_code 但仍在错误转换中使用。

**修复方案**: 要么实现功能，要么完全移除变体。

**预估工作量**: 30 分钟

### 2.4 GUI SharedState 锁竞争

**文件**: `src/gui.rs:21-37`

**问题**: 使用 `Mutex` 而 `RwLock` 对读多写少操作更高效。

**修复方案**: 考虑将 `Mutex<SharedState>` 替换为 `RwLock<SharedState>`。

**预估工作量**: 1 小时

---

## PRIORITY 3: 中优先级问题 (风险: LOW)

### 3.1 downloader.rs 中的大函数

**文件**: `src/downloader.rs`

**问题**:
- `parallel_download` 函数 (~240 行)
- `do_download` 函数 (~100 行)

**修复方案**: 拆分为更小的聚焦函数：
- `merge_segments()`
- `setup_progress_tracking()`
- `calculate_segment_ranges()`

**预估工作量**: 2-3 小时

### 3.2 重复的 URL 模式匹配

**文件**: `src/resolvers/mod.rs`, `src/history.rs`

**问题**: ResolverRegistry 和 Platform 枚举都有 URL 匹配逻辑，导致重复和潜在不一致。

**修复方案**: 创建共享的 URL 匹配工具函数。

**预估工作量**: 1-2 小时

### 3.3 公开 API 缺少文档

**文件**: `src/collaboration/client/ws.rs`, `src/collaboration/server/handler.rs`

**问题**: `CollaborationClient` 和 `MessageHandler` 等关键类型缺少文档。

**修复方案**: 为所有公共类型添加文档注释。

**预估工作量**: 2 小时

### 3.4 测试覆盖缺口

**文件**: `src/gui.rs`, `src/resolvers/`

**问题**:
- `src/gui.rs` - 没有单元测试
- `src/resolvers/` - 有限的集成测试

**修复方案**: 添加测试：
- GUI 回调（可以 mock slint 组件）
- Resolver 选择逻辑
- 错误处理路径

**预估工作量**: 3-4 小时

---

## PRIORITY 4: 低优先级 / 技术债务 (风险: LOW)

### 4.1 未使用的 CookiePool 功能

**文件**: `src/resolvers/cookie_pool.rs`

**问题**: `CookiePool` 有大量功能（LRU、TTL、命中计数）但可能过度设计。

**修复方案**: 验证实际使用模式，不需要则简化。

**预估工作量**: 1 小时（审计）

### 4.2 downloader.rs 中的魔法数字

**文件**: `src/downloader.rs:71-72`

**问题**: 硬编码值如 `10 * 1024 * 1024` (10MB 阈值) 和 `300` 秒（段超时）。

**修复方案**: 提取为有文档的命名常量。

**预估工作量**: 30 分钟

---

## 实施阶段

### Phase 1: 关键安全修复 (1-2 天)

1. 修复 GUI 和 i18n 模块中所有 `unwrap()` 调用
2. 添加适当的错误处理包装器

### Phase 2: 死代码清理 (2-3 天)

1. 审计所有 90+ dead_code 项
2. 分类：删除 / 实现 / 文档化
3. 系统性移除

### Phase 3: 架构改进 (3-4 天)

1. 拆分 downloader.rs 中的大函数
2. 重构 History::add_entry
3. 改进协作模块结构

### Phase 4: 测试与文档 (2-3 天)

1. 添加缺失的单元测试
2. 文档化公共 API
3. 标准化错误类型

---

## 成功标准

- [ ] GUI 和 i18n 模块中零 `unwrap()` 调用
- [ ] `#[allow(dead_code)]` 少于 30 处
- [ ] 所有公共 API 有文档
- [ ] 测试覆盖率从 ~60% 提升到 ~75%
- [ ] 异步上下文中无 panic 风险
- [ ] 所有 clippy 警告已解决

---

## 按优先级分类的文件

| 优先级 | 需修改的文件 |
|--------|-------------|
| P1 (关键) | `src/gui.rs`, `src/i18n.rs`, `src/history.rs` |
| P2 (高) | `src/downloader.rs`, `src/resolvers/mod.rs`, `src/resolvers/token_manager.rs` |
| P3 (中) | `src/collaboration/client/queue.rs`, `src/resolvers/douyin_api.rs`, `src/resolvers/zhixueyun.rs` |
| P4 (低) | `src/resolvers/cookie_pool.rs`, `src/collaboration/` (多个) |

---

## 待审核任务清单

### P1 - 关键问题
- [ ] 修复 GUI 中 15 处 `lock().unwrap()`
- [ ] 修复 i18n 中 4 处 `CURRENT_LANG.lock().unwrap()`
- [ ] 拆分 `on_download_clicked` 大函数
- [ ] 重构 `History::add_entry` 使用 Builder 模式

### P2 - 高优先级
- [ ] 审计 90+ 处 dead_code
- [ ] 为 downloader.rs 添加错误上下文
- [ ] 实现或移除 `UnsupportedUrl`/`MissingToken`
- [ ] 将 `Mutex` 改为 `RwLock`

### P3 - 中优先级
- [ ] 拆分 downloader.rs 大函数
- [ ] 创建共享 URL 匹配工具
- [ ] 添加公共 API 文档
- [ ] 添加缺失的单元测试

### P4 - 低优先级
- [ ] 审计 CookiePool 简化
- [ ] 提取魔法数字为常量
- [ ] (未来) 考虑 async trait