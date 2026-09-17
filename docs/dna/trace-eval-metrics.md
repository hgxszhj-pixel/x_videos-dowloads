# Trace Eval 指标体系 (Trace-Driven Eval)

> **起源**: 2026-09-17 trace instrumentation v1 落地后,把 NDJSON 转成可量化回归集。
> **目的**: 发版前跑 1 次 baseline → 跑改动 → 跑 current → `eval_trace.py` 对比,**量化改动效果**(不是只看"修了哪个 bug")。
> **对应决策**: 复刻 LangSmith Trace 思路 — LLM-as-a-Judge 对历史 trace 批量打分 → 我们用 4 个简单指标自评。

---

## 4 指标 schema

每个 session 文件算 1 个 session 贡献,4 个指标从所有 session 聚合:

| 指标 | 类型 | 计算 | 含义 |
|------|------|------|------|
| `success_rate` | float 0.0-1.0 | `ok_sessions / total_sessions` | session_close.output.status == "ok" 的占比 |
| `avg_total_ms` | int | `Σ(session_close.duration_ms_proxy) / N` | session_close 到 fetch_video_info 之间的总耗时(秒级) |
| `resolver_chain_length` | float | `Σ(resolver_try events) / N` | 平均每个 session 串行 resolver 多少次(BUG-D 卡死点) |
| `stall_count` | int | `Σ(events where step in {resolver_try,download_one} and duration_ms ≥ 30000)` | stall ≥ 30s 的事件总数(BUG-E / BUG-D 都触发) |

### 指标来源(NDJSON 5 元组)

```json
// 一次 session_close
{"step":"session_close","output":{"status":"ok","total_steps":5}}

// 一次 resolver_try(在 resolve_items_serially 内)
{"step":"resolver_try","input":{"idx":2,"title":"..."},"duration_ms":0}

// 一次 download_one(TraceStep RAII drop)
{"step":"download_one","input":{"title":"..."},"duration_ms":31500}  ← stall ≥ 30s
```

---

## baseline vs current 对比维度

`eval_trace.py --baseline traces/2026-09-14-baseline/ --current traces/2026-09-17-current/` 输出:

| 维度 | 字段 | 输出 | 判定 |
|------|------|------|------|
| success_rate | delta = current - baseline | +0.05 | ✅ 改善(从 0.6 → 0.65) |
| avg_total_ms | delta = current - baseline | -3000 | ✅ 改善(快 3 秒) |
| resolver_chain_length | delta | -1.0 | ✅ 改善(链短 = 不卡) |
| stall_count | delta | -5 | ✅ 改善(stall 减少) |

> **判定逻辑**: success_rate ↑ / avg_total_ms ↓ / resolver_chain_length ↓ / stall_count ↓ = 改善。
> 反之 = 退化,需 review diff。

---

## 文件布局

```
traces/
├── 2026-09-14-baseline/    ← 发版前跑 N 个真实链接
│   ├── <session_id>-gui-single.ndjson
│   └── ...
└── 2026-09-17-current/     ← 改动后跑相同 N 个链接
    └── ...
```

> **关键**: baseline 和 current 用 **同一组测试 URL**(否则对比无意义)。

---

## 不做的事(防 scope creep)

- ❌ **不做统计显著性检验**(样本小,N=5-20,卡方/t 检验过度)
- ❌ **不做实时 dashboard**(离线 grep / markdown 输出足够)
- ❌ **不接 LLM-as-a-Judge**(4 个简单指标自评已够用,LLM 增加延迟和成本)
- ❌ **不写配置文件**(默认 `--baseline/--current` 两个目录,够简单)
- ❌ **不接 LangSmith / OpenTelemetry**(本地 NDJSON 闭环)

---

## 验证标准(实跑 1 次必须满足)

```bash
# 1. 脚本可执行
python3 scripts/eval_trace.py --help

# 2. 空目录不崩溃
python3 scripts/eval_trace.py --baseline /tmp/empty --current /tmp/empty2
# 期望: exit 0 + 警告 "无 trace 数据"

# 3. fixture 测试覆盖核心计算
python3 -m pytest tests/test_eval_trace.py -v
# 期望: ≥5 case 全过

# 4. 真实 baseline vs current 对比
python3 scripts/eval_trace.py --baseline traces/2026-09-14/ --current traces/2026-09-17/
# 期望: markdown 表格输出,4 指标 delta 一目了然
```

---

## 相关 memory / dna

- `docs/dna/trace-schema.md` — 5 元组 schema 来源
- `MEMORY.md:local-ndjson-trace-instrumentation.md` — NDJSON 怎么写
- `MEMORY.md:async-spawn-trace-bisect.md` — 4 trace 点二分(事前手动 → 事前自动)
- `~/memory/decisions.md:2026-09-17 — TokenPilot 不安装` — 决策主线

## 工程纪律价值
- **可量化改动效果**: 不再"修了 bug 但不知道变好还是变坏"
- **零外部依赖**: 纯 stdlib Python,本地 markdown 输出,跟"不装 LangSmith / TokenPilot"决策对齐
- **决策可逆**: 4 指标可扩展(后续可加 download_throughput / retry_count 等)
- **复用价值**: 任何有 NDJSON trace 的 Rust/Python 项目都可移植这个 eval 模板
