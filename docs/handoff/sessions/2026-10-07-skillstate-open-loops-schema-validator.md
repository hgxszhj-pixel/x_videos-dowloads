# SKILL.state open_loops schema validator — A1+A2+A4+A5 完整落地 (2026-10-07)

## 触发

论文 [arxiv 2608.26263v2](https://arxiv.org/html/2608.26263v2) (Google Research
2026-08-28): procedural skill 执行 = 显式 state transition,不是 append-only
conversation。每步只喂 `(P, Σₜ, Oₜ)`,中间 reasoning 用完即弃。

## 核心数据 (Table 5 budget-matched)

| 配置 | Score | Avg Prompt |
|------|-------|-----------|
| Full ReAct | 0.84 | 36,362 |
| Truncated Sliding Window | 0.18 | 1,800 |
| ReAct + LLMLingua | 0.22 | 1,810 |
| **SKILL.state (Structured)** | **0.94** | **1,905** |

→ 短 prompt ≠ 好状态;结构化 state 保留 relational dependency。

## 错误分布 (Gemma-4-31B T=100)

- 68% Premature State Overwrite / Deletion
- 20% Schema Comprehension / Type Coercion
- 12% JSON Syntax / Formatting

→ 退化源于 **structured output adherence**,不是 reasoning 能力。

## Section 7 Limitations (论文自我警告)

- L1: 没有 fixed schema (依赖模型自主 keep state)
- L2: deferred relevance (近期 history 不一定相关)
- **L3: task defined over history itself** (close 一项时核心信息是"为什么 close",不是"做了什么")

L3 是本项目落地最关键的一环 — Just Closed 段如果只剩标题,history 断裂,
state/history decoupling 反而丢信号。

## 落地动作 (commit b2fba5b)

### A1 — open_loops schema validator (commit-msg hook)

**位置**: `scripts/openloops_schema.py` + `.git/hooks/commit-msg` 第 81-98 行
**触发**: HANDOFF.md 是 staged 文件之一时 (与 handoff_size_guard 一致)
**规则**:
- Active 段每条 slot 必填 4 字段: `owner / blocker / next / touched`
- Blocked 段每条 slot 必填 3 字段: `waiting-on / eta / touched`
- Just Closed 段每条 slot 必填日期 `(YYYY-MM-DD)` + **`reason:`** + **`ref:`**
- 字段缺失 → 拒绝 commit (exit 1)
- touched > 14 天 → 软警告
- legacy "## Open Items" → 警告但不阻塞

### A2 — staleness 软警告

集成在 A1 脚本内 (staleness check 跑在 schema validation 之后,通过)。
touched > 14 天时 stdout 警告但不 exit 1,提醒人工 close / downgrade / 重新 touch。

### A4 — Just Closed 必填 `reason` + `ref` (L3 防御)

论文 Section 7 L3 直接落地:
- `JUST_CLOSED_REQUIRED = ["reason", "ref"]` 加入 validator
- `CLOSED_REF_RE` 校验 ref 形如 `<name>.md` 或 `docs/handoff/sessions/<name>.md`
- HANDOFF.md 5 条 Just Closed 全部从 inline `— 摘要;路径` 重构为 `— reason: ... · ref: ...`
- 负向测试: 缺 reason → exit 1 + 精确报错;ref 后缀非 .md → exit 1 + 精确报错

### A5 — Close 迁移工作流规则 (不是代码)

schema 强制后,使用流程也要明确:

1. Active → Just Closed 迁移前,在 `docs/handoff/sessions/YYYY-MM-DD-<topic>.md` 写
   "为什么 close"完整 reasoning (根因、决策、剩余 follow-up)
2. HANDOFF.md Just Closed slot 只写 ≤ 80 chars 一行摘要 + 引用 session 文件 path
3. 完整 trajectory 在 session 文件里 (git 内 append-only,带 date slug)

**为什么分离 sessions/ 而不是直接写长段在 HANDOFF.md**:
- HANDOFF.md 受 handoff_size_guard 200 行硬限制
- sessions/ 是 git 内 append-only,跨 commit 引用稳定 (filename 不变)
- 3 段结构的 state/history decoupling 才能真落地

## 关键设计选择

1. **检查 staged version (`git show :HANDOFF.md`) 非 working tree** — 与 handoff_size_guard 一致
2. **解析时容错** — 空 Blocked 段不报错;legacy 格式给警告而非阻塞
3. **schema 用字段名+`:` 分隔** — 匹配 HANDOFF.md 现有 markdown 风格
4. **staleness 用 touched 字段值** — 表达"上次认真看过",非文件元数据
5. **不强制类型 (type coercion 留给人工)** — 与论文 5.7 节对齐: 强制 key 存在 + 非空,类型灵活
6. **ref regex 用 `[^·\n\`]+?\.md`** 而非 `[\w./-]+` — 后者不匹配中文/hyphen,前者匹配 sessions slug

## 测试矩阵

| 场景 | 期望 | 实测 |
|------|------|------|
| 全字段 commit | exit 0 | exit 0 ✓ |
| 删 `reason:` | exit 1 + 字段名错误 | exit 1 + "missing field 'reason:' (L3 防御 — 必填 reason + ref)" ✓ |
| ref 后缀 `.txt` | exit 1 + 格式错误 | exit 1 + "must look like '<name>.md' or 'docs/handoff/sessions/<name>.md'" ✓ |
| 完整 hook chain (Intent + size + schema + Ref-trace + DID/VC) | 全绿 | rep=100.0 ✓ |

## Token 节省 bench (4 维度)

| 维度 | PRE (扁平) | NOW (3 段+schema) | Δ |
|------|-----------|------------------|---|
| A. Full-read cost | 62L | 74L | +274t (+18.6%) |
| B. Section-only | — | — | -128t (Active 段命中提前) |
| C. 10 query hit rate | 7/10 | 9/10 | +2 query 可结构化命中 |
| D. 90-day drift | 13×80=1040 chars | 2×150=300 chars (auto-archive) | **-212t 长期收益** |

**关键 trade-off**: 初期 token 略涨 (+18.6%),但 hit rate 7→9,90 天后
staleness auto-archive 自动省 -212t。结构化收益 ≥ 短期 token 成本。

## Deferred

### A3 — 跨 working memory 推广

候选目标:
- `~/.claude/memory/decisions.md` — 决策日志
- `~/.claude/projects/.../memory/MEMORY.md` — 项目 memory 索引
- `~/memory/projects/<name>.md` — Obsidian vault 项目笔记

**defer 至 2026-10-13** — 用户原话 "先测试一段时间如果效果好,就 schema 应用到..."。
观察期 7 天,看 A1 hook 在真实 commit 流量下的触发频率和误报率。

## 教训 (写给未来的自己)

1. **SKILL.state 风格 = 显式 schema + 机器校验兜底** — 缺一不可,手写 markdown
   会自然漂移成 free-form,失去 state/history decoupling 的关键收益
2. **L3 防御的核心是 `reason`** — close 一个 open loop,审计者第一问是"为什么",
   不是"做了什么"
3. **ref 字段强制 `.md` 路径** — 这是 schema 验证可机器化的关键;free-form 链接
   不可校验
4. **session 文件 + slot 引用** — 完整 trajectory 在 git 内 append-only,working memory
   保持小而精

## Commit

`b2fba5b` — `feat(handoff): Just Closed 必填 reason+ref (L3 defense) + A5 工作流规则`
4 hook 全绿,rep=100.0
