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

---

**核心洞见**: 过度复杂的代码往往遵循设计模式和最佳实践。问题在于**时机**：在需要之前添加复杂性会导致代码更难理解、引入更多 bug，花更长时间实现，更难测试。

**好代码是简单解决今天问题的代码，而非提前解决明天问题的代码。**

<!-- rtk-instructions v2 -->
<!-- 完整 RTK 命令文档已移至 docs/rtk-commands.md -->
**RTK Commands**: `rtk <cmd>` 自动过滤输出，60-90% token 节省
<!-- /rtk-instructions -->
