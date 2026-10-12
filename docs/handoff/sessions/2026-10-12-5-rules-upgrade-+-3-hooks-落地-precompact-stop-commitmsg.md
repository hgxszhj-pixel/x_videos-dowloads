# 2026-10-12 — 5 CLAUDE.md 规则升级 + 3 hook 落地

## Why

Toutiao 视频 "agent 越用越顺靠四层边界" 的启发 → 发现本项目子任务层 (Rule 5) 调用率严重不足 + 对话层 (Rule 6) 缺自动交接 detector + 记忆系统缺自动化沉淀 — 3 个 gap 全部对应 CLAUDE.md 规则层的"自觉执行",**自觉不可靠**,需落到 hook 层强制。

承接 [[agent-4-layer-boundary-vs-cc-2026-10-12]] 的 5 项规则升级,把"应该做的事"变成"不做就报错"。

## What changed

**5 项 CLAUDE.md 内联编辑** (107→110 行):
- Demand Elegance 加 **Demand Delegation 子项** — ≥3 同类项 / >50KB 文件 / 多步降级抓取 → pause 30s 判是否 dispatch subagent
- Rule 5 加 **4 项主对话硬跑黑名单** — Explore / multimodal-looker / code-reviewer / loop
- Rule 6 改写为 **"自动生成 mini 交接包"** — 80% 上下文时主动落 HANDOFF.md
- Rule 12 增 **"未分流 = 失败"** 检测 — Stop hook 验 subagent 调用率
- 记忆系统加 **自动化模式 (agent 生成 + 人 review)** — commit hook 软提示

**3 个 hook 落地** (1 个扩,2 个新):

| Hook | 文件 | 触发 | 行为 |
|------|------|------|------|
| PreCompact mini-handoff | `~/.claude/hooks/precompact-mini-handoff.sh` (3.6K) | /compact 时 | 读 transcript + HANDOFF.md open_loops → 输出 3 段 mini (≤200 token) 注入 post-compaction context;同时追加到 mempalace NDJSON |
| Stop subagent-rate | `~/.claude/hooks/stop-subagent-rate.sh` (2.5K) | session 结束 | 数 tool_use: Agent=0 且 agent-eligible ≥ 阈值 (默认 10) 且 turns ≥ 4 → stderr WARN;**不阻塞 stop** |
| commit-msg memory-prompt | `scripts/git_hooks/memory_prompt.py` (3.4K) | git commit 时 | commit 含 INSIGHT_SIGNALS (fix/refactor/perf/feat/发现/教训/...) 且 memory/ 今日未更新 → stderr 软提示;**不阻塞 commit** |

**settings.json 接线**:
- PreCompact 链: 1 → 2 hooks (HANDOFF.md persister + mini-handoff)
- Stop 链: 2 → 3 hooks (fmt-gate + anti-slop + subagent-rate)

**commit-msg 注入新步骤**: Intent Lock + size_guard + openloops 之后 / Ref-trace 之前,`sys.path.insert` 调 memory_prompt;**失败 fallback** `except Exception` 不阻塞 commit。

**2 条 memory 沉淀**:
- `agent-4-layer-boundary-vs-cc-2026-10-12.md` — 视频 4 层 ↔ CC 对照
- `three-rule-hooks-precompact-stop-commitmsg-2026-10-12.md` — 5 规则 → 3 hook 对照表 + 阈值/触发/坑

## Verification (3/3 pass)

- **PreCompact mini-handoff** — 8 turns synthetic transcript → 输出 `turns=8 | tools: Read=5, Bash=2, Grep=1 | last_user: u8: again` + open_loops 段;HANDOFF.md 注入的 `## Open Loops — Active` 段正确被 awk 提取
- **Stop subagent-rate** — 8 turns + 0 Agent + 8 eligible → `SUBAGENT_MIN_THRESHOLD=5` 触发 stderr WARN;阈值=10 时正确静默;short transcript (<4 turns) 静默
- **commit-msg memory-prompt** — `fix:` commit + 全部 memory mtime 强制回退到 2024 → 软提示触发;`docs(readme): ...` 正确 skip;日常 session (memory 当日已被 touched) 正确 skip
- **settings.json 合法性** — `python3 -c "import json; json.load(...)"` 通过;PreCompact 2 hooks / Stop 3 hooks

## 已知坑 (实施踩的 2 个)

1. bash `$(python3 ... <<'PY' ... PY)` 嵌套触发 zsh "unterminated here-document" + 输出污染 → 改 mktemp + 临时 .py 文件 (Stop hook 用此方案)
2. memory_prompt 首次自测"全 skip" 实际是因今日 memory **已**被 touched (agent-4-layer 那条 30 分钟前写) — **正确行为**,不是 bug

## 风险 / 后续 (1-2 周观察期)

- `SUBAGENT_MIN_THRESHOLD=10` 阈值待调:长 session (>50 turns) 误报多 → 调 15
- memory_prompt INSIGHT_SIGNALS regex 缺中文新词 → 1-2 周后按实际触发率补词
- 改 `scripts/git_hooks/memory_prompt.py` 后**必须** `bash scripts/install_git_hooks.sh` 同步 installed copy (参 [[trace-link-commit-injection]] 教训)
- 重启 Claude Code 让新 hook 生效 (settings.json 启动时加载)
