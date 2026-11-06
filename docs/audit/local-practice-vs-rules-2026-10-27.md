# 本机「规则存在 ≠ 行为发生」执行率审计（2026-10-27）

> 触发：头条视频《Claude Code 18 个用法》→ 对照本机提示词层。
> 结论先行：**这期视频的 18 条，本机提示词层已覆盖约 80%。真正的缺口不是「规则不够」，
> 而是「规则有了，行为没发生」。**下面是可以复算的执行率证据。

## 一、口径（先钉死，避免再犯分母错）

- 语料：`~/.claude/projects/-Users-hgxszhj-x-videos-dowloads/*.jsonl`，**35 个 session 转录**（不是 session 摘要 —— 摘要会漏，见下文「三」）。
- 工具调用计数：**必须匹配 `"name":"<Tool>","input"`**。只数 `"name":"<Tool>"` 会把系统提示里
  重复的 schema 算进去，**虚高 1–2 个数量级**：
  - `"name":"Skill"` = 378 → 真实调用 **6**
  - `"name":"EnterPlanMode"` = 4351 → 真实调用 **6**
  - `"name":"Agent"` = 410 → 真实调用 **38**
- 技能总数口径：**含 `SKILL.md` 的目录**。`~/.claude/skills/` 下另有 4 个不含 SKILL.md 的目录
  （`hookify/` 插件源码仓、`learned/` 空目录、`skill-manager/` 工具仓、`workflow`→`~/.claude/intelligence` 符号链接）。
  trim 前 **119** → trim 后 **32**（+ 新增 `criteria`）+ 归档 **88**。

## 二、执行率实测（35 sessions）

| 工具 / 行为 | 真实调用 | 对应规则 | 判定 |
|---|---:|---|---|
| `Bash` | 4140 | — | 基线 |
| `Edit` | 1058 | — | 基线 |
| `Read` | 741 | — | 基线 |
| `TaskUpdate` | 402 | Rule 10 每步后检查点 | ✅ 在跑 |
| `TaskCreate` | 222 | instinct 8 任务管理 | ✅ 在跑 |
| `AskUserQuestion` | 58 | instinct 1b 自决策白名单 | ✅ 在跑 |
| `WebSearch` | 42 | — | ✅ |
| `Agent`（子代理） | **38** | Rule 5 黑名单 + instinct 2 | ⚠️ **占 Bash 的 0.9%**，memory `agent-4-layer-boundary` 已自记「调用率严重不足」 |
| `EnterPlanMode` | **6** | instinct 1a（confidence 0.85）+ Rule「Plan Mode 触发器」 | ⚠️ **定义了却几乎不触发** |
| `Skill` | **6** | 120 个技能在场 | 🔴 **装了就忘**，与 `docs/audit/skill-usage-2026-10-27.md` 结论一致 |

**推论**：本机提示词层不缺规则，缺的是**触发**。三条低执行率规则（Skill / EnterPlanMode / Agent）
恰好是「写成文档最容易、执行门槛最高」的三类 —— 需要显式决定「这次要不要用它」。

## 三、坑：拿 session 摘要当证据会得到反向结论

先数 `docs/handoff/sessions/`（37 篇 curated 摘要）+ memory 目录：

| 关键词 | 摘要+memory 命中 | 原始转录真实调用 |
|---|---:|---:|
| `AskUserQuestion` | **0** | **58** |
| `EnterPlanMode` | **0** | **6** |
| `/btw` | 0 | 395（提及） |
| `/rewind` | 0 | 746（提及） |

摘要里写的是「这轮做了什么」，不写「调用了哪些工具」→ **工具使用率在摘要里结构性不可见**。
→ 判断「某行为是否真在发生」必须查原始 JSONL，摘要只能用于「决策是否被记录」。

（这就是 `commit-gate-chain-7-gates-2026-10-22.md` 里「拿真阳性当阴性」的同一类错误：
**选错了证据源，结论方向就反了**。）

## 四、跟视频 18 条的对照结论

| 分档 | 视频内容 | 本机状态 |
|---|---|---|
| 一·第一天（6 条） | `/init` 瘦身 CLAUDE.md、plan mode、`@文件`+截图+完整报错、`Esc`/`/rewind`、选择题+SPEC、`/model`+`/effort` | ✅ 全有对应机制。**但** `EnterPlanMode` 只触发 6 次；`/init` 0 次；`SPEC.md` 0 命中且 `instincts.md` 明写「Plan 模式已死」→ **第⑤条与本机既有立场正面冲突** |
| 二·每天（6 条） | 一事一会话 + `/context` `/clear`、纠正两次就重来、**让它自己验收+贴证据**、`/btw`、`/compact`、`/rename` + `--continue`/`--resume` | ✅ `/context` `/compact` 在跑；`--continue`/`--resume`/`/rename` **在实践中用过（转录 36/29/1 次）但全局配置 0 命中** → 靠肌肉记忆，没进提示词层 |
| 三·赢过八成的人（6 条） | 重复流程写 skill、必做之事用 hook、调查交 subagent、`/code-review`、`/goal`、`-w` worktree + `-p` 无头 | hook 极强（650 次提及）、subagent 弱（38 次）、Skill 极弱（**6 次**）、`claude -p` 在用但 **worktree 0 命中** |

### 四·一、视频 #01 的删除判据，本机 rules/ 正好缺这一条

视频 #01 给 CLAUDE.md 的**判断标准**是：**「每一句問：拿掉它，Claude 會做錯嗎？」**
（旁白：*「重要的規則反而會被它忽略」* —— 规则越多，模型越容易忽略真正重要的那条。）

本机 `rules/` = 955 行 / ≈9.1k token/session，**没有这条判据** —— 现有的是**增量**纪律（13 Rules、instincts），
**没有任何一条说「什么时候该删一条」**。视频自己给的答案就在同一条幻灯片上：
**「它一直漏掉的規則 → 從 CLAUDE.md 改成 hook」**（#14）。

→ **本机 rules/ 的下一步不是加，是按 #01 逐条做减法测试**：拿掉这句，模型会做错吗？不会 → 删。
→ 这是把「Demand Elegance」从「改动 ≥2 文件时问一句」升级成**常设的 rules/ 修剪判据**。

## 五、提示词层体积实测（rules/ 是多大代价）

### 五·〇 口径修正（第一次测量的数不可信）

第一次用 `usage.input_tokens` **单值**测，两臂混进了**缓存命中/未命中**两种 run：
命中缓存的 run 报 `input=7676 / cache_read=28032`（总提示词 35708，与未命中的 ~34.7k 一致），
看起来像「上下文少了一半」—— **那不是上下文变小，是缓存命中**。

> **正确口径**：`input_tokens + cache_read_input_tokens + cache_creation_input_tokens`（总提示词 token，缓存无关）。
> 已作为 **H10** 写进 skill `criteria`。
> **推论**：凡是用「input_tokens 变小」当证据的结论，先确认不是缓存。

**对照臂**：从空目录跑 `claude -p "reply with exactly: OK" --output-format json`；
对照臂 = 真的把 `~/.claude/rules/` 整目录移走再跑（不是沙箱 `CLAUDE_CONFIG_DIR` —— 那个不被扫）。

### 五·一 实测结果（改后，重测口径）

> 三个方法论坑在填数之前先记下来（详见本节末「口径与坑」）：
> ① 回滚演练的「改前臂」曾只还原 3 个文件、漏 `testing.md` → 那一臂是中间态，不是真改前；
> ② 臂内噪声 σ≈170–780，与臂间差同量级 → 差 < ~1000 token 不能直接写成结论；
> ③ 口径必须缓存无关（H10，见 skill `criteria`）。

**同一时段（同一批脚本连跑）、同一口径（`input + cache_read + cache_creation`）的三臂**：

| 臂 | n | 实测值 | 均值 | σ |
|---|---|---|---|---|
| **真改前**（4 个文件全还原 + 逐项 marker 自检全 0） | 5 | 33303 / 34114 / 35083 / 34273 / 34113 | **34177** | 633 |
| **无 rules 基线**（真把 `~/.claude/rules/` 整个移走） | 5 | 25424 / 25562 / 26447 / 26233 / 25263 | **25786** | 522 |
| **改后**（当前生效状态） | 3 | 35037 / 34738 / 35036 | **34937** | 172 |

| 量 | 值 |
|---|---|
| **Δ rules/ 层（改前）** | 34177 − 25786 = **8391 token/session** |
| **Δ rules/ 层（改后）** | 34937 − 25786 = **9151 token/session** |
| **本轮 5 处改动的净增** | **+760 token/session**（SE ≈ 300） |

**结论要带噪声声明**：+760 看起来接近显著（t≈2.5），但**同配置重复基线之间本身就差 432**
（早先一批基线 25354 vs 同槽 25786，同一份 `rules/` 移走，配置完全相同），
而**同一个 post-edit 配置在两批基线下算出的 Δ 相差 620**（9771 vs 9151）。
→ **漂移底噪 ±620 ≈ 效应量 760**。
→ 正确说法：**本轮改动对 rules/ 体积的影响 ≈ +0.8k token/session（占 9.1k 的 8%），
量级与测量漂移底噪相当 —— 既不能说「没变」，也不能说「显著变大」**。
设计意图是 token 中性（Rule 6 同位替换 + Rule 3 措辞收窄 + 判据下沉为 skill），实际基本符合。

**另一个更硬的数**：rules/ 层现在常驻 **≈ 9.1k token/session**（12 文件 **955 行 / 33.0 KiB**，改前 924 行 / 31.0 KiB），
而 skill 侧 trim 省的只有 1.1k —— **rules/ 才是提示词层的大头，比 skill 大一个量级**。

### 口径与坑（本轮 3 个，编号接 H10）

1. **回滚演练只还原了 3 个文件** —— 漏 `testing.md`（改动 1 所在文件）。
   备份目录 `config-backup-2026-10-27-v2/rules/common/testing.md` 存的是**改后**副本（1857 B），
   真改前在更早的 `config-backup-2026-10-27/rules/common.bak/testing.md`（946 B）。
   → 那一臂测的是「改动 2/3/4/5 未应用、改动 1 已应用」的**中间态**，不是真改前。
   → **判据：回滚演练每一步都要逐文件 grep marker 自检**（真改前应全 0），不能只看「文件已换回」。
   （上表「真改前」臂已加 4 个 marker 自检，实测全 0。）
2. **臂内噪声 σ 170–780，极差可达 ~2000** —— 任何 <1000 token 的差必须先跟噪声比。
3. **口径必须缓存无关**（H10）—— 第一次用 `usage.input_tokens` 单值，命中缓存的 run 报
   `input=7676 / cache_read=28032`，看起来像「上下文少了一半」。
4. ⚠️ **我自己在同一份文档里犯了 H3**：坑 5 明明写了「`.ts-rules-backup/` 是点目录，不计入加载集」，
   **正文却一直用 991 行**（≈ 924 + 71 = 995，含了点目录）。
   重新从备份逐文件复算才拿到真值：**改前 12 文件 / 924 行 / 31.0 KiB → 改后 955 行 / 33.0 KiB（+31 行）**。
   → **教训**：写「已排除某项」的坑，和正文里用的数字，必须来自**同一次枚举**；
   否则坑写得再对，正文照样用着被排除项凑出来的分母。



- `rules/` 实际加载集 = 12 个 `.md`（`common/` 11 + `claude-code-best-practice.md`）= **955 行 / 33.0 KiB**（改前 924 行 / 31.0 KiB）；
  `.ts-rules-backup/`（3 文件 71 行）是点目录备份，不计入。
- 官方建议 `CLAUDE.md ≤ 200 行` —— 本机 CLAUDE.md **111 行 ✅**，但 **rules/ 955 行 = 官方上限的 4.8 倍**（改前 924 行 = 4.6 倍）。
  官方那条上限是给单文件 CLAUDE.md 的，rules/ 没有等价约束 → **这是一个没人管的地方**。
- 已省：`skill` 侧 trim Δ1145（见 `skill-usage-2026-10-27.md`）。两层合计 ≈ **10.5k token/session** 常驻。

### 5.1 两条冗余（rules/ 里可以直接删的）

| 条目 | 位置 | 为什么冗余 |
|---|---|---|
| Rule 6「上下文接近 80% 时**自动生成 mini 交接包草稿**写入 HANDOFF.md」 | `~/.claude/CLAUDE.md` | `PreCompact` hook `precompact-mini-handoff.sh` **已经自动做这件事**（≤200 token，写 mempalace NDJSON + 注入 post-compact 上下文）。让模型在 80% 时手工再写一遍 = 同一件事做两遍，还占常驻 token。**且**「80%」假设固定窗口，官方现在说 1M 窗口 + 自动 compaction，模型侧几乎不会「接近 80%」。 |
| `instincts.md` 「Plan 模式已死」 | `~/.claude/rules/common/instincts.md` | 与视频第⑤条（做大功能先写 SPEC）**正面冲突**，但真正的问题是：`EnterPlanMode` 35 sessions 只触发 6 次 —— 它既没死也没活。立场可以定，**执行率必须先修**（见 §五 动作 2）。 |

## 六、由此得出的动作（按 ROI）

1. **不要再往 rules 里加规则**（本轮已加的 `criteria` skill 是按需加载，不占每轮预算）。
   加规则对「执行率」几乎无帮助 —— 本机 650 次 hook 提及 vs 6 次 Skill 调用就是证据。
   **同时该做减法**：rules/ 常年驻 **≈9.1k token/session**；Rule 6 已被 PreCompact hook 覆盖 → ✅ 已删。
2. **把三条低执行率行为改成「有触发条件就必做」**：`EnterPlanMode`（架构决策时）、
   `Agent`（≥3 文件或多步降级抓取时，Rule 5 黑名单已写但没执行）、`Skill`（判据层/写作类）。
   已有 Stop hook 在查 subagent 调用率 —— **那扇门就是现成的执行率探测器**，可扩展成三门。⏳ 未做
3. ~~**会话接续写进 rules**~~ → **撤销这条**（新 Rule 6「拿掉它，Claude 会做错吗？」的第一次实际应用）：
   `--continue` / `--resume` 是**用户敲的 CLI 参数**，模型无法调用；rules/ 是注入给**模型**的上下文，
   写进去模型用不上 → 判据答案 =「不会做错」→ **不进 rules/**。
   正确去处是 `docs/handoff/` 与 `HANDOFF.md`（**用户**读的），不是常驻提示词层。⏳ 未做（改投文档）
4. **`claude -w` / `claude -p` 是两套机制，不是「并行开关」**（OCR 逐帧校对后修正）：
   视频 #18 原片两个方块分别是
   `claude -w 名字` → **独立副本 + 新分支**（`.claude/worktrees/feature-auth/` + 分支 `worktree-feature-auth`），
   `claude -p "任務"` → **放進腳本和自動化**（无头跑一次）。
   总结页也写作「✓ worktree 跟 claude -p」——**视频从没说过 `-w` 能「同时开好几个 Claude」**。
   本机现状：`claude -p` 在用（无头脚本化 ✅），**worktree 隔离 0 命中**（全局配置 + rules 都没写）。
   → 补的是「多分支并行改代码时用 `-w` 起隔离副本」，不是「并行」。

5. ⏳ **待授权（2 处全局配置）**：
   - ① `~/.claude/skills/criteria/SKILL.md` 加 **H11「做 A/B 对照实验：先问这个差超过噪声了吗」**
     （臂内噪声先于臂间差 + 回滚/还原必须逐项 marker 自检）。
     本轮实测踩了这两条（改前臂漏还原 `testing.md`；同配置两次基线差 432 token）。
   - ② `~/.claude/CLAUDE.md` 第三问的 **`H1–H9` → `H1–H10`**（1 个字符）。
     `criteria` 现已有 H10，不改 = 模型只查 9 条、漏 1 条。
   **两处都被 auto mode 分类器以「自我修改 + 用户未点名该改动」拦下** ——
   该文件不在本轮用户点名的 3 个文件里；`CLAUDE.md` 虽点名过，但这是**第 6 处改动**。
   → 需要用户**再点名一次**这两个文件才能落地。
   （注：H10 已在上一轮同一文件落地，`validate_skill.py` 五道门全过。）

## 五·二、视频原片 OCR 逐帧校对（修正我先前的 3 处说法）

抽帧 OCR 转录（#07–#18 + PART 3 + 总结页，繁体正文 / 简体字幕）逐条比对后：

| # | 原片实际内容 | 修正了什么 |
|---|---|---|
| **#09** | 标题「**讓它自己驗收**」：寫出例子 +「寫完跑測試」+ 叫它把**證據貼出來**（輸出、指令、結果） | 这就是官方第一原则「给 Claude 一个能自己验收的方法」。本机已有**最强实例**（commit-msg 7 门 + 变异测试 + trace/eval 管线 + 「能让它红」用例），**远超视频建议** |
| **#13** | 对比框原文：`CLAUDE.md ／ 每一次對話都整份讀` vs `Skill ／ 平常只放說明，用到才載入` | ⚠️ **视频的机制说法与实测不符**：skill 的 **description 是每次 session 常驻的**（只是被 Claude Code 强截断到 ~12.6 token/技能）。「用到才載入」的只有正文。「Skill 比 CLAUDE.md 便宜」成立，但「不占成本」不成立 |
| **#14** | 「**CLAUDE.md 是建議；hook 每次一定會跑**」+「它一直漏掉的規則 → 從 CLAUDE.md 改成 hook」 | 这条**直接就是本机的解法**：Rule 6 已被 `PreCompact` hook 覆盖 → 应下沉为 hook 而不是留成建议 |
| **#18** | 见上 | 修正「`-w` = 并行」的错误说法 |
| **总结页** | 「你勾得到幾個？」：一、第一天 = 6 项**全勾**；二、每天 = 前 2 项勾、后 4 项空；三、赢过八成 = **6 项全空** | 视频自己把 tier 1 定为「人人都该有」，tier 2/3 是分水岭。本机 tier 1 **机制全有**（但执行率近零），tier 2/3 **工具全在但没在用** |

## ref

- `docs/audit/skill-usage-2026-10-27.md` — 技能分档与 trim 结果
- `docs/audit/principles-2026-10-27.md` — 29 条原则 / 6 处冲突
- `memory/skill-trim-and-criteria-2026-10-27.md` — trim + 判据层
- `memory/agent-4-layer-boundary-vs-cc-2026-10-12.md` — 子代理调用率不足的既有记录
- `memory/commit-gate-chain-7-gates-2026-10-22.md` — 「拿真阳性当阴性」的同型错误
