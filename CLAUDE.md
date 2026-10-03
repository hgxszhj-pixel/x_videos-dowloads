# CLAUDE.md

@andrej-karpathy-skills:karpathy-guidelines

## Commit 规则（Intent Lock）

每次 commit message 必须包含一行 `Intent: <一句话>`，否则被 `.git/hooks/commit-msg` 拒绝。

详见 [`scripts/README_intent_lock.md`](scripts/README_intent_lock.md)。

---

# 编程理念

**核心原则**: 谨慎优于速度，简单优于复杂，直接解决当前问题而非预判未来需求。

---

# Agent Skills

| Task | Location |
|------|----------|
| Issue tracker | `docs/agents/issue-tracker.md` |
| Triage labels | `docs/agents/triage-labels.md` |
| GitNexus | See `.claude/skills/gitnexus/*/` |

# DNA（分档派 + 消灭派，2026-09-13 采纳）

AI 编辑代码前**按档 grep** 三份文档，把"踩过的坑不再踩"从口头约定升级为流程约束。**不每轮必读** — 见表格"何时读"列,仅在该列触发时查；常规改动直接动键盘。

| 文档 | 何时读 | 核心规则 |
|------|--------|---------|
| [`docs/dna/三档地盘.md`](docs/dna/三档地盘.md) | **任何编辑前** | 🟢 自由档 / 🟡 审查档 / 🔴 禁止档 — 🔴 档改前必 grep `~/.claude/projects/-Users-hgxszhj-x-videos-dowloads/memory/MEMORY.md` |
| [`docs/dna/新功能前必查.md`](docs/dna/新功能前必查.md) | **写新代码前** | 30 秒 grep 3 项：skill / 项目内模块 / 血泪教训 |
| [`docs/dna/review-checklist.md`](docs/dna/review-checklist.md) | **PR/review 时** | 5 阶段 pre-merge 清单：机械门禁 → 三档合规 → 变异测试 → 文档同步 → Commit 模板 |

变异测试（防 AI 假测试）：`bash scripts/mutation_smoke.sh` [smoke|full|manual]

---

**核心洞见**: 过度复杂的代码往往遵循设计模式和最佳实践。问题在于**时机**：在需要之前添加复杂性会导致代码更难理解、引入更多 bug，花更长时间实现，更难测试。

**好代码是简单解决今天问题的代码，而非提前解决明天问题的代码。**

<!-- rtk-instructions v2 -->
<!-- 完整 RTK 命令文档已移至 docs/rtk-commands.md -->
**RTK Commands**: `rtk <cmd>` 自动过滤输出，60-90% token 节省
<!-- /rtk-instructions -->
