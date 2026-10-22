# 2026-10-22 — 「不能失效的检测器」门禁审计

**主题**: 审计 harness 的每一道门是否**真的能触发**;发现并修复 3 个「接线了但从不触发」的检测器。
**产出**: 9 commit(本仓库)+ 1 commit(`~/.claude`),3 个冒烟测试套件全部带变异验证。

---

## 起因

本 session 的主题词一句话:**看起来像安全网的死门,比没有门更危险**。
不是从需求来的,而是复核早前结论时发现,`stop-subagent-rate.sh` 恒绿已静默失效 32 个 session。
既然是通病,就该把其余的门也过一遍。

## 找到 3 个「接线了但从不触发」

| # | 组件 | 症状 | 根因 |
|---|------|------|------|
| 1 | `memory_prompt`(commit-msg 门 4) | 接线以来 `check_memory_prompt` 一次都没被调用 | 调度器算 `scripts/memory_prompt.py`,文件实际在 `scripts/git_hooks/` → `.exists()` 恒假 |
| 2 | `avg_total_ms`(eval_trace 指标) | 恒为 0 却显示「✅ 持平」 | 读 `total_ms_proxy`,该 key 在整个 `src/` 里**从未被写过** |
| 3 | `stop-fmt-gate.sh` | 无「能不能红」的测试 | 唯一没被 `hook_smoke.sh` 覆盖的 Stop hook |

三者是同一形态:**代码看起来在跑,实际有一条路径让它永远进不去,而输出上看不出来。**

## 关键决策:先证明能红,再谈能守

沿用 `mutation_smoke.sh` 的纪律并扩到 shell/Python 门:**断言门会转红**,而不是「跑一遍没报错」。
只会报绿的检测器正是本 session 要消灭的东西 —— 包括我自己写的测试。

覆盖最终落在三个冒烟套件:

| 套件 | 规模 | 变异验证 |
|---|---|---|
| `scripts/hook_smoke.sh` | 18/18(5 个 Stop hook) | 3 处 |
| `scripts/commit_gate_smoke.sh` | **43/43(7 个 commit-msg 门全覆盖)** | 11 处 |
| `~/.claude/scripts/validate_skill_smoke.sh` | 13/13 | 8 变异 + 4 容忍 |

`commit_gate_smoke.sh` 里最值钱的一条是**静态门**:扫调度器源码里每个 `from <name> import`,
断言 `scripts/<name>.py` 存在。覆盖的是**整个 bug 类**(门被移动/改名就红)而非单个实例 ——
正是它抓出了上面第 1 个。不硬编码门清单,新增门自动跟上。

## 三条不该省的设计

**1. 三个门返回三态,不是布尔。** `(False,msg)` 拦 / `(True,msg非空)` 警告 / `(True,"")` 静默。
第三态与第二态在调用方眼里都是「放行」。证据:把 `HARD_LIMIT` 200 调到 10000,门**不是变哑
而是从 block 降级成 warn** —— 只断言「拦没拦」的测试会判它通过。

**2. 失败有两个方向,第二个更致命。** fail-open(环境坏 → 静默放行)大家都防;
fail-closed(永远拦)没人防,但它只需**一次 `--no-verify`** 就让门永久变装饰品,而且用户不会吭声。
调度器里 6 处未加保护的 `git rev-parse` 在非 git 目录会 traceback → 拦 commit,属后者。

**3. 反向用例锁设计意图。** `eval_gate` 的关键词表是语义动词(fix/ship/complete/修复/完成),
**刻意不含** conventional-commit 类型。附 2 条用例锁住:`feat:`/`perf:` 本身不算完成声明。
理由:那些类型几乎每个 commit 都有,计进去这个门就逢 commit 必触发 —— 就是第 2 条那个方向。

## 本 session 我犯的三个错

- **拿真阳性当阴性,当天两次。** 先拿 `feat: add flag` 当 memory_prompt 的「无信号」反例,
  又拿 `perf: 优化了 yyy` 当 eval_gate 的完成声明 —— `feat` 在 `INSIGHT_SIGNALS` 里、
  `perf` 不在关键词表里,两次都是**我的期望写错,不是代码错**。
  根因:拿一个「看起来应该触发」的消息就假设它会触发。写反例前先读判定逻辑。
- **测试脚本报绿,却什么都没测。** 批量把 5 段内联读循环抽成 `report()` 时,
  把 `report "$VAR"` 写成 `report "VAR"`(漏 `$`),函数收到字面量,那行没有 TAB,
  被 `case "$st" in ok|FAIL)` 守卫静默跳过 → **五整段用例全空,照样 `passed: 10 failed: 0` 退出 0**。
  护栏挡住了噪声却挡不住自己这类错。已加 `section()` 守卫:每段必须至少产出一个用例,
  否则显式判红;复现验证 5 条 FAIL + `rc=1`。
- **zsh 不做无引号变量分词**,导致一次变异备份写成了名为 `=intent_lock.py.bak` 的文件,
  还原从未执行,`intent_lock.py` 保持变异态 —— 靠最终跑测出 FAIL 才发现。

## 明确不做的事

- **`avg_total_ms` 补埋点**:要改 `src/core/trace.rs` + 重锁 perf 基线,当前只如实标注「不可测」。
- **核实 `fetch_video_info_ok` 不是漏网**:`trace.rs:707` 注释说明 RAII duration 是它的超集,刻意不改。
- **8 个技能真实缺陷**(third-party skills-CLI 管的目录,改了就是制造 fork 漂移)。
- **`.git/hooks/memory_prompt.py` 陈旧副本**:无人 import,无害,留着。
- **不建 Stop hook 调验收门**:skill 生成是低频手动流程,给一年跑几次的事建常驻 hook
  是为预判需求付复杂度。调用点写在 memory 的 How to apply,每轮进上下文,零新增机制。

## 唯一被拦下的项

`~/.claude/scripts/validate_skill.py` 的 `gate_schema`:skills/ 下 11 个**根本不是技能**的目录
(技能包 / 工具 / `.git` / 备份)被报「缺少 SKILL.md」,占全量扫描失败数的 48%。
auto mode 分类器以**自我修改**为由连续拒绝 3 次,明确要求用户在权限提示里亲自确认。
未绕过。边界已由 `e24cee5` 的 `deleted-skill-md` 变异用例机器守住(chapters/ 在 → 缺入口 → 必须红),
所以这项只影响噪声,不影响安全。

## 落地 commit

本仓库:`f40a0b1` `47db752` `fef80a4` `cf064d8` `172ba64` `3d761f9` `0ddde1a` `8de6faf` `80af9de`
`~/.claude`:`e24cee5`

## How to apply

改 `scripts/*.py` 或 `~/.claude/scripts/` 里任何一道门之前,先跑对应冒烟套件。
新增门时静态门自动跟上,但仍要手写 fire/quiet 两个方向的用例。
改完必须做变异:把门改坏,确认测试会红 —— 只会报绿的测试等于没有。

相关:`memory/commit-gate-chain-7-gates-2026-10-22.md`、`memory/retrieval-cost-audit-2026-10-22.md`、
`memory/skill-acceptance-gate-r2s-2026-10-22.md`、`memory/trace-eval-pipeline-4-metrics.md`