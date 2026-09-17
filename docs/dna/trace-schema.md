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
| 2 | `resolve_items_serially` | `src/gui_dioxus/handlers.rs:782` | step=`resolver_try` / extra=`{resolver_idx, resolver_name, cookies_count}` | BUG-D: 20s 后到底卡哪个 resolver |
| 3 | `download_one` | `src/gui_dioxus/handlers.rs:971` | step=`download_one` / extra=`{bytes_done, stall_after_ms}` | BUG-E: batch 第 3 个 stall 60s+ 无日志 |
| 4 | `videoAuth` 调用 | `src/main.rs` 智学云逻辑 | step=`video_auth` / extra=`{course_id, code, response_size}` | 智学云 session > 7 天 code=100067 反直觉 |

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
