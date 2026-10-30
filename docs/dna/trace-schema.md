# Trace 数据模型 (Instrumentation Schema)

> **起源**: 2026-09-17 采纳 LangSmith Trace 思路,把"AI Agent 调试"反着用 — 调试 AI Agent 写的下载代码。
> **目的**: 把 BUG 复发定位从"事后猜 1 小时" → "事前自动采,事后 grep 5 分钟"。
> **对应决策**: `~/memory/decisions.md:2026-09-17 — TokenPilot (zjunlp/LightRSI) **不安装**` 的反面(我们用本地 trace 替代 LangSmith)。

---

## 核心 5 元组 (Schema)

每个 trace 事件就是一行 NDJSON,5 个核心字段 + N 个扩展字段:

| 字段 | 类型 | 必填 | 含义 |
|------|------|------|------|
| `ts` | string (RFC3339 nano) | ✅ | 时间戳,精确到 ns(用于串行链路分析) |
| `session_id` | string (ULID) | ✅ | 整个调用链共享一个 session_id(用于 trace grep) |
| `step` | string (kebab-case) | ✅ | 步骤名,如 `fetch_video_info` / `resolver_try` / `download_one` |
| `input` | object | ✅ | 入参(URL / 路径 / 配置,需 sanitize 敏感字段) |
| `output` | object | ✅ | 出参(状态 / 字节数 / 错误码) |
| `duration_ms` | u64 | ✅ | 该步骤耗时(毫秒) |
| `extra` | object | ⬜ | 扩展字段(cookies_count / resolver_idx / http_status 等) |

### 完整 schema 例子

```json
{"ts":"2026-09-17T14:23:01.234567Z","session_id":"01JB5K8Z9R3X7V4Y","step":"resolver_try","input":{"url":"https://...","cookies":0,"timeout":20000},"output":{"status":"timeout","err":null},"duration_ms":20001,"extra":{"resolver_idx":0,"resolver_name":"generic_resolver"}}
{"ts":"2026-09-17T14:23:21.235891Z","session_id":"01JB5K8Z9R3X7V4Y","step":"resolver_try","input":{"url":"https://...","cookies":0,"timeout":20000},"output":{"status":"ok","err":null},"duration_ms":5421,"extra":{"resolver_idx":1,"resolver_name":"zhixueyun_resolver"}}
{"ts":"2026-09-17T14:23:26.756012Z","session_id":"01JB5K8Z9R3X7V4Y","step":"fetch_video_info","input":{"url":"https://..."},"output":{"status":"ok","video_count":24},"duration_ms":25521,"extra":{"resolver_chain_length":2,"final_resolver":"zhixueyun_resolver"}}
```

---

## 文件布局

```
~/.local/share/x_videos-dowloads-trace/
├── 2026-09-17/
│   ├── 01JB5K8Z9R3X7V4Y-batch.ndjson   ← 一个 batch 一个文件
│   └── 01JB5K8Z9R4ABCDEF-single.ndjson
├── 2026-09-18/
│   └── ...
└── .session-meta.json                  ← session_id → 起始时间/CLI args 映射
```

- **NDJSON** = 每行一个 JSON 对象,append-only,grep 友好
- **目录按日期分** = 防止单目录文件过多导致 ls 卡顿
- **session_id 用 ULID** = 26 字符、字典序=时间序(grep 时自然按时间排序)

---

## 4 个故障点 instrument 规范

> 来源: 4 个高发故障点(BUG-B / BUG-C / BUG-D / BUG-E)+ 文章 LangSmith Trace 4 大作用

| # | 故障点 | 文件:行 | trace_event 调用规范 | 防的是什么 |
|---|--------|---------|---------------------|-----------|
| 1 | `fetch_video_info_impl` | `src/gui_dioxus/handlers.rs:349` | step=`fetch_video_info` / extra=`{resolver_chain_length, final_resolver}` | BUG-D: 串行 resolver 卡死,不知卡哪个 |
| 2 | `resolve_items_serially` | `src/gui_dioxus/handlers.rs:782` | step=`resolver_try`(marker)+ `resolver_try_done`(真实耗时)/ extra=`{backend, course_item_count}` | BUG-D: 20s 后到底卡哪个 resolver |
| 3 | `download_one` | `src/gui_dioxus/handlers.rs:971` | step=`download_one` / extra=`{bytes_done, stall_after_ms}` | BUG-E: batch 第 3 个 stall 60s+ 无日志 |
| 4 | `videoAuth` 调用 | `src/main.rs` 智学云逻辑 | step=`video_auth` / extra=`{course_id, code, response_size}` | 智学云 session > 7 天 code=100067 反直觉 |

---

## 2026-10-30 修订:真实耗时 + 嵌套 session 隔离

> **触发**:下载管线「哪个阶段慢」分析被卡住 —— 唯一可信的耗时信号是一个不透明的
> `download_one` 大块(99s–634s),resolver 阶段耗时**根本没被测到**。

### 问题 1:`duration_ms` 写死 0

旧代码 `trace_event("resolver_try", ..., 0, ...)` 把 `duration_ms` 写死 0,
而 `eval_trace.py` 的 stall 判定是 `duration_ms >= 30_000` ——
**resolver 这一步永远不可能被判成 stall**,而 BUG-D 恰恰就是 resolver 链把会话挂住。

### 解法:`TraceStage` 两段式埋点

`src/core/trace.rs` 新增 `TraceStage` RAII 原语:

| API | 行为 |
|-----|------|
| `TraceStage::start(step, input, extra)` | **立刻**写 marker 事件(step=`<step>`, `duration_ms=0`, `output={"phase":"start"}`),返回句柄 |
| `TraceStage::without_marker(done_step, input, extra)` | 不写 marker,只写 done(用于聚合 stage) |
| `TraceStage::finish(output)` | 写 done 事件(step=`<step>_done`),`duration_ms` = **真实**耗时 |
| `TraceStage::discard()` | 放弃这次 stage,**一条事件都不写** |
| `Drop` 兜底 | 没 finish 就 drop(early return / panic)→ 写 done,`output={"status":"abandoned"}` |

**duration 由类型内部用 `Instant` 计算,调用点没有 duration 参数** ——
结构上就写不死 0(类型签名挡住手滑,不是靠 code review 记住)。

**marker 必须保留**:只在结束时写一条事件的话,进程被 kill / 掉电时**一条都没有**,
grep 不到卡在哪个 idx。marker 立刻落盘 = 卡死现场的第一手证据。

### step 清单(2026-10-30)

| step | 何时发 | duration 含义 | 参与 stall 判定 |
|------|--------|--------------|-----------------|
| `resolver_try` | GUI 课节池:每条课节**开始处理**时 | 恒 0(marker) | 名义在列,实际永不命中 |
| `resolver_try_done` | GUI 课节池:每次**真实解析尝试**结束(API / Drission / skipped / exists) | **实测** | ✅ |
| `fetch_video_info` | GUI:单条 URL 解析(TraceStep) | 实测 | ✅ |
| `download_one` | GUI:单个课节下载(TraceStep) | 实测 | ✅ |
| `cli_resolve` | CLI:解析阶段**开始** | 恒 0(marker) | ❌ |
| `cli_resolve_done` | CLI:解析阶段**结束**(聚合) | 实测(聚合) | ❌ 聚合,不重复计 |
| `cli_resolve_attempt` | CLI:单个后端尝试**开始** | 恒 0(marker) | ❌ |
| `cli_resolve_attempt_done` | CLI:单个后端尝试**结束**(fxtwitter/ytdlp/registry/scrapling) | **实测** | ✅ |
| `cli_download_run` | CLI:整次运行**开始** | 恒 0(marker) | ❌ |
| `cli_download_run_done` | CLI:整次运行**结束**(聚合) | 实测(聚合) | ❌ 聚合,不重复计 |

> **聚合 vs 叶子**:stall 判定只看叶子。同一次慢解析会同时命中
> `cli_resolve_attempt_done`(叶子)和 `cli_resolve_done`(聚合),
> 两个都计 = 同一段墙钟耗时被数两遍 ⇒ stall_count 虚高,把「测得更准」误判成「性能退化」。

### 问题 2:嵌套 session 互相覆盖

旧实现只有**一个全局** `SESSION_ID` + 一个全局文件句柄。
`batch_download_impl` 开 "batch" session → `start_download_impl` 又开 "gui-single" session,
**直接把全局文件句柄顶掉**:

- batch 自己的文件只剩一行 `session_open`(句柄已被丢弃,再也写不进去)
- batch 的 `session_close` 落进某个 item 的文件
- `eval_trace.py` 判 batch 文件「有 open 无 close」→ **整个 batch session 被丢弃**

### 解法:session 栈

`src/core/trace.rs` 把全局单例改成 `Vec<TraceSessionState>` 栈:

- `trace_session_open` **压栈**,每个 session 持有**自己的**文件句柄
- `trace_event` 写到**最内层**(当前)session
- `TraceStage` / `TraceStep` 在**构造时**记下 session_id,结束时**按 id 写回**
  (并发场景下这一点很重要:batch 并发 3 个 item,各自的事件不能串台)
- `session_close` **按 id 写回自己的文件**,再从栈里按 id 移除
  (close 顺序未必是栈序 —— 并发时更不是)

### 已知边界(未解决,别当成已修)

- **并发 item session 的归属**:session 栈是**全局**的,不是 task 局部的。
  `batch_download_impl` 并发 3 个 item,各自 `start_download_impl` 开一个 "gui-single"
  session ⇒ 栈里同时有 2–3 个 item session。此时:
  - 普通 `trace_event()` 在**调用时**解析「最内层」⇒ 可能落进另一个 item 的文件;
  - `TraceStep` / `TraceStage` 在**构造时**绑定 session_id ⇒ 构造那一刻的最内层是谁就写给谁,
    同样存在串台窗口。
  - 实测影响面很小:batch 的 3 个 task 几乎同时启动,`fetch_video_info` 的 TraceStep
    都在任何 item session 打开**之前**就构造好了 ⇒ 正常都落在 batch 文件里。
  - 真正要彻底解决需要 **task-local 上下文**(`tokio::task_local!`),本次没做。
    这条是**遗留**问题,不是本次引入的 —— 旧实现只有一个全局 session,串台面更大。
- **CLI 下载的叶子阶段**(manifest / segments / merge)在 `src/commands/download/**`,
  本次未埋点 ⇒ CLI 侧只有 run 级聚合 `cli_download_run_done`,看不到分段耗时。
- **课程池流程本身没有 session**:课节池的事件落在 `no-session/<date>.ndjson` fallback 文件里,
  `eval_trace.py` 判「无生命周期事件」→ 不计入 success_rate / resolver_chain_length 分母
  (stall 仍并入)。给课节池补一个 session 会让 `resolver_chain_length` 从 0 跳到 ~29,
  属于需要重新锁基线的决定,留给上层决策。

### 调用模式(伪代码)

```rust
// 模式 1: 入口处开 trace
let trace_session = trace_session_open(&format!("batch-{}-{}", url, chrono::Utc::now().timestamp()));
let _g = trace_session;  // RAII 关闭

// 模式 2: 步骤边界打 trace(用 guard 自动算 duration_ms)
let _trace = trace_step("fetch_video_info")
    .input(json!({"url": url}))
    .extra(json!({"timeout_ms": 20000}))
    .on_exit(move |output| {
        output.set("video_count", video_count);
    });
```

---

## 不做的事(防止 scope creep)

- ❌ **不上传外网**: 100% 本地 NDJSON,跟"不装 TokenPilot / LangSmith"决策对齐
- ❌ **不引入外部 crate**: `serde_json` 已存在,无新依赖
- ❌ **不动平台 API 契约**: 只在 `handlers.rs` 入口 + 串行 resolver 内打桩,不碰 `src/platform/*.rs`
- ❌ **不做实时分析**: 离线 grep / `jq` 即可,不写聚合 dashboard
- ❌ **不改任何返回值**: trace 是纯旁路,函数签名零变化

---

## 验证标准(实跑 1 次必须满足)

```bash
# 1. trace 文件存在
ls ~/.local/share/x_videos-dowloads-trace/$(date +%Y-%m-%d)/

# 2. NDJSON 每行合法 JSON
jq -e . < trace.ndjson | head -1

# 3. 5 元组字段齐全
jq -r '. | select(.ts == null or .step == null or .duration_ms == null) | "MISSING: \(.ts) \(.step)"' trace.ndjson
# 期望: 无输出

# 4. session_id 跨步骤一致(grep 应有 ≥ 3 行)
grep -c '"session_id":"01JB5K8Z9R3X7V4Y"' trace.ndjson

# 5. BUG-D 复发模拟: 让 resolver 卡 60s,trace 应有 step=resolver_try status=timeout duration_ms≈60000
```

---

## Mutation testing 验证(2026-09-17)

按 dna review-checklist P3 跑过 2 个 Rust mutation,确认测试是"真测试":

| 变异 | 位置 | 捕获测试 |
|------|------|---------|
| `let duration_ms = 0u64`(RAII duration 废) | `src/core/trace.rs:202` | `test_trace_step_raii` 断言 `≥ 50ms` + `test_integration_4_fault_points` 断言 `≥ 20/30ms` |
| `output_mutator.take()` 不触发 on_exit 闭包 | `src/core/trace.rs:204` | `test_trace_step_raii` 断言 `output["status"] == "done"` |
| **2026-10-30** `TraceStage::finish` 的 duration 写死 0 | `src/core/trace.rs` `finish()` | `test_trace_stage_records_real_duration` 断言 done 事件 `duration_ms ≥ 40` |
| **2026-10-30** session open **覆盖**栈而不是压栈(旧全局语义) | `src/core/trace.rs` `trace_session_open()` | `test_nested_sessions_keep_own_files`(外层文件只剩 `session_open`)+ `test_outer_session_close_goes_to_its_own_file_while_inner_open` |
| **2026-10-30** close 只关最内层(不退化成按 id 关自己) | `src/core/trace.rs` `TraceSession::close()` | `test_outer_session_close_goes_to_its_own_file_while_inner_open`(外层 close 落进内层文件,`who` 变成 `"inner"`) |

> 2026-10-30 的 3 个变异都是**实跑验证过会红**的,不是「看着像能抓」。
> 复现方式见下(`mutation_smoke.sh manual` 只覆盖 Rust,本轮用手改 + 还原的方式跑)。

复跑命令(Rust 端用 `mutation_smoke.sh manual` 自动 revert):

```bash
bash scripts/mutation_smoke.sh manual src/core/trace.rs 202 \
  'let duration_ms = self.started_at.elapsed().as_millis() as u64' \
  'let duration_ms = 0u64'
# 期望: ✅ MUTATION KILLED + 自动 revert
```

变更前 checklist: 改 `src/core/trace.rs` → 重跑 mutation #1 + #2。

---

## 相关 memory

- `~/.claude/projects/-Users-hgxszhj-x-videos-dowloads/memory/async-spawn-trace-bisect.md` — 同款思路,4 trace 点二分(事前手动 → 事前自动)
- `~/.claude/projects/-Users-hgxszhj-x-videos-dowloads/memory/bug-d-isolation-and-cookies-zero.md` — 故障点 1+2 来源
- `~/memory/decisions.md:2026-09-17 — TokenPilot **不安装**` — 决策主线

## 相关 dna

- `docs/dna/trace-eval-metrics.md` — 4 指标 baseline vs current(下游消费者)
- `docs/dna/trace-link.md` — commit ↔ NDJSON 关联(下游消费者,Ref-trace 注入)
