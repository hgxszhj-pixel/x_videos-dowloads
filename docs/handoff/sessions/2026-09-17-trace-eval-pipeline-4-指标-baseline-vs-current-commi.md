# 2026-09-17 — trace eval pipeline (4 指标 baseline vs current, commit `767011d`)


### 1. 工具交付

| 文件 | 类型 | 行数 | 职责 |
|------|------|------|------|
| `docs/dna/trace-eval-metrics.md` | 新增 | ~110 | 4 指标 schema + baseline vs current 判定 |
| `scripts/eval_trace.py` | 新增 | ~220 | Metrics.from_dir / compare / format_report,3 flags CLI |
| `tests/test_eval_trace.py` | 新增 | ~220 | 8 测试覆盖核心计算 + diff + stall + 格式 |

### 2. 4 指标

| 指标 | 方向 | 防什么 |
|------|------|--------|
| `success_rate` | ↑ 改善 | session 成功率 |
| `avg_total_ms` | ↓ 改善 | 总耗时近似 |
| `resolver_chain_length` | ↓ 改善 | 链短 = 不卡 |
| `stall_count` | ↓ 改善 | ≥ 30s stall 事件 |

### 3. 验证

- `python3 -m py_compile`: OK
- `python3 -m pytest tests/test_eval_trace.py -v`: **8 passed, 0 failed**
- 退出码: 0=无退化, 1=一般错误, 2=任一指标退化

### 4. 使用流程

```bash
# 发版前:N 个真实链接 → baseline;改动后同样 N 个 → current
python3 scripts/eval_trace.py \
  --baseline traces/2026-09-17-baseline/ \
  --current  traces/2026-09-17-current/
# exit 0 = 放心发版;exit 2 = 看哪行 ❌ 退化
```

### 5. 决策链

- 复刻 LangSmith "LLM-as-a-Judge 对 trace 批量打分" → 我们用 4 简单指标自评,零 LLM 成本
- 跟 trace instrumentation(commit baeea9c)同源,组合 = 完整可观测 + 可量化

---

