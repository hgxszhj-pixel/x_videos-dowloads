# X Video Downloader — 项目交接

> 当前状态、最近 session 索引、待办。完整 session 日志在 [`docs/handoff/sessions/`](docs/handoff/sessions/)。
> 本轮拆分 (2026-10-03): 从 2117 行单文件 → ~70 行索引 + 21 个独立 session 文件。

## 当前状态 (2026-10-12)

- **主分支**: `master`,最近 commit `563a3fd` (docs(handoff): 2026-10-12 x-video-downloader perf baseline 锁定 + 4 open issue 闭环)
- **Workspace**: `gitbutler/workspace`,x-video-downloader 5 文件 modified 累计待 commit (perf session 代码,等用户本地 commit)
- **测试基线**: 375 passed, 12 ignored (perf session 2026-10-12 终轮验证)
- **perf baseline 锁定**: x-video-downloader `docs/perf-baseline.md` 4 open issue 全部闭环,regression 阈值建立
- **活跃产品线**: x-video-downloader / shanhu SaaS / exam-lookup
- **codegraph MCP**: v1.6.2 健康(2026-10-08 修复 + 重建);198 files / 4,067 nodes / 8,578 edges
- **关键 hook**: commit-msg (Intent Lock + **handoff_size_guard** + Ref-trace + DID/VC),pre-commit (cargo fmt + clippy)

## Sessions 索引 (最近 → 更早)

每条链接到完整 session 文件。28 个文件,日期范围 2026-10-12 → 2026-09-02。

- 2026-10-12 — [x-video-downloader perf baseline 锁定 + 4 open issue 解决 + commit pending](docs/handoff/sessions/2026-10-12-x-video-downloader-perf-baseline-locked-+-4-open-issues-resolved-+-commit-pending.md)
- 2026-10-08 — [the-scribble-thing skill 接入 + 2 demo 验证 + Toutiao 转录 + cron 排队等额](docs/handoff/sessions/2026-10-08-the-scribble-thing-skill-接入-+-2-个-demo-验证-+-Toutiao-转录-+-cron-排队等额.md)
- 2026-10-08 — [codegraph 启动失败修复 + v1.6.2 全量重建 + daemon.log 瘦身 (65.4 KB → 398 B)](docs/handoff/sessions/2026-10-08-codegraph-启动失败修复-+-v1.6.2-重建-+-daemon-log-瘦身.md)
- 2026-10-07 — [SKILL.state open_loops schema validator A1+A2+A4+A5 完整落地 (L3 防御 + A5 工作流)](docs/handoff/sessions/2026-10-07-skillstate-open-loops-schema-validator.md)
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

## Sessions Archive

完整 28 个文件,按日期排序,详见 [`docs/handoff/sessions/`](docs/handoff/sessions/)。

**更早 (2026-09-01 之前)**: 历史 session 摘要见 git history (`git log --oneline -- HANDOFF.md`);WikiSkill 决策日志 + skill 健康度盘点 (`756d49d` D-001~D-021);京峰教育/SHANHU 珊瑚 v1 报告"已推翻";智学云 `zhixueyun-analyzer` skill (08-29);全局 Claude Code 配置改造 5 hooks + 15 deny (08-28);tulingvip-m3u8-download v3.2 + douyin-mp4-download v1 (08-25);darwin 优化 web-protocol-recovery (08-22);opencode failover wrapper (08-21);OpenCode 多 provider (08-17);P1 重构 (08-16);GUI 性能 (08-13);小鹅通端到端 6 节 8.5GB (08-09)。

## Open Loops — Active (需要行动)

> SKILL.state 风格:每条 state slot 显式 owner/blocker/next/touched;history 见 `docs/handoff/sessions/`。新增条目用 `<verb-noun-action>` 标题,2 行内完成。

- **x-video-downloader perf commit** — owner: 用户 · blocker: 本 session 视角无 `.git` (x-video-downloader 目录) · next: 跑 session 文件中 commit 模板,需 `Intent:` 行;落地后 source 改 `gitbutler/workspace` 同步 · touched: 2026-10-12
- **moscan root 三条卸载** — owner: 用户 · blocker: 厂商脚本须 handoff · next: 跑 Paragon 卸载脚本 + OrbStack helper+plist + 搜狗 qlgenerator;备份包 `~/Desktop` · touched: 2026-10-04
- **moscan 真实点击验证** — owner: 用户 · blocker: macOS 辅助功能权限未开 · next: 系统设置 → 隐私与安全 → 辅助功能 → 允许 moscan;开后跑端到端 · touched: 2026-10-04
- **H3 green tier 剩 6 IO-bound 文件** — owner: 引擎 · blocker: mock 成本高 · next: 等集成测试框架或抽 pure-fn 分支 · touched: 2026-10-04
- **planner agent tier 化 (deep/light)** — owner: 引擎 · blocker: 待规划 · next: 下轮开 `/plan` 命令给方案 · touched: 2026-10-03

## Open Loops — Blocked / Waiting (被外部卡住)

- **scribble demo3 提交（4 工具对比图）** — waiting-on: the-scribble-thing daily free quota (UTC reset @ 00:00 = Beijing 08:00);session-only cron `b30767c8` 需 Claude 进程在 08:00 前不退出 · eta: 2026-10-09 08:00 cron 跑 `run_tools_compare.sh` → 产出 `preview3.mp4` + `sprite_tools_compare.png`;失败则人工 `$5` 买 unlock code 重试 · touched: 2026-10-08
- **更早 sessions 归档清理 (≤ 2026-09-01)** — waiting-on: 不紧急,作为 free-text 聚合行保留可读性足够 · eta: 季度归档时一并处理 · touched: 2026-10-03

## Just Closed (last 6 sessions) — 详情见 sessions/

> 关闭动作 (A5 工作流):把"为什么 close"一段写进 `docs/handoff/sessions/YYYY-MM-DD-<topic>.md`,此处只引用 path。reason 字段给一行摘要,ref 给完整 trajectory 链接。L3 防御:无 reason 不可审计决策;无 ref 不可追溯上下文。

- **x-video-downloader perf baseline 锁定 + 4 open issue 闭环** (2026-10-12) — reason: max_concurrent 4→8 (bench sweet spot 280ms);bumpalo/长 URL/AES 三个假设全验证完;375 tests + 3 bench 数字锁定 + 2 clippy 修复;用户 commit pending · ref: `docs/handoff/sessions/2026-10-12-x-video-downloader-perf-baseline-locked-+-4-open-issues-resolved-+-commit-pending.md`
- **the-scribble-thing 接入 + 2 demo + Toutiao 转录 + cron 排队** (2026-10-08) — reason: 月亮IT 视频 4 工具对比 → 装 skill（sha256 验证）→ 2 demo 验证（house+feedback loop 40s）→ 第 3 次撞 daily quota 429 → 排 cron `b30767c8` 08:00 Beijing 续做;transcript 落 `$CLAUDE_JOB_DIR/tmp/toutiao_transcript.txt` · ref: `docs/handoff/sessions/2026-10-08-the-scribble-thing-skill-接入-+-2-个-demo-验证-+-Toutiao-转录-+-cron-排队等额.md`
- **codegraph 启动失败修复 + v1.6.2 重建 + daemon.log 瘦身** (2026-10-08) — reason: npx cache 半残 (`lib/dist/` 缺失,SIGKILL) → 清 3 类残骸 (主包+pack cache+rename) → npx repull → handshake ✓ → index 198 文件重建 → log 65.4 KB → 398 B · ref: `docs/handoff/sessions/2026-10-08-codegraph-启动失败修复-+-v1.6.2-重建-+-daemon-log-瘦身.md`
- **SKILL.state open_loops schema validator (A1+A2+A4+A5)** (2026-10-07) — reason: 论文 L3 防御 + 显式 schema + 机器校验兜底;5 条 Just Closed 重构为 reason/ref;4 hook 全绿 rep=100.0 · ref: `docs/handoff/sessions/2026-10-07-skillstate-open-loops-schema-validator.md`
- **moli 1.1.14 安装** (2026-10-07) — reason: 7 次 GitHub 自取失败 → 用户直传 tarball → 本地 staged install;Rule 12 第 2 级激活 · ref: `moli-install-network-blocked-2026-10-07.md`
- **Web fallback rule 12 升级** (2026-10-07) — reason: 7 级降级链 `WebFetch → moli → ego-lite → agent-browser → camoufox → chrome → miniapp`,moli Markdown 替代 camoufox + ego-lite 填补已登录 SaaS Keychain 解密 cookie 中间档 · ref: `web-fallback-rule-2026-10-07.md`

## 决策索引

- 跨项目决策: [`~/.claude/memory/decisions.md`](file:///Users/hgxszhj/.claude/memory/decisions.md)
- 项目特定: [`~/.claude/projects/-Users-hgxszhj-x-videos-dowloads/memory/MEMORY.md`](file:///Users/hgxszhj/.claude/projects/-Users-hgxszhj-x-videos-dowloads/memory/MEMORY.md)
