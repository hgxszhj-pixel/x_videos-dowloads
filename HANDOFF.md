# X Video Downloader — 项目交接

> 当前状态、最近 session 索引、待办。完整 session 日志在 [`docs/handoff/sessions/`](docs/handoff/sessions/)。
> 本轮拆分 (2026-10-03): 从 2117 行单文件 → ~70 行索引 + 21 个独立 session 文件。

## 当前状态 (2026-10-04)

- **主分支**: `master`,最近 commit `a0f72c0` (C2 false alarm 修订)
- **Workspace**: `gitbutler/workspace`,3 文件 H3 partial 待 commit (xbogus/theme/strategy_native)
- **测试基线**: 320+ tests (H3 green tier 4 文件 pure-fn 测试已绿,具体数待 cargo test 终轮)
- **活跃产品线**: x-video-downloader / shanhu SaaS / exam-lookup
- **关键 hook**: commit-msg (Intent Lock + **handoff_size_guard** + Ref-trace + DID/VC),pre-commit (cargo fmt + clippy)

## Sessions 索引 (最近 → 更早)

每条链接到完整 session 文件。23 个文件,日期范围 2026-10-04 → 2026-09-02。

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

完整 21 个文件,按日期排序,详见 [`docs/handoff/sessions/`](docs/handoff/sessions/)。

**更早 (2026-09-01 之前)**: 历史 session 摘要见 git history (`git log --oneline -- HANDOFF.md`);WikiSkill 决策日志 + skill 健康度盘点 (`756d49d` D-001~D-021);京峰教育/SHANHU 珊瑚 v1 报告"已推翻";智学云 `zhixueyun-analyzer` skill (08-29);全局 Claude Code 配置改造 5 hooks + 15 deny (08-28);tulingvip-m3u8-download v3.2 + douyin-mp4-download v1 (08-25);darwin 优化 web-protocol-recovery (08-22);opencode failover wrapper (08-21);OpenCode 多 provider (08-17);P1 重构 (08-16);GUI 性能 (08-13);小鹅通端到端 6 节 8.5GB (08-09)。

## 待办 / Open Items

- **HANDOFF.md 拆分** (本轮完成): 2117 行 → ~70 行索引 + 22 个独立 session 文件,降低 96% 行数
- **5 维度启发式审计** (本轮完成): docs/audit/2026-10-03-*.md — 4 critical + 3 high + 4 medium + 15 heuristic flags;🔴 C1 (theme.rs iced) / 🔴 C2 (6 处生产 panic) / 🟠 C3 (http_client panic) / 🟠 H1 (collaboration 隔离) **4/4 经逐项源码验证为 false alarm**(panic 3 类上下文 / cfg gate / dioxus re-export);🔴 C4 (HANDOFF 测试基线 298→320 漂移) 已在 `b3d39d4` 修复;`audit-heuristic-blind-spots.md` 沉淀 4 类盲点 + panic 3-context 表格
- **H3 green tier partial** (本轮完成 4/10): theme.rs (5) + xbogus.rs (5) + strategy_native.rs (2) + path.rs 测试已绿;剩 6 IO-bound 文件推迟 (mock 成本高,留待 integration test)
- **handoff_size_guard hook** (本轮完成): staged HANDOFF.md > 200 行 hard reject,> 100 行 soft warn;`docs/handoff/sessions/` 季度归档 (> 90 天 → `archive/YYYY-Qx/`) 协同防目录无界膨胀
- **用户全局 13 Rules** (本轮完成): workflow-light-plan 自决策白名单 + 暂停条件,workflow-tdd-red-green 0.8→0.9,新增 workflow-agent-output-bounded
- **planner agent tier 化** (deep/light): 下轮决策,需要单独规划
- **更早 sessions 归档清理** (2026-09-01 之前): 现以聚合行保留;后续如需独立文件可批量处理

## 决策索引

- 跨项目决策: [`~/.claude/memory/decisions.md`](file:///Users/hgxszhj/.claude/memory/decisions.md)
- 项目特定: [`~/.claude/projects/-Users-hgxszhj-x-videos-dowloads/memory/MEMORY.md`](file:///Users/hgxszhj/.claude/projects/-Users-hgxszhj-x-videos-dowloads/memory/MEMORY.md)
