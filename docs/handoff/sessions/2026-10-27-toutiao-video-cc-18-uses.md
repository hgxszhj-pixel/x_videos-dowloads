---
session: 2026-10-27
topic: 头条视频《Claude Code 18 个用法》→ 本机提示词层对照审计 + rules/ 体积实测
status: closed
commits: none
---

# 头条视频《Claude Code 18 个用法》(2026-10-27)

## 起因

用户给头条视频链接 `toutiao.com/video/7701280131142124060/`，问「视频的内容对我的启发？」。

## 视频内容（无字幕，走帧 OCR + 官方节目单交叉取证）

视频源 = 《AI懶人報》**EP431**（2026-10-15），讲 Claude Code 的 18 个用法，分三档。
Toutiao 无字幕 → 管线：camoufox 取 mp4 → `ffmpeg` 0.5fps → 灰度 64×36 rawvideo →
Python mean-abs-diff 去重（58 张）→ 全分辨率抽帧 → 2 个 `multimodal-looker` 并行 OCR。

**18 条（官方节目单原文，已与 KKBOX + radoxo 两处独立核对）**：

| 档 | 条目 |
|---|---|
| 一·第一天就该会（6） | ① `/init` 生成 CLAUDE.md 再删到只剩它猜不到的 ② `Shift+Tab` 切 plan mode 先计划 ③ 用 `@文件名`+截图+完整报错给它看 ④ `Esc` 停下、`Esc` 两次或 `/rewind` 退回 ⑤ 做大功能前让它用选择题先问你、写成 SPEC.md ⑥ `/model` + `/effort` 选模型与思考深度 |
| 二·每天都用得到（6） | ⑦ 一件事一个对话，`/context` 看用量、`/clear` 清空 ⑧ 纠正两次还错就重来 ⑨ 把能自己跑的检查写进要求、叫它贴出证据 ⑩ `/btw` 问小问题不占空间 ⑪ `/compact` 讲清楚要留什么 ⑫ `/rename` 取名，隔天 `--continue` / `--resume` 接回 |
| 三·赢过八成的人（6） | ⑬ 重复流程写成 skill ⑭ 一定要做的事用 hook ⑮ 调查交给 subagent ⑯ `/code-review` 换一双眼 ⑰ `/goal` 让它做到条件达成 ⑱ `claude -w` + `claude -p` 让好几个 Claude 同时工作 |

**元命题**：官方第一条实践是「**给 Claude 一个能自己验收的方法**」——这是「要一直盯着」跟「可以走开」的差别。

### 视频自己的论纲（开场两张 + 官方实践页，OCR 逐帧确认）

| 帧 | 内容 |
|---|---|
| f001/f005 | 开场对照：同一个「幫我修好登入的 bug」，**左边**只说 bug → Claude「改了 6 個地方」→「已修好」→「還是壞的」；**右边**加「改完跑測試」→ Claude 自己 `Bash(npm test)` → `✗ 2 failed` → 再改 → `✓ 14 passed` → **「測試全部通過，結果貼在上面。」** |
| f009 | 标题：**「你要一直盯著」**（红字）vs **「你可以起身去倒杯水」**（绿字） |
| f014/f018/f024 | **「大部分的建議，來自同一個限制」** —— 进度条 `context 0% → 92% → 94%`；说明「越滿，越容易忘記前面講過的話、犯更多錯」（"performance degrades as it fills"）；绿框补注：**「官方放在第一條：給 Claude 一個自己驗收的方法」**（"the difference between a session you watch and one you walk away from"） |
| f026 | 路线图：PART 1 一、第一天就該會（6 蓝） / PART 2 二、每天都用得到（6 红） / PART 3 三、贏過八成的人（6 灰）；**「每一個用法講兩件事：怎麼打・什麼時候用」** |
| f127 | PART 2 副标：**「管好 context，讓它自己驗收」** |

### tier 1 逐帧 OCR 要点（#01–#06）

| # | 标题 | 卡片原文（要点） |
|---|---|---|
| #01 | 寫好 CLAUDE.md | 怎麼打：`/init` 讓它看過程式寫草稿；**判斷：每一句問「拿掉它，Claude 會做錯嗎？」**；要留：它猜不到的指令、規定、環境的坑。旁白：**「重要的規則反而會被它忽略」** |
| #02 | Shift+Tab 先計畫 | plan mode **只讀不改**；Ctrl+G 編輯計畫；什麼時候用：一句話講不完、要改好幾個檔案的時候 |
| #03 | 給它看，不要描述 | `@檔名` + 貼截圖 + 整段錯誤 +「修根本原因，確認 build 會過」；叫它自己去拿（給網址、GitHub 用 `gh`）。旁白：**「服務溝通最省 context 的方法」** |
| #04 | Esc 跟 rewind | `Esc` 停下（對話都還在）；`Esc` 兩下或 `/rewind` 退回**任何一步**，4 選項：① 還原對話和程式碼 ② 只還原對話 ③ 只還原程式碼 ④ 從這裡開始摘要 |
| #05 | 讓它先問你 | 「仔細問我問題，最後寫進 **SPEC.md**」+ 選擇題面板；然後**開一個新的對話**照 SPEC.md 做 |
| #06 | 選模型跟 effort | `/model` 換模型、`/effort` 調思考深度（low…max）；**簡單的用低 effort**；大部分寫程式用 Sonnet、架構決定用 Opus |

### 抽帧 OCR 逐帧校对（#07–#18 + PART 3 + 总结页，繁体正文 / 简体字幕）

OCR 校对后**修正了我先前的 3 处说法**：

| # | 原片实际内容 | 修正 |
|---|---|---|
| #09 | 标题「**讓它自己驗收**」：寫出例子 +「寫完跑測試」+ 叫它把**證據貼出來**（輸出、指令、結果） | = 官方第一原则。本机已有最强实例（commit-msg 7 门 + 变异 + trace/eval + 「能让它红」用例） |
| #13 | 对比框：`CLAUDE.md ／ 每一次對話都整份讀` vs `Skill ／ 平常只放說明，用到才載入` | ⚠️ **机制说法与实测不符**：skill **description 每次 session 常驻**（只是被强截断到 ~12.6 token/技能）。「Skill 比 CLAUDE.md 便宜」✅，「不占成本」❌ |
| #14 | 「**CLAUDE.md 是建議；hook 每次一定會跑**」+「它一直漏掉的規則 → 從 CLAUDE.md 改成 hook」 | = 本机解法本身。Rule 6 应下沉为 hook（已被 PreCompact hook 覆盖） |
| #18 | `claude -w 名字` → **独立副本 + 新分支**；`claude -p "任務"` → **放進腳本和自動化** | ❌ 我先前说「视频说 `-w` 能并行」——**视频从没这么说**。总结页写「✓ worktree 跟 claude -p」 |
| 总结页 | 「你勾得到幾個？」：tier 1 = 6 项**全勾**；tier 2 = 前 2 勾后 4 空；tier 3 = **6 项全空** | 视频自己把 tier 1 定为「人人都该有」，tier 2/3 是分水岭 |

其余逐帧可读项（供引用）：#07 `/context` 面板「■ 系統與工具 12% ■ CLAUDE.md 4%」；#08 「/clear，寫一個更具體的開頭」；#10 `/btw`；#11 「CLAUDE.md 先寫好：壓縮時要保留什麼」；#12 `/rename oauth-migration` + `claude --continue`；#15 「讀過的每個檔案都會塞進主對話」+ `.claude/agents/` 自定义审查员 frontmatter；#16 「全新的 subagent 只看 diff」+「只回報影響正確性的問題」（审查看单分 正確性×2 / 風格×2）；#17 `/goal 全部測試通過，而且沒有型別錯誤`（循环：還沒達成→繼續 / Claude 做一輪 / 另一個模型檢查）。

**不可读帧（不猜）**：f151（#09 淡入，正文被洗白）、f264（交叉淡化，近空白）、f146/f182/f233/f288（局部遮挡或淡出）。
**画面系统**：投影片正文繁体、顶部导航 + 底部旁白字幕简体；总结页 tier 1 导航格数（5）与清单项数（6）不一致，帧内无法判定原因，未推测。

## 本机对照结论

详见 `docs/audit/local-practice-vs-rules-2026-10-27.md`。摘要：

- **18 条里本机提示词层已覆盖 ~80%**；缺口不是规则，是**执行率**。
- 35 个 session 转录实测（口径：必须匹配 `"name":"<Tool>","input"`）：
  `Bash` 4140 / `Edit` 1058 / `Read` 741 / `TaskUpdate` 402 / `AskUserQuestion` 58 /
  `Agent` **38** / `EnterPlanMode` **6** / `Skill` **6**。
- **`rules/` 层体积实测 Δ ≈ 9.1k token/session**（同槽三臂，改前 8391 / 改后 9151 / 无 rules 基线 25786，
  口径 = 总提示词 token）。本轮 5 处改动净增 **+760 ± 漂移底噪 620** —— 与测量漂移同量级，
  既不能称「没变」也不能称「显著变大」。
- 两处冗余：Rule 6 已被 `PreCompact` hook 覆盖；`claude -w` 在 2.1.311 = `--worktree`，不是并行。
- 本机 rules **不含**任何官方已废弃建议模式（IMPORTANT / ultrathink / think hard / 偷懒 / 上下文窗口管理 全 0 命中）。

## 踩的坑

1. **场景阈值过冲**：按帧差 0.015 检出 367 帧 → 换 0.5fps + 灰度差 → 58 张有效帧。
2. **只匹配 `"name":"<Tool>"` 会虚高 1–2 个数量级**（Skill 378→6 / EnterPlanMode 4351→6 / Agent 410→38）。
3. **session 摘要里工具使用率结构性不可见**：摘要给出 AskUserQuestion=0 / EnterPlanMode=0，原始转录是 58 / 6。
   → 「拿真阳性当阴性」的同型错误，判行为是否发生必须查原始 JSONL。
4. **`claude doctor prompt-audit` 不是 CLI 子命令**，会忽略参数直接跑通用 doctor。
5. **`.ts-rules-backup/` 是点目录**，不计入 rules/ 实际加载集（12 文件 / **924 行**，不是 15 / 1006）。

## 未闭合

- ✅ 已闭合：3 处全局规则改动已获用户点名授权并应用（见下节）。
- OCR 逐帧转录仅作为交叉验证，权威内容以官方节目单为准。

## 本视频直接催生的规则改动（已应用）

用户「按照你建议执行」→ 在权限提示里逐个点名 `~/.claude/CLAUDE.md` / `rules/common/git-workflow.md` /
`rules/common/coding-style.md` → **5 处改动全部落地**（备份 `~/.claude/_archive/config-backup-2026-10-27-v2/`）：

| 改动 | 内容 | 来源 |
|---|---|---|
| ① `testing.md` | 新增「测试有效性判据」（H1 能让门红） | 判据层 |
| ② `git-workflow.md` | commit 模板加 `Intent: <一句话>`（消除「全局模板缺 Intent → 项目 commit-msg 拒绝」活摩擦）+ 追加「Pre-commit 检查对象 = staged」 | 判据层 H4 |
| ③ `coding-style.md` | Immutability 收窄为「externally exposed 才不可变」+ Error Handling 加「error type 保持可 downcast」+ 顺手修 checklist 里同源的绝对禁令 | 判据层 X5/D6 |
| ④ `CLAUDE.md` | Rule 3 措辞收窄（解 X1）+ 质量自检加**第三问**（写完成前 invoke `criteria` 对照 H1–H10，命中即修） | 判据层 |
| ⑤ `CLAUDE.md` Rule 6 **同位替换** | 旧「上下文 80% 自动写 mini 交接包」（已被 `PreCompact` hook 覆盖）→ **「提示词层是减法」**：加规则前先问「拿掉它，Claude 会做错吗？」+「反复漏掉的规则 → 挪进 hook」 | **本视频 #01 + #14** |

**13 Rules 条数不变**（1–13 实测在位）—— 同位替换，不是新增第 14 条。
**顺带**：skill `criteria` 加 **H10**（测 token 成本要用总提示词 token，见下节）。

### 权限坑

即使用户已点名文件，**第一次 Edit 仍可能被 auto mode 分类器误拒**，理由自相矛盾
（`the user authorized the specific file … so this action should be allowed`）。
→ **原样重试一次即通过**，不必换工具绕过（重试走的是同一授权路径，不是规避）。

### 测量口径修正（本次新增 H10）

第一次测 `rules/` 体积用 `usage.input_tokens` 单值 → 两臂混了**缓存命中/未命中**，
命中缓存的 run 报 `input=7676 / cache_read=28032`（总提示词 35708，与未命中的 ~34.7k 一致），
看起来像「上下文少了一半」。**那不是上下文变小，是缓存命中。**
正确口径 = `input_tokens + cache_read_input_tokens + cache_creation_input_tokens`。

### 测量坑 2：回滚演练的「改前臂」只回滚了 3 个文件

回滚演练脚本把 `CLAUDE.md` / `git-workflow.md` / `coding-style.md` 换回改前版本，
**漏了 `testing.md`**（改动 1 所在文件）—— 备份目录里存的是**改后**副本，
真正的改前副本在更早的 `config-backup-2026-10-27/rules/common.bak/testing.md`（946 B vs 改后 1857 B）。
→ 那一臂测的是「改动 2/3/4/5 未应用、**改动 1 已应用**」的中间态，不是真改前。
→ **判据**：回滚演练的每一步都必须**逐文件校验 marker**，不能只看「文件已换回」——
脚本里已加 4 个 marker 的 grep 自检（真改前应全为 0）。

### 测量坑 3：臂内噪声 σ 170–780，且**同配置重复基线就差 432**

同臂 5 次 run 的极差可达 ~2000 token（臂内 σ 170–780）。
更硬的一条：**同一份 `rules/` 移走、配置完全相同**的两批基线，均值差 **432**
（早先 25354 vs 同槽 25786）；**同一个 post-edit 配置**在两批基线下算出的 Δ 差 **620**（9771 vs 9151）。
→ 任何「改前 vs 改后」的差只要 **< ~1000 token**，都必须先跟噪声比，
不能直接写成结论（否则就是把噪声讲成信号）。
→ **两臂必须同一时段连跑**，否则跨时段漂移会盖过效应。

## ref

- `docs/audit/local-practice-vs-rules-2026-10-27.md` — 执行率 + 体积审计（主报告）
- `docs/audit/skill-usage-2026-10-27.md` — skill 分档与 trim
- `docs/handoff/sessions/2026-10-27-skill-trim-criteria-layer.md` — 同日上一轮
