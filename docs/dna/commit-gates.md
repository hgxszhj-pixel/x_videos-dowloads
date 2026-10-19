# Commit-msg Gates (Harness² + Project DNA)

> **起源**: 2026-10-19 — 8 个 gate 散在 `scripts/git_hooks/commit-msg` 里,
>         新会话每次都要 grep 一遍才知道有什么。本文一表拉全,codegraph 二次核验。
> **目的**: 任何 session 调 `git commit` 之前,看一眼这表,知道会撞上哪些门。
> **逃生口汇总**: 全部 gate 都允许人工 override,但要写明 `[skip-xxx]` 或
>                `[DID:..]/[VC:..]` trailer(已被下层 gate 自动注入)。

## 7 段顺序执行表 (从 `scripts/git_hooks/commit-msg` 拆出)

| # | Gate | 触发条件 | 失败动作 | Escape hatch | 来源 |
|---|------|---------|---------|------------|------|
| 1 | **Intent Lock** | commit msg 必须含 `Intent: <一句话>` (10-200 字符) | 拒 commit + 提示格式 | 无 — 这是设计本身的契约 | `scripts/intent_lock.py` |
| 2 | **HANDOFF.md size guard** | HANDOFF.md 是 commit 一部分 时 | > 200 行 = 拒;<br>> 100 行 = 警告 | 拆分到 `docs/handoff/sessions/<date>.md` | `scripts/handoff_size_guard.py` |
| 3 | **open_loops schema** | HANDOFF.md 含 "Open Loops" / "Just Closed" 段 | Active 4 字段 / Blocked 3 字段 / Just Closed 含日期,任一缺 = 拒 | 迁移到 SKILL.state 3-section 风格 | `scripts/openloops_schema.py` |
| 4 | **memory-prompt 软提示** | commit 含学习/修复/重构信号 + memory/ 今日无新增 | stderr 软提示,人 review 后决定 | `try/except` 永远不阻 | `scripts/memory_prompt.py` |
| 5 | **eval-gate** ⭐ Harness² | commit msg 含"完成声明"关键词 + staged 含 *.rs | `cargo fmt --check` + `clippy --all-targets -- -D warnings` 任一不过 = 拒 | `[skip-eval]` 或 已签 `[DID:`/`[VC:` trailer | `scripts/eval_gate.py` |
| 6 | **Ref-trace 注入** | 今天 `~/.local/share/x_videos-dowloads-trace/YYYY-MM-DD/` 有 .ndjson | 自动追加 `Ref-trace: <abs_path>` 行 | 已含 `Ref-trace:` 行 no-op | `scripts/trace_link.py` |
| 7 | **DID/VC trailer 签发** | commit msg 不含 `[DID:` `[VC:` | 自动追加 `[DID: <id>] [VC: <hash>] rep=<score>` | 已签 no-op | `scripts/agent_did.py` |

## 关键词表 (eval-gate 用)

**英文** (regex word-boundary): `fix(es|ed)?` / `done` / `pass(es|ed)?` / `resolved?` / `complete[sd]?` / `implemented?` / `shipped?` / `successfully`

**中文** (子串): 修复 / 完成 / 通过 / 搞定 / 解决 / 实现 / 已修 / 调通 / 落地 / 交付

**否定** (trailer 排除): `Intent:` / `Ref-trace:` / `[DID:` / `[VC:` 行**不算**关键词命中
(否则 Intent Lock 字符串"实现 Harness² 思想本地化"会自我误触)

## 完整代码定位 (codegraph 一次性可查)

```
scripts/git_hooks/commit-msg              ← 7 段顺序编排
├── intent_lock.py  ::validate_intent     ← Gate 1
├── handoff_size_guard.py ::check_handoff_size  ← Gate 2
├── openloops_schema.py ::check_openloops_schema  ← Gate 3
├── memory_prompt.py ::check_memory_prompt  ← Gate 4 (软)
├── eval_gate.py  ::check_eval_gate       ← Gate 5 (核心)
├── trace_link.py  ::inject_ref_trace     ← Gate 6 (注入)
└── agent_did.py  ::issue_did             ← Gate 7 (签发)
```

## 调试 cheat sheet

```bash
# 模拟一次 commit 看会被哪些 gate 拦
git commit --allow-empty -m "fix: $INTENT" 2>&1 | head -30

# 临时绕过 eval-gate 但保留 Intent Lock
git commit -m "fix: $INTENT [skip-eval]"

# 看当前 commit msg 会被注入什么 trailer
cat .git/COMMIT_EDITMSG
```

## 装/改/卸流程

1. 源在 `scripts/git_hooks/*` (git 跟踪)
2. 装/同步:`bash scripts/install_git_hooks.sh` 复制到 `.git/hooks/*`
3. 改源后必须跑 2 同步 — 详见 [[wpr-dual-copy-and-agents-md]] 通用模式

## 跨条目 link

- [[eval-gate-commit-time-2026-10-19]] — Gate 5 详细实现
- [[handoff-size-guard-hook]] — Gate 2
- [[open-loops-skillstate-style-2026-10-07]] — Gate 3 schema
- [[trace-link-commit-injection]] — Gate 6
- [[three-rule-hooks-precompact-stop-commitmsg-2026-10-12]] — hook 设计层面
