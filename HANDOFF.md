# X Video Downloader — 项目交接

> 当前状态、最近 session 索引、待办。完整 session 日志在 [`docs/handoff/sessions/`](docs/handoff/sessions/)。

## 当前状态 (2026-10-22)

- **HEAD**: `80af9de` test(commit-gate): agent_did 补 8 例 + 加「空段判红」守卫 · **当前分支**: `gitbutler/workspace` (默认 PR 目标: `main`)
- **本轮主线**: 门禁审计 —— 「看起来像安全网的死门比没有门更危险」。修 3 个从未触发的检测器
  (`memory_prompt` 路径错位 / `avg_total_ms` 读不存在的 key / `stop-fmt-gate` 无测试),
  3 个冒烟套件全部带变异验证:**hook_smoke 18/18 · commit_gate_smoke 43/43(7 门全覆盖)· skill_gate 13/13**
- **测试基线**: 374 passed, 12 ignored (perf session 2026-10-12 终轮验证;1 pre-existing flake `core::trace::tests::test_integration_4_fault_points` 单独跑 pass,非本轮引入)
- **perf baseline 锁定**: x-video-downloader `docs/perf-baseline.md` 4 open issue 全部闭环,regression 阈值建立
- **活跃产品线**: x-video-downloader / shanhu SaaS / exam-lookup
- **codegraph MCP**: v1.6.2 健康(2026-10-08 修复 + 重建);198 files / 4,067 nodes / 8,578 edges
- **关键 hook**: commit-msg (Intent Lock + **handoff_size_guard** + openloops_schema + Ref-trace + **memory_prompt** + DID/VC),pre-commit (cargo fmt + clippy),Stop (fmt-gate + anti-slop + **subagent-rate**),PreCompact (HANDOFF persister + **mini-handoff**)

## Sessions 索引 (最近 → 更早)

每条链接到完整 session 文件。本索引只保留**最近 25 条**(2026-10-04 起),全部 36 个文件在
`docs/handoff/sessions/` 里按日期命名 —— 全量清单交给目录本身,不在这里再抄一份。



- **更早的 0 条**(2026-09-02 ~ 2026-09-25)已从索引移出,文件仍在 `docs/handoff/sessions/`,按日期命名排序。索引只保留最近窗口 —— 全量清单由目录本身承担,不在这里再抄一份。
- 2026-10-04 — [moscan 模块 25/26/27：卸载面板 + guard 根因(两个 HOME 来源) + 复制卸载命令 + Paragon/OrbStack 残留判定](docs/handoff/sessions/2026-10-04-moscan-模块-25-26-27-卸载面板-+-guard-根因-+-复制卸载命令-+-Paragon-残留判定.md)
- 2026-10-04 — [audit 4 处 false alarm 闭环 (C1/C2/C3/H1) + H3 green tier partial 4 文件 pure-fn 测试](docs/handoff/sessions/2026-10-04-audit-4-处-false-alarm-闭环-C1-C2-C3-H1-+-H3-green-tier-p.md)
- 2026-10-04 — [CLI flag 完整清单落档 M4 closure (25 个 flag 按 6 类分组)](docs/handoff/sessions/2026-10-04-cli-flag-清单-落档-M4-closure.md)
- 2026-10-03 — [HANDOFF.md 拆分 + size-guard hook + 13 Rules + TDD 升档 + agent 输出上限](docs/handoff/sessions/2026-10-03-handoff-md-split-+-handoff-size-guard-hook-+-instincts-13-rules-+-TDD-升档-+-agent-输出上限.md)
- 2026-09-30 — [x.com 直链下载策略修复 + 单段跳过合并](docs/handoff/sessions/2026-09-30-x-video-downloader-x.com-直链下载策略修复-+-单段跳过合并-Rust-下载.md)
- 2026-09-29 — [orca-video EP8 教程片方案 #1 并行 clip 渲染](docs/handoff/sessions/2026-09-29-orca-video-EP8-教程片-方案-#1-并行-clip-渲染-ProcessPoolExe.md)
- 2026-09-26 — [orca-video 教程片 v3 字幕丢字修复](docs/handoff/sessions/2026-09-26-orca-video-教程片-字幕时间轴丢字修复-+-v3-全量重渲.md)
- 2026-09-26 — [exam-lookup 代码审计修复 B1–B12 + 源码入 git](docs/handoff/sessions/2026-09-26-exam-lookup-代码审计修复-B1–B12-+-源码纳入-git-基线.md)
- 2026-09-25 — [orca-linear skill 接入 (pnpx install + orca CLI verify)](docs/handoff/sessions/2026-09-25-orca-linear-skill-接入-pnpx-install-+-orca-CLI-verif.md)
- 2026-09-22 — [exam-lookup 保密教育真题落库 + 依据配到条级](docs/handoff/sessions/2026-09-22-exam-lookup-保密教育真题落库-+-依据配到-条-+-电信侧首开条款依据-+-生成题升级.md)
- 2026-09-18 — [微信小程序逆向环境 (WMPFDebugger @ Intel macOS)](docs/handoff/sessions/2026-09-18-微信小程序逆向环境-WMPFDebugger-@-Intel-macOS.md)
- 2026-09-18 — [trace.rs 三连小外科 (dead_code + fallback log + serial_test)](docs/handoff/sessions/2026-09-18-trace.rs-三连小外科-dead_code-+-fallback-log-+-serial_t.md)
- 2026-09-18 — [OpenCode 净室重装 (no provider/auth clean state)](docs/handoff/sessions/2026-09-18-OpenCode-净室重装-no-provider-auth-clean-state.md)
- 2026-09-18 — [heuristic_judge_trace.py: 拒绝 LLM-as-a-Judge,纯启发式](docs/handoff/sessions/2026-09-18-heuristic_judge_trace.py-拒绝-LLM-as-a-Judge-纯启发式-4-.md)
- 2026-09-17 — [本地 NDJSON trace instrumentation v1: 4 故障点覆盖](docs/handoff/sessions/2026-09-17-本地-NDJSON-trace-instrumentation-v1-4-故障点覆盖-commit-.md)
- 2026-09-17 — [trace eval pipeline 4 指标 (baseline vs current)](docs/handoff/sessions/2026-09-17-trace-eval-pipeline-4-指标-baseline-vs-current-commi.md)
- 2026-09-17 — [local-skill-sync v1: gitee 仓库同步工具 8 子命令 + 16 tests](docs/handoff/sessions/2026-09-17-local-skill-sync-v1-gitee-仓库同步工具-8-子命令-+-16-tests.md)
- 2026-09-17 — [commit-msg hook 自动注入 Ref-trace (session_id 跳转 + 4 测试)](docs/handoff/sessions/2026-09-17-commit-msg-hook-自动注入-Ref-trace-session_id-跳转-+-4-t.md)
- 2026-09-14 — [T-Mem 轻接方法论落地 + L1 commit hook 实装 (无 commit, memory + hook)](docs/handoff/sessions/2026-09-14-T-Mem-轻接方法论落地-+-L1-commit-hook-实装-无-commit-memory-.md)
- 2026-09-14 — [SHANHU batch_video_state 0% pending 永久卡死 BUG 修复](docs/handoff/sessions/2026-09-14-SHANHU-batch_video_state-批量下载按钮-0%-pending-永久卡死-BU.md)
- 2026-09-13 — [SHANHU GUI "Status: Fetching..." 永久卡死两层叠加 bug 修复](docs/handoff/sessions/2026-09-13-SHANHU-GUI-"Status-Fetching..."-永久卡死两层叠加-bug-修复-co.md)
- 2026-09-06 — [SHANHU session 过期主动检测 (P1 commit `5ca9e1c`)](docs/handoff/sessions/2026-09-06-SHANHU-session-过期主动检测-P1-commit-`5ca9e1c`.md)
- 2026-09-06 — [GUI 单 URL 智能识别 + 批量视频面板 checkbox UI](docs/handoff/sessions/2026-09-06-GUI-单-URL-智能识别-+-批量视频面板-checkbox-UI-commits-`a9ca1.md)
- 2026-09-06 — [`shanhu-analyzer` skill 创建 (无 commit)](docs/handoff/sessions/2026-09-06-`shanhu-analyzer`-skill-创建-无-commit.md)
- 2026-09-02 — [Resolver Skeleton 共享基类 SDD (commits `c437a6b`..`ca2c45b`)](docs/handoff/sessions/2026-09-02-Resolver-Skeleton-共享基类-SDD-commits-`c437a6b`..`ca2.md)

- **更早的 12 条**(2026-09-02 ~ 2026-09-25)已移出索引,文件仍在 `docs/handoff/sessions/`,按日期命名排序。索引只保留最近窗口 —— 全量清单交给目录本身,不在这里再抄一份。
## Sessions Archive

完整 35 个文件,按日期排序,详见 [`docs/handoff/sessions/`](docs/handoff/sessions/)。

**更早 (2026-09-01 之前)**: 摘要见 git history(`git log --oneline -- HANDOFF.md`)。含 WikiSkill 决策日志 + skill
健康度盘点(`756d49d`);SHANHU 珊瑚 v1 报告已推翻;智学云 skill(08-29);全局配置 5 hooks + 15 deny(08-28);
tulingvip-m3u8 v3.2 + douyin-mp4 v1(08-25);web-protocol-recovery(08-22);opencode failover(08-21);
多 provider(08-17);P1 重构(08-16);GUI 性能(08-13);小鹅通端到端 8.5GB(08-09)。

## Open Loops — Active (需要行动)

> SKILL.state 风格:每条 state slot 显式 owner/blocker/next/touched;history 见 `docs/handoff/sessions/`。新增条目用 `<verb-noun-action>` 标题,2 行内完成。

- **airgorah macOS 移植两条收尾验证** — owner: 用户 · blocker: 各需用户一次动作(Cmd-Q 优雅退出 / 以普通用户启动 GUI 并在系统对话框输密码) · next: ① 退出窗口验 agent 靠 socket EOF 自清;② 普通用户起 GUI 点网卡验 osascript 提权分支 · touched: 2026-10-16
- **Toutiaohao 9 AM launchd 自跑首验** — owner: launchd · blocker: 无人值守,失败仅 exit 1 日志,无桌面通知 · next: 明天 9 AM 后 `tail -F ~/Horizon/logs/toutiao-cron.log` 验 exit 0 + 跳 `/graphic/articles`;失败 → 重 `~/Horizon/.venv/bin/python scripts/toutiao_login.py` 刷 session · touched: 2026-10-16
- **sk-cp- API key rotate (本 session 泄露 3x)** — owner: 用户 · blocker: 安全 item,session 末尾必做;SHA-256 20bc7c53e2ce06cb95fa103d4bbcbb3eb81421a5abb08b304cb9da5e459c25e7;备份 `~/Horizon/.env.bak-20261016-leaked` chmod 600 · next: 去 minimax 控制台 → 失效旧 key → 写新 key 到 `~/Horizon/.env` → smoke 一次 publish 验 · touched: 2026-10-16
- **x-video-downloader perf commit** — owner: 用户 · blocker: 本 session 视角无 `.git` (x-video-downloader 目录) · next: 跑 session 文件中 commit 模板,需 `Intent:` 行;落地后 source 改 `gitbutler/workspace` 同步 · touched: 2026-10-12
- **moscan root 三条卸载** — owner: 用户 · blocker: 厂商脚本须 handoff · next: 跑 Paragon 卸载脚本 + OrbStack helper+plist + 搜狗 qlgenerator;备份包 `~/Desktop` · touched: 2026-10-04
- **moscan 真实点击验证** — owner: 用户 · blocker: macOS 辅助功能权限未开 · next: 系统设置 → 隐私与安全 → 辅助功能 → 允许 moscan;开后跑端到端 · touched: 2026-10-04
- **H3 green tier 剩 6 IO-bound 文件** — owner: 引擎 · blocker: mock 成本高 · next: 等集成测试框架或抽 pure-fn 分支 · touched: 2026-10-04
- **planner agent tier 化 (deep/light)** — owner: 引擎 · blocker: 待规划 · next: 下轮开 `/plan` 命令给方案 · touched: 2026-10-03
- **3 hooks 观察期 (PreCompact mini-handoff / Stop subagent-rate / commit-msg memory-prompt)** — owner: 主对话 · blocker: 需 1-2 周 session 数据判误报率 · next: 监控 SUBAGENT_MIN_THRESHOLD=10 误报/漏报;memory_prompt INSIGHT_SIGNALS 触发率;PreCompact 在 HANDOFF.md 不存在时退化行为 · touched: 2026-10-12
- **`avg_total_ms` 要真埋点** — owner: 引擎 · blocker: 需改 `src/core/trace.rs` 写 `total_ms_proxy` + 重锁 perf 基线,当前只如实标 `⚪ 不可测` · next: 下轮做 trace instrumentation 时一并落地,落点 `session_close` · touched: 2026-10-22

## Open Loops — Blocked / Waiting (被外部卡住)

- **validate_skill.py 第 6 类误报修复** — waiting-on: 需用户在 **auto mode 之外** 于权限提示里亲自批准改 `~/.claude/scripts/validate_skill.py`(分类器连续 3 次以自我修改拒绝,对话里的「继续」不算授权) · eta: 下次会话 · touched: 2026-10-22 · 影响: skills/ 下 11 个非技能目录(技能包/工具/`.git`/备份)被报「缺少 SKILL.md」,占全量扫描失败数 48%;**只影响噪声不影响安全** —— 边界已由 `e24cee5` 的 `deleted-skill-md` 变异用例机器守住

- **scribble demo3 提交（4 工具对比图）** — waiting-on: the-scribble-thing daily free quota (UTC reset @ 00:00 = Beijing 08:00);session-only cron `b30767c8` 需 Claude 进程在 08:00 前不退出 · eta: 2026-10-09 08:00 cron 跑 `run_tools_compare.sh` → 产出 `preview3.mp4` + `sprite_tools_compare.png`;失败则人工 `$5` 买 unlock code 重试 · touched: 2026-10-08
- **更早 sessions 归档清理 (≤ 2026-09-01)** — waiting-on: 不紧急,作为 free-text 聚合行保留可读性足够 · eta: 季度归档时一并处理 · touched: 2026-10-03

## Just Closed (last 8 sessions) — 详情见 sessions/

> 关闭动作 (A5 工作流):把"为什么 close"一段写进 `docs/handoff/sessions/YYYY-MM-DD-<topic>.md`,此处只引用 path。reason 字段给一行摘要,ref 给完整 trajectory 链接。L3 防御:无 reason 不可审计决策;无 ref 不可追溯上下文。

- **门禁审计:「不能失效的检测器」 (9 commit + 1 in ~/.claude)** (2026-10-22) — reason: 复核早前结论时发现 `stop-subagent-rate.sh` 恒绿已静默失效 32 个 session,判定为通病 → 把其余门全过一遍,找到 **3 个「接线了但从不触发」**:`memory_prompt` 调度器算 `scripts/memory_prompt.py` 而文件在 `scripts/git_hooks/`(`.exists()` 恒假,接线以来一次没调用)、`avg_total_ms` 读 `total_ms_proxy` 而该 key 在 `src/` 里从未被写过(恒 0 却显示「✅持平」)、`stop-fmt-gate.sh` 无「能不能红」的测试;扩 `mutation_smoke` 纪律到 shell/Python 门(断言会红而非「跑一遍没报错」)→ **hook_smoke 18/18 + commit_gate_smoke 43/43(7 门全覆盖)+ skill_gate 13/13,共 22 处变异验证**;`commit_gate_smoke` 里的静态门扫调度器每个 `from X import` 断言 `X.py` 存在,**覆盖整类 bug 而非单个实例**,正是它抓出第 1 个;三条设计:① 三个门返回**三态**(拦/警告/静默),证据是 `HARD_LIMIT` 调到 10000 时门不是变哑而是从 block 降级成 warn,只断言「拦没拦」的测试会判它通过;② fail-closed 比 fail-open 更致命,一次 `--no-verify` 就让门永久变装饰品,调度器 6 处未保护 `git rev-parse` 属此类;③ `eval_gate` 关键词表刻意不含 conventional-commit 类型,附 2 条反向用例锁住;自事故:批量抽 `report()` 时漏写 `$`,**五整段用例全空却照样 `passed: 10 failed: 0` 退出 0** —— 护栏挡住了噪声却挡不住自己这类错,已加 `section()` 空段判红守卫并复现验证 rc=1;未闭合:validate_skill.py 第 6 类误报修复被 auto mode 以自我修改拒绝 3 次(边界已由 `e24cee5` 变异用例机器守住,只影响噪声) · ref: `docs/handoff/sessions/2026-10-22-detector-that-cannot-fire-gate-audit.md` + `memory/commit-gate-chain-7-gates-2026-10-22.md` + `memory/retrieval-cost-audit-2026-10-22.md`
- **Toutiaohao 头条号 daily auto-publish (0 commit, work in ~/Horizon)** (2026-10-16) — reason: 用户问「每天9点定时发布新闻,写一个脚本」→ 4 文件落地 (toutiao_publish.py + toutiao_login.py + launchd plist + pyproject.toml camoufox extra),3-click publish 流程关键发现(底部"定时发布"→schedule modal "预览并定时发布"→preview modal "定时发布",每步 modal 用 `:has-text("...")` substring),8 selector 教训(`get_by_role` 失效 / force-click label cover / `insertText` body / `:has-text` 强于 `name=` / 成功 URL = `/articles` not `/publish_success` / playwright 1.60.0 `pageerror` crash patch);end-to-end smoke 2026-10-16 15:23:53 通过(exit 0,跳 `/graphic/articles`);launchd plist 已 load(`com.horizon.toutiao-daily` 9 AM daily);未闭合:首次 9 AM 自跑待观察 + sk-cp- key rotate(本 session 泄露 3x) · ref: `docs/handoff/sessions/2026-10-16-toutiao-daily-publish-3-click-流程-落定-+-launchd-已-load.md`
- **Airgorah macOS 移植 (0 commit)** (2026-10-16) — reason: 用户问「我的 Mac 能不能跑 Airgorah」并顶回「可以源码编译」→ 先 root 下实测内置 Broadcom 卡(airport 空壳/`airport en0 sniff 6` 无 .cap/`airodump-ng en0` 报 Failed initializing/aireplay exit=1 → 硬件侧无解,不是权限);随后按分层 probe 移植:GTK4 栈需拼 201 个 keg 的 pkgconfig 目录 + Homebrew 自带 system `.pc`(brew install bzip2),`cargo check -p airgorah` 55.52s 通过、build 21 MB,卡点全在 agent 的 8 个 Linux 专有 API;移植 9 文件 810 行(**Linux 路径零改动**,全 cfg 分支):`AF_PACKET`→`/dev/bpf`(ioctl 码按 `_IOC` 现算并用 C 探针从 SDK 取值对齐,bpf_hdr 64 位 20 字节布局编译期 assert)、`SO_PEERCRED`→`LOCAL_PEERCRED`、`pkexec`→osascript 提权、sysfs→networksetup、iw→system_profiler、xterm→Terminal.app;端到端:GUI 与 agent 双跑起来,点网卡弹诚实的 `Monitor mode failed`(监听模式在 macOS 无解,不假成功);verify: agent 27 tests / GUI 3 tests 全绿、BPF 抓包 en0 1 帧 157 B、agent 日志 `listening on /var/run/airgorah/0-67106.sock for uid 0`;未闭合:agent 靠 EOF 自清(需 Cmd-Q 优雅退出)与非 root osascript 提权分支(各需用户一次动作) · ref: `docs/handoff/sessions/2026-10-16-airgorah-macOS-移植-9-文件-+-GUI-agent-跑通-+-监听模式硬错误.md`
- **MCP/Agent 启动成本审计 (档 1+2, settings flip, 0 commit)** (2026-10-15) — reason: `/context` 显示 MCP+agents 占 32.5% context window → 30-day grep 95 文件 (63 memory + 32 sessions) 砍 19+11 = 30 niche agent (Tier 1 niche / Tier 2 兜底);settings.json 翻 3 plugin false (context7 / skill-creator / skill-doctor);估算省 ~5.7-5.9k t session 启动成本 (200k 窗口的 2.85-2.95%);删优于禁 (rm vs disable),全 backup 到 `.bak-20261015/` 兜底 ≥ 90 天;9 兜底留档 3 评估 (≥ 2026-12) · ref: `docs/notes/mcp-agent-audit-2026-10-15.md` + `memory/mcp-agent-trim-2026-10-15.md`
- **rea 逆向 + Macs Fan Control 一比一复刻 (0 commit)** (2026-10-15) — reason: 用户贴截图要求「利用 rea 一比一复刻」→REA 真跑(analyze 8989 过程 / 322 符号 / 2-of-3 反编译) 后转 otool 补 `setFanRPM` 写入编码(键长 4→`flt ` 浮点、2→`rpm*4`);7 个风扇键模板由**重签副本 + main 断点**运行期直读(`F{}ID/Mn/Mx/Ac/Tg/Sf/Md`,len=5)并与本机 1164 键空间交叉定案;主窗口原生控件(风扇胶囊/预设胶囊/刷新盘/底部按钮+?)全部按量值自绘,去标题栏整体像素差 10.12→**9.73**;自检 22/22;调速仍需用户装 root 特权助手 · ref: `docs/handoff/sessions/2026-10-15-rea-mfc-clone-Macs-Fan-Control-一比一复刻.md`
- **OMC HUD 安装 + mattpocock-skills marketplace SHA pin 滞后诊断 (无 commit)** (2026-10-15) — reason: 系统提示 `[OMC] HUD not configured` → 装 `omc-hud.mjs` + 切 settings.json statusLine(用户授权覆盖自定 shell);同 session 撞 `mattpocock-skills` 升级 1.2.3 → 1.3.x 失败,根因 = `claude-plugins-official/marketplace.json` SHA pin 落后 upstream,决策 D-2026-10-15-2 选择"等 marketplace bump"而非手动 pin(避免下次 update 回滚);project 0 commit,仅 settings.json + 1 memory 改动 · ref: `docs/handoff/sessions/2026-10-15-OMC-HUD-安装-+-marketplace-SHA-pin-滞后诊断.md` + `memory/claude-plugins-official-marketplace-lag-2026-10-15.md`
- **x-video-downloader 2 carry-over fix (E0433 bench + CLAUDE.md cargo test)** (2026-10-12) — reason: pre-existing E0433 根因在 `src/hls/downloader.rs:177` 而非 bench 文件(bench 通过 `#[path]` 拉 src 代码,`#[cfg(macos)]` 调 `crate::drission_client`);14 行 stub mod 修复,runtime 行为不变;CLAUDE.md `cargo test --lib` → `--bins` 因项目 0 `[lib]` + 3 `[[bin]]`;2 commit (205ccf0 + 42fe3ad) 一次性 push 通过,无需 force · ref: `docs/handoff/sessions/2026-10-12-x-video-downloader-2-carry-over-fix-(E0433-bench-+-CLAUDE.md-cargo-test).md`
- **PhAI Labs 论文真伪审查 (无 commit, memory only)** (2026-10-12) — reason: Toutiao 文章 `7695660887541154354` → camoufox `<article>` 选中文避评论 · Rule 12 tier 5 fallback 命中;逐项核对 arXiv 2610.13912 / 2609.14709 / GitHub / LabHorizon / ReasonFlux-Coder-7B 全部独立验证 ✓;产出 5 必查项判读模板 (论文 §5 limitations / negative baseline / 小差距表述 / scope 范围声明 / failed episodes) + PhAI Labs「严谨论文 + 营销转译」分层画像 · ref: `memory/phai-labs-paper-vs-media-discipline.md`

## 决策索引

- 跨项目决策: [`~/.claude/memory/decisions.md`](file:///Users/hgxszhj/.claude/memory/decisions.md)
- 项目特定: [`~/.claude/projects/-Users-hgxszhj-x-videos-dowloads/memory/MEMORY.md`](file:///Users/hgxszhj/.claude/projects/-Users-hgxszhj-x-videos-dowloads/memory/MEMORY.md)
