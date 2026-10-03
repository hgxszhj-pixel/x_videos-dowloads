# 2026-09-14 — SHANHU batch_video_state 批量下载按钮 0% pending 永久卡死 BUG-A 修复 (commit `33f02f6`)


### 1. 症状
用户点新 `batch_video_state` 面板 "▶ 全部下载" → BatchState UI 永远停在 "0% pending"。3 条 `get_video_info` 解析成功（m3u8 length 101），下载器日志也出，但 BatchState 进度条不动。

### 2. 根因 — 异步 UI 状态机复制漏步
复制旧 Batch Download 入口 (`app.rs:498-540`) 到新 `batch_video_state` 入口 (`app_batch_video.rs:132-171`) 时漏了第 4 步 `poller.send(PollMsg::Batch { progress_arcs, total_count })`。

`use_progress_poller` 是 `Coroutine<PollMsg>`，只在收 `PollMsg::Batch` 后才进入批量轮询循环（`poller.rs:101-159`）。新代码没发这条消息 → poller 永远 idle → `batch_state.items[i].progress` 永远 = 0.0 → UI 卡 "0% pending"。

下载器 (`batch_download_impl` → `start_download_impl`) 内部 `progress_arc.store(percent, Ordering::Relaxed)` 真在写原子 — 进度条修复后实际验证见 2026-09-14 (cont.) BUG-B/C session。

### 3. 异步 UI 状态机复制 5 步清单

| # | 步骤 | `app.rs:498-540` 旧入口 | `app_batch_video.rs:132-171` 新入口 |
|---|------|---|---|
| 1 | 创建 atomic arcs (progress/speed/cancel) | ✅ L498-500 | ✅ L145-147 |
| 2 | 初始化 `batch_state` | ✅ L514-520 | ✅ L156-162 |
| 3 | `is_downloading = true`（poller gate） | ✅ L522 | ✅ L164 |
| **4** | **poller.send(PollMsg::Batch)** | **✅ L529-532** | **❌ 漏** ← BUG-A |
| 5 | `spawn(batch_download_impl(...))` | ✅ L535-540 | ✅ L167-171 |

**规则 #18**:复制异步 UI 状态机入口时，镜像完整 N 步清单；任何「依赖外部 Coroutine 收消息才进入轮询」的入口必须显式 `poller.send`，否则 poller 永远 idle，UI 进度原子无消费者。

### 4. 修复（`33f02f6`）
- `BatchVideoPanel` 加 `poller: Coroutine<PollMsg>` prop
- `app.rs:399` 调用处透传 `poller`
- `app_batch_video.rs:166` 之前补 `poller.send(PollMsg::Batch { progress_arcs_for_poll, total_count })`
- `handlers.rs:127` 加 `[batch_download]` sub-task 入口/出口/失败 4 条 tracing 日志

### 5. BUG-B / BUG-C (同日同会话触发并修复)
`fetch_video_info_impl` 失败路径不写回 store 的问题在用户实测时暴露 — 同日触发并修复,见 [## 最新 Session: 2026-09-14 (cont.)](#) (`b96a104` + `286accd`),3 轮 ultragoal 迭代命中 3 个独立 bug。

### 6. 验证
- `cargo check` ✅ (只有 block 0.1.6 future-incompat 与本次无关)
- `cargo test --bin` ✅ **287 passed; 0 failed** (无回归)
- `cargo clippy --bin` ✅ clean
- `git diff --stat` ✅ 3 文件 +59/-5 (surgical)
- 用户实测: BUG-A 关闭 (进度条动),但暴露 BUG-B (status 永远 completed) + BUG-C (success 标志丢失),同日修复见新 session;再跑 3-item 又暴露 BUG-D (4m27s 无 sub-task 启动 日志),诊断 trace 见 cont. 2 session

### 7. 验证日志格式 (sub-task 4 条,实测 3 条齐全)
跑 GUI "▶ 全部下载"，日志应出现:
```
[batch_download] sub-task 启动 idx=0 url=https://wx-bgfcbieh.rexxi.cn/PC/#/yueke3/3529714?chapter=3557155
[batch_download] fetch 成功 idx=0 title=xxx resolved_url=https://...m3u8
[batch_download] 下载完成 idx=0 url=...    (或 [batch_download] 下载失败 idx=0 url=... err=...)
```
3 段齐全 = BUG-A 关闭。第 4 段失败日志格式见 §8。

### 8. 诊断日志格式（sub-task 统一约定）
```
[batch_download] sub-task 启动 idx={} url={}
[batch_download] fetch 成功 idx={} title={} resolved_url={}
[batch_download] 下载完成 idx={} url={}
[batch_download] 下载失败 idx={} url={} err={}
[batch_download] 解析失败 idx={} url={} err={}
```
前缀 `[<feature_name>]` 让 `tracing` 输出可 grep 快速过滤。

### 9. 关联
- 教训 #14「第三方 API 响应字段格式永远不能硬编码假设」→ BUG-A 是它的 GUI 层变种（**第三方协议不仅指 HTTP API，也指 Coroutine 协议**）
- `docs/dna/review-checklist.md` → 待加「异步 UI 入口复制」专项 checklist
- `docs/dna/三档地盘.md` → batch_video_state / poller 标 🟡 审查档（GUI runtime 难单测覆盖）

---

