# 判据层落地到全局规则的 5 处改动（2026-10-27）

> **状态**：**5 处全部已应用**（改动 1 当日首次授权；改动 2/3/4 于 2026-10-27 第二次授权，
> 用户在权限提示里逐个点名 `~/.claude/CLAUDE.md` / `git-workflow.md` / `coding-style.md`）。
> **备份**：`~/.claude/_archive/config-backup-2026-10-27/`（首次）+ `~/.claude/_archive/config-backup-2026-10-27-v2/`（二次，含改动前的 3 个文件 + testing.md），可直接覆盖回滚。
> **判据正文已落地**：`~/.claude/skills/criteria/SKILL.md`（已注册可用，按需加载，不占每次 session 预算）。
> **推导依据**：`docs/audit/principles-2026-10-27.md` §2（H1–H10）与 §4（ROI 1–4）。
> **改动 5 的来源**：头条视频《Claude Code 18 个用法》#01 的删除判据 —— 见文末。

---

## 改动 1 — `~/.claude/rules/common/testing.md` ✅ 已应用

**为什么**：testing.md 现在只有覆盖率 80% + TDD 红绿，缺「测试有效性」维度。H1 是全部 9 条里证据最硬的一条（9 个 memory 文件 + 7 道门实战）。

**原文**（替换这一行）：
```
## 智能体（Agent）支持
```

**改为**：
```markdown
## 测试有效性判据（新增/修改判定脚本与门禁时必读）

覆盖率 80% 只管**有多少**，不管**测得对不对**。以下为判据层：

- **改 1–2 行期望行为后测试仍全绿 = 测试在说谎**。此时不是代码对，是测试没咬住契约。
- **新增/修改任何判定脚本、门禁、验收规则，必须附一条「能让它红」的用例，并实跑确认 rc≠0**。
  > 依据：本机 `commit-msg` 7 门曾因漏 `$` 整段空转仍 rc=0（假绿灯）；`stop-subagent-rate.sh` 读错 JSONL 形状长期静默失效。
  > **「已 codified」≠「有效」** —— 没验过能红的门禁，只是看起来在工作。
- **修复实现代码，不修测试代码**（除非测试本身写错，且改动要说明理由）。
- 完整判据层见 skill `criteria`（H1 门禁自举 / H2 不可测≠零值 / H3 分母可复算 / H4 检查 staged）。

## 智能体（Agent）支持
```

---

## 改动 2 — `~/.claude/rules/common/git-workflow.md` ✅ 已应用

**为什么**：两处活摩擦 —— ① 全局 commit 模板**没有 `Intent:` 行**，而项目 `.git/hooks/commit-msg` 强制它，缺失即拒（X4）；② 缺 staged 判据（H4）。

**原文**（替换这一整块）：
```
## Commit Message Format
```
<type>: <description>

<optional body>
```

Types: feat, fix, refactor, docs, test, chore, perf, ci
```

**改为**：
```markdown
## Commit Message Format
```
<type>: <description>

<optional body>

Intent: <一句话>
```

Types: feat, fix, refactor, docs, test, chore, perf, ci

> `Intent:` 行是 **Intent Lock** 门禁的硬要求（缺行直接拒绝 commit）。
> 无门禁的项目可省，但有 `.git/hooks/commit-msg` 的仓库必带。
```

**并在其后追加**：
```markdown
## Pre-commit 检查对象 = staged，不是工作区

**看到**：pre-commit 门禁、暂存区校验、size/schema guard。
**做**：读 `git diff --cached` 的 **staged 内容**，不是 working tree。

> 依据：size guard 检查 staged version；eval_gate 按 staged 含 `*.rs` 决定跑不跑 fmt/clippy。
> 详见 skill `criteria`（H4）。
```

---

## 改动 3 — `~/.claude/rules/common/coding-style.md` ✅ 已应用

**为什么**：① Immutability 写成语言无关的绝对禁令，迁到 Rust 即误伤（`let mut` 是惯用写法，X5）；② 缺 typed error 判据（D6，6 个 memory 文件支撑）。

**原文**：
```
ALWAYS create new objects, NEVER mutate existing ones:

```
// Pseudocode
WRONG:  modify(original, field, value) → changes original in-place
CORRECT: update(original, field, value) → returns new copy with change
```

Rationale: Immutable data prevents hidden side effects, makes debugging easier, and enables safe concurrency.
```

**改为**：
```markdown
For **externally exposed** data structures, create new objects and return copies — NEVER mutate in place:

```
// Pseudocode
WRONG:  modify(original, field, value) → changes original in-place
CORRECT: update(original, field, value) → returns new copy with change
```

For **internal, local** state, use the language's idiomatic mutation (Rust `let mut`, JS object spread).
A blanket "NEVER mutate" rule is wrong across languages and gets pushed aside when it misfires.

Rationale: Immutable data prevents hidden side effects, makes debugging easier, and enables safe concurrency.
```

**并在 Error Handling 小节追加**：
```markdown
**Error types must stay downcastable.** Never flatten a typed error into a string
(`anyhow!("{}", e)` / `format!`) — that silently destroys `downcast_ref::<MyError>()`.
Use a blanket `impl From<E> for anyhow::Error` or `anyhow::Error::new(e)`.
```

---

## 改动 4 — `~/.claude/CLAUDE.md` ✅ 已应用

**为什么**：Rule 3 与执行哲学「Demand Elegance」是**现存唯一一处硬矛盾**（Rule 3 说"不改善周围代码"，Demand Elegance 说"hacky fix 要重写"）。Rule 7 要求「暴露冲突，不要平均它们 —— 选择一个」。按"更晚引入 + 有实证支撑"选 Demand Elegance。

**原文**（13 Rules 表格第 3 行）：
```
| 3 | Surgical Changes | 只触碰必须改的，不改善周围代码 |
```

**改为**：
```
| 3 | Surgical Changes | 不顺手改善**无关**代码、不做**投机**重构；同一改动范围内可重写实现（解 X1：Demand Elegance 优先） |
```

**并在「质量自检双问」后追加一行**：
```markdown
> **质量自检双问** — 提交前两问：(1) "Would a Staff engineer approve this?"（命名/边界/测试/可读性）；(2) "Diff main behavior when relevant"（改了核心逻辑必跑回归对比 baseline）— 任一答案为否则不交付
> **第三问（判据层）** — 写"完成"前 invoke skill `criteria`，逐条对照 H1–H10；命中即修，别只报告。
> ⚠️ **已应用的版本写的是 H1–H9**（`criteria` 当时只有 9 条，H10 是之后补的）→ 现存 1 处过期引用，
> 修正需改 `~/.claude/CLAUDE.md`（用户本轮已点名该文件，但**这是第 6 处改动**，分类器按「未授权该改动」拒绝 → 待重新点名）。
```

---

## 改动 5 — `~/.claude/CLAUDE.md` Rule 6 替换 ✅ 已应用（来源：视频 #01）

**为什么**：改动 1–4 都是**加判据**。但 `rules/` 层实测 **≈9.1k token/session**（12 文件 955 行），
且**只有增量纪律、没有任何一条说「什么时候该删一条」**。
头条视频《Claude Code 18 个用法》#01 给 CLAUDE.md 的判断标准原文正是这句：

> **「每一句問：拿掉它，Claude 會做錯嗎？」**（旁白：*重要的規則反而會被它忽略*）

配套的 #14：**「它一直漏掉的規則 → 從 CLAUDE.md 改成 hook」**（CLAUDE.md 是建議；hook 每次一定會跑）。

**旧的 Rule 6 直接可删** —— 「上下文接近 80% 时自动生成 mini 交接包写入 HANDOFF.md」
已被 `PreCompact` hook `precompact-mini-handoff.sh` 完全覆盖（≤200 token，写 mempalace NDJSON + 注入 post-compact 上下文），
让模型再手写一遍 = 同一件事做两遍，还占常驻 token。

**原文**（13 Rules 表格第 6 行）：
```
| 6 | 上下文预算不是建议 | 上下文接近 80% 时**自动生成 mini 交接包草稿**写入 HANDOFF.md（含：做完了什么 / 敲定规矩 / 下一步起点 + touched），`/compact` 不再丢失上下文结构。显式提示超支，不悄悄膨胀 |
```

**改为**：
```
| 6 | 提示词层是减法 | 加任何一条提示词/规则前先问：**「拿掉它，Claude 会做错吗？」** 不会 → 不加；已存在的 → 删（每条规则都在稀释其余规则的注意力）。**反复漏掉的规则 → 从 CLAUDE.md 挪进 hook**（hook 每次一定跑，CLAUDE.md 只是建议）。上下文交接已由 `PreCompact` hook 自动生成 mini 交接包，**不用手写**；上下文吃紧时仍要显式说明，不悄悄膨胀 |
```

**条数不变**：CLAUDE.md 明写「规则数冻结在 13，要新增一条必须先删一条」—— 这里是**同位替换**
（旧的自动化职责交给 hook，腾出的位置放缺失的删减判据），不是新增第 14 条。

**顺带修正**：`coding-style.md` 的 Code Quality Checklist 里 `- [ ] No mutation (immutable patterns used)`
仍带着改动 3 收窄掉的绝对禁令 —— 同文件自相矛盾，已一并改为
`- [ ] No mutation of externally exposed data (immutable patterns used)`。

---

## 不做的事（避免过度工程）

- **不新增第 14 条 instinct**：按 CLAUDE.md「规则数冻结在 13，要新增一条必须先删一条」，新判据并入既有 instinct 3「完成前验证」的行动里。
- **不新建 `rules/common/criteria.md`**（子代理原建议）：rules 每次 session 全量注入，判据正文放那里等于把刚省下的 token 又花回去 —— 改为按需加载的 skill，只在 rules 里留一行指针。
- **不动 13 Rules 的条数与结构**，只收窄 Rule 3 措辞。
- **不新增任何 hook**（ROI 1–4 全部零 hook 成本）。
