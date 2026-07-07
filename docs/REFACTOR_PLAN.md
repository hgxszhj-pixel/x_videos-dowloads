# 代码重构计划

> 创建时间: 2026-07-04
> 状态: 规划中

---

## 一、问题汇总

### 1.1 大文件 (>800行)

| 文件 | 行数 | 主要问题 |
|------|------|----------|
| `src/gui_dioxus.rs` | 1188 | 单体 GUI，UI + 业务逻辑混杂 |
| `src/downloader.rs` | 1137 | 5 种职责混在一起 |
| `src/main.rs` | 1062 | CLI + 下载编排 + resolver 链混杂 |
| `src/collaboration/server/db.rs` | 891 | 530+ 行 SQL 内联 |
| `src/types.rs` | 806 | 40+ dead_code，Builder 模式不一致 |

### 1.2 架构问题

| 问题 | 位置 | 影响 |
|------|------|------|
| 循环依赖 | `core/mod.rs` re-exports | 模块边界模糊 |
| 命名冲突 | `Platform`, `DownloadStatus`, `HistoryItem` 多处定义 | 类型不安全 |
| 孤儿模块 | `gui_backup/` | 技术债务 |
| 深层嵌套 | `collaboration/` 最深 4 层 | 可维护性差 |

---

## 二、重构计划

### Phase 1: 核心结构修复 (高优先级)

#### 1.1 修复 `core/mod.rs` 循环依赖

**问题**: `core/mod.rs` re-exports 了 `crate::downloader`, `crate::muxer` 等，造成隐式循环依赖。

**方案**:
```
现状:
  core/mod.rs --re-exports--> crate::downloader

改进:
  删除 core/mod.rs 的 re-exports
  保留 constants.rs, cache.rs, progress.rs, segment.rs 作为真正的核心模块
  将 re-export 移到 lib.rs 或删除（按需导入）
```

**文件变更**:
- `src/core/mod.rs` - 删除 re-exports
- `src/lib.rs` - 按需导入需要的类型

---

#### 1.2 拆分 `main.rs`

**目标**: 1062 行 → 3 个模块

**拆分方案**:
```
src/
├── main.rs           # 入口，仅做命令路由
├── cli.rs            # CLI 参数定义 (160行)
├── commands/         # 子命令
│   ├── mod.rs
│   ├── download.rs   # download 命令
│   ├── history.rs    # history 命令
│   └── serve.rs      # serve 命令
└── resolver_chain.rs # URL 解析链路
```

**验证清单**:
- [ ] `cargo build` 通过
- [ ] 所有 CLI 参数仍然有效
- [ ] `cargo test` 通过

---

#### 1.3 拆分 `gui_dioxus.rs`

**目标**: 1188 行 → 组件化结构

**拆分方案**:
```
src/gui/dioxus/
├── mod.rs           # 模块导出
├── app.rs           # 主 App 组件 (精简)
├── components/      # UI 组件
│   ├── mod.rs
│   ├── url_input.rs     # URL 输入框
│   ├── video_card.rs    # 视频信息卡片
│   ├── progress_bar.rs  # 进度条
│   ├── download_list.rs # 下载列表
│   └── status_bar.rs    # 状态栏
├── state/           # 状态管理
│   ├── mod.rs
│   ├── app_state.rs     # AppState
│   └── history_state.rs # HistoryState
├── handlers/        # 业务逻辑
│   ├── mod.rs
│   ├── download.rs      # 下载处理
│   ├── video_info.rs    # 视频信息获取
│   └── platform.rs      # 平台检测
└── theme.rs         # 主题配置 (从 gui/theme.rs 迁移)
```

**验证清单**:
- [ ] `cargo build` 通过
- [ ] GUI 功能和之前一致
- [ ] 进度条显示正常

---

### Phase 2: 清理与标准化 (中优先级)

#### 2.1 清理 `gui_backup/`

**方案**: 删除或迁移有用的代码

```bash
# 如果没有有用的代码
rm -rf src/gui_backup/

# 如果有，迁移到 gui/
mv src/gui_backup/* src/gui/
rm -rf src/gui_backup/
```

---

#### 2.2 统一类型命名

**问题**:
- `history::Platform` vs `crate::types::Platform`
- `DownloadStatus` 在 `history.rs` 和 `types.rs` 重复
- `HistoryItem` 在 `gui_dioxus.rs` 和 `gui/types.rs` 重复

**方案**:
```
保留: crate::types::Platform
删除: history.rs 中的 Platform 别名

保留: types.rs 中的 DownloadStatus, HistoryItem
删除: gui_dioxus.rs 中的重复定义
```

---

#### 2.3 提取 `db.rs` 的 SQL

**方案**: 将 SQL 移到单独文件

```
src/collaboration/server/
├── db.rs
└── schema.sql    # 提取的 SQL 文件
```

**好处**: SQL 可读性更好，可用工具验证

---

#### 2.4 移除死代码

**方案**: 清理 40+ `#[allow(dead_code)]`

```bash
# 查找所有 dead_code
grep -rn "allow(dead_code)" src/

# 逐个审查是否真的不用
```

---

### Phase 3: 架构优化 (长期)

#### 3.1 拆分 `downloader.rs`

**目标**: 1137 行 → 3 个模块

**拆分方案**:
```
src/downloader/
├── mod.rs        # 导出 + 入口
├── config.rs     # DownloaderConfig
├── parallel.rs   # 并行下载逻辑
└── merge.rs      # 分段合并逻辑
```

---

#### 3.2 平台解析器独立 crate (可选)

**考虑**: 12 个平台解析器是否该独立成 crate？

**优点**:
- 清晰的边界
- 可单独测试
- 发布到 crates.io

**缺点**:
- 版本同步麻烦
- 当前规模不需要

**结论**: 暂时保持单 crate，用 feature flags 组织

---

## 三、实施顺序

```
Week 1:
  [ ] Phase 1.1 - 修复 core/mod.rs 循环依赖
  [ ] Phase 1.2 - 拆分 main.rs

Week 2:
  [ ] Phase 1.3 - 拆分 gui_dioxus.rs
  [ ] Phase 2.1 - 清理 gui_backup/

Week 3:
  [ ] Phase 2.2 - 统一类型命名
  [ ] Phase 2.3 - 提取 db.rs SQL
  [ ] Phase 2.4 - 移除死代码

Week 4:
  [ ] Phase 3.1 - 拆分 downloader.rs
  [ ] 全面测试和验证
```

---

## 四、验证标准

每个 Phase 完成后必须满足:

1. **编译通过**: `cargo build --release`
2. **测试通过**: `cargo test`
3. **Lint 通过**: `cargo clippy -- -D warnings`
4. **功能不变**: CLI 和 GUI 行为与之前一致
5. **无性能下降**: 下载速度、内存使用无明显变化

---

## 五、风险与缓解

| 风险 | 影响 | 缓解措施 |
|------|------|----------|
| 拆分后循环依赖 | 编译失败 | 先画依赖图，再动手 |
| GUI 功能回归 | 用户体验下降 | 逐组件测试 |
| 类型删除后引用报错 | 编译失败 | 先 grep 查找所有引用 |
| 工期超期 | 影响其他功能 | 优先保证 Phase 1 |

---

## 六、参考资料

- CLAUDE.md - 项目编程理念
- docs/HANDOFF.md - 历史变更记录
- `src/lib.rs` - 代码质量报告 (40+ dead_code 位置)
