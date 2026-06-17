# Slint → Dioxus 迁移计划

**项目**: x_videos-dowloads
**创建时间**: 2026-06-11
**状态**: 规划中

---

## 一、迁移背景

### 问题
- Slint 在 macOS 上渲染性能限制
- 进度条 UI 更新有延迟
- Slint 社区相对较小，资源有限

### 目标
迁移到 Dioxus，获得更好的性能、更多社区支持、跨平台能力

---

## 二、Dioxus 简介

| 特性 | Slint | Dioxus |
|------|-------|--------|
| 架构 | 声明式 | 声明式 + Signal |
| 状态管理 | 属性绑定 | Signal/Context |
| 渲染 | WebGPU/OpenGL | WebGPU/WASM/DOM |
| 平台 | Desktop/Web/Native | Desktop/Web/Mobile |
| 社区 | 小 | 活跃 |
| 性能 | 中等 | 高 |
| 学习曲线 | 低 | 中 |

---

## 三、当前 Slint UI 结构分析

### 3.1 文件结构
```
ui.slint # 560 行 - UI 定义
src/gui.rs        # ~1600 行 - Rust 集成逻辑
src/main.rs       # ~900 行 - 应用入口
```

### 3.2 UI 组件清单 (ui.slint)

| 组件 | 行数 | 复杂度 | Dioxus 等价 |
|------|------|--------|-------------|
| Window | 1-15 | ⭐ | Element |
| 属性定义 | 16-90 | ⭐⭐ | State struct |
| URL Section | ~100-150 | ⭐⭐ | Input component |
| Video Info | ~150-200 | ⭐⭐ | Card component |
| Progress Section | ~200-300 | ⭐⭐⭐ | Progress component |
| Batch Progress | ~280-400 | ⭐⭐⭐ | List + Progress |
| History/Bookmark | ~400-560 | ⭐⭐⭐ | Table component |

### 3.3 Slint 核心概念映射

| Slint 概念 | Dioxus 等价 | 说明 |
|------------|-------------|------|
| `in-out property <T>` | `use_state` / `Signal<T>` | 响应式状态 |
| `callback name()` | `on:click` / Event Handler | 事件处理 |
| `text <=> root.prop` | `value: prop` / `on_input` | 双向绑定 |
| `VerticalLayout` | `VStack` / `FlexColumn` | 垂直布局 |
| `HorizontalLayout` | `HStack` / `FlexRow` | 水平布局 |
| `Rectangle` | `Container` / `Div` | 容器 |
| `Text` | `Text` / `Label` | 文本 |
| `TextInput` | `Input` / `TextField` | 输入框 |
| `Button` | `Button` | 按钮 |
| `ProgressIndicator` | `ProgressBar` | 进度条 |
| `Slider` | `Slider` | 滑块 |
| `TouchArea` | `on:click` | 点击区域 |

---

## 四、迁移任务分解

### 4.1 准备工作 (预计 1-2 天)

| 任务 | 工作量 | 说明 |
|------|--------|------|
| 添加 Dioxus 依赖 | 低 | Cargo.toml |
| 创建项目结构 | 低 | src/ui/ 目录 |
| 配置构建目标 | 低 | desktop, web |

### 4.2 核心迁移 (预计 5-7 天)

#### Phase 1: 基础架构 (2 天)
```
1. 创建 UI 模块结构
2. 实现基础 Window/Container
3. 实现 VStack/HStack 布局
4. 迁移属性定义到 State/Signal
```

#### Phase 2: 组件迁移 (2-3 天)
```
1. URL Section (Input + Buttons)
2. Video Info Section (Card)
3. Progress Section (ProgressBar)
4. Batch Progress (List + 5 ProgressBars)
```

#### Phase 3: 事件处理 (1-2 天)
```
1. 迁移 callback 到 on:click
2. 迁移 text <=> 绑定到 on_input
3. 实现 Rust 端事件处理
```

### 4.3 集成迁移 (预计 2-3 天)

| 任务 | 工作量 | 说明 |
|------|--------|------|
| 进度更新机制 | 中 | invoke_from_event_loop → signal.update |
| BatchManager 集成 | 中 | 保持现有逻辑 |
| 历史记录集成 | 低 | 复用 types.rs |
| 配置管理 | 低 | 复用 config.rs |

### 4.4 测试与优化 (预计 2 天)

| 任务 | 工作量 | 说明 |
|------|--------|------|
| 构建验证 | 低 | cargo build |
| 功能测试 | 中 | GUI 功能验证 |
| 性能对比 | 低 | vs Slint |
| Bug 修复 | 中 | 预计5-10 个 |

---

## 五、详细任务清单

### 5.1 依赖添加 (Day 1)

```toml
# Cargo.toml
dioxus = { version = "0.6", features = ["desktop", "web"] }
dioxus-hooks = "0.6"
dioxus-core = "0.6"
dioxus-desktop = "0.6"
```

### 5.2 目录结构

```
src/
├── ui/
│   ├── mod.rs           # 模块导出
│   ├── app.rs          # 主应用组件
│   ├── components/
│   │   ├── mod.rs
│   │   ├── url_input.rs
│   │   ├── video_info.rs
│   │   ├── progress.rs
│   │   ├── batch_progress.rs
│   │   └── history.rs
│   └── styles.rs        # 样式定义
├── gui.rs              # 重命名为 gui_dioxus.rs 或删除
└── main.rs             # 更新入口
```

### 5.3 组件迁移映射

#### url_input.rs (Dioxus)
```rust
#[component]
fn UrlInput(
    url: Signal<String>,
    on_fetch: EventHandler<()>,
    on_clear: EventHandler<()>,
) -> Element {
    let mut text = url.clone();
    rsx! {
        div { class: "url-section",
            input {
                r#type: "text",
                placeholder: "Enter X.com/Twitter video URL",
                value: "{text.read()}",
                on_input: move |e| text.set(e.value().to_string()),
            }
            button { "Fetch Info", onclick: move |_| on_fetch.call(()) }
            button { "Clear", onclick: move |_| on_clear.call(()) }
        }
    }
}
```

#### progress.rs (Dioxus)
```rust
#[component]
fn ProgressSection(
    progress: Signal<f32>,
    speed: Signal<String>,
    eta: Signal<String>,
) -> Element {
    rsx! {
        div { class: "progress-section",
            div { "Downloading... {progress.read() * 100.0}%" }
            progress { value: "{progress.read()}" }
            div { "Speed: {speed.read()}" }
            div { "ETA: {eta.read()}" }
        }
    }
}
```

---

## 六、风险评估

| 风险 | 影响 | 缓解措施 |
|------|------|----------|
| Dioxus API 变化 | 中 | 使用稳定版本 0.6 |
| 状态管理复杂性 | 中 | 渐进式迁移，从简单组件开始 |
| 样式系统差异 | 低 | 使用内联样式或 CSS-in-Rust |
| 第三方库兼容性 | 中 | 检查 rfd/slint 特供库 |
| 性能反而下降 | 低 | 预先测试对比 |

---

## 七、验证清单

迁移完成后验证：

- [ ] `cargo build` 通过
- [ ] GUI 启动无崩溃
- [ ] URL 输入和 Fetch Info 工作
- [ ]视频信息正确显示
- [ ] 单个下载进度正确更新
- [ ] 批量下载进度正确更新
- [ ] 历史记录显示和操作正常
- [ ] 取消下载功能正常
- [ ] 性能优于或持平 Slint 实现

---

## 八、时间估算

| Phase | 任务 | 预计时间 |
|-------|------|----------|
| 1 | 准备工作 | 0.5 天 |
| 2 | 核心迁移 | 5 天 |
| 3 | 集成迁移 | 2 天 |
| 4 | 测试优化 | 2 天 |
| **总计** | | **9.5 天** |

---

## 九、建议执行顺序

1. **先小后大**: 先迁移 History组件 (最简单)
2. **先独立后集成**: 先迁移 Video Info (无异步依赖)
3. **最后复杂**: Progress Section涉及多线程同步

---

## 十、替代方案

如果 Dioxus 迁移工作量过大，考虑：

1. **egui** — 性能最优，即时模式，但 UI 风格偏向工具类
2. **保留 Slint + 优化**: 不迁移，优化现有 Slint 实现
3. **分离 UI进程**: 将渲染放到独立进程，通过 IPC 通信