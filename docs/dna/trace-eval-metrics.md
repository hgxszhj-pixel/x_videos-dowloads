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
| `avg_total_ms` | int | `Σ(session_close.total_ms_proxy) / N'` ,其中 `N'` = **真测到该 key 的 session 数** | session 墙钟总耗时(session_open → session_close,**含全部等待**)。2026-10-30 起 `src/core/trace.rs` 真的写这个 key 了;旧基线仍没有 ⇒ 对旧基线显示「⚪ 不可测」 |
| `resolver_chain_length` | float | `Σ(resolver_try events) / N` | 平均每个 session 串行 resolver 多少次(BUG-D 卡死点) |
| `stall_count` | int | `Σ(events where step ∈ STALL_ELIGIBLE_STEPS and duration_ms ≥ 30000)` | stall ≥ 30s 的**叶子**事件总数(BUG-E / BUG-D 都触发) |

> **分母为什么是 `N'` 而不是 `N`**:分子只累加了带 `total_ms_proxy` 的 session。
> 拿全部 session 当分母会把「没测到」摊成「耗时 0」,系统性低报(H3:分母必须可复算)。
> 变异测试:`test_denominator_counts_only_sessions_with_total_ms`(8000 被摊成 4000 → 必红)。

### `stall_count` 的 step 白名单(2026-10-30 起)

```python
STALL_ELIGIBLE_STEPS = frozenset({
    "resolver_try",              # 兼容旧 trace(新埋点里是 marker,duration 恒 0)
    "resolver_try_done",         # GUI:单次解析尝试(新)
    "cli_resolve_attempt_done",  # CLI:单个后端尝试(新)
    "download_one",              # GUI:单个课节下载
    "fetch_video_info",          # GUI:单个课节信息获取
})
AGGREGATE_STEPS = frozenset({"cli_resolve_done", "cli_download_run_done"})  # 明确**不**参与
```

**为什么是白名单而不是「任何 ≥30s 的事件都算」**:聚合步骤覆盖了叶子步骤的时间区间。
同一次慢解析会同时命中 `cli_resolve_attempt_done`(叶子,60s)和 `cli_resolve_done`(聚合,60s),
两个都计 = 同一段墙钟耗时被数两遍 ⇒ `stall_count` 虚高,发版对比会把
「测得更准」误判成「性能退化」。

**指标口径不变**:`resolver_chain_length` 仍然只按 `resolver_try` 事件数算。
新埋点多了 `resolver_try_done` 事件,但它**不**进链长指标 ——
否则同一个指标名会同时表示「课节数」和「尝试数」,已锁基线直接失效。

### ⚠️ 换埋点后必须重新锁基线 —— **现在由机器强制了**

2026-10-30 之前,`resolver_try` 的 duration 是写死的 0 ⇒ resolver 阶段**永远**不可能
被判成 stall。埋点修好后,同样的运行会**多出** `resolver_try_done` 带来的 stall。

⇒ **基线与 current 必须来自同一套埋点版本**,否则 `stall_count` 的 delta 是
「测得更准」而不是「性能变差」。重锁:`bash scripts/install_perf_baseline.sh`。

**2026-10-30 起这条规则不再只靠人记**:`src/core/trace.rs` 的
`TRACE_SCHEMA_VERSION`(当前 `2`)写进每个 session 的
`session_open` / `session_close` 的 `extra.schema_version`;
`eval_trace.py` 的 `check_version_compatible()` 发现两侧版本不一致
(或任一侧有 session 缺版本)⇒ **拒绝出判决**,退出码 **3**:

```
⛔ [schema-mismatch] 拒绝出判决: 埋点版本不一致: baseline=['1'] vs current=['2']
   修: 用**同一套埋点版本**的数据重锁基线 —— bash scripts/install_perf_baseline.sh <日期> <标签>
```

退出码语义(调用方据此分别处置):

| 码 | 含义 | 该做什么 |
|----|------|---------|
| 0 | 无退化(全部指标改善或持平) | — |
| 1 | 基线不可用(空/无 session) | 重锁基线 |
| 2 | **真的**检测到退化 | 排查性能 |
| 3 | 埋点版本不兼容 | 重锁基线(**不是**性能问题) |
| 4 | current 侧无可用 session ⇒ **未出判决** | 正常(常态),不必处理 |

**两个必须记住的例外**(都在 current 侧没有 session 时发生):

1. **不做版本判定** —— 只用了 CLI(没开 GUI session)时,今天的 trace 只落在
   `no-session/` fallback 里 ⇒ `current.total_sessions == 0` 是**常态**。
   current 的版本集合自然是空的,照常比对会得到「版本不一致」⇒ **永久误报**
   (2026-10-22 把「静默漏报」换成「永久误报」的老错误)。
2. **不出判决(退出码 4)** —— 没有分母时,4 个指标全部显示「⚪ 不可测」。
   照常比较会得到 `success_rate | 1.000 | 0.000 | -1.000 | ❌ 退化`
   ⇒ rc=2 ⇒ stop-trace-eval **每天报一个假的「trace regression detected」**。
   这是 2026-10-30 实测发现的:版本检查(退出码 3)恰好把它挡住了,
   **一旦用户重锁基线(version 对上)就会复现** —— 属于「已 codified 但没被验过会红」。

> **H2(不可测 ≠ 零值)在退出码层面的同款要求**:没有测量结果 ≠ 测过且没变。
> 所以既不能返回 0(=「无退化」),也不能返回 2(=「真退化」),
> 用独立的 4 表示「本次没测」。

**验证**(`tests/test_eval_trace.py::TestNoVerdictWhenCurrentHasNoSessions`,5 个用例):
变异点 = 删掉 `compare()` 开头的 `if current.total_sessions == 0:` 提前返回
⇒ 3 条红;把 `return 4` 改回 `return 0` ⇒ 3 条红。

### 指标来源(NDJSON 5 元组)

```json
// 一次 session_close —— total_ms_proxy = session 墙钟总耗时(2026-10-30 起真埋)
{"step":"session_close","output":{"status":"ok","total_ms_proxy":43210},"extra":{"schema_version":2}}

// 一次 session_open —— extra.schema_version = 埋点版本(eval 靠它拒绝跨版本比较)
{"step":"session_open","output":{"file_path":"..."},"duration_ms":0,"extra":{"prefix":"gui-single","schema_version":2}}

// 一次 resolver_try(在 resolve_items_serially 内)—— **开始标记**,duration 恒 0
{"step":"resolver_try","input":{"idx":2,"title":"..."},"output":{"phase":"start"},"duration_ms":0}

// 一次 resolver_try_done(同一次尝试的**真实耗时**)← stall 判定看它
{"step":"resolver_try_done","input":{"idx":2,"url":"..."},"output":{"status":"ok","attempted":true,"backend":"api"},"duration_ms":31500,"extra":{"backend":"api","course_item_count":29}}

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

## Mutation testing 验证(2026-09-17)

按 dna review-checklist P3 跑过 2 个 Python mutation,确认测试是"真测试":

| 变异 | 位置 | 捕获测试 |
|------|------|---------|
| `success_rate = (total - ok) / total`(倒置) | `scripts/eval_trace.py:120` | `test_single_session_ok_metrics` 断言 `== 1.0` + `test_improvement_all_metrics` / `test_regression_success_rate_drops` 判定方向 |
| `verdict = "✅" if delta > 0 else "❌"`(lower_is_better 方向反) | `scripts/eval_trace.py:144` | `test_improvement_all_metrics` 断言 `resolver_chain_length` ✅ |
| **2026-10-30** stall 白名单退回旧的硬编码三元组(不含 `resolver_try_done` / `cli_resolve_attempt_done`) | `scripts/eval_trace.py` stall 判定 | `test_resolver_try_done_stall_counted` + `test_cli_resolve_attempt_done_stall_counted` + `test_leaf_and_aggregate_not_double_counted`(实跑:旧代码 4 条红) |
| **2026-10-30** 把聚合步骤也塞进 stall 白名单 | `scripts/eval_trace.py` `STALL_ELIGIBLE_STEPS` | `test_aggregate_steps_not_counted_as_stall` + `test_step_sets_are_disjoint` |

复跑命令(Python 端 `mutation_smoke.sh` 不覆盖,手测;**必须先清 `__pycache__`** 否则 revert 后 pytest 跑旧字节码):

```bash
find . -name "*.pyc" -delete
find . -name "__pycache__" -type d -exec rm -rf {} + 2>/dev/null

cp scripts/eval_trace.py scripts/eval_trace.py.bak-mutation
sed -i.bak 's|m.success_rate = m.ok_sessions / m.total_sessions|m.success_rate = (m.total_sessions - m.ok_sessions) / max(m.total_sessions, 1)|' scripts/eval_trace.py
rm -f scripts/eval_trace.py.bak
python3 -m pytest tests/test_eval_trace.py -v
mv scripts/eval_trace.py.bak-mutation scripts/eval_trace.py
# 期望: Pytest: 5 passed, 3 failed
```

变更前 checklist: 改 `scripts/eval_trace.py` → 重跑 mutation #4 + #5。

---

## 相关 memory / dna

- `docs/dna/trace-schema.md` — 5 元组 schema 来源
- `docs/dna/trace-link.md` — 闭环关联 commit ↔ NDJSON(本指标体系的下游,Ref-trace 注入)
- `MEMORY.md:local-ndjson-trace-instrumentation.md` — NDJSON 怎么写
- `MEMORY.md:async-spawn-trace-bisect.md` — 4 trace 点二分(事前手动 → 事前自动)
- `~/memory/decisions.md:2026-09-17 — TokenPilot 不安装` — 决策主线

## 工程纪律价值
- **可量化改动效果**: 不再"修了 bug 但不知道变好还是变坏"
- **零外部依赖**: 纯 stdlib Python,本地 markdown 输出,跟"不装 LangSmith / TokenPilot"决策对齐
- **决策可逆**: 4 指标可扩展(后续可加 download_throughput / retry_count 等)
- **复用价值**: 任何有 NDJSON trace 的 Rust/Python 项目都可移植这个 eval 模板
