# My Skill Coverage — 常用 / 偶用 / 未触达

> **起源**: 2026-10-19 自检 4 类「该用没用」诊断 (skill-mcp-usage-discipline-2026-10-19.md)
> **目的**: 防止「装了就忘」 — 110+ skills 装在 ~/.claude/skills/ + plugin marketplaces,
>         多数从来没被 Skill tool 唤起过。每条新 session 第一件事是来这查一下,挑该用的。
> **维护**: 每次用了 / 决定不用的 skill,在状态栏更新。`Date` 列为最近一次有效触发。

**统计**: ~/.claude/skills/ = 111 项 + plugin marketplaces ~30 项 = 约 140 个 skill。
**本文档只列我直接相关的,非相关 skill 永远不进表(避免噪音)。**

---

## 🟢 常用 (本 session 或近 7 天用过,直接放进工作流)

| Skill | 何时该用 | 最近触发 |
|-------|---------|---------|
| `using-superpowers` | **任何 session 起手** — 铁律"if 1% 可能适用则必须 invoke" | 永远在用 (系统层) |
| `superpowers:brainstorming` | "Let's build X" / 新功能需求模糊时 | 偶尔 |
| `superpowers:writing-skills` | 写 / 改 SKILL.md | 2026-10 |
| `superpowers:systematic-debugging` | 报 bug 第一步 | 偶尔 |
| `superpowers:test-driven-development` | tdd-guide agent 等价物 | 偶尔 |
| `hookify` / `hookify-local:conversation-analyzer` | 把"踩过的坑"转 hook | 2026-10 |
| `understand-anything` | 用户甩一篇链接/概念要快速消化 | 2026-10 |
| `book-to-skill` | 把 50KB 文档转 token-skill | 2026-10-07 |

## 🟡 偶用 (本次没主动 invoke,但场景触发了本该用)

| Skill | 何时该用 | 漏报案例 |
|-------|---------|---------|
| `code-review-canonical` | 写完代码 review 一次 | eval-gate 落地时**没派** code-reviewer |
| `plan-canonical` | 架构决策 / 跨模块重构 | 写 eval-gate 150 行脚本**没走 plan** |
| `debug-canonical` | 复杂 bug 隔离 | BUG-D / BUG-E 那类多 hypothesis 场景**没用** |
| `superpowers:verification-before-completion` | 写"完成"前 — 必走 verify | **本 session 4 个 todo 全没走** |
| `superpowers:writing-plans` | 实施步骤拆分 | TaskCreate 是廉价版,writing-plans 是结构化版 |
| `mattpocock-skills:writing-skills` | 写 skill 的 SOP | 与 superpowers:writing-skills 二选一 |
| `agent-skills:api-and-interface-design` | 公共 API 改动 | 改 clean.rs 接受参数时**没用** |
| `agent-skills:incremental-implementation` | 长任务分阶段 | eval-gate 一次写完,本可分阶段 |
| `agent-skills:doubt-driven-development` | "这个我是不是假设太多" | 大段代码前**没用** |

## ⚪ 未触达 (装在本机但 30+ 天没 trigger)

| Skill | 何时该用 | 为何没触达 |
|-------|---------|----------|
| `antfu` / `antfu-design` | Vue 生态风格 / antfu 个人风格 | 不用 Vue |
| `draw-io` / `slidev` / `remotion-best-practices` | 画图 / 幻灯片 / 视频 | 几乎不做演示 |
| `firecrawl-scraper` | 通用 web 抓取 | 走 camoufox / ego-lite 更准 |
| `page-agent` | UI 自动化 | 当前项目无 UI agent 需求 |
| `proactive-self-improving-agent` | 自我反思循环 | 类似 ultragoal 已在用 |
| `shannon` | 黑客松式 brainstorm | 非黑客松场景 |
| `shanhu-analyzer` | 珊瑚 SaaS 课程解析 | 只在下载小鹅通课用 |
| `douyin-video-download` | 抖音下载 | 当前项目不接抖音 |
| `learn` / `teach` / `read` | 学习循环 | 当前工作流未嵌入 |
| `health` / `hunt` | 自我优化 / bug hunt | ultragoal 已替代 |
| `skills-updater` | 自动更新 skills | 手动更可控 |
| `ralph-tui-*` (4 个) | TUI 任务管理 | ultragoal state 已替代 |
| `nitro` / `nuxt` / `pinia` / `slidev` | Nuxt 全家桶 | 项目不用 |
| `graphify` / `darwin-skill` / `check` | 数据图 / Darwin OS / 检查工具 | 没找到契合场景 |
| `skill-evolve` / `skill-manager` | skill 自我进化 | 与 writing-skills 部分重叠 |
| `gitnexus-*` (6 个) | 代码图分析 — ⚠️ **本 session 至少应触发 `gitnexus-exploring`** | 把 codegraph 顶替了 |

## 🔴 永久 watch (装而不删,仅重评触发)

| Skill | 重评触发条件 |
|-------|------------|
| `cli-anything` (plugin) | 真要做 CLI 工具时再 enable (现 false) |
| `last30days` | 30 天新闻雷达,已用 Thysrael/Horizon 替代 |
| `skill-creator` / `skill-doctor` | 与 writing-skills 重叠,已 false |
| `pyright-lsp` / `rust-analyzer-lsp` | 复杂 lsp 推理时再 enable |
| `context7` (plugin) | 装而不 enable,等真需要查 lib 文档时 |
| `karpathy-skills` (plugin) | false,仅当用户明确 ask 时 |
| `fast-jev-compaction` (plugin) | 测过 install,2.1.278 已装但 `/reload-plugins` 待验证 |
| `token-weather-usage` | UI 装饰, 用了而已 |
| `blast-radius` | 已装,等 blast-radius 决策时再深入 |

---

## 跨条目 link

- [[skill-mcp-usage-discipline-2026-10-19]] — 本表是 5 改进之"Skill tool 前置化"的具体落地
- [[mcp-agent-trim-2026-10-15]] — 30-day 95→65 trim 是"减法",本表是"用得对"维度
- [[claude-plugins-official-marketplace-lag-2026-10-15]] — 11 兜底 plugin,本表 ⚪ + 🔴 段
