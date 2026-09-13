# 架构派 · Pre-merge Review Checklist

> **起源**: 2026-09-13 采纳「五派」中的 **架构派**（精神版，不照搬"完全不读代码"）。
> **目的**: 把 `selftest/handoff/clippy/test/coverage` 这些已有工具钉成 reviewer pre-merge 5 阶段清单 — **用约束+测试围城,代替逐行人工审查**。
> **核心问题**: 你已经有 track_quality.sh / mutation_smoke.sh / HANDOFF.md / Intent Lock,但散落在各处;reviewer 不知道"该看什么、什么算 PASS"。
>
> ⚠️ **为什么不照搬 Uncle Bob "完全不读代码"**: 与 karpathy 风格 + 项目"谨慎优于速度" + 你 smoke test 习惯都冲突。本清单保留"代码要读,但有 5 道关卡先帮你挡住 80% 的坑"。

---

## 5 阶段 Pre-merge 清单

> **任何 PR / commit / AI 自动改动**,在合入前必须 5 阶段全部 ✓。任何一阶段 ✗ = 不通过。

### Phase 1 · 机械门禁 (必须绿)

```bash
bash scripts/track_quality.sh all
```

- ✅ cargo test 全绿
- ✅ cargo clippy 0 warning
- ✅ `agent_did.py reputation` 没有近 N 次 fail_event 累积

**未通过**: 直接修复,不许绕过。

### Phase 2 · 三档合规 (查 DNA 档位)

读 [`三档地盘.md`](三档地盘.md),确认本次改动每个文件的档位处理:

| 档位 | 必做 |
|------|------|
| 🟢 自由档 | 跑通 Phase 1 即可 |
| 🟡 审查档 | Phase 1 + Phase 4 (人工 review) |
| 🔴 禁止档 | Phase 1 + grep `~/.claude/projects/-Users-hgxszhj-x-videos-dowloads/memory/MEMORY.md` 全部相关条目 + commit message 解释"为什么这次不会重蹈覆辙" |

**未通过**: 缺解释 → 不许 merge。

### Phase 3 · 防 AI 假测试 (变异测试)

```bash
bash scripts/mutation_smoke.sh smoke
```

- ✅ cargo-mutants 已装 → `mutants survived = 0`(或解释哪些 survived 是无意义变异)
- ⚠️ cargo-mutants 未装 → 至少人工抽查:改 1-2 行 expected behavior,确认测试会 fail

**未通过**: 变异 survived 但没解释 → 测试在说谎,补测试。

### Phase 4 · 文档同步 (HANDOFF Session 段)

如果本次改动涉及:
- 新增 / 修改 / 废弃 CLI flag → 更新 `HANDOFF.md` Session 段
- 新增 / 修改对外 API(模块导出、平台 endpoint)→ 更新 `HANDOFF.md` Session 段
- 修改 🟡/🔴 档代码 → 更新 `HANDOFF.md` Session 段

```bash
# 快速检查 Session 段是否最新
grep -A 5 "## Session" HANDOFF.md | head -20
```

**未通过**: 改了 API 但 HANDOFF 没动 → reviewer 卡住。

### Phase 5 · Commit 模板 (Intent Lock 强制)

commit message 必须含:

```
<type>: <description>

<body>

Intent: <一句话>
Refs: docs/dna/<相关文档> | MEMORY.md:<条目名>
```

**Intent 行强制** (`.git/hooks/commit-msg` 拒绝无 Intent 的 commit)。
**Refs 行推荐** (本次改动引用了哪条 DNA / 哪条 memory,方便日后追溯)。

**未通过**: Intent 缺/Refs 错 → commit-msg hook 自动拒。

---

## Reviewer 实际操作流程 (5 分钟)

```
1. 看 PR diff
2. 跑 Phase 1 (track_quality.sh) — 30 秒
3. 读 dna 三档地盘 — 确认 Phase 2 通过 — 1 分钟
4. 跑 Phase 3 (mutation_smoke.sh smoke) — 30 秒
5. 读 HANDOFF.md Session 段 — 确认 Phase 4 通过 — 1 分钟
6. 看 commit message — 确认 Phase 5 通过 — 30 秒
7. 抽查 🟡/🔴 档关键 diff — 1-2 分钟
8. ✓ / ✗ 决策
```

**总耗时 5 分钟**,比起逐行读 200 行 diff 省 80% 时间。

---

## 与 Karpathy "谨慎优于速度" 的平衡

> "完全信任约束+测试、不读代码" → ❌ 太激进,与项目哲学冲突
> "完全人工逐行读" → ❌ 太保守,失去 AI 协作红利

**本清单的折中**:
- 🟢 档:几乎不读(只信 Phase 1)
- 🟡 档:抽样读关键路径(Phase 4 抽 5-10 行)
- 🔴 档:必须逐行读 + 必查 memory(Phase 2 + Phase 4 全读)

这才是「谨慎优于速度」的真实落地 — **信任分层,而不是信任一刀切**。

---

## 维护规则

- **新增 Phase**: 提 issue 讨论;不要私自加,会膨胀成"60 步 checklist"反而无人用
- **删除 Phase**: 如果某个 Phase 长期不再抓到 bug,讨论是否删除
- **本文件本身**: 半年复审一次,看 5 阶段是否还匹配当前流程

---

## Why / How to apply

**Why**: 你已经有 selftest/handoff/Intent Lock 等工具,但散落在 scripts/ + CLAUDE.md + .git/hooks,**没有任何地方告诉 reviewer"按这 5 步走"**。结果是 reviewer 不知道该看什么,要么漏看要么瞎看。

**How to apply**:
- 把本文件路径贴进 PR 模板(`.github/PULL_REQUEST_TEMPLATE.md`)
- 或者 reviewer agent 的 system prompt 注入"先读 review-checklist.md 再 review"
- 半年后看实际使用情况,如果发现 Phase X 长期不被执行 → 删了它,别留死规则
