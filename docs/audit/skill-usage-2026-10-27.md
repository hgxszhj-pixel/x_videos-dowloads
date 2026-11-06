# 技能用量分档扫描（2026-09-29 → 2026-10-27）

> 只读扫描。本报告不修改/删除/移动任何技能目录或配置。
> 先例方法：agent 侧 trim（`memory/mcp-agent-trim-2026-10-15.md`）—— 30 天 grep 用量 → 分档 → 备份归档。本报告是其在 skill 侧的对称版本。

## 概览

| 项 | 值 |
|---|---|
| 技能总数（`~/.claude/skills/*/SKILL.md`） | **119** |
| Tier 1 — 活（保留） | **12** |
| Tier 2 — 休眠（保留，标注） | **16** |
| Tier 3 — 死（trim 候选） | **91** |
| 技能库总体积 | 52.5 MB |
| Tier 3 体积 | 29.1 MB（占 55.3%） |
| 扫描的 JSONL 文件数 | 64（主会话 31 + `subagents/` 33） |
| JSONL 记录行数 | 179,611 |
| JSONL 总字节 | 390,909,122（≈373 MiB） |
| 语料时间跨度 | 2026-09-21T09:33Z → 2026-10-27T10:10Z |
| 30 天统计窗口 | 2026-09-27T00:00Z → 2026-10-27（窗口外 6 天证据单独计数，不参与分档） |
| 扫描的全局文档/脚本文件 | 121 |

**最刺眼的一个数字**：全语料 `Skill` 工具真实调用共 **6 次**，其中只有 **3 次**指向本地技能（另 3 次指向 plugin 技能 `superpowers:dispatching-parallel-agents` / `superpowers:understand-anything` / `oh-my-claudecode:hud`）。119 个技能，3 次调用 —— 装着不用的问题比 agent 侧更严重。

**但真正被"拿去干活"的只有 8 个**（见 Tier 1 表的「证据类型」列）：另外 4 个 Tier 1 是靠"被文档引用 ≥2 次"抬上来的，证据性质是**创作/维护**而非**使用**。

### 目录构成

| 形态 | 数量 | 说明 |
|---|---|---|
| 真实目录 | 82 | 本机自建或从上游 `add` 装入的拷贝 |
| symlink → `~/.agents/skills/*` | 33 | 与 agent 侧共用的真实源 |
| symlink → 其他 | 4 | `cua-driver`→`~/.cua-driver/skills`、`understand` + `understand-chat`→`~/.claude/plugins/Understand-Anything/...`、`ego-browser`→`~/.local/share/ego/ego-skills` |

---

## Tier 1 — 活（保留）

判定：≥1 次强证据（`Skill` 工具真实调用 / 直接 `Read` 该 `SKILL.md` / 手动 slash command），**或** ≥2 次中证据（Agent prompt 引用 + 全局文档引用合计）。

| 技能 | 最近使用 | 证据类型 | 次数 |
|---|---|---|---|
| `cargo-verify-on-startup` | 2026-10-25 | **强** · `Skill` 工具调用（`"input":{"skill":"cargo-verify-on-startup"}`）| 1 次调用 + 6 次 SKILL.md 改写（接入项目 CLAUDE.md 启动命令） |
| `douyin-video-download` | 2026-10-12 | **强** · `Skill` 工具调用 | 1 次调用 |
| `the-scribble-thing` | 2026-10-08 | **强** · `Skill` 工具调用 | 1 次调用 |
| `book-to-skill` | 2026-10-22 | **强** · 手动 slash `/book-to-skill`（raw grep 确认 2 次）+ `scripts/extract.py` 真实执行 | 2 次 slash + 4 次 SKILL.md 读取 + 4 个文档引用 |
| `shanhu-analyzer` | 2026-10-03 | **强** · `Read` 该 `SKILL.md` | 1 次 Read + 1 个文档引用 |
| `harness2-recursive-improvement` | 2026-10-19 | **强** · `Read` 该 `SKILL.md` | 1 次 Read + 12 次 Edit + 13 次文件操作 |
| `terminal-rsi-recursive-agent` | 2026-10-19 | **强** · `Read` 该 `SKILL.md` | 1 次 Read + 21 次 Edit（v0.1.0→v0.2.0）+ 2 个文档引用 |
| `news-radar` | 2026-10-16 | **中** · Agent prompt 引用 + 2 个文档引用 | 1 次 Agent prompt + 2 读 + 2 文档 + 4 次写入（端到端装好、key 走通） |
| `jq` | 2026-10-22 | **中** · 4 个文档/脚本引用 | ⚠️ 证据性质是**被 book-to-skill 生成物**，非被使用 |
| `write-a-skill` | 2026-10-07 | **中** · 3 个文档引用 | ⚠️ 证据性质是**本地自治 fork**（上游 mattpocock 已删，本地保留手改的 Content Discipline 节） |
| `rexxi-course-m3u8` | 2026-10-15 | **中** · 2 个文档引用 | ⚠️ 证据性质是**经验沉淀被 DNA 文档引为实证**，非被调用 |
| `fireworks-tech-graph` | 2026-10-15 | **中** · 2 个文档引用 | ⚠️ 证据性质是**安装记录**（9.1 MB，单个最大 Tier 1） |

**Tier 1 拆解**：真正"被调用过"= **8 个**（前 8 行）；只因文档引用数 ≥2 抬上来的 = **4 个**（后 4 行）。

---

## Tier 2 — 休眠（保留，标注）

判定：无强证据，但被全局文档/hook/脚本引用（说明写进了工作流但近期没用），或有 1 次中证据。

| 技能 | 最后一次证据 | 证据类型 | 被哪里引用 |
|---|---|---|---|
| `web-protocol-recovery` | 2026-10-12 | 文档引用 ×1 | `memory/wpr-dual-copy-and-agents-md.md`（双份拷贝，master 为权威版）— 4.0 MB |
| `darwin-skill` | 2026-10-15 | 文档引用 ×1 | `memory/book-to-skill-poc-evaluation.md`："我们手写的工作流 skill(`darwin-skill` …)" — 5.9 MB |
| `zhixueyun-analyzer` | 2026-10-27 | 文档引用 ×1 | `memory/jfedu-rexxi-shanhu-m3u8-download.md`（作为 shanhu-analyzer 的同类模板被提及） |
| `gitnexus-analyzer` | 2026-10-22 | 文档引用 ×1 | `memory/skill-acceptance-gate-r2s-2026-10-22.md`（审计表；结论：故意的工具变体，非缺陷） |
| `remotion-best-practices` | 2026-10-22 | 文档引用 ×1 | 同上（审计表；结论：真重复，两份只差 1 个文件） |
| `slidev` | 2026-10-22 | 文档引用 ×1 | 同上（审计表；结论：结构不同的两种形态） |
| `vercel-react-best-practices` | 2026-10-22 | 文档引用 ×1 | 同上（审计表；两份均有断链） |
| `tsdown` | 2026-10-22 | 文档引用 ×1 | 同上（链向不存在的 `../tsdown-migrate/SKILL.md`） |
| `vueuse-functions` | 2026-10-22 | 文档引用 ×1 | 同上（6 条链接指向不存在的同级技能目录） |
| `openclaw-self-healing` | 2026-10-22 | 文档引用 ×1 | 同上（`docs/DOCKER.md` 链向不存在的 `configuration.md`） |
| `firecrawl-scraper` | 2026-10-15 | 文档引用 ×1 | `memory/fireworks-tech-graph-skill-integration.md`（对比提及："不同于 firecrawl-scraper / gitnexus 等偏数据/分析"） |
| `skill-creator` | 2026-10-19 | 文档引用 ×1 | `memory/MEMORY.md` — ⚠️ 该处指的是**同名 plugin**（`context7/skill-creator/skill-doctor`），不是本地这份，存在命名空间歧义 |
| `to-issues` | 2026-10-15 | 文档引用 ×1 | `memory/claude-skills-symlink-pattern.md` — "mattpocock 已删 diagnose/to-issues/to-prd/zoom-out/caveman/write-a-skill"（上游删除清单，非工作流引用） |
| `to-prd` | 2026-10-15 | 文档引用 ×1 | 同上（同一行的上游删除清单） |
| `zoom-out` | 2026-10-15 | 文档引用 ×1 | 同上（同一行的上游删除清单） |
| `diagnose` | 2026-10-15 | 文档引用 ×1 | 同上（同一行的上游删除清单） |

> **Tier 2 的真实含金量**：16 个里，**只有 `web-protocol-recovery` 和 `rexxi-course-m3u8` 家族算"写进了工作流"**；其余 14 个的"引用"全部来自**审计报告表格**或**上游删除清单**——即"我们知道它有毛病"，不是"我们用它干活"。按任务定义它们仍是 Tier 2（保留），但如果要做第二轮 trim，这 14 个是第一批候选。

---

## Tier 3 — 死（trim 候选）

判定：30 天内零强证据、零中证据，且未被任何全局文档引用。**91 个技能，占技能库体积的 55.3%（29.1 MB）**。

归档建议图例：
- **A 归档** — 零证据、无托管关系、与本项目无关 → 直接移入备份目录
- **B 新建/新装未用** — 2026-10 内才创建或安装，尚无使用记录 → 观察 30 天再定
- **C 工具托管** — symlink 指向 plugin/工具目录 → 移除 `~/.claude/skills` 下的链接即可，源不动
- **D 项目已有副本** — 项目级 `.claude/skills/` 已覆盖同名 → 可归档全局副本
- **E 同名遮蔽** — 被 Claude Code 内置同名技能遮蔽，本地这份不会暴露 → 直接归档
- **F 前端/动效家族** — 整个 JS/前端技能族，本项目是 Rust CLI → 整族归档

| 技能 | 目录大小 | symlink? | 归档建议 |
|---|---|---|---|
| `page-agent` | 18538 KB | 否 | **A 归档** — 最大单点 18.5 MB，零证据 |
| `task-observer` | 3326 KB | 是 | **A 归档** — 3.3 MB，零证据 |
| `media-use` | 1806 KB | 否 | **A 归档** — 媒体解析能力已被 multimodal-looker 覆盖 |
| `hyperframes-animation` | 1240 KB | 否 | **F 前端/动效家族** — 与 hyperframes 共 11 个，4.6 MB，整体零证据 |
| `hyperframes-creative` | 1175 KB | 否 | **F 前端/动效家族** |
| `understand` | 332 KB | 是 | **C 工具托管** — symlink → Understand-Anything 插件；移除链接即可（源在插件目录） |
| `security-review` | 223 KB | 否 | **E 同名遮蔽** — Claude Code 内置同名技能，本地这份不会被暴露，可直接归档 |
| `skills-updater` | 198 KB | 否 | **A 归档** — 零强证据、零中证据、未被任何全局文档引用 |
| `distilly` | 197 KB | 是 | **B 新装未用** — 2026-10 才装，与 book-to-skill 职责重叠，观察 30 天 |
| `pua` | 193 KB | 是 | **A 归档** — 零强证据、零中证据、未被任何全局文档引用 |
| `hyperframes` | 156 KB | 否 | **F 前端/动效家族** — 家族根 |
| `watch` | 135 KB | 否 | **A 归档** — 零强证据、零中证据、未被任何全局文档引用 |
| `hyperframes-cli` | 128 KB | 否 | **F 前端/动效家族** |
| `turborepo` | 127 KB | 否 | **F 前端家族** |
| `nuxt` | 115 KB | 否 | **F 前端家族** |
| `vue-best-practices` | 110 KB | 否 | **F 前端家族** |
| `cua-driver` | 101 KB | 是 | **C 工具托管** — symlink → `~/.cua-driver/skills` |
| `hyperframes-audio` | 100 KB | 否 | **F 前端/动效家族** |
| `hyperframes-core` | 98 KB | 否 | **F 前端/动效家族** |
| `ego-browser` | 96 KB | 是 | **C 工具托管** — symlink → `~/.local/share/ego/ego-skills` |
| `vitest` | 94 KB | 否 | **F 前端家族** |
| `pnpm` | 84 KB | 否 | **F 前端家族** — 文档命中是包管理器同名误判 |
| `hyperframes-registry` | 79 KB | 否 | **F 前端/动效家族** |
| `brainstorming` | 78 KB | 是 | **A 归档** — 零强证据、零中证据、未被任何全局文档引用 |
| `unocss` | 64 KB | 否 | **F 前端家族** |
| `vue-testing-best-practices` | 63 KB | 否 | **F 前端家族** |
| `vitepress` | 62 KB | 否 | **F 前端家族** |
| `zhixueyun-kc-video` | 60 KB | 否 | **A 归档** — 零强证据、零中证据、未被任何全局文档引用 |
| `antfu-design` | 56 KB | 否 | **F 前端家族** |
| `nitro` | 55 KB | 否 | **F 前端家族** |
| `memory-retrieval-learning` | 48 KB | 是 | **A 归档** — 零强证据、零中证据、未被任何全局文档引用 |
| `graphify` | 47 KB | 否 | **A 归档** — 零强证据、零中证据、未被任何全局文档引用 |
| `vue-router-best-practices` | 43 KB | 否 | **F 前端家族** |
| `debug-canonical` | 40 KB | 否 | **A 归档** — canonical 三兄弟 |
| `pinia` | 31 KB | 否 | **F 前端家族** |
| `agent-memory-systems` | 30 KB | 是 | **B 检视未用** — 2026-10-19 仅检视，未启用 |
| `hyperframes-keyframes` | 23 KB | 否 | **F 前端/动效家族** |
| `antfu` | 22 KB | 否 | **F 前端家族** |
| `vite` | 21 KB | 否 | **F 前端家族** |
| `setup-matt-pocock-skills` | 19 KB | 是 | **A 归档** — 零强证据、零中证据、未被任何全局文档引用 |
| `triage` | 19 KB | 是 | **A 归档** — 零强证据、零中证据、未被任何全局文档引用 |
| `systematic-debugging` | 17 KB | 是 | **A 归档** — 零强证据、零中证据、未被任何全局文档引用 |
| `vue` | 17 KB | 否 | **F 前端家族** — 本项目为 Rust CLI，前端技能零证据 |
| `proactive-self-improving-agent` | 15 KB | 否 | **A 归档** — 零强证据、零中证据、未被任何全局文档引用 |
| `improve-codebase-architecture` | 13 KB | 是 | **A 归档** — 零强证据、零中证据、未被任何全局文档引用 |
| `git-hooks-automation` | 12 KB | 是 | **A 归档** — 零强证据、零中证据、未被任何全局文档引用 |
| `design` | 11 KB | 否 | **A 归档** — 零强证据、零中证据、未被任何全局文档引用 |
| `ralph-tui-create-beads-rust` | 11 KB | 否 | **A 归档** — ralph-tui 家族 |
| `hyperframes-studio` | 11 KB | 否 | **F 前端/动效家族** |
| `ralph-tui-create-beads` | 10 KB | 否 | **A 归档** — ralph-tui 家族 4 个整体零证据 |
| `ralph-tui-create-json` | 10 KB | 否 | **A 归档** — ralph-tui 家族 |
| `code-review-canonical` | 9 KB | 否 | **A 归档** — canonical 三兄弟（code/debug/plan-review）整体零证据 |
| `ralph-tui-prd` | 9 KB | 否 | **A 归档** — ralph-tui 家族 |
| `health` | 9 KB | 否 | **A 归档** — 零强证据、零中证据、未被任何全局文档引用 |
| `plan-canonical` | 9 KB | 否 | **A 归档** — canonical 三兄弟 |
| `eli5` | 8 KB | 是 | **A 归档** — 零强证据、零中证据、未被任何全局文档引用 |
| `hunt` | 7 KB | 否 | **B 新装未用** — 2026-10-15 从 Waza 复制，观察 30 天 |
| `agent-memory-architecture-cn-media` | 7 KB | 否 | **B 新建未用** — 2026-10-19 写出，观察 30 天 |
| `skill-selection-3-criteria` | 7 KB | 否 | **B 新建未用** — 2026-10-19 写出，观察 30 天 |
| `check` | 7 KB | 否 | **A 归档** — 零强证据、零中证据、未被任何全局文档引用 |
| `claudemd-rules-to-skills` | 7 KB | 否 | **B 新建未用** — 2026-10-19 写出，观察 30 天 |
| `draw-io` | 7 KB | 否 | **A 归档** — 零强证据、零中证据、未被任何全局文档引用 |
| `tdd` | 7 KB | 是 | **A 归档** — 零强证据、零中证据、未被任何全局文档引用 |
| `learn` | 6 KB | 否 | **A 归档** — 零强证据、零中证据、未被任何全局文档引用 |
| `think` | 6 KB | 否 | **A 归档** — 零强证据、零中证据、未被任何全局文档引用 |
| `remember` | 6 KB | 是 | **A 归档** — 零强证据、零中证据、未被任何全局文档引用 |
| `skill-evolve` | 5 KB | 否 | **B 改写未用** — 2026-10-03 改写，观察 30 天 |
| `find-skills` | 5 KB | 是 | **A 归档** — 零强证据、零中证据、未被任何全局文档引用 |
| `gitnexus-pr-review` | 5 KB | 否 | **A 归档** — 项目级副本不含此项 |
| `gitnexus-refactoring` | 4 KB | 否 | **D 项目已有副本** — 项目级 `.claude/skills/gitnexus/` 已含同名 |
| `read` | 4 KB | 否 | **A 归档** — 零强证据、零中证据、未被任何全局文档引用 |
| `gitnexus-cli` | 4 KB | 否 | **D 项目已有副本** |
| `knowledge-distill` | 4 KB | 否 | **B 新建未用** — 2026-10-19 写出，观察 30 天 |
| `better-icons` | 4 KB | 是 | **A 归档** — 零强证据、零中证据、未被任何全局文档引用 |
| `write` | 3 KB | 否 | **A 归档** — 零强证据、零中证据、未被任何全局文档引用 |
| `gitnexus-debugging` | 3 KB | 否 | **D 项目已有副本** |
| `understand-chat` | 3 KB | 是 | **C 工具托管** — symlink → Understand-Anything 插件 |
| `gitnexus-exploring` | 3 KB | 否 | **D 项目已有副本** |
| `conventional-commit` | 3 KB | 是 | **A 归档** — 零强证据、零中证据、未被任何全局文档引用 |
| `gitnexus-guide` | 3 KB | 否 | **D 项目已有副本** |
| `agent-memory-mcp` | 3 KB | 是 | **B 检视未用** — 2026-10-19 仅检视，未启用 |
| `shannon` | 3 KB | 否 | **A 归档** — 零强证据、零中证据、未被任何全局文档引用 |
| `teach` | 3 KB | 否 | **A 归档** — 零强证据、零中证据、未被任何全局文档引用 |
| `gitnexus-impact-analysis` | 3 KB | 否 | **D 项目已有副本** |
| `memory-ecosystem` | 2 KB | 否 | **A 归档** — 零强证据、零中证据、未被任何全局文档引用 |
| `orca-linear` | 2 KB | 是 | **A 归档** — 零强证据、零中证据、未被任何全局文档引用 |
| `workflow-discovery` | 2 KB | 否 | **A 归档** — 零强证据、零中证据、未被任何全局文档引用 |
| `web-design-guidelines` | 1 KB | 否 | **A 归档** — 零强证据、零中证据、未被任何全局文档引用 |
| `openspec` | 1 KB | 否 | **A 归档** — 零强证据、零中证据、未被任何全局文档引用 |
| `grill-with-docs` | 0 KB | 是 | **A 归档** — 零强证据、零中证据、未被任何全局文档引用 |
| `grill-me` | 0 KB | 是 | **A 归档** — 零强证据、零中证据、未被任何全局文档引用 |

### Tier 3 的成族结构（比逐个删更值钱）

| 家族 | 成员数 | 合计体积 | 判据 |
|---|---|---|---|
| **hyperframes 动效族** | 11 | ~4.6 MB | 全家零证据；与本项目（Rust 视频下载 CLI）毫无关系 |
| **Vue/前端族** | 16 | ~1.6 MB | `vue` `vue-best-practices` `vue-testing-best-practices` `vue-router-best-practices` `vueuse-functions` `nuxt` `nitro` `pinia` `vite` `vitepress` `vitest` `unocss` `turborepo` `pnpm` `antfu` `antfu-design` —— 本机没有前端项目 |
| **gitnexus 族** | 7 | ~30 KB | 其中 6 个在项目级 `.claude/skills/gitnexus/` 已有同名副本（全局副本冗余） |
| **ralph-tui 族** | 4 | ~40 KB | 全家零证据 |
| **canonical 三兄弟** | 3 | ~58 KB | `code-review-canonical` / `debug-canonical` / `plan-canonical` 全家零证据 |

### 单点收益 Top 5（只搬这 5 个就回收 25.6 MB / 技能库的 49%）

1. `page-agent` — 18.5 MB
2. `task-observer` — 3.3 MB
3. `media-use` — 1.8 MB
4. `hyperframes-animation` — 1.2 MB
5. `hyperframes-creative` — 1.2 MB

---

## 附：扫描方法与口径

### 实际执行的 grep 命令（已验证可复现）

```bash
cd ~/.claude/projects

# A. Skill 工具真实调用（工具定义会被 grep 到，故必须同时要求 "input":{"skill":）
find . -name '*.jsonl' -exec grep -ohoE '"name":"Skill","input":\{"[a-z]+":"[^"]+"' {} \; \
  | sort | uniq -c
# 实际输出（6 条，其中 3 条是本地技能）：
#   1 "name":"Skill","input":{"skill":"cargo-verify-on-startup"
#   1 "name":"Skill","input":{"skill":"douyin-video-download"
#   1 "name":"Skill","input":{"skill":"the-scribble-thing"
#   1 "name":"Skill","input":{"skill":"oh-my-claudecode:hud"
#   1 "name":"Skill","input":{"skill":"superpowers:dispatching-parallel-agents"
#   1 "name":"Skill","input":{"skill":"superpowers:understand-anything"

# B. SKILL.md 路径引用（按目录名归一到技能）
find . -name '*.jsonl' -exec grep -ohoE 'skills/[A-Za-z0-9._@+-]+/SKILL\.md' {} \; \
  | sort | uniq -c | sort -rn | head -10
# 实际输出 top：
#  170 skills/cargo-verify-on-startup/SKILL.md
#  131 skills/jq/SKILL.md
#  126 skills/terminal-rsi-recursive-agent/SKILL.md
#  107 skills/news-radar/SKILL.md
#   96 skills/harness2-recursive-improvement/SKILL.md

# C. 手动 slash command（/clear /exit /context /plugin /reload-plugins /mcp 为内置，不计）
find . -name '*.jsonl' -exec grep -ohoE '<command-name>[^<]{0,60}</command-name>' {} \; \
  | sort | uniq -c | sort -rn | head
# 实际输出：33 /clear、11 /exit、9 /context、8 X、8 /plugin、7 /reload-plugins、2 /book-to-skill
```

B 的 raw 计数是"出现次数"（一条命令里出现多次路径会重复计），**分档用的是去重后的结构化计数**：逐条 JSONL 解析出 `tool_use` 块，按 `input.file_path` / `input.command` 字段归一，并把 `Write`/`Edit` 的 **`content` 字段排除**（写在文档正文里的技能名是提及，不是引用）。

### 信号强度定义（本报告实际采用）

| 代号 | 信号 | 强度 | 备注 |
|---|---|---|---|
| S1 | `tool_use{name:"Skill", input.skill:"<name>"}` | **强** | 结构化解析，排除工具定义 |
| S2 | `tool_use{name:"Read", input.file_path:"…/skills/<name>/SKILL.md"}` | **强** | 直接把技能读进上下文 |
| S3 | Bash 直接读取该 `SKILL.md` | **强**（若为"照着做"）/ 审计型不计入 | 区分"读来执行"与"读来审计" |
| M1 | `<command-name>/<name></command-name>` | 中 | 排除内置命令 |
| M2 | `Agent` 工具的 prompt/description 显式点名该技能 | 中 | |
| M3 | 全局文档 / hook / 脚本引用 | 中 | 121 个文件逐行核验，见下 |
| — | 纯文本提及（assistant/user 正文出现技能名） | **弱，不计入分档** | 见"被排除的信号" |
| A1 | 该技能自己的 `SKILL.md` 被 Write/Edit | 单列，不计入"使用" | 属创作/维护证据 |

### 被排除的信号（口径说明，避免虚高）

1. **系统提示里的技能清单复读**。每轮对话都会注入一份可用技能列表，命中量极大（`workflow-authoring` 出现 349 次即典型）。做法：在每个 text 块中定位 `The following skills are available` / `available for use with the Skill tool` 标记，把该标记之后的整段切掉再匹配。实测切掉后，模糊短名的命中从 54 处（清单内残留）降到 0 污染。
2. **assistant 正文里的 `` `xxx` skill `` 字样**。按任务要求不作为使用证据 —— 实测其主要来源是**技能库管理对话本身**（"已删/保留/备份/重复/该用没用"），会把 `distilly` `grill-me` `hunt` `learn` `tdd` `read` `write` 全部虚高。
3. **tool_result 回声**。`tool_result` 记录在 JSONL 里 `type:"user"`，其中的文本是工具输出而非用户输入。第一版口径把它误算成"用户提及"，导致 `darwin-skill`/`security-review`/`skill-creator` 等凭空拿到 5–27 次"用户提及"。复核后全部剔除 —— 复核结论：**这些技能的用户提及数实为 0**。

### 逐行核验排除的同名误判（文档引用信号 M3 的假阳性）

| 技能 | 命中的文档 | 实际是什么 | 处置 |
|---|---|---|---|
| `jq` | `~/.claude/hooks/rtk-rewrite.sh` | **jq 二进制**的依赖检查与报错文案 | 剔除 |
| `pnpm` | `~/.claude/scripts/project-analyzer.sh` | **pnpm 包管理器**的 lockfile 探测 | 剔除 |
| `learn` | `memory/ai-research-sites.md` | `machinelearning.apple.com` / `pinecone.io/learn/` | 剔除 |
| `watch` | `memory/open-loops-skillstate-style-2026-10-07.md` | **YouTube URL** `youtube.com/watch?v=…` | 剔除（该技能因此落到 Tier 3） |
| `skill-creator` | `memory/MEMORY.md` | 指**同名 plugin**（`context7/skill-creator/skill-doctor`） | 保留但标注歧义 |

### 命名空间歧义 / 遮蔽问题

1. **本地 skill 与 plugin/内置同名（16 个）**：`systematic-debugging` `brainstorming` `tdd` `security-review` `understand` `understand-chat` `grill-me` `grill-with-docs` `teach` `eli5` `triage` `ego-browser` `remember` `improve-codebase-architecture` `setup-matt-pocock-skills` `gitnexus-analyzer`。多数以 `plugin:` 前缀并存（如 `superpowers:systematic-debugging` 与本地 `systematic-debugging` 同时出现在可用列表），**统计时按目录名归一**，但用户视角会混淆。
2. **`security-review` 被内置遮蔽**：可用技能列表里 `security-review` 只出现在**内置分组**（末尾），本地 `~/.claude/skills/security-review`（223 KB）实际**从未暴露给模型** —— 属于"装了但 Claude Code 看不见"的死重。
3. **plugin 前缀**：`Skill` 工具的 `input.skill` 可为 `plugin:skill`（本语料中 3 次），本报告按"本地技能目录名"归一时不匹配 plugin 前缀，故这些调用不计入任何本地技能。
4. **同一技能的多种写法**：按目录名归一（`zhixueyun-analyzer` vs `zhixueyun-kc-video` 是两个独立技能，不合并；`skill-creator` vs `skill-create` 是两个，不合并）。
5. **`source-command-*` 前缀族不在本次范围**：`~/.agents/skills` 里有 30 个 `source-command-*` 技能未 symlink 进 `~/.claude/skills`，因此**未暴露**给 Claude Code，语料里也不会以本地技能身份出现。

### 数据缺口与已知偏差（影响可信度的部分）

1. **扫描范围只覆盖 `~/.claude/skills` 的 119 个**。`~/.agents/skills` 实际有 **117 个 SKILL.md，只有 33 个 symlink 进来**，其余 **84 个从未暴露给 Claude Code**。语料中 `skills/continuous-learning/SKILL.md` 等 56 次命中来自 `~/.claude/plugins/marketplaces/{everything-claude-code,ecc}/skills/`（plugin 市场目录），**不在本次 119 之列**，属于 scope 外数据缺口。
2. **语料只覆盖 1 个项目**（`-Users-hgxszhj-x-videos-dowloads`）的会话。`~/.claude/projects/` 下共 64 个 JSONL，全部属于该项目；其他项目若有会话未落在本机，则相应技能的"死"判定会偏乐观。
3. **窗口比 30 天略宽**：语料起点 2026-09-21，早于窗口 6 天。窗口外证据单独计入 `pre` 字段，**不参与分档**（实测 119 个技能的窗口外证据均为 0，故无实际影响）。
4. **本次扫描自身的会话也在语料内**：会话文件数在扫描期间从 62 涨到 64，`maxTs` 已到 2026-10-27T10:10Z。有轻微自指污染（本次任务生成的 `/book-to-skill` 相关讨论可能推高 `book-to-skill` 的计数），但不影响其 Tier 1 判定（`/book-to-skill` slash command 在本次任务之前已有 2 次）。
5. **"审计型读取"与"使用型读取"的区分靠人工复核**：raw grep 无法区分 `cat SKILL.md` 是"照着做"还是"查 frontmatter"。已逐条人工复核 17 个有 Bash 读取的技能：只有 `book-to-skill` 属使用型，其余 16 个均为审计/安装检视（`wc -l`、`head -12` 看 frontmatter、`diff` 两份副本、`md5` 比对）。
6. **Tier 1 里有 4 个是"引用数抬上去"的**（`jq` `write-a-skill` `rexxi-course-m3u8` `fireworks-tech-graph`）。按任务给定的判定公式它们算 Tier 1，但证据性质是创作/维护/安装记录，**不是被调用**。若改用更严的"仅强证据"口径，Tier 1 会缩到 **8 个**。

### 结论与建议（下一步，非本任务动作）

- **第一刀**：只搬 5 个（`page-agent` `task-observer` `media-use` `hyperframes-animation` `hyperframes-creative`），回收 25.6 MB，零风险（全部零证据）。
- **第二刀**：成族搬 —— hyperframes 剩余 6 个 + Vue/前端族 16 个 + ralph-tui 4 个 + canonical 3 个 + gitnexus 全局副本 7 个。
- **例外保留**：`security-review`（内置遮蔽，可直接删）、`understand` / `understand-chat` / `ego-browser` / `cua-driver`（工具托管，删链接不删源）。
- **观察期**：`distilly` `hunt` `agent-memory-architecture-cn-media` `skill-selection-3-criteria` `claudemd-rules-to-skills` `knowledge-distill` `skill-evolve` `agent-memory-systems` `agent-memory-mcp` —— 都是 2026-10 新建/新装，尚未获得一次使用机会，30 天后再判。
- **元问题**：119 个技能里 8 个真被调用、3 次 `Skill` 调用；同期 agent 侧 trim 砍掉 30 个 agent。这份技能库的主要成本不是磁盘（52.5 MB 无所谓），而是**每轮会话注入 119 条技能描述的 context 开销**（对照 `mcp-agent-trim-2026-10-15.md` 里 session 启动省 5.7–5.9k tokens 的量级）。trim 的收益主要在这里，不在磁盘。

---

## 结果回填（2026-10-27 trim 已执行）

| 项 | 数 | 说明 |
|---|---|---|
| trim 前技能数 | **119** | 口径：`~/.claude/skills/*/SKILL.md`（本报告口径，✅ 可复算） |
| 移出（Tier 3） | 91 | 移出扫描路径 |
| 从归档区回滚 | 3 | `code-review-canonical` / `plan-canonical` / `debug-canonical` —— 项目账本 `docs/dna/my-skill-coverage.md` 🟡 段标为「该用没用」，归档等于删待用能力 |
| 归档 | **88** | `~/.claude/_archive/skills-2026-10-27/`（`~/.claude/skills` 的**兄弟**目录，不在扫描路径）+ `restore.sh` 一键回滚 + `MANIFEST.md` |
| trim 后技能数 | **32** | 含新增判据层 skill `criteria`；31 + 1 |
| 实体体积回收 | 29.5 MB | |
| **实测 token 收益** | **Δ 1145 token/session** | 真移动前后对照（n=2），非估算 |

**两条口径校正（都踩过）**：
1. **元问题里的 "119 条技能描述注入" 是我按字节估的「9–14k token」** —— 实测 Claude Code 对 skill description
   **强截断**（~12.6 token/技能），真实总量就是上面那个 Δ1145。**按字节估 token = 错**。
2. **`~/.claude/skills/` 下还有 4 个不含 `SKILL.md` 的目录**（`hookify/` 插件源码仓、`learned/` 空目录、
   `skill-manager/` 工具仓、`workflow`→`~/.claude/intelligence` 符号链接）——
   **数技能必须用「含 SKILL.md 的目录」口径**，用 `ls -d */` 会虚报 4 个（本机一度报成 35，实为 32）。
