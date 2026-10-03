# 2026-09-17 — 本地 NDJSON trace instrumentation v1 (4 故障点覆盖, commit `d41e733`)


### 1. 工具交付

复刻 LangSmith Trace 思路(本地化,零外部依赖,跟"不装 TokenPilot"决策对齐):

| 文件 | 类型 | 行数 | 职责 |
|------|------|------|------|
| `docs/dna/trace-schema.md` | 新增 | ~150 | 5 元组 schema + 4 故障点规范 + 5 项验证标准 |
| `src/core/trace.rs` | 新增 | ~280 | `trace_session_open/close` + `trace_event` + `TraceStep` RAII |
| `src/core/mod.rs` | 改 | +1 | `pub mod trace;` 注册 |
| `src/gui_dioxus/handlers.rs` | 改 | +30 | 4 故障点打桩 + session 开/关 |

### 2. 4 故障点覆盖

| # | 故障点 | 行号 | trace 类型 | 防的 BUG |
|---|--------|------|-----------|---------|
| 1 | `start_download_impl` | :41 | session_open("gui-single") | session 边界 |
| 2 | `fetch_video_info_impl` | :358 | TraceStep RAII + 内部 Ok/Err trace_event | BUG-D 解析耗时 |
| 3 | `resolve_items_serially` | :836 | 循环 trace_event("resolver_try") + idx/title | BUG-D 串行链 |
| 4 | `download_one` | :1023 | TraceStep RAII + title | BUG-E batch stall |

### 3. 验证结果

- `cargo check`: 0 errors (2 dead_code warnings on test helper)
- `cargo test --test-threads=1`: **289 passed, 12 ignored, 0 failed**
- 新增 trace 模块 2 个测试通过

### 4. 使用方法

```bash
# BUG 复发时 grep
ls -t ~/.local/share/x_videos-dowloads-trace/$(date +%Y-%m-%d)/ | head -1
cat ~/.local/share/x_videos-dowloads-trace/2026-09-17/<session_id>.ndjson | jq .

# 找耗时最长 step(自动暴露卡死位置)
jq -s 'sort_by(-.duration_ms) | .[:5]' < trace.ndjson
```

### 5. 决策链

- `~/memory/decisions.md:2026-09-17 — TokenPilot 不安装` → 选本地 trace 而非 LangSmith
- `MEMORY.md:async-spawn-trace-bisect.md` → 复用"N trace 点二分"思路(事前手动 → 事前自动)

---

