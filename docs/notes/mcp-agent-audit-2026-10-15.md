---
description: MCP plugin + agent trim 2026-10-15 — 30-day 用量 grep 砍 19+11 niche agent + 3 未用 plugin,session 启动成本 ~5k tokens
type: meta
---

# MCP/Agent 启动成本审计 (2026-10-15)

## 触发

`/context` 显示 MCP 49.1k tokens (24.6%) + agents 12.7k (6.4%) = **65.1k / 200k = 32.5% context window**,远超必要。用户询问"哪些 skill / MCP 不必启动"。

## 方法

**Grep 30-day 调用证据**(不靠"猜测"):

```js
// 扫 95 个文件
const memDir = '~/.claude/projects/-Users-hgxszhj-x-videos-dowloads/memory/';   // 63 memory notes
const sessDir = './docs/handoff/sessions/';                                    // 32 session logs
// 对每个候选项(label, 关键词) 跨 95 文件 case-insensitive grep
// 关键词命中 = 该候选 "30 天内至少被代码/文档引用一次" 的最弱证据
```

## 档 1 — 19 高置信删 (2026-10-15 22:22 已执行)

### Niche / 跨语言 / 完全 0 调用
| 文件 | 大小 | 理由 |
|------|------|------|
| engineering-feishu-integration-developer.md | 20.6K | 项目无飞书集成 |
| engineering-incident-response-commander.md | 20.6K | 无 oncall 流程 |
| engineering-multi-agent-systems-architect.md | 28.6K | 0 调用,niche MAS 框架 |
| devops_engineer.md | 12.9K | 无 CI/CD 编排 |
| engineering-sre.md | 3.8K | 无生产 ops |
| database-reviewer.md | 4.2K | 项目非 SQL,纯 Rust |
| python-reviewer.md | 3.3K | 无 Python 项目 |
| go-build-resolver.md | 3.2K | 无 Go 项目 |
| go-reviewer.md | 2.7K | 无 Go 项目 |
| frontend_dev.md | 8.6K | 项目无前端模块 |
| frontend-ui-ux-engineer.md | 3.4K | 同上 |
| product_manager.md | 6.3K | 个人项目无 PM 流程 |
| tech-leader.md | 4.2K | 个人项目无 tech-lead |
| chief-of-staff.md | 5.5K | 无邮件 triage |
| qa_engineer.md | 12.3K | 自写测试 + drop test_engineer 兜底 |
| testing-api-tester.md | 12.2K | 0 调用 |
| testing-performance-benchmarker.md | 11.9K | 0 调用 |
| testing-reality-checker.md | 9.7K | 0 调用 |
| sisyphus.md | 5.2K | 自写 /orchestrate 即可 |

**档 1 合计 165.3 KB** 文件 + **估算 ~3.8k tokens** session 启动节省

## 档 2 — 11 兜底也删 (2026-10-15 22:25 已执行)

> 即使 0 调用,plugin/system reminder 强制仍触发骨架 → 用户决策:全部砍
>
| 文件 | 大小 | 替代品 |
|------|------|--------|
| code-reviewer.md | 8.6K | oh-my-claudecode:code-reviewer + agent-skills:code-reviewer |
| planner.md | 6.9K | oh-my-claudecode:plan + Skill `plan-canonical` |
| architect.md | 6.1K | oh-my-claudecode:architect + Skill `darwin-skill` |
| tdd-guide.md | 2.8K | oh-my-claudecode:test-engineer + Skill `tdd` |
| explore.md | 3.0K | Skill `Explore` (haiku, 只读) |
| oracle.md | 2.9K | Skill `oracle` + 普通主对话 |
| loop-operator.md | 0.9K | Skill `loop` (built-in) |
| harness-optimizer.md | 0.9K | Skill `harness-audit` |
| build-error-resolver.md | 3.7K | Skill `build-fix` |
| engineering-minimal-change-engineer.md | 11.0K | CLAUDE.md Rule 3 + Rule 14 哲学 |
| engineering-video-streaming-engineer.md | 12.6K | Skill `download-agent-loop` (项目专属) |

**档 2 合计 59.4 KB** 文件 + **估算 ~1.4k tokens** session 启动节省

## Plugins — 3 翻 `false` (已执行)

| Plugin | before | after | 理由 |
|--------|--------|-------|------|
| `context7@claude-plugins-official` | true | false | 0 调用,未 OAuth 认证,纯 dead 2 tool (~270 t 永转) |
| `skill-creator@claude-plugins-official` | true | false | 仅 1 历史 mention (claude-skills-symlink-pattern.md) |
| `skill-doctor@claude-community` | true | false | 仅 1 历史 mention (book-to-skill-poc-evaluation.md) |

**节省 ~500-700 tokens** session 启动

## 合计估算

| 维度 | 节省 |
|------|------|
| Agent 文件(档 1+2 = 30 个) | **~5.2k t** |
| Plugin flips | **~500-700 t** |
| **Total session 启动节省** | **~5.7-5.9k t** |

> 200k context 窗口的 **2.85-2.95%**(对比之前 33% MCP+agents 占比 → 估降到 30%)

## 当前 state

### ~/.claude/agents/ 剩余 9 个 (Tier 3 搁置 — 本次未动)
```
backend_dev.md               (9.4K)
doc-updater.md               (3.3K)
document-writer.md           (3.0K)
e2e-runner.md                (4.0K)
engineering-git-workflow-master.md   (3.3K)
librarian.md                 (3.2K)
multimodal-looker.md         (3.0K)
refactor-cleaner.md          (2.6K)
security-reviewer.md         (4.4K)
```
**保留理由**:git-workflow-master 与 OMC git-master 有微妙差异;refactor-cleaner 提供 dead-code 视角;其余为偶发兜底。

### ~/.claude/agents/.bak-20261015/ 30 文件全 backup
+ `~/.claude/.bak-20261015-settings.json` (settings 备份)

## 回滚 (单文件级别)

```bash
# 恢复任一 agent
cp ~/.claude/agents/.bak-20261015/<name>.md ~/.claude/agents/

# 恢复全部 30 个 agent
cp ~/.claude/agents/.bak-20261015/*.md ~/.claude/agents/

# 恢复 settings.json
cp ~/.claude/.bak-20261015-settings.json ~/.claude/settings.json

# 翻回任一 plugin (手动编辑 settings.json enabledPlugins 字段)
```

## 已知坑 / 风险

1. **本节段未亲自跑 `/context`**:本次 session 启动时已加载,实际节省需下次启动验证
2. **plugin description 缓存**:Claude Code 可能缓存 plugin 描述,翻 false 后首次 session 仍可能有 stale 输出
3. **回滚单 agent 不重建 settings.json plugin**:回滚 agent 与 disabled plugin 独立,可分别回滚

## 决策

- **D-2026-10-15-3**: MCP/Agent 启动成本审计 — 30 天 grep 0 调用即删(2 档),保留 9 兜底,settings.json 翻 3 plugin false
- **D-2026-10-15-4**: 删除 fallback 优先于禁用 — agent 文件 rm 而非 disabled 标记,plugins 翻 false 而非从 marketplace 移除(保留恢复路径)
- **D-2026-10-15-5**: 永久保留 `.bak-20261015/` 兜底直到下一次 audit (≥ 90 天)

## 下一步

- 重启 Claude Code 生效 plugin disable
- 重启后跑 `/context` 验证节省是否命中
- 若与项目 plugin-skill (oh-my-claudecode) 冲突 → 恢复 1 个最相关 (`engineering-video-streaming-engineer.md` 是项目直接 relevant 的兜底)
- 跟 normal /cleanup 流程合并档 3 (9 搁置) 评估 (≥ 2026-12)
