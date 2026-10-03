# 2026-09-18 — heuristic_judge_trace.py (拒绝 LLM-as-a-Judge,纯启发式 4 维 1-5 分质性打分)


### 1. 触发
- 用户读 Toutiao《AI Agent开发离不开langsmith-trace》,问对找 bug 有啥帮助
- 文章第 3 论点提"用 LLM-as-a-Judge 对历史 Trace 批量打分";评估对 x_videos-dowloads 价值后**拒绝**接入,改为纯启发式可选工具

### 2. 1 file (148 行,纯 stdlib)

| 文件 | 类型 | 职责 |
|------|------|------|
| `scripts/heuristic_judge_trace.py` | 新增 | 4 维度 1-5 分质性打分(可选,**不**接入主 pipeline) |

注:中途改过一次名(`llm_judge_trace.py` → `heuristic_judge_trace.py`),因最终不调 LLM,文件名误导读者。

### 3. 4 维度启发式打分(与 LLM rubric 1-5 标度对齐,便于将来切回 LLM 时直接映射)

| 维度 | 规则 | 阈值 |
|------|------|------|
| **efficiency** | resolver 链越短越好 | 0→5, >6→1 |
| **robust** | stall 次数越少越好 | 0→5, ≥4→1 |
| **readable** | 结构完整(open + close + 核心事件) | 0/1/2+ 缺 → 5/3/1 |
| **suspicious** | 异常信号累加(基线 1,可疑到 5) | resolver>6 +2;stall>4 +2;missing +1/件 |

与 `eval_trace.py` 共享 `STALL_THRESHOLD_MS=30_000` + `CORE_STEPS = {resolver_try, fetch_video_info, download_one}`(对齐 `src/core/trace.rs` 实际 step 名)

### 4. 验证(已做)

| fixture | resolvers | stalls | 结构 | eff | rob | rd | sus |
|---------|-----------|--------|------|-----|-----|----|----|
| `happy` | 1 | 0 | 完整 | 4 | 5 | 5 | 1 |
| `mid` | 5 | 3 | 完整 | 2 | 2 | 5 | 3 |
| `bad` | 8 | 8 | 缺 close | 1 | 1 | 3 | 5 |

3 fixture 行为单调合理。

### 5. 决策
- 🟢 **拒绝 LLM-as-a-Judge**(13 条规则第 5 条 + 确定性 IO 领域质性评分无价值);落 `~/memory/decisions.md` 2026-09-18 entry
- 🟢 **不接入主 pipeline**:`scripts/heuristic_judge_trace.py` docstring 明写不写 `docs/dna/`、不入 CI 门禁
- 🟢 **1-5 标度对齐 LLM rubric**:将来真需升级,改 `heuristic_judge()` 内部分支即可,接口不变(可逆性)
- 🟢 **纯 stdlib**:`urllib`/`requests`/sdk 一概不引(无 key 无网络依赖 = 零成本 / 100% 复现)

### 6. 已知 follow-up
- 零(脚本一次性交付,如有演化参考 `~/memory/decisions.md` 2026-09-18 entry)
- 重评触发:项目从"下载器"演化为"多步推理 + 工具调用编排 Agent"时重启 LLM judge 评估

---

