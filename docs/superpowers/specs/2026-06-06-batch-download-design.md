#批量下载增强设计方案

**项目**: x_videos-dowloads  
**日期**: 2026-06-06  
**目标**: Phase 4批量下载增强 — 多视频并行下载 + 独立进度显示

---

## 1. 背景与目标

当前批量下载（gui.rs:522）为**串行 for 循环**，一个接一个下载，多视频用户无法同时获取多个进度。

**本方案目标**:
1. 多视频**真正并行**下载（并发数用户可配置 1-N）
2. 每个视频**独立进度**显示（标题 + 进度条 + 速度 + 状态）
3. 失败后**继续全部跑完**，完成后统一显示失败列表（可重试）

---

## 2. 设计决策

|决策点 | 选择 |
|--------|------|
| 调度器位置 | GUI 层新建 `BatchManager`，不依赖 `DownloadQueue` |
| 并发控制 | `tokio::sync::Semaphore`，N 由用户配置 |
| 失败策略 | 遇错继续，完成后统一失败列表 + 重试 |
| 进度展示 | 列表模式（ListView），每行独立进度 |

---

## 3. 架构设计

### 3.1 BatchManager（新建于 gui.rs 顶部或独立模块）

```rust
use tokio::sync::{Semaphore, RwLock};
use std::sync::Arc;
use futures::future::join_all;

// ============================================================================
// BatchItem - 单个视频的下载状态
// ============================================================================
#[derive(Debug, Clone)]
pub struct BatchItem {
    pub url: String,
    pub title: String,
    pub status: BatchItemStatus,
    pub progress: f32,         // 0.0 - 100.0
    pub speed: u64,            // bytes/s
    pub bytes_downloaded: u64,
    pub total_bytes: u64,
    pub error_message: Option<String>,
    // 每个视频独立的进度原子量，供 UI 轮询使用
    pub progress_arc: Arc<AtomicU32>,
    pub speed_arc: Arc<AtomicU64>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BatchItemStatus {
    Pending,
    Downloading,
    Paused,
    Completed,
    Failed(String),
    Cancelled,
}

// ============================================================================
// BatchManager - 并行调度器
// ============================================================================
#[derive(Debug)]
pub struct BatchManager {
    items: RwLock<Vec<BatchItem>>,
    concurrency: usize,
    sem: Semaphore,
}

impl BatchManager {
    pub fn new(concurrency: usize) -> Self {
        Self {
            items: RwLock::new(Vec::new()),
            concurrency,
            sem: Semaphore::new(concurrency),
        }
    }

    /// 添加视频到待下载列表
    pub fn add(&self, url: String, title: String) {
        let item = BatchItem {
            url,
            title,
            status: BatchItemStatus::Pending,
            progress: 0.0,
            speed: 0,
            bytes_downloaded: 0,
            total_bytes: 0,
            error_message: None,
            progress_arc: Arc::new(AtomicU32::new(0)),
            speed_arc: Arc::new(AtomicU64::new(0)),
        };
        self.items.write().push(item);
    }

    ///启动批量下载
    pub async fn start(&self) {
        let items = self.items.read().clone();
        let futures: Vec<_> = items.iter().map(|item| {
            let sem = self.sem.clone();
            let item = item.clone();
            async move {
                // 获取并发许可
                let _permit = sem.acquire_owned().await.unwrap();
                // 执行下载
                self.download_single(&item).await
            }
        }).collect();
        join_all(futures);
    }

    /// 重试失败项
    pub async fn retry_failed(&self) {
        let items = self.items.read();
        let failed: Vec<_> = items.iter()
            .filter(|i| matches!(i.status, BatchItemStatus::Failed(_)))
            .cloned()
            .collect();
        drop(items);

        for item in failed {
            item.status = BatchItemStatus::Pending;
            item.progress = 0.0;
            item.error_message = None;
            //重新调度...
        }
    }

    /// 获取所有项（供 UI 绑定）
    pub fn get_items(&self) -> Vec<BatchItem> {
        self.items.read().clone()
    }

    /// 统计信息
    pub fn stats(&self) -> BatchStats {
        let items = self.items.read();
        BatchStats {
            total: items.len(),
            pending: items.iter().filter(|i| i.status == BatchItemStatus::Pending).count(),
            downloading: items.iter().filter(|i| i.status == BatchItemStatus::Downloading).count(),
            completed: items.iter().filter(|i| i.status == BatchItemStatus::Completed).count(),
            failed: items.iter().filter(|i| matches!(i.status, BatchItemStatus::Failed(_))).count(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct BatchStats {
    pub total: usize,
    pub pending: usize,
    pub downloading: usize,
    pub completed: usize,
    pub failed: usize,
}
```

### 3.2 并发控制（Semaphore 模式）

```rust
//核心调度伪代码
let futures: Vec<_> = self.items.read().iter().map(|item| {
    let sem = self.sem.clone();
    let item = item.clone();
    async move {
        let _permit = sem.acquire_owned().await;
        download_with_progress(item).await
    }
}).collect();

futures::join_all(futures); // 并行执行，直到全部完成或被 cancel
```

- `Semaphore::new(N)` 控制同时最大 N 个下载
- 用户通过 GUI 设置项配置 N 值（1-10）
- 每个下载任务独立 `progress_arc` / `speed_arc`，互不干扰

### 3.3 进度聚合 → UI 更新

```rust
// UI 轮询线程（每 200ms）
fn poll_and_update_ui(app: &AppWindow, manager: &BatchManager) {
    let items = manager.get_items();
    let json_items: Vec<serde_json::Value> = items.iter().map(|item| {
        serde_json::json!({
            "title": item.title,
            "status": format!("{:?}", item.status),
            "progress": item.progress_arc.load(Ordering::Relaxed) as f32 / 100.0,
            "speed": item.speed_arc.load(Ordering::Relaxed),
        })
    }).collect();
    app.set_download_list_progress(serde_json::to_string(&json_items).unwrap());
}
```

---

## 4. GUI 改造

### 4.1 Slint UI 列表项结构

```slint
struct BatchListItem {
    title: string,
    status: string,
    progress: float,
    speed: string,
    error: string,
}
```

### 4.2 列表渲染

```slint
ListView {
    property<[BatchListItem]> items;

    for item in items {
        HorizontalLayout {
            Text { text: item.title; }
            ProgressBar { progress: item.progress; }
            Text { text: item.speed; }
            Text { text: item.status; }
        }
    }
}
```

### 4.3 并发数设置

在 GUI 设置面板增加滑块：

```slint
Slider {
    value: concurrency;
    minimum: 1;
    maximum: 10;
    changed => { app.set_concurrency(self.value); }
}
Text { text: "并发数: " + concurrency; }
```

---

## 5. 错误处理策略

| 场景 | 行为 |
|------|------|
| 单视频下载失败 | `status = Failed(err)`，继续下载后续 |
| 全部下载完成 | 显示完成统计 + 失败列表 |
| 用户点击"重试" | 仅对 Failed 状态项重新调度 |
| 用户取消 | 中断所有正在下载的任务 |

### 失败列表展示

下载完成后，GUI 弹出失败面板：

```slint
Rectangle {
    if show_failures_panel {
        ColumnLayout {
            Text { text: "失败视频 (" + failed_count + ")"; }
            for item in failed_items {
                Text { text: item.title; }
                Text { text: item.error_message; }
                Button { text: "重试"; }
            }
            Button { text: "关闭"; }
        }
    }
}
```

---

## 6. 实现步骤

### Phase 4.1 — BatchManager 核心
1. 新建 `BatchItem` / `BatchManager` 结构
2. 实现 `add()` / `get_items()` / `stats()`
3. Semaphore 并发控制逻辑
4. 单视频下载 + 进度回调集成

### Phase 4.2 — GUI 集成
1. Slint ListView 绑定
2. UI 轮询更新
3. 并发数设置滑块
4. 失败列表弹窗

### Phase 4.3 — 错误处理 + 重试
1. 失败策略实现（继续全部跑完）
2. 重试逻辑
3. 取消逻辑

---

## 7. 测试计划

| 测试 | 方法 |
|------|------|
| 并发数生效 | 配置 N=2，提交 5 个视频，验证同时只有 2 个下载中 |
| 失败继续 | 5 个视频中第2 个失败，验证其余 3 个继续下载 |
| 进度独立 | 5 个视频同时下载，验证每个进度独立更新 |
| 重试成功 | Failed 状态项点击重试后成功下载 |
| UI 更新 | 轮询线程正确刷新 ListView 数据 |

---

## 8. 文件变更

| 文件 |变更 |
|------|------|
| `src/gui.rs` | 新增 `BatchManager` / `BatchItem`，改造批量下载逻辑 |
| `ui/app.slint` | 新增 ListView /失败弹窗 / 并发数设置 |

---

**设计完成，待用户确认后进入实现阶段。**