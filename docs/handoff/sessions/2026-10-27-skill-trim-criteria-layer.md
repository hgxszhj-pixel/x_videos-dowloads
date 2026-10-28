---
session: 2026-10-27
topic: skill 侧对称 trim + 判据层落地（头条《别再写"你是资深专家"了》第 6/7 条）
status: closed-with-one-blocked-loop
commits: none
---

# skill 侧对称 trim + 判据层（2026-10-27）

## 起因

用户读头条《Claude 工程负责人：别再写"你是资深专家"了，附实测》（老詹EthanSun，2026-10-24，
基于 Felix Rieseberg 52 分钟访谈的**媒体转述**，非 Anthropic 官方规范），问：
① 这篇有什么启发？② 本地 Claude Code 提示词是否也有同类问题？③ 按建议执行。

文章 7 条旧习惯里，本地提示词**已对齐 6 条**（无任何人设指令；Rule 5「模型仅用于判断调用」
与文章第③条同构；对抗审查比文章建议更强 —— code-reviewer + 7 道 commit 门 + 变异测试 +
Stop hook subagent-rate 门 + trace/eval 管线）。真问题只有两个：**第⑥条提示词膨胀**
与**第⑦条缺「怎么判断好坏」的判据层**。

## 做了什么

### 1. skill 侧对称 trim（agent 侧 trim 的对称动作）

`mcp-agent-trim-2026-10-15` 砍了 30 个 agent；本次砍 91 个 skill。

- 30 天零触达扫描分档：Tier1=12 / Tier2=16 / Tier3=91（`docs/audit/skill-usage-2026-10-27.md`）
- 91 个移出 → 3 个从归档区回滚 → **保留 32 + 归档 88**（口径：只数含 `SKILL.md` 的目录；
  `~/.claude/skills/` 另有 4 个非技能目录 `hookify/`(插件源码仓) / `learned/`(空) / `skill-manager/`(工具仓) / `workflow`→`~/.claude/intelligence`(符号链接) 不计入）
- 归档区 `~/.claude/_archive/skills-2026-10-27/`（`~/.claude/skills` 的**兄弟**目录，
  不在扫描路径内）+ `MANIFEST.md` + `restore.sh` 一键回滚
- 实体体积 29.5 MB

**实测 Δ = 1145 token/session**（真移动前后对照，n=2）。

### 2. 判据层 skill `criteria` 落地

`~/.claude/skills/criteria/SKILL.md` —— 10 条隐含原则 H1–H10（H10 = 测 token 用总提示词 token，同日补），每条「看到 X → 做 Y」：
门禁自举 / 不可测≠零值 / 分母可复算 / 检查 staged / 环境层优先 / 解释器版本 /
双份拷贝不同质 / 上游静默重试 / 改前读全文。

推导全文：`docs/audit/principles-2026-10-27.md`（29 条原则 / 15 条全局+项目均未覆盖 /
9 条"没意识到的" / 6 处冲突 X1–X6）。

## 踩的坑（4 个，全部有牙齿）

1. **我第一次报的 token 数是错的。** 按 description 字节数估「9–14k token 每次必载」——
   实际 Claude Code 在系统提示里对 skill description **强截断**，真实成本 ~12.6 token/技能，
   91 技能合计 Δ1145。**按字节估 token = 错**，必须实测。
2. **金丝雀法测不出这个量级。** 单塞 4000 字节高熵 description 只多 ~33 token，
   噪声基线 ±1000，效应被淹没；低熵串（`QQMARKER`×600）更糟，会被分词器压缩到几乎 0。
   → 只能用「真移动前后」对照，且从空目录跑以去掉 repo/git 上下文噪声。
3. **`$CLAUDE_CONFIG_DIR/skills` 根本不被扫。** 沙箱里塞 10 个技能 token 完全不变
   （`CANARY_cfgdir` == `BASE`），只能动真实 `~/.claude/skills`。
   → 副产品结论：**Claude Code 只扫 `~/.claude/skills`，不扫 `~/.agents/skills`**
   （后者 117 个 SKILL.md 里 84 个从未暴露给模型）。
4. **相对 symlink 移位后全断。** 25 个 symlink 用 `../../.agents/skills/X` 相对目标，
   移到归档区后解析成 `~/.claude/.agents/skills/X` → 20 个断链。已全部重写为**绝对路径**
   并逐个 `[ -e ]` 复检。
5. **账本判死 ≠ 真死。** 子代理只扫全局文档，把 `code-review-canonical` / `plan-canonical` /
   `debug-canonical` 判成 Tier3，但项目自己的 `docs/dna/my-skill-coverage.md` 🟡 段明写它们是
   **「该用没用」**（该用而没 invoke）—— 归档等于删掉待用能力 → 已回滚。
   → **trim 前必须用项目级账本交叉校验**。

## 未闭合

**4 处全局规则改动待用户点名授权**（改 `~/.claude/rules/**` 与 `~/.claude/CLAUDE.md`
被 auto mode 分类器以「自我修改」拒绝，需用户在权限提示里亲自批准）。
提案已写好待审阅：`docs/audit/proposed-rules-2026-10-27.md`（每处含 before/after 全文）。
备份：`~/.claude/_archive/config-backup-2026-10-27/`。

## ref

- `docs/audit/skill-usage-2026-10-27.md` — 分档报告
- `docs/audit/principles-2026-10-27.md` — 29 条原则 + 6 处冲突
- `docs/audit/proposed-rules-2026-10-27.md` — 4 处待授权改动
- `docs/dna/my-skill-coverage.md` — 项目账本已更新（32 项 + 归档段）
- `memory/skill-trim-and-criteria-2026-10-27.md` — memory 落档
