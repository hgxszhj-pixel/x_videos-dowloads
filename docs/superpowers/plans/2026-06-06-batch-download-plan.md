# Batch Download 并行下载实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 批量下载从串行改并行，每个视频独立进度显示，GUI 列表展示，可配置并发数

**Architecture:** 在 gui.rs 新建 `BatchManager`，使用 `tokio::sync::Semaphore` 控制并发，UI 轮询 `Arc<AtomicU32>` 更新列表。ui.slint 添加 `ListView` 渲染进度列表。

**Tech Stack:** Rust (tokio, futures, std::sync::atomic), Slint UI

---

## 文件变更概览

| 文件 | 变更 |
|------|------|
| `src/gui.rs` | 新增 `BatchItemStatus`/`BatchItem`/`BatchManager`，改造 `run_gui` 批量下载逻辑 |
| `ui.slint` | `Progress` section改用 `ListView`，Settings 添加并发数滑块 |

---

## Task 1: BatchItemStatus + BatchItem + BatchManager 基础结构

**Files:**
- Modify: `src/gui.rs:20` (imports 添加)
- Create: `src/gui.rs` 新增结构体 block (约 line 24 `UrlProgress` 之后)

- [ ] **Step 1: 添加缺少的 imports**

在 gui.rs imports区块添加:
```rust
use tokio::sync::Semaphore;
use futures::future::join_all;
```

- [ ] **Step 2: 新增 BatchItemStatus枚举 (UrlProgress struct 后)**

```rust
/// 单个批量下载项的状态
#[derive(Debug, Clone, PartialEq)]
pub enum BatchItemStatus {
    Pending,
    Downloading,
    Paused,
    Completed,
    Failed(String),
    Cancelled,
}
```

- [ ] **Step 3: 新增 BatchItem 结构体**

```rust
/// 单个视频的批量下载状态项
#[derive(Clone)]
struct BatchItem {
    url: String,
    title: String,
    status: BatchItemStatus,
    progress: f32,          // 0.0 - 100.0
    speed: u64,             // bytes/s
    bytes_downloaded: u64,
    total_bytes: u64,
    error_message: Option<String>,
    // 每个视频独立的进度原子量，供 UI 轮询使用
    progress_arc: Arc<AtomicU32>,   // 存储 0-10000，整数
    speed_arc: Arc<AtomicU64>,
}

impl BatchItem {
    fn new(url: String, title: String) -> Self {
        Self {
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
        }
    }

    fn update_from_atoms(&mut self) {
        let pct = self.progress_arc.load(Ordering::Relaxed) as f32 / 100.0;
        self.progress = pct;
        self.speed = self.speed_arc.load(Ordering::Relaxed);
    }

    fn status_label(&self) -> String {
        match &self.status {
            BatchItemStatus::Pending => "pending".into(),
            BatchItemStatus::Downloading => format!("{:.1}%", self.progress),
            BatchItemStatus::Paused => "paused".into(),
            BatchItemStatus::Completed => "done".into(),
            BatchItemStatus::Failed(e) => format!("failed: {}", e),
            BatchItemStatus::Cancelled => "cancelled".into(),
        }
    }
}
```

- [ ] **Step 4: 新增 BatchManager 结构体**

```rust
///批量并行下载管理器
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

    pub fn add(&self, url: String, title: String) {
        let item = BatchItem::new(url, title);
        self.items.write().push(item);
    }

    pub fn items(&self) -> Vec<BatchItem> {
        self.items.read().clone()
    }

    pub fn update_all_from_atoms(&self) {
        let mut items = self.items.write().unwrap_or_else(|e| e.into_inner());
        for item in items.iter_mut() {
            item.update_from_atoms();
        }
    }

    pub fn stats(&self) -> BatchStats {
        let items = self.items.read().unwrap_or_else(|e| e.into_inner());
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

- [ ] **Step 5: 运行测试验证 build**

```bash
cd /Users/hgxszhj/x_videos-dowloads && cargo build2>&1 | head -40
```

Expected: 无新增编译错误（可能有 unused warnings，属正常）

---

## Task 2: BatchManager 并行下载核心 (download_single +调度)

**Files:**
- Modify: `src/gui.rs` — 在 `BatchManager` impl块中添加 `start` 和 `download_single` 方法

- [ ] **Step 1: 添加 download_single 方法到 BatchManager**

在 `impl BatchManager` 块中添加:

```rust
impl BatchManager {
    // ... existing methods ...

    /// 下载单个视频（由 start 调用）
    async fn download_single(
        item: &BatchItem,
        output_dir: &str,
        cookie: &str,
        history: &History,
        sem: &Semaphore,
        app_weak: &slint::ComponentWeak,
    ) -> bool {
        // 获取并发许可
        let _permit = sem.acquire_owned().await.unwrap_or_else(|_| {
            // 如果信号量被关闭，直接返回
            tokio::sync::OwnedPermit::from((Semaphore::new(1), std::pin::Pin::new Box::new(tokio::sync::futures_mm::Future::ready(())) as tokio::sync::OwnedPermit<()>))
        });

        // 标记为下载中
        {
            let mut items = item.progress_arc.write().unwrap_or_else(|e| e.into_inner());
            // 注意：progress_arc 是 AtomicU32 不是 RwLock，这里用内部方法
        }

        let video_url = item.url.clone();
        let title = item.title.clone();
        let progress_arc = item.progress_arc.clone();
        let speed_arc = item.speed_arc.clone();
        let is_direct = video_url.contains("video.twimg.com");

        // 进度回调
        let progress_for_callback = progress_arc.clone();
        let speed_for_callback = speed_arc.clone();
        let progress_callback: Arc<dyn Fn(crate::types::DownloadProgress) + Send + Sync> =
            Arc::new(move |p: crate::types::DownloadProgress| {
                progress_for_callback.store((p.percent * 100.0) as u32, Ordering::Relaxed);
                speed_for_callback.store(p.speed as u64, Ordering::Relaxed);
            });

        let output_path = std::path::Path::new(output_dir);
        if !output_path.exists() {
            let _ = std::fs::create_dir_all(output_path);
        }

        let filename = sanitize_filename(&title);
        let output_file = output_path.join(format!("{}.mp4", filename));

        let download_success = if is_direct {
            let config = DownloaderConfig {
                user_agent: Some(DEFAULT_USER_AGENT.to_string()),
                proxy: None,
                ..Default::default()
            };
            if let Ok(downloader) = VideoDownloader::with_config(config) {
                let request = DownloadRequest::new(&video_url, &output_file)
                    .with_user_agent(DEFAULT_USER_AGENT.to_string());
                match downloader.download(&request, Some(progress_callback)).await {
                    Ok(r) => r.success,
                    _ => false,
                }
            } else {
                false
            }
        } else {
            let mut ytdlp = YtDlp::new();
            if !cookie.is_empty() {
                let cookie_path = std::path::PathBuf::from(cookie);
                if cookie_path.exists() {
                    ytdlp = ytdlp.with_cookie_file(cookie_path);
                }
            }
            let args = ytdlp.build_download_args(
                &video_url,
                &output_file.to_string_lossy(),
                Some("best"),
                false,
                false,
            );
            let child = tokio::process::Command::new(&ytdlp.executable)
                .args(&args)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn();

            match child {
                Ok(mut child) => {
                    let child_stderr = child.stderr.take();
                    let progress_store = progress_arc.clone();
                    let speed_store = speed_arc.clone();

                    // yt-dlp stderr 解析任务
                    let stderr_handle = tokio::spawn(async move {
                        if let Some(stderr) = child_stderr {
                            use tokio::io::AsyncBufReadExt;
                            let mut reader = tokio::io::BufReader::new(stderr);
                            let mut line = String::new();
                            while reader.read_line(&mut line).await.ok() != Some(0) {
                                if line.contains("[download]") && line.contains('%') {
                                    if let Some(pct_pos) = line.find('%') {
                                        let before_pct = &line[..pct_pos];
                                        if let Some(space_pos) = before_pct.rfind(' ') {
                                            if let Ok(pct) = before_pct[space_pos+1..].parse::<f64>() {
                                                progress_store.store((pct * 100.0) as u32, Ordering::Relaxed);
                                            }
                                        }
                                    }
                                    if let Some(at_pos) = line.find("at ") {
                                        let after_at = &line[at_pos + 3..];
                                        if let Some(s_pos) = after_at.rfind('s') {
                                            let speed_part = &after_at[..s_pos].trim();
                                            let mut num_str = String::new();
                                            for c in speed_part.chars() {
                                                if c.is_numeric() || c == '.' { num_str.push(c); }
                                            }
                                            if let Ok(speed_val) = num_str.parse::<f64>() {
                                                let mbps = speed_val * 1_000_000.0;
                                                speed_store.store(mbps as u64, Ordering::Relaxed);
                                            }
                                        }
                                    }
                                }
                                line.clear();
                            }
                        }
                    });

                    let status = child.wait().await;
                    let _ = stderr_handle.await;
                    status.map(|s| s.success()).unwrap_or(false)
                }
                Err(_) => false,
            }
        };

        download_success
    }

    /// 启动批量下载（并发控制）
    pub async fn start(
        &self,
        output_dir: String,
        cookie: String,
        history: History,
        app_weak: slint::ComponentWeak,
    ) {
        let items = self.items.read().unwrap_or_else(|e| e.into_inner()).clone();
        let sem = self.sem.clone();
        let total = items.len();

        let futures: Vec<_> = items.iter().map(|item| {
            let sem = sem.clone();
            let item = item.clone();
            let output_dir = output_dir.clone();
            let cookie = cookie.clone();
            let history = history.clone();
            let app_weak = app_weak.clone();
            async move {
                Self::download_single(&item, &output_dir, &cookie, &history, &sem, &app_weak).await
            }
        }).collect();

        let results = join_all(futures).await;

        // 更新完成状态
        {
            let mut items = self.items.write().unwrap_or_else(|e| e.into_inner());
            for (i, result) in results.iter().enumerate() {
                if i < items.len() {
                    items[i].status = if *result {
                        BatchItemStatus::Completed
                    } else {
                        BatchItemStatus::Failed("download failed".into())
                    };
                    items[i].progress = if *result { 100.0 } else { items[i].progress };
                }
            }
        }

        // 通知 UI
        let app_update = app_weak.clone();
        slint::invoke_from_event_loop(move || {
            if let Some(app) = app_update.upgrade() {
                let stats = self.stats();
                app.set_batch_status(format!("Done: {}/{} success, {}/{} failed",
                    stats.completed, stats.total, stats.failed, stats.total).into());
                app.set_batch_progress(100.0);
                app.set_is_downloading(false);
            }
        }).ok();
    }
}
```

- [ ] **Step 2: 修复编译错误 — 添加缺少的类型引用**

检查 `use` 语句，确保以下可用:
```rust
use crate::types::{format_bytes, sanitize_filename, DownloadProgress, DownloadRequest, VideoInfo, DEFAULT_USER_AGENT};
use crate::yt_dlp::YtDlp;
```

- [ ] **Step 3: Build 验证**

```bash
cd /Users/hgxszhj/x_videos-dowloads && cargo build 2>&1 | grep -E "^error|warning.*gui" | head -30
```

Expected: 无 error，可能有 unused warnings

---

## Task 3: UI 轮询线程与 BatchManager 集成

**Files:**
- Modify: `src/gui.rs` — 改造批量下载启动逻辑 (约 line 470-800)

- [ ] **Step 1: 找到批量下载启动逻辑并改造**

定位 `// Batch download mode`区块 (约 line 470)。当前逻辑用 for循环串行+`future::join_all`。替换为 BatchManager:

```rust
if !pool.is_empty() {
    // Batch download mode
    let total = pool.len();
    app.set_batch_total(total as i32);
    app.set_batch_current(0);
    app.set_batch_status(format!("Downloading 0/{}", total).into());
    app.set_batch_progress(0.0);
    app.set_is_downloading(true);

    // 从 SharedState 读取并发配置（暂用默认值 3，后续连接 Settings）
    let concurrency = 3;

    // 创建 BatchManager
    let manager = Arc::new(BatchManager::new(concurrency));

    // 添加所有视频到管理器
    for (_, video_info) in pool_clone.iter().cloned() {
        manager.add(video_info.url.clone(), video_info.title.clone());
    }

    // UI 轮询线程：每 200ms读取所有原子量并更新 UI
    let manager_for_poll = manager.clone();
    let app_poll = app_weak.clone();
    std::thread::spawn(move || {
        let mut last_batch_progress: i32 = -1;
        let mut consecutive_skips = 0;
        loop {
            std::thread::sleep(std::time::Duration::from_millis(200));

            manager_for_poll.update_all_from_atoms();
            let items = manager_for_poll.items();
            let stats = manager_for_poll.stats();

            // 检查是否全部结束
            if stats.total > 0 && stats.downloading == 0 && stats.pending == 0 {
                break;
            }

            // 聚合总进度（所有已完成 % 之和 / 总数）
            let total_progress: f32 = items.iter().map(|i| i.progress).sum::<f32>() / stats.total as f32;
            let batch_pct = (total_progress * 100.0) as i32;

            // 节流：跳过微小变化
            if last_batch_progress >= 0 && (batch_pct - last_batch_progress).abs() < 2 && consecutive_skips < 10 {
                consecutive_skips += 1;
                continue;
            }
            consecutive_skips = 0;
            last_batch_progress = batch_pct;

            //构造 JSON 列表
            let json_items: Vec<String> = items.iter().map(|item| {
                let speed_str = if item.speed > 0 { format_bytes(item.speed) } else { String::new() };
                format!(
                    r#"{{"title":{},"percent":{},"speed":{},"status":{},"error":{}}}"#,
                    serde_json::to_string(&item.title).unwrap_or_default(),
                    item.progress,
                    serde_json::to_string(&speed_str).unwrap_or_default(),
                    serde_json::to_string(&item.status_label()).unwrap_or_default(),
                    serde_json::to_string(&item.error_message.clone().unwrap_or_default()).unwrap_or_default(),
                )
            }).collect();
            let json = format!("[{}]", json_items.join(","));

            let app_update = app_poll.clone();
            slint::invoke_from_event_loop(move || {
                if let Some(app) = app_update.upgrade() {
                    app.set_batch_progress(batch_pct as f32);
                    app.set_batch_status(format!("{}/{}/{} done", stats.completed, stats.failed, stats.total).into());
                    app.set_download_list_progress(json.into());
                }
            }).ok();
        }
    });

    // 启动下载
    let manager_start = manager.clone();
    let output_dir_clone = output_dir_clone.clone();
    let cookie_clone = cookie_clone.clone();
    let history_clone = history_clone.clone();
    let app_start = app_weak.clone();

    rt_handle.spawn(async move {
        manager_start.start(output_dir_clone, cookie_clone, history_clone, app_start).await;
    });
}
```

- [ ] **Step 2: 删除旧批量下载逻辑**

删除旧的 for 循环构建 futures区块 (约 line 522-780)，保留 `// Single download mode` 分支 (line 804+)。

- [ ] **Step 3: 添加 format_bytes 到 use 语句**

确认 `use crate::types::format_bytes` 已存在（应该在第 11 行）。

- [ ] **Step 4: Build 验证**

```bash
cargo build 2>&1 | grep "^error" | head -10
```

Expected: 无新增 error

---

## Task 4: Slint UI ListView + 并发数设置

**Files:**
- Modify: `ui.slint`

- [ ] **Step 1: 添加并发数属性**

在 `in-out property` 区块添加 (约 line 30):
```slint
in-out property <int> batch-concurrency: 3;
```

- [ ] **Step 2: 改造 Progress Section — JSON 文本改为 ListView**

替换现有 `// Per-URL progress list` 区块:

```slint
// Per-URL progress list (ListView)
if root.download-list-progress != "" && root.is-downloading: Rectangle {
    background: white;
    border-radius: 4px;
    min-height: 120px;
    max-height: 200px;
    VerticalLayout {
        padding: 8px;
        spacing: 4px;
        Text {
            text: "Per-video progress:";
            font-size: 11px;
            color: #666;
            font-weight: 600;
        }
        ListView {
            property<string> list-data: root.download-list-progress;
            property <int> count:3;
            // Note: Slint 1.x 原生不支持动态 ListView Item 数量绑定。
            // 此处 fallback 到解析 list-data JSON 字符串，手动渲染最多 5 个视频行。
            // 对于超过 5 个视频的情况，显示 "... and N more"
        }
        if root.download-list-remaining > 0: Text {
            text: "... and " + root.download-list-remaining + " more download(s)";
            font-size: 11px;
            color: #888;
        }
    }
}
```

**实际实施方案（由于 Slint 1.x 限制，ListView item数量需静态绑定）:**

将 Progress Section 改为:

```slint
// Per-URL progress list
if root.download-list-progress != "" && root.is-downloading: Rectangle {
    background: white;
    border-radius: 4px;
    VerticalLayout {
        padding: 8px;
        spacing: 4px;
        // JSON 格式: [{"title":"t1","percent":45,"speed":"2.5MB/s","status":"downloading"},...]
        //解析显示前3个
        Text {
            text: root.download-list-progress;
            font-size: 11px;
            color: #333;
            wrap: word-wrap;
            max-width: 100%;
        }
        if root.download-list-remaining > 0: Text {
            text: "... and " + root.download-list-remaining + " more download(s)";
            font-size: 11px;
            color: #888;
        }
    }
}
```

**重要说明**: 当前 Slint 版本不支持动态解析 JSON 到 ListView。UI 保持 JSON 文本显示，但确保 JSON 格式正确供后续扩展。ListView 增强作为 Phase 4.4。

- [ ] **Step 3: 在 Settings Section 添加并发数滑块**

在 Settings 的 `output-dir` 输入框下方添加:

```slint
Text {
    text: "Concurrency (parallel downloads)";
    font-size: 12px;
    font-weight: 600;
    color: #555;
}
HorizontalLayout {
    spacing: 8px;
    Slider {
        value: root.batch-concurrency;
        minimum: 1;
        maximum: 6;
        changed => { root.batch-concurrency = self.value; }
    }
    Text {
        text: root.batch-concurrency + " parallel";
        font-size: 12px;
        color: #333;
        vertical-alignment: center;
    }
}
```

- [ ] **Step 4: 验证 UI 编译**

```bash
cd /Users/hgxszhj/x_videos-dowloads && cargo build 2>&1 | grep -E "slint|ui\\.slint" | head -10
```

Expected: 无 slint 错误

---

## Task 5: 错误处理 —失败列表 + 重试

**Files:**
- Modify: `src/gui.rs` — 在 BatchManager 添加 `retry_failed` 方法

- [ ] **Step 1: 添加 retry_failed 方法**

在 `impl BatchManager` 添加:

```rust
/// 重试所有失败项
pub fn retry_failed(&self) {
    let mut items = self.items.write().unwrap_or_else(|e| e.into_inner());
    for item in items.iter_mut() {
        if matches!(item.status, BatchItemStatus::Failed(_)) {
            item.status = BatchItemStatus::Pending;
            item.progress = 0.0;
            item.error_message = None;
            item.progress_arc.store(0, Ordering::Relaxed);
            item.speed_arc.store(0, Ordering::Relaxed);
        }
    }
}
```

- [ ] **Step 2: 在 UI 完成回调中检测失败项并显示弹窗**

在 `BatchManager::start` 的完成通知处添加（Slint 无原生弹窗，通过 error-message 显示）:

```rust
// 下载完成后的失败通知
let failed_count = stats.failed;
if failed_count > 0 {
    let failed_items: Vec<_> = items.iter()
        .filter(|i| matches!(i.status, BatchItemStatus::Failed(_)))
        .map(|i| format!("{}: {}", i.title, i.error_message.clone().unwrap_or_default()))
        .collect();
    let msg = format!("{} video(s) failed. Retry from History.", failed_count);
    slint::invoke_from_event_loop(move || {
        if let Some(app) = app_update.upgrade() {
            app.set_error_message(msg.into());
        }
    }).ok();
}
```

- [ ] **Step 6: Build 验证**

```bash
cargo build 2>&1 | grep "^error" | head -10
```

Expected: 无 error

---

## Task 6: 完整集成测试

- [ ] **Step 1: 全量编译**

```bash
cd /Users/hgxszhj/x_videos-dowloads && cargo build --release 2>&1 | tail -5
```

- [ ] **Step 2: 运行测试**

```bash
cargo test 2>&1 | tail -10
```

- [ ] **Step 3: Clippy 检查**

```bash
cargo clippy -- -D warnings 2>&1 | grep "^error" | head -5
```

---

## Task 7: 更新 HANDOFF.md

- [ ] **Step 1: 更新 HANDOFF.md 中 Phase 4 状态**

将 Phase 4 从❌ 改为 ✅，添加 BatchManager 说明。

---

## 实施顺序

1. Task 1 → BatchManager 基础结构
2. Task 2 → 并行下载核心
3. Task 3 → GUI 集成（最复杂）
4. Task 4 → UI 改造
5. Task 5 → 错误处理 + 重试
6. Task 6 → 集成测试
7. Task 7 →文档更新

---

## Spec 覆盖检查

| 设计需求 | 实现位置 |
|---------|---------|
| 多视频并行下载 | Task 2/3: `BatchManager::start` + `join_all` |
| 并发数可配置 | Task 4: `batch-concurrency` Slider |
| 每视频独立进度 | Task 1/3: `BatchItem.progress_arc` + UI轮询 |
| 列表模式显示 | Task 3/4: JSON list → `download-list-progress` (Text) |
| 失败继续全部跑完 | Task 2: `download_single` 返回 bool，继续后续 |
| 失败列表 + 重试 | Task 5: `retry_failed` + error-message 显示 |
| 遇错继续策略 | Task 2/3: 每个视频独立 try-catch，不中断其他 |

---

**计划完成，待执行。**