# 原则清单（三步提炼）2026-10-27

> 方法：Felix Rieseberg 第 7 条「别只把 AI 当传送带，让它学你的标准」的三步法。
> 第 1 步汇总真实反馈 → 第 2 步浓缩成可判好坏的判据 → 第 3 步与现有规则基线反向对照，找出「一直在遵守但没意识到」的。
> **本报告只读分析，未修改任何规则 / hook / 技能。**

---

## 0. 方法与证据口径

### 0.1 实际读了什么

| 源 | 位置 | 规模 | 读法 |
|---|---|---|---|
| ① memory 文件 | `~/.claude/projects/-Users-hgxszhj-x-videos-dowloads/memory/*.md` | **71 个**（任务描述说 73，实测 71；`ls *.md`=72 含索引） | 抽 frontmatter `description` 全文 + 关键词概念计数 |
| ① memory 索引 | 同目录 `MEMORY.md` | 99 行 | 全量对比文件清单 |
| ② DNA 文档 | `docs/dna/*.md` | 9 个 | 全文读 |
| ③ commit 门禁 | `scripts/git_hooks/commit-msg` + `docs/dna/commit-gates.md` | 7 道门 | 读编排顺序 + 每道门的判据 |
| ③ 冒烟测试 | `scripts/hook_smoke.sh` | 299 行 / 43 用例 | 读 fixture 与 runner 结构 |
| ④ 变异测试 | `scripts/mutation_smoke.sh` | 88 行 | 全文读（smoke / manual 两种模式 + 退出码语义） |
| ⑤ 用户反馈 | `~/.claude/projects/**/*.jsonl` | 87 个文件 / **860 条真实人类消息** | 先 grep 缩到候选行，再精读少量行（未整读 390MB） |
| ⑥ 写作规范 | — | **未找到原始文件** | 只在 session transcript 里被描述 |

### 0.2 反向对照的 grep 口径（第 3 步的硬证据）

```bash
G=("$HOME/.claude/CLAUDE.md" "$HOME/.claude/rules/claude-code-best-practice.md")
for f in "$HOME"/.claude/rules/common/*.md; do G+=("$f"); done      # 13 个全局基线文件
D=(/Users/hgxszhj/x_videos-dowloads/docs/dna/*.md)                 # 9 个项目 DNA 文件
grep -rniE "<pattern>" "${G[@]}"   # GLOBAL：全局规则基线
grep -rniE "<pattern>" "${D[@]}"   # DNA：项目级已编码规则
```

- **GLOBAL=0** = 全局规则里查不到对应条文 → 判「未 codified」。
- 关键词会误命中（如 `mutation` 命中 `coding-style.md:48` 的 "No mutation (immutable patterns used)"）。**每个数字都人工判读过首行命中**，误报已在 §3 注明。
- 「出现次数」= 命中该概念的 **memory 文件数**（关键词文件级计数，泛化关键词会偏高，报告中已标注）。

### 0.3 已知缺口（影响可信度）

1. **写作规范（源 ⑥）原始证据链断在本报告之外**。全盘 grep `引用.{0,10}口径|其他博主` 在 `~/memory`、`~/.claude`、`x_videos-dowloads` 均 **0 命中**；只在 `~/.claude/projects/-Users-hgxszhj-x-videos-dowloads/fce1eae8-1cdc-427c-b3b1-07a5d479cbde.jsonl` 里被**转述**为「从审稿意见提炼」。该条按任务描述采信，未独立验证。
2. **memory 索引与实际文件不一致**：5 个文件不在 `MEMORY.md` 索引里（`gui_dioxus-runtime-crash`（索引里是 `gui_dioxus-runtime-context-crash`，疑似改名未同步）、`orca-video-gpu-xfade-path`、`eval-gate-commit-time-2026-10-19`、`wuzuapi-baseurl-no-double-slash`、`understand-anything-architecture`）；另有 4 个文件**没有 frontmatter description**（`async-spawn-trace-bisect`、`downloadresult-success-check`、`ultragoal-methodology-loop`、`wupai-five-schools-dna-adoption-2026-09-13`）。
3. **源 ⑤ 信号密度极低**：860 条人类消息里「纠错型」关键词命中仅 **5 条**（且 4 条为真实纠正）。**结论：本机的主要反馈通道不是「用户事后纠正」，而是「用户把标准写进任务提示」**（10 条含 `必须/不要/以后/记住/每次` 的显式标准消息，如「以后访问网页的规则顺序 WebFetch-moli-ego-lite-agent-browser-camoufox-chrome」→ 已落成 `instincts.md` 第 12 条）。这改变了第 3 步的取证方式：行为证据主要来自 memory + 门禁 + DNA。
4. 未审 `~/.claude/rules/.ts-rules-backup/`（备份目录）与 `~/.claude/skills/*/SKILL.md` 正文（本次只审规则基线）。

---

## 1. 原则清单（分类）

**判据写法约定**：每条写成「看到 X → 就该做 Y」，可机械执行，不含「要严谨」这类空话。

### A. 产物质量判据（怎么判断产物好不好）

| # | 原则（可执行判据） | 证据来源 | 出现次数 | 已 codified? |
|---|---|---|---|---|
| A1 | 改 1–2 行期望行为后测试仍全绿 ⇒ **测试在说谎**，不算通过；必须有变异验证证据 | `mutation-smoke-trace-three-piece`、`skill-acceptance-gate-r2s-2026-10-22`、`commit-gate-chain-7-gates-2026-10-22`、`wupai-five-schools-dna-adoption` | 9 文件 | **项目 DNA ✅**（review-checklist Phase 3）/ **全局 ❌**（GLOBAL=0） |
| A2 | 声称「已修复/已完成」时必须附**可复现命令 + 实际输出**，不能只给描述 | 用户纠正「怎么还是没有修复问题？」（2026-10-16）、`eval-gate-commit-time-2026-10-19`、`commit-gate-chain-7-gates` | 门禁 7 道 | 全局**部分 ✅**（Rule 12 失败要响亮 + instincts 3 完成前验证 ⇒ 有「要验证」，无「证据必须贴出来」） |
| A3 | 指标字段**不存在 ≠ 值为 0**；字段缺失时必须显式输出「⚪ 不可测」并披露，绝不显示「✅ 持平」 | `retrieval-cost-audit-2026-10-22`（`total_ms_proxy` 从没被写过却显示持平）、`expect-vs-allow-dead-code`（unfulfilled 永久误报） | 1 文件（概念扩散到 5 文件的「未验证」案例） | ❌ **GLOBAL=0 / DNA=0** |
| A4 | 产物的验收标准取自**下游真实消费端**，不是「看起来对」 | 用户纠正「格式不对，无法上传到今日头条」（2026-10-16）、`toutiao-daily-publish-2026-10-16`；写作规范二条是此原则的写作版 | 2 处 | ❌ GLOBAL=0 |
| A5 | 结论里的每个数字**分母必须可复算**；扫描类结论要写明扫描根与排除规则，分母变了必须重述结论 | `skill-acceptance-gate-r2s-2026-10-22`（首扫单根漏 2 技能，「117」分母本身错）、`retrieval-cost-audit`（10.3:1 归因）、`docs/audit/skill-usage-2026-10-27.md`（口径说明段） | 2 文件 | ❌ **GLOBAL=0 / DNA=0** |
| A6 | 启发式判断必须走「声明 ↔ 使用」**多路核对**（use / cfg gate / re-export / 公开 API），否则 false alarm 反复 | `audit-heuristic-blind-spots`（4 路，2 次同型失误） | 1 文件 | ❌ GLOBAL=0 |
| A7 | 审计 / 扫描结论必须**逐条区分真阳性与误报**，且误报类要逐条验过「能红」才上线 | `skill-acceptance-gate-r2s-2026-10-22`（12/12 = 8 变异 + 4 容忍）、`audit-heuristic-blind-spots` | 8 文件（误报类） | ❌ **GLOBAL=0 / DNA=0** |
| A8 | 凡是要读者核对的数字 / 口径，必须出现在**它被使用的那一处**；不引入外部署名为权威背书 | 写作规范（引用口径写在引用处 / 正文不点其他博主名字） | 源 ⑥（原始文件缺失） | ❌ 未 codified（§0.3 缺口 1） |

### B. 变更纪律

| # | 原则（可执行判据） | 证据来源 | 出现次数 | 已 codified? |
|---|---|---|---|---|
| B1 | 删最后一个 caller 时必须连带检查**公开出口**（`pub use` 重导出 / API 签名），否则留下悬空导出 | `docs/dna/edit-preflight.md` Rule 2、`expect-vs-allow-dead-code` | DNA=8 命中 | **项目 DNA ✅** / 全局 ❌ |
| B2 | 改**该领域**前先读血泪教训全文（memory 逐条），不是只读索引 | `docs/dna/三档地盘.md` 🔴 档、`docs/dna/新功能前必查.md` ③、`audit-heuristic-blind-spots` | DNA=42 命中 | **项目 DNA ✅** / **全局 ❌ GLOBAL=0** |
| B3 | 写新功能第一行代码前，跑完 30 秒 3 项 grep（skill / 项目内模块 / 教训） | `docs/dna/新功能前必查.md` | DNA=6 | **项目 DNA ✅** / 全局 ❌ |
| B4 | 改动 ≥2 文件先停 30 秒问「有更优雅的方式吗」；是 hacky fix 就重写为已知全貌下的解 | `~/.claude/CLAUDE.md` 执行哲学 Demand Elegance | 规则基线 | 全局 ✅（但与 Rule 3 冲突，见 §3.1） |
| B5 | 任何新增门禁 / 校验必须写明**逃逸阀**，并声明默认是 fail-open 还是 fail-closed | `scripts/hook_smoke.sh:279`（`must FAIL OPEN when its dependency is gone`）、`web-fallback-rule-2026-10-07`（should-try 曾 fail-open 静默放行） | 2 文件 | ❌ GLOBAL=0 |

### C. 验证与证据

| # | 原则（可执行判据） | 证据来源 | 出现次数 | 已 codified? |
|---|---|---|---|---|
| C1 | 门禁 / 判据脚本自身必须**先证明它能红**（写一条让它失败的用例并实跑确认会红），否则是假绿灯 | `commit-gate-chain-7-gates-2026-10-22`（测试脚本漏 `$` → 整段空转仍 rc=0）、`skill-acceptance-gate-r2s-2026-10-22`（8 变异） | 9 文件 | ❌ **GLOBAL=0**（DNA=8，仅项目） |
| C2 | 改动 hook / 门禁后必须**沙箱验证 + 反向逐字节校验**再上线 | `web-fallback-rule-2026-10-07`（接线脚本先沙箱验证 + 反向逐字节校验） | 2 文件 | ❌ GLOBAL=0 / DNA=0 |
| C3 | 检查对象是**将要提交的 staged 内容**，不是工作区当前状态 | `handoff-size-guard-hook`（检查 staged version 非 working tree）、`commit-gate-chain`（eval_gate 检查 staged 含 `*.rs`） | 8 文件 | ❌ GLOBAL=0（DNA=1，零星） |
| C4 | 改核心逻辑必跑**回归对比 baseline**，不改也要跑 | `~/.claude/CLAUDE.md` 质量自检双问、`docs/dna/edit-preflight.md` Rule 4 | GLOBAL=1 / DNA=36 | 全局 ✅ + 项目 ✅ |
| C5 | 变更完成 = 确定性验收门全过 + **证据附在结论旁**（不是「我跑过了」） | `eval-gate-commit-time-2026-10-19`（Harness² 本地化）、`commit-gate-chain` | 门禁 5–7 道 | 部分 ✅（instincts 3 有「完成前验证」，无「证据形态」判据） |

### D. 工具与环境陷阱

| # | 原则（可执行判据） | 证据来源 | 出现次数 | 已 codified? |
|---|---|---|---|---|
| D1 | 脚本 / hook 失败时，先确认真实解释器版本与 **PATH 实际解析**（`which` / `--version` / `type -a`），再怀疑代码 | `commit-msg-hook-python-310-path`（`/usr/bin/python3`=3.9.6 拒 `Path \| None`）、`uv-pyenv-path-shadowing`（uv 二进制被 pyenv shim 挡） | 27 文件（泛化关键词，偏大） | ❌ **GLOBAL=0 / DNA=0** |
| D2 | clone / fetch 前先查 `git config --global --list`：全局 `url.*.insteadOf` 会**静默改写所有 URL** | `ghfast-top-git-rewrite-trap`（劫持所有 github fetch，SSL 失败）、`moli-install-network-blocked`（proxy 7 次失败）、`wuzuapi-baseurl-no-double-slash` | 1 文件（+2 邻近） | ❌ GLOBAL=0 / DNA=0 |
| D3 | 双份拷贝改一处后必须**同步第二份 + 跑同步脚本 + 验证**；且两份关系可能是「故意变体 / 真重复 / 结构不同」，不能一刀切去重 | `wpr-dual-copy-and-agents-md`、`trace-link-commit-injection`（`install_git_hooks.sh`）、`claude-skills-symlink-pattern`、`skill-acceptance-gate-r2s`（gitnexus/remotion/slidev 三种关系） | 15 文件 | **项目 DNA ✅**（`trace-link.md`，仅限 hook）/ 全局 ❌ GLOBAL=0 |
| D4 | 装 skill / 插件前先 `ls -l` 查重 + 看是否 symlink；`cp` / `rsync` 会**穿透链接覆写目标** | `claude-skills-symlink-pattern`（约 20 个 symlink） | 1 文件 | ❌ GLOBAL=0 |
| D5 | 上游报错不能靠**静默重试**掩盖：设 `maxRetries:0` + 短超时，并用 `curl` 隔离「真失败 vs 假失败」 | `opencode-wuzuapi-upstream-silent-retry`（308s→41.6s）、`darwin-optimize-web-protocol-recovery`（非触发静默降级） | 3 文件 | ❌ GLOBAL=0 / DNA=0 |
| D6 | 错误类型必须**保留可 downcast 的具体类型**；禁止用 `anyhow!()` / 字符串化把 typed error 降级 | `anyhow-from-e-strips-types`（`downcast_ref` 永远返 None） | 6 文件 | ❌ GLOBAL=0 / DNA=0 |
| D7 | 「能列出目录」≠「这个通道能用」：先验证**能力清单**（模型 catalog / API 路径 / 平台支持），再接线 | `wuzuapi-anthropic-channel-does-not-exist`（0 个 Claude 模型）、`claude-plugins-official-marketplace-lag`（SHA pin 滞后却报 "already at latest"）、`wechat-macos-reverse-blocked`（README 假阳性） | 3 文件 | ❌ GLOBAL=0 |
| D8 | 抓取 / 网络 / 下载类路径必须设**硬超时 + stall 检测**，不能只靠手动 Cancel | `single-url-bug-e-v3`（stall_detector + 120s 硬超时）、`bug-d-isolation-and-cookies-zero`（20s timeout wrap） | 2 文件 | 全局 ❌ / **项目 ✅**（`trace-schema.md` 有 `stall_count` 指标） |

### E. 流程与协作

| # | 原则（可执行判据） | 证据来源 | 出现次数 | 已 codified? |
|---|---|---|---|---|
| E1 | 凭据一旦泄露**当场 rotate**，不等会话结束 | `horizon-news-radar-install-2026-10-16`（sk-cp- token 本 session 泄露 3 次）、`toutiao-daily-publish-2026-10-16` | 3 文件 | ✅ **全局**（`security.md:20` "Rotate any secrets that may have been exposed"） |
| E2 | 标准要**写进提示 / 门禁**，不能靠人记忆；用户口头说过的顺序要立刻落成可执行条文 | 用户「以后访问网页的规则顺序 …」→ `instincts.md` 12；commit-msg 7 门 | 10 条用户标准消息 + 7 门 | ✅ 全局（instincts）+ 项目 |
| E3 | 每个 commit 带 `Intent:` + `Refs:`，可追溯到具体 DNA / memory 条目 | `intent_lock`、`trace-link-commit-injection`、`review-checklist.md` Phase 5 | 门禁 1/6 道 | **项目 ✅** / **全局 ❌**（`git-workflow.md` 模板无 Intent 行，见 §3.2） |
| E4 | 文档 / 记忆有**体积上限**，超限先归档再写（防无界膨胀） | `handoff-size-guard-hook`（>200 行拒 commit，>100 行警告）、`handoff-sessions-quarterly-archive`、`CLAUDE.md`（memory >300 行须归档） | 3 处 | ✅ 全局（memory 300 行）+ 项目 ✅（HANDOFF 200 行） |
| E5 | 保留**过程证据**（trace）而不只留结论：BUG 复发定位时间从 1h → 5min | `local-ndjson-trace-instrumentation`、`trace-eval-pipeline-4-metrics`、`on-exit-raii-real-outcome-pattern` | 3 文件 | 项目 ✅ / 全局 ❌ |
| E6 | 索引 / 清单类产物必须**能被脚本校验**（分母对得上、条目对得上） | `MEMORY.md` 索引漏 5 个文件（§0.3 缺口 2）、`skill-acceptance-gate-r2s`（分母错） | 2 处 | ❌ GLOBAL=0 |

> 合计 **29 条**：A=8 / B=5 / C=5 / D=8 / E=6（其中 3 条已在 §3 列为「已覆盖、无需新增」，此处为完整性保留）。
> **GLOBAL 覆盖 6 条**（A2 部分、B4、C4、E1、E2、E4）、**项目 DNA 覆盖 8 条**、**全局 + 项目均未覆盖 15 条**。

---

## 2. 第 3 步：我一直在遵守但没意识到的

判定口径：**行为证据 ≥2 处** + **GLOBAL=0**（全局规则里查不到任何对应条文）。以下 **9 条**（要求 ≥5，已达标）。

| # | 隐含原则 | 行为证据（≥2 处） | 规则文件反向核对（grep + 结果） | 建议落在哪个文件 |
|---|---|---|---|---|
| H1 | **门禁 / 测试必须先证明自己能红**（自举验证） | ① `commit-gate-chain-7-gates-2026-10-22`：「测试脚本自身假绿灯」——漏 `$` → 整段空转仍 rc=0 ② `skill-acceptance-gate-r2s-2026-10-22`：8 个变异证明门能红 + 4 个容忍逐条验过能红 ③ `mutation-smoke-trace-three-piece`：3 变异 3 KILLED | `grep -rniE "变异测试\|mutation[_ ]smoke\|反向用例\|能红\|假绿灯\|测试在说谎" "${G[@]}"` → **GLOBAL=0**；`"${D[@]}"` → DNA=8（仅项目级） | `~/.claude/rules/common/testing.md`（新建「测试有效性判据」小节）+ `instincts.md` 一条 instinct |
| H2 | **不可测 ≠ 零值**：字段缺失必须显式标注，不得显示为「持平 / ✅」 | ① `retrieval-cost-audit-2026-10-22`：`total_ms_proxy` 在 src 里从没被写过，恒为 0 却显示「✅持平」→ 改 `⚪ 不可测` + 披露 ② `expect-vs-allow-dead-code`：`#[expect]` 在 test 构建里永久 unfulfilled 误报 ③ `web-fallback-rule-2026-10-07`：拿不到就声明「拿不到」，不许包装成「内容分析」 | `grep -rniE "不可测\|零值\|恒为 ?0\|假持平" "${G[@]}"` → **GLOBAL=0**；`"${D[@]}"` → **DNA=0** | `~/.claude/rules/common/instincts.md`（新增 instinct「完成声明必须附证据，缺字段标不可测」） |
| H3 | **统计口径 / 分母必须可复算** | ① `skill-acceptance-gate-r2s-2026-10-22`：首扫只覆盖单根 → 漏 2 个技能，「117 这个分母是错的」 ② `retrieval-cost-audit-2026-10-22`：input:output=10.3:1 归因 ③ `docs/audit/skill-usage-2026-10-27.md`：专设「口径说明 / 被排除的信号」章，逐行核验排除同名误判 | `grep -rniE "分母\|口径\|全量枚举\|漏扫" "${G[@]}"` → **GLOBAL=0**；`"${D[@]}"` → **DNA=0** | `~/.claude/rules/common/instincts.md`（新增 instinct「给统计 / 扫描结论前先确认分母可复算」） |
| H4 | **检查对象是 staged（将要提交的内容），不是工作区** | ① `handoff-size-guard-hook`：size guard 检查 **staged version** 而非 working tree ② `commit-gate-chain`：eval_gate 按 staged 含 `*.rs` 判定是否要跑 fmt/clippy ③ `skillstate-handoff-skill-md`：open_loops schema 校验读暂存区 | `grep -rniE "staged\|暂存区" "${G[@]}"` → **GLOBAL=0**；`"${D[@]}"` → DNA=1（`commit-gates.md` 提到 staged 但非通用条文） | `~/.claude/rules/common/git-workflow.md`（Pre-commit 检查小节） |
| H5 | **全局 git config 会静默改写 URL**，操作前先查 | ① `ghfast-top-git-rewrite-trap`：`url.https://ghfast.top/.insteadof https://github.com/` 劫持全部 github fetch，本机 SSL 失败 ② `moli-install-network-blocked-2026-10-07`：proxy 7 次 GitHub 下载全失败 ③ `wuzuapi-baseurl-no-double-slash`：baseURL 斜杠 / host 配错被误判成权限问题 | `grep -rniE "insteadOf\|git config --global\|url\..*rewrite" "${G[@]}"` → **GLOBAL=0**；`"${D[@]}"` → **DNA=0** | `~/.claude/rules/common/instincts.md`（新增 instinct「环境层先于代码层：先查全局 config / 镜像 / 代理」） |
| H6 | **脚本 / hook 的解释器版本与 PATH 真实解析必须先确认** | ① `commit-msg-hook-python-310-path`：`/usr/bin/python3`=3.9.6 拒 `Path \| None` → 整个 hook 崩、commit 被拒 ② `uv-pyenv-path-shadowing`：uv 装的二进制被 pyenv shim 挡 ③ `vimrc-shell-metachars`：注释里的反引号被 zsh 当命令替换 | `grep -rniE "/usr/bin/python3\|python3 ?版本\|shim\|pyenv" "${G[@]}"` → **GLOBAL=0**；`"${D[@]}"` → **DNA=0** | 同 H5，合并进同一条「环境层先于代码层」instinct |
| H7 | **同名 / 双份拷贝关系可能不同，不可一刀切去重** | ① `skill-acceptance-gate-r2s-2026-10-22`：gitnexus=故意工具变体 / remotion=真重复 / slidev=结构不同的两形态（坏得不一样） ② `wpr-dual-copy-and-agents-md`：权威版 vs 装机版 ③ `claude-skills-symlink-pattern`：symlink 拷贝 cp 会穿透覆写 | `grep -rniE "双拷贝\|两份\|去重\|副本同步" "${G[@]}"` → **GLOBAL=0**；`"${D[@]}"` → DNA=3（仅 `trace-link.md` 的 hook 同步） | `~/.claude/rules/common/instincts.md`（并入「双份拷贝」子条）或 `skills` 相关说明 |
| H8 | **上游静默重试掩盖真实错误** → `maxRetries:0` + 短超时 + `curl` 隔离 | ① `opencode-wuzuapi-upstream-silent-retry`：SDK/TUI 静默重试 5 分钟 → 308s 降到 41.6s ② `darwin-optimize-web-protocol-recovery`：非触发的静默降级 ③ `wuzuapi-anthropic-channel-does-not-exist`：`/v1/messages` 永远 502 | `grep -rniE "静默重试\|maxRetries\|静默降级" "${G[@]}"` → **GLOBAL=0**；`"${D[@]}"` → **DNA=0** | `~/.claude/rules/common/instincts.md`（并入 H5/H6 同一条） |
| H9 | **改前必读该领域的血泪教训全文**（不是只读索引） | ① `docs/dna/三档地盘.md` 🔴 档：改前必读 MEMORY 全部相关条目 ② `docs/dna/新功能前必查.md` ③ 项 ③ `audit-heuristic-blind-spots`：因为没做 4 路核对才被同型 false alarm 打脸 2 次 | `grep -rniE "血泪\|MEMORY\.md" "${G[@]}"` → **GLOBAL=0**；`"${D[@]}"` → DNA=42（项目级已完整） | `~/.claude/CLAUDE.md` 执行哲学「Self-Improve Loop」旁补一句：触发点从「用户纠正后」扩到「动 🔴 档前」 |

> **为什么「没意识到」**：这 9 条在**项目 DNA 层**大多已经写成条文，但**全局规则层**只有「做什么」（触发条件 + 动作），没有「怎么判断好坏」——所以从全局视角看，它们是「我一直在遵守、但规则文件里查不到」的行为。
> 其中 H5 / H6 / H8 / H7 在**全局与项目两层都查不到**，是最彻底的无意识区。

---

## 3. 与现有 13 Rules / instincts 的重叠与冲突

### 3.1 ⚠️ 已被实证推翻或互相矛盾的（重点）

| # | 冲突 / 推翻 | 证据 | 建议 |
|---|---|---|---|
| **X1** | **Rule 3「Surgical Changes：只触碰必须改的，不改善周围代码」** vs **执行哲学「Demand Elegance：是 hacky fix 就重写为已知全貌下的优雅解」** —— 后者本身就是「改周围」，二者直接矛盾 | `~/.claude/CLAUDE.md` Rule 3 原文 vs 执行哲学第 3 条原文 | 按 Rule 7「暴露冲突，不要平均」**选 Demand Elegance**（更晚引入、有实证支撑），把 Rule 3 收窄为「不顺手改善**无关**代码、不做**投机**重构；允许在同一改动范围内重写实现」 |
| **X2** | **「已 codified 即有效」被推翻**：`stop-subagent-rate.sh` 读错 JSONL 形状，**长期静默失效**（已修） | `retrieval-cost-audit-2026-10-22` | 推论写进判据层：**任何门禁上线前必须有「让它红」的用例**（H1）。这是本报告的第一条建议 |
| **X3** | **CLAUDE.md Rule 12 自身承认违规**：「子任务层调用率严重不足；本 session 是 #3 反例（主 agent 自己 OCR 25 张图）」，且 §0.3 缺口 2 显示 Stop hook 读 JSONL 形状错误 | `agent-4-layer-boundary-vs-cc-2026-10-12`、`retrieval-cost-audit-2026-10-22` | 规则已有但**没有判据**（多少算够？什么算「本该 dispatch」？）。这是「缺判据维度」的典型样本 |
| **X4** | **全局 `git-workflow.md` 的 commit 模板与项目门禁不一致**：全局模板是 `<type>: <description>` + body，**没有 Intent 行**；项目 `.git/hooks/commit-msg` 强制 `Intent: <一句话>`，缺失即拒 | `~/.claude/rules/common/git-workflow.md` vs `scripts/git_hooks/commit-msg` Gate 1 + `docs/dna/review-checklist.md` Phase 5 | 全局模板补 `Intent:`（或注明「有 commit-msg 门禁时必带」）。否则用户按全局规则写会被拒 |
| **X5** | **`coding-style.md`「Immutability: NEVER mutate existing ones」适用范围过宽**：对 Rust 项目是误伤（Rust 惯用 `let mut`）。全局规则把语言无关的诉求写成了语言相关的禁令 | `~/.claude/rules/common/coding-style.md` vs `anyhow-from-e-strips-types` 等 Rust memory | 收窄为「**对外暴露的数据结构**用不可变；内部局部可变状态用语言惯用写法」。这是「规则跨语言迁移时会被推翻」的样本 |
| **X6** | **索引被当成权威**：MEMORY.md 索引漏 5 个文件、4 个文件无 description | §0.3 缺口 2 | 推论：索引类产物也要有 E6 判据（可被脚本校验） |

### 3.2 已有覆盖、无需新增（全局规则已能挡住，勿重复）

- 最小改动 / 只碰必须改的 → Rule 3（除 X1 收窄）
- 完成前验证 → instincts 3「完成前验证」（除缺「证据形态」）
- 错误处理不静默吞掉 → `coding-style.md` Error Handling + instincts 11
- 不可变性 → `coding-style.md` Immutability
- 子代理输出长度上限 → instincts 13（≤5 bullet）
- 凭据 rotate → `security.md`（GLOBAL=2 命中）
- 基线回归 → CLAUDE.md 质量自检双问（GLOBAL=1 命中）
- 上下文预算 80% 写 handoff → Rule 6
- 新功能前 30 秒必查 / 三档地盘 / 变异测试 / Intent Lock → **项目 DNA 已有**，只需决定是否提升到全局

---

## 4. 建议落地（按 ROI 排序，8 条）

> 只列建议，**本次不改任何文件**。

| ROI | 改哪个文件 | 加什么 | 为什么优先 |
|---|---|---|---|
| **1** | **新建** `~/.claude/rules/common/criteria.md`（判据层，约 10 条「看到 X → 做 Y」）+ `~/.claude/CLAUDE.md` 执行哲学加 1 行指针 | 把 §1 的 15 条「全局 + 项目均未覆盖」中的可迁移判据集中成一层。内容建议：A3 / A5 / C1 / C3 / D1 / D2 / D6 / H5+H6+H8 合并条 / H7 | 用户明确要的「判据维度」目前完全空缺；独立文件**不动 13 Rules 的冻结**，也不加第 14 条 |
| **2** | `~/.claude/rules/common/testing.md` | 新增「测试有效性判据」小节：*改 1–2 行期望行为后全绿 = 测试在说谎；新增/修改判据脚本必须附一条让它红的用例并实跑确认会红* | H1 证据最硬（9 个 memory 文件 + 7 门实战），且直接堵住 X2 的失效模式；testing.md 现在只有覆盖率 80% + TDD，缺「测试有效性」 |
| **3** | `~/.claude/rules/common/instincts.md` | 新增 1 条 instinct「**完成声明 = 命令 + 输出**」：触发「声称完成 / 修复 / 优化」→ 动作「贴可复现命令与实际输出；指标字段缺失标 `⚪ 不可测`，不显示持平」 | 同时覆盖 A2 / A3 / H2；零新增 hook 成本，一次改动堵两类假绿灯 |
| **4** | `~/.claude/rules/common/git-workflow.md` | ① commit 模板加 `Intent: <一句话>`（解 X4）② 新增「Pre-commit 检查对象 = `git diff --cached` 的 staged 内容，不是工作区」（H4） | 模板不一致是**正在发生的摩擦**（commit 被拒）；staged 判据 8 个 memory 文件支撑 |
| **5** | `~/.claude/rules/common/coding-style.md` → Error Handling | 加一条「错误类型必须保留可 downcast 的具体类型；禁止用 `anyhow!()` / 字符串化把 typed error 降级」（D6） | 6 个 memory 文件；是一条纯判据、无需改代码结构即可执行的规则 |
| **6** | `~/.claude/CLAUDE.md` Rule 3 + 执行哲学 | 按 X1 收窄 Rule 3 为「不顺手改善无关代码、不做投机重构；同一改动内可重写实现」，并在执行哲学加注「解 X4 优先级：Demand Elegance 优先」 | 消除现存唯一一处**硬矛盾**（Rule 7 要求暴露冲突而非平均） |
| **7** | `~/.claude/rules/common/coding-style.md` Immutability | 收窄为「对外暴露的数据结构用不可变；内部局部可变状态用语言惯用写法」（X5） | 防止「规则跨语言迁移被实证推翻」这一类长期损耗 |
| **8** | `MEMORY.md`（索引，非规则） | 补 5 个漏索引条目 + 给 4 个无 frontmatter 的文件补 `description`，并写一条「索引可被脚本校验」的判据（E6 / H3 / X6） | 索引是 H3（分母可复算）最直接的受害者；成本低、立即可做 |

> **优先级说明**：ROI 1–4 覆盖了 15 条未 codified 判据中价值最高的 4 组，且**没有一条需要新增 hook**。

---

## 5. 不建议做的（明确写出，避免过度工程）

1. **不新增第 8 道 commit 门禁**。现有 7 门已 43/43 覆盖，`commit-gate-chain` 记录其失效方向是「fail-fast 非聚合」；再加门只会让「完成」越来越难达成。当前缺口是**判据缺失**，不是**门禁缺失**——先把判据写清（ROI 1–4），再考虑门禁化。
2. **不把 71 条 memory 全部提升为规则**。memory 是「事故记录」，规则是「判据」；提升门槛须是「≥2 次出现 + 可执行」，否则重演「提示词越堆越长」（同日 `docs/audit/skill-usage-2026-10-27.md`：119 技能 description ≈ 10–14k token 每次必载，agent 侧已 trim 过、skill 侧尚未做对称动作）。
3. **不把 Rust 专属细节写进全局 common rules**。`pub use` 重导出、`#[expect]` vs `#[allow]` 属项目 DNA（`edit-preflight.md`）；全局只留可迁移抽象（「删最后一个使用者要连带检查公开出口」）。
4. **不动 13 Rules 的数量**（冻结中）。新增判据走独立文件 + 指针，不加第 14 条；instincts 也优先用「补判据」而非「加第 14/15 条」。
5. **不做全量 jsonl 反馈挖掘**。实测 860 条人类消息里纠错型仅 5 条，信号密度太低，成本远高于收益。真实反馈在 memory + 门禁 + DNA 三处。
6. **不新建「反馈自动收集中枢」**（自动从 jsonl 抓反馈入库）。同上理由，且会引入新的门禁维护成本——与第 5.1 条直接冲突。
7. **不为「未 codified」而补记忆条目**。§2 的 9 条若只写进 memory 而没进规则文件，下次仍然「没意识到」——它们的价值恰恰在于**被 grep 得到**。

---

## 附：一条元观察

第 3 步最有价值的副产品：**「codified」不等于「有效」，也不等于「可被检索到」**。

- `stop-subagent-rate.sh` 长期静默失效（codified 但无效）→ X2
- MEMORY.md 索引漏 5 个文件（codified 但检索不到）→ X6
- DNA 层 8 条判据全局 grep 不到（codified 但作用域不对）→ §2 的 9 条

因此本清单的每一条都同时记了**两个维度**：`已 codified?`（grep 结果）+ **出现次数**（行为证据数）。只有两者同时达标，才建议提升为全局规则。
