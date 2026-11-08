# 视频内容分析：一个人指挥 50 多个 AI Agent

> 源文件：`/Users/hgxszhj/Downloads/一个人指挥 50 多个 AI Agent，干了 20 个工程师的活！.mp4`
> 57 min / 1922×1080 / 30fps / **英文旁白**（中文标题是二创者加的）
> 受访人：**Lauren Tan**（下三分之一字幕：Software engineer @ SpaceXAI；社交账号 @poteto；
> 履历 Meta → Netflix → Cursor → React core）；主持人 Colin
> 分析依据：完整 ASR 转写（1003 段 / 3424 s）+ 关键帧 OCR + 2 次外部检索

---

## 0. 先纠正标题

标题说「50 多个 AI Agent，干了 20 个工程师的活」。**视频里没有这个数字。**
真实数字是：

| 说法 | 视频原文 |
|---|---|
| 上个月合并 | **1000 个 PR**（"last month I shipped a thousand PRs, which is ridiculous"） |
| 本月（12 号） | **已落地近 800 个 PR** |
| 某天早上 | **一觉醒来 20 个 PR 已被自动合并** |
| 重构 Grokbot 的量 | **600+ 个 PR** |
| 并行 agent 数 | **本地 2–3 个** |

「50+」大概率来自 **P-Stack 插件里的 ~50–60 个 skill**，不是 agent 数量。
听众若按标题预期「50 个 agent 流水线」，会严重误判这条内容的重点 —— 重点是
**信任曲线 + 硬约束架构**，不是并发规模。

---

## 1. 主线：信任曲线（Trust Curve）

她画了一张图，纵轴是自己对 agent 的信任度，横轴是时间。三个阶段：

1. **重写循环（早期）**：一次只敢跑 1–2 个 agent，逐字读每一个输出。
   管理类比：你不信任下属，就会变成微观管理者，**吞吐被锁死**。
2. **放手但仍在环里**：本地多 agent、开始写 skill、开始用 evals。
3. **自动合并 + 在 main 上 review**：agent 自己开 PR、自己合，**人只在 main 上事后看**。

关键论断：**没有捷径**。能不能到第 3 阶段取决于**你个人对 agent 的信任**，
不是技术问题。所以「先跳到 100 个云 agent」= 白烧 token。

## 2. 第一块基石：Verification 是唯一最重要的 skill

> "the most important skill that you should have in your toolbox when you work with agents is verification"

**定义**：agent 能**真的把代码跑起来**并验证 —— 抓 CPU trace、heap snapshot、
开 iOS 模拟器、按用户真实使用路径操作应用。

**没有它会怎样（关键论断）**：
> "If you don't have a verification skill, **you are the verifier**. You're the bottleneck."

人肉验证 = 人成了瓶颈 = 无法并行 = 信任永远建立不起来。

### 2.1 `control-glass` skill（Cursor 内部代号 glass = agent window）

教 agent 用 **Chrome DevTools Protocol / Apple 模拟器工具**驱动自己的应用：
跑起来、抓 trace、拿火焰图。代码本身不难写，**难的是让 agent 知道自己面对的是什么**。

### 2.2 feature map（这个 skill 的杀手锏）

光会驱动应用不够 —— agent 不知道「左边 sidebar 卡了」对应哪个代码。
feature map 是一份文件，告诉 agent：

- 每个功能**从用户视角怎么走到**（UI 路径、点击序列）
- 所有**快捷键**
- 所有 **DOM 元素 / 用于 CDP 选择器的属性**

效果：把**低质量用户报告**（内部 Slack 里常见「截图 + ？？？」）变成可执行的复现步骤。
P-Stack 里提供 `create-verification`（自动探索代码生成初始 feature map）
和 `maintain-verification`（保持更新）两个 skill。

## 3. 第二块基石：Skills = 把 agent 训成新同事

**类比**：招了个代码写得很好但**完全不懂业务背景**的新人 → 你怎么教他？
答案是写 skill（就是 markdown，编码大量指令/信息）。

> Twitter 上有人叫它 "pull the agent to a different latent space"
> —— 给它高质量的起始 token，它会在更高的空间里做模式匹配

**P-Stack**（P = potato snack，梗自 YC CEO Garry Tan 的 G-Stack；两人同姓但无亲属关系）：
Lauren Tan 自己的 MIT 许可 Cursor 插件，**~50–60 个 skill / 23 个 playbook / 2 个 subagent**。
按需定制，不抄 G-Stack。

**建造方法（可复用）**：
纯增量，来自**观察 agent 的失败模式**：
> "Stop hallucinating, actually go and search up, look up the code.
> Use a lot of side agents and stop guessing."

每个失败模式 → 变成一个 skill。

**维护方法**：
- `eval` playbook（"potato mode" 下）—— **eval 就是 agent 的单元测试**
- 协调 agent 先写 rubric → 派一堆 subagent 去跑
- 目录名**故意起得看不出是在被评测**（agent 能察觉被测，然后改变行为）
- **换模型当 judge** 交叉复核，防止协调模型偏袒
- **爬山法**：`/loop` 反复跑同一个 eval，直到全 10/10
- 每改一次 skill 就跑一次 eval
- 强调：维护 skill **需要品味和观察力**，本质是「坐在副驾当 backseat driver」

## 4. 第三块基石：本地 → 云 → 自动合并

**先本地，再云。**
本地能**亲眼观察** agent 怎么跟应用交互（点了什么、调了哪些 API）。
没建立信任就上云 = 100 个 agent 并行烧钱且产出不可信。

**Benny（云 agent）**：把**所有**用户 bug 报告接进来，
在云端开一台自己的电脑、跑 Cursor、用**同一套 control-glass skills** 尝试复现 bug。
> 实例：Benny 复现后发现 —— **这个 bug 在 main 上已经修了**，
> 只要再发一个 Cursor build 就行。省掉人工排查一小时。

**终点**：agent 自己提 PR → 自动合并 → 人只在 main 上事后 review。

## 5. 第三根支柱：重写 vs 重构（Greenfield 才是主战场）

**反直觉结论**：
- **Brownfield（旧代码）状态其实很好** —— 尤其是配套基础设施齐全时
- **Greenfield（新项目）才是最大风险，也是最大机会**

理由：
- 大厂代码质量其实不高（"before AI slop, we had human slop"），
  但大厂建了**框架 / 约定 / 护栏 / 权限隔离** → agent 在里面表现好
- **纯 vibe code 出来的新项目 = 零护栏** → agent 每次都走捷径解 →
  代码库逐渐失控且没人理解 → "optimized for shortcuts"

**Grokbot 案例**：纯 vibe code，**人类一行都没读过** → 必须重构。
她为此花 **600+ 个 PR** 迁到新架构 Dune。重构完成后：
- 现在**不看代码**了（"I'm very excited, I don't look at the code anymore"）
- PM / 设计师 / GTM 都能直接提代码
- 不再半夜担心有人合进一个性能回归

**PR 规范**：无大小上限（50 ~ 1000+ 行都有），但**鼓励拆成多个 PR**：
- git history 是**极其丰富的上下文来源**
- 原子化 PR 便于定位 bug 和回滚

## 6. Dune 架构 —— 为 agent 而生的分层约束（干货密度最高的部分）

Dune = Grokbot 的架构代号（**「Electron 应用的 Next.js」**，专为 agent 写代码设计）。
核心思想：**把人类 review 时的口头意见，全部变成机器可执行的硬约束。**

### 6.1 三层结构

| 层 | 内容 | 例子 |
|---|---|---|
| **代码库结构** | feature / entry point / transcript card 等概念；**一个 feature 一个目录，代码全部内聚** | 80% 的工作被封装在一个目录内，agent 不用满仓库乱找 |
| **依赖阻断规则** | CI 检查**依赖图**，禁止非法 import | `electron/main` 与 `electron/renderer` 互不可 import（进程隔离差 → 主线程代码跑进渲染线程 → 掉帧） |
| **硬约束** | import CI、lint 规则、编译器诊断 | 违反 = **CI 直接失败并报错** |

### 6.2 硬约束 vs 软约束（本片最实用的一条）

- **软约束**：`rules` / `skills` / `BugBot`（Cursor 的 AI code review）/ style guide
  → agent **会忘**，可能不一致地应用
- **硬约束**：CI、lint、编译器 → **agent 想违反都违反不了**

> "if you only have rules and skills and the style guide,
> **it is only a matter of time before your code base looks like complete trash**"

> "Every time you have to [leave a human review comment], you should consider that a code smell.
> Instead of me commenting on the PR, **how do I turn this into a hard rule?**
> A lint rule? A CI failure?"

### 6.3 被禁掉的东西（示范「约束先于规模」）

- **`useEffect` 全禁**（React 最大的坑）→ CI 失败并「大声骂你」
- **代码注释全禁** —— agent 写的注释 99% 是废话历史：
  > "oh, Lauren said you should never do this" —— 那只是一句针对某个 PR 的临时意见，
  > agent 却把它固化成了代码里的永久规则

### 6.4 两条设计原则

1. **「The shortest path is the best path」**
   agent 天生爱走捷径 → 那就让捷径正好是最佳路径
   （既迁就 agent 的本性，又保证结果正确）
2. **为「最笨的 agent」设计** —— 不要让它思考，规则要外显到不用推理

### 6.5 技术栈本身也是一层约束

**Rust 又火起来了，因为编译器太严**（borrow checker 必须被满足）。
只要不让 agent 写 `unsafe`，你就得到一条极强的信任：
> "if the code compiles, it probably works and it's good"

**人不再需要亲自检查** —— 交给编译器和静态分析。

## 7. 成本 / ROI（诚实的一节）

她明确说：自己在 **AI lab 有无限 token**，**不能照搬**。
但 ROI 逻辑通用：

- 前期投入很大（重构代码库 + 加全套约束 = 烧 token）
- 但价值是：**让最天真的 agent 也能写出好代码**
- 对比项：**招一个人** vs **花 token 把代码库改造成「连 dumb agent 都能干好」**
- 她做不到的事：**独自一人建立这种强度的约束体系**（pre-agent 时代要好几年）
- 更根本的价值：**不用变成 10,000 人的工程组织**，保持组织精简

顺带宣布：**Grok 4.6**（当天发布），**每 token 成本与 4.5 相同** → 同价换更高智能。
Cursor / SpaceXAI 优化的是**成本 vs 智能的帕累托前沿**，不追求最大的模型。

## 8. 非工程师也能用（结尾）

- Cursor / Agent Window / CLI / IDE 都是**面向开发者的 power user 工具**
- **Grokbot = 非技术人员的「Cursor 时刻」**
  - 界面像 **iMessage**
  - 每个 agent 有名字，像一个「人」
  - 自然编排；**每个账号一个 agent**
  - PM 可以让 agent 总结「Lauren 昨晚干了啥」
  - PM 直接说「这是 bug，我修了」→ 上去 review 一下就 stamp
- **Dune 的严格约束是这件事成立的前提** —— 非专家也能贡献高质量代码

---

## 9. 对本机 Claude Code 的可落地清单

| # | 做法 | 本机对应 | 优先级 |
|---|---|---|---|
| 1 | **Verification skill**：让 agent 能自己跑起来验证 | 已有 `run` skill / Playwright（`e2e-runner`）/ `mcp__camoufox-reverse__*` / `mcp__js-reverse-mcp__*` 全是现成的验证工具 | **高**（本机已具备，缺的是「用起来」） |
| 2 | **feature map**：告诉 agent 每个功能怎么走到 | 本机缺。本机 `~/.claude/skills/` 有 30+ skill 但**没有索引地图** —— 可生成 `~/.claude/skills/MAP.md` 供 agent 导航 | **高** |
| 3 | **把口头规则变成硬门禁** | 本机已有 `scripts/hook_smoke.sh`（18/18）+ commit-msg 7 门 + Stop hooks —— **这正是她说的「硬约束」路线**，已经走在前面 | 已在做 |
| 4 | **eval = agent 的单元测试** | 本机 `~/.claude/scripts/validate_skill_smoke.sh`（8 变异 + 4 容忍 = 12/12）就是 skill 的 eval；**爬山上还差「skill 改完自动跑」** | **中** |
| 5 | **skill 要有「不要写注释」这类禁令** | 本机 skill 文档普遍带大量解释性注释 —— 与 Dune 相反，但注释是给人看的，**不必照搬** | 低 |
| 6 | **Rust 当硬约束层** | 本仓库就是 Rust，`cargo clippy -D warnings` + commit-time eval gate 已是等价物 | 已在做 |
| 7 | **别在没建立信任前就并行开 agent** | 直接对应本机 memory 里「子代理调用率不足」的反面：**要先有 eval 证明单 agent 可信，再谈并发** | 中 |

**最该立刻做的一件**：本机 `~/.claude/skills/` 目录已经有 30+ 个 skill，
但**没有任何一张「地图」告诉 agent 每个 skill 干什么、什么时候用** ——
这正是 feature map 的思路，且是现成的低成本收益。

## 10. 相关

- 记忆条目：`video-bilingual-subtitle-pipeline-2026-10-29.md`（本视频的双语烧录管线）
- 技能：`~/.claude/skills/bilingual-subtitle-video/`（本视频的中英双语字幕产出管线）
- 外部：`P-Stack`（github 上搜 "P-Stack cursor"，MIT 许可，Cursor 插件）
