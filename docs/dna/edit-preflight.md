# Edit Preflight — 修改前自检(Reflect mod 来源)

> 本文件记录"用户纠正 / Claude 自检失败 → 转规则"的实战教训。  
> 触发时机:每次 Edit/Write/destructive 动作前 30 秒 grep;命中即调整方案,避免重犯。
> **来源**: 2026-10-11 头条文章 → Mutex 改造 → 4 atomic commit 收尾时累积。

---

## Rule 1 — Revert/Reformat 后必 Re-Read

**症状**: `git checkout HEAD -- <file>` 或 `cargo fmt` 后立刻 Edit,报 "String to replace not found"。

**根因**: 缓存读人的 mental model ≠ 文件当前 bytes;rustfmt / revert 会改 indentation / 链式方法换行,Edit 字符串匹配失败。

**执行**:
1. revert 或 rustfmt 后 → **强制 Re-Read** 目标文件(只 Read 必要行)
2. 复制当前 bytes 到 Edit 的 `old_string`,不靠记忆
3. **大文件 Edit diff ≥30 行时立刻怀疑 rustfmt debt**,直接 revert + 从 HEAD 重做

**实证**: `src/platform/shanhu.rs` 第一次 Edit 产生 191 行 diff,主要为 rustfmt whitespace;revert 后第二次 Edit = 14 行 clean diff。

---

## Rule 2 — 删最后 caller 必须连带 `pub use` 重导出

**症状**: 删完所有 internal caller,`cargo check` 报 `unused import: token_manager::TokenManager`。

**根因**: `pub use foo::Bar` 是 re-export,即便内部 0 caller,只要它是 `pub` API 就被视作 live。

**执行**:
1. 删除前先 `grep -rn "<TypeName>" src/` 列全部 internal use site
2. 删完内部 caller 后 → **必跑** `cargo check`(不是只 cargo build)
3. 看到 unused warning → 立刻删 `pub use` re-export,**不要 ignore warning**

**实证**: 本会话删完 `YtdlpBacked.token_manager` 全部 caller 后,漏删 `mod.rs` 的 `pub use token_manager::TokenManager`,cargo check 立即报警。

---

## Rule 3 — Hook schema 校验的事前规则

**症状**: `commit-msg` hook 拒 commit,提示 "missing fields ['waiting-on', 'eta']"。

**根因**: HANDOFF.md `open_loops` 条目从 legacy 4-field (`owner/blocker/next/touched`) 转 SKILL.state 3-field (`waiting-on/eta/touched`),但**只改了 `Blocked` 段一条**,`Active` 段 4 条仍 legacy 格式。

**执行**:
1. Edit `HANDOFF.md` open_loops 前 → **先 grep 当前 entry 的 schema**(`grep -E "(owner|waiting-on):" HANDOFF.md`)
2. **批量改**:所有 entry 一次性切 3-field,不要漏段
3. 提交前用 `cat HANDOFF.md | grep -E "(owner|waiting-on):"` 复核

**实证**: 90450da 第一次 commit 被 hook 拒(仅 `Blocked` 段 1 条合规),改完后通过。

---

## Rule 4 — `cargo check` 失败先 stash 隔离 baseline

**症状**: 修改某文件后 `cargo check` 报 error,error 指向未触碰的行(file path 不在我 Edit 范围)。

**根因**: baseline pre-existing defect(`drission_client` 缺失、`stats_recent_7d_window` 时间窗 flake 等),不是我的改动引入。

**执行**:
1. `git stash` 暂存我的改动
2. 重跑 `cargo check` / `cargo test`
3. **失败依旧** = baseline 缺陷,**不属于本 commit 范围**,记录到 HANDOFF open_loops 独立修
4. `git stash pop` 恢复,**不试图在 baseline defect 上"顺带修"**(scope creep)

**实证**: 本会话 `src/hls/downloader.rs:177` 的 `drission_client` 编译错 + `commands/history_stats.rs::stats_recent_7d_window` 时间窗 flake,均为 stash 隔离后确认 baseline,未污染本轮 commit。

---

## Rule 5 — rustfmt 大批文件先 AskUserQuestion

**症状**: 单次 commit 18 文件,触及 🔴 档 (shanhu_session.rs 等),diff 体量 +429/-269,reviewer 一眼看不全。

**根因**: repo-wide rustfmt 债一次性消化,scope 远超用户当前任务的边界。

**执行**:
1. 触发 stop-fmt-gate hook 拒 commit,**不要强行加 `--no-verify`**
2. **必 AskUserQuestion** 让用户在 {本任务范围 / 全仓一次性 / 拆 N 个 commit} 三选一
3. 用户选全仓 → commit message **必明确列出触及的 🔴 档文件**,让 reviewer 一眼看到 risk

**实证**: 本会话 stop-fmt-gate hook 误触后,AskUserQuestion 给"提交全 18 文件 (推荐)"选项,用户确认后 commit message 列出 shanhu_session.rs 等 🔴 档 + 声明"0 逻辑改动"。

---

## Rule 6 — 第三方 API 撞 quota 立即查 reset time + 排 cron

**症状**: the-scribble-thing 429 (daily quota exceeded),反复 Retry 仍然 429。

**根因**: quota 是 calendar-bound (UTC 00:00 = Beijing 08:00),不等时间窗口重置,retry 无效。

**执行**:
1. 撞 429 **第一动作不是 retry**,而是查 quota reset 时间
2. 排 `CronCreate` (session-only),`recurring: false` 一次性,prompt 写明 "重试 X"
3. **session-only cron 警告**:若 Claude 进程可能退出,改 `durable: true` 写 `.claude/scheduled_tasks.json`

**实证**: 2026-10-08 cron `b30767c8` 排 Beijing 08:00 重试,本会话发现 session-only cron 在进程退出后会失效,**留作 open loop 待 follow-up**。

---

## 防膨胀

- 单条 Rule ≤ 10 行(本文件 ~80 行,8 条 Rule 留空间)
- 新增 Rule 必须先 grep 现有 6 条 + 删 / 合并最相近的一条
- 任何 Rule 触发 2 次以上 → 升级为 hook(pre-commit / commit-msg / stop)

---

## 关联 DNA 文档

- [`docs/dna/三档地盘.md`](三档地盘.md) — 🔴 档文件改动前必读 tier 边界
- [`docs/dna/新功能前必查.md`](新功能前必查.md) — grep 3 项(skill / 项目内模块 / 血泪教训)
- [`docs/dna/review-checklist.md`](review-checklist.md) — 5 阶段 pre-merge checklist

> **why**: 本文件 = Self-Improve Loop 的物理载体。用户 CLAUDE.md execution philosophy 第 2 条明文要求 "规则必须可执行"——本文件每条 Rule 给"症状 / 根因 / 执行"三段,直接对应。