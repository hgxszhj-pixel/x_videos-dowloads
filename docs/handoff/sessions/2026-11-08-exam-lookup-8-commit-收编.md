# 2026-11-08 — exam-lookup 未提交成果收编（8 commit / 44 文件 / +10526 −190）

> 来源：用户「继续」—— 把上一轮攒下的 exam-lookup 成果跑自检后按依赖顺序入库。
> 仓库：`x_videos-dowloads`，分支 `gitbutler/workspace`。

## 本轮做完了什么

把工作区里攒了多轮、始终没进 git 的 exam-lookup 成果收编成 **8 个语义切分的 commit**，
顺序按**内部 import 依赖**排（先被依赖的先进），不是按功能重要性排。

| # | commit | 内容 | 规模 |
|---|--------|------|------|
| 1 | `94c19ec` | 插件系统 —— 热加载 + 崩溃隔离 + FSRS 复习 + 联网收编 | 6 文件 +1571 |
| 2 | `0c16e31` | 题库数据层 —— 格式契约 + 幂等落库 + 双哨兵 | 10 文件 +2928 −41 |
| 3 | `852cede` | 四级答案兜底链 + 原文证据包 | 9 文件 +1859 −59 |
| 4 | `7542f9f` | 外部答案源接入 + 裁判与基准工具 | 12 文件 +2472 |
| 5 | `cbf08dd` | 前端与 webui —— 排序/填空题型 + 内网共用 + 复习装载 | 3 文件 +1186 −76 |
| 6 | `bc88a31` | 收录两篇视频分析（50+ Agent 编排 / Anthropic 审查流程） | 2 文件 +444 |
| 7 | `1539156` | 技能覆盖表按 2026-10-27 trim 后的真实口径重算 | 1 文件 +26 −14 |
| 8 | `b7b1234` | 项目内加入 `download-agent-loop` skill | 1 文件 +40 |

合计 **44 文件 / +10526 −190**（`git diff --stat 94c19ec~1..b7b1234`）。

## 为什么按这个顺序切

内部 import 图决定：

```
plugins/  (无内部依赖)      → 1
kb_db / kb_docs_* / schema  → 2   (被 3/4/5 引)
answer_chain / answer_evidence / ask_ctx / llm_claude / kb_answer_resolve
                             → 3
kb_answer_kaoshibao / kb_answer_web / kb_kaoshibao_api / doc_adjudicate
apply_adjudication / write_evidence / compare_exam_txt / bench_*
                             → 4   (依赖 2/3 的数据结构)
solve.py / sync_banks.py / webui.py / ask.html / README.md
                             → 5   (依赖 1–4 全部)
```

先落数据层再落调用层 ⇒ **任意一个 commit checkout 出来都能跑**，不存在「中间态 import 不上」。

## 提交前跑的自检（9 条全绿）

入库前把每个模块自己的 selftest 跑了一遍，作为「这些代码确实活着」的证据：

```
plugins selftest        OK      kb_answer_kaoshibao   OK
kb_db.py selftest       OK      kb_mark_verified      OK
answer_chain selftest   OK      kb_review_queue       OK
ask_ctx selftest        OK      kb_kaoshibao_api      OK
webui selftest          OK
```

**自事故（值得记）**：第一次跑这批自检，**9 条里 8 条报 FAIL**，看着像代码全坏了 —— 其实是我的循环
把命令行拼错了：

```bash
# 错：python3 -m kb_db.py selftest  → "Error while finding module specification for 'kb_db.py'"
python3 -m $c          # $c = "kb_db.py selftest"
# 对：python3 kb_db.py selftest
python3 -m plugins selftest   # 包才用 -m，脚本不用
```

教训：**「全红」先怀疑自己的调用形式**，尤其是 `-m` + 带 `.py` 的模块名这种明显非法组合。
真 bug 不会 8/9 同时以「找不到模块」的方式失败。

## 门禁

8 个 commit 全部过 commit-msg 的 7 道门：

- **Intent Lock** —— 每个 commit 都带 `Intent: <一句话>`
- **review_eval** —— recall **1.00**（3/3 fixture 全检出，0 误报），每次都跑
- handoff_size_guard / openloops_schema / Ref-trace / memory_prompt / DID/VC —— 均通过
- **eval_gate** —— 本轮无 staged `*.rs`，按设计跳过（不是静默跳过）

## 收编进 git 之后还留在库外的

| 项 | 状态 |
|----|------|
| `docs.sqlite` 及题库数据 | 仍是本地数据，**没进 git**（体积 + 本地内容） |
| `exam-lookup/plugins/srs_golden.json` | **进了 git** —— FSRS-6 的 7 条黄金向量，是行为契约不是数据 |
| `~/Downloads/Orca-1.4.250.app.bak` | 739 MB 回滚副本，在库外，等用户决定删否 |

## 诚实边界

- `srs_golden.json` 里的 FSRS-6 参数是**移植**来的（从 `py-fsrs` 的实现反推成纯 Python），
  7 条黄金向量能锁住「和原实现一致」，但**不等于**原库全部用例覆盖。
- 外部答案源（考试宝等）的在线行为**本轮没有重跑**（自检是离线的），线上风控策略变没变需要实测。
- 本仓分支 `gitbutler/workspace` **仍未推到远端** —— 本仓没有配 fork remote（与 Tendedero 不同仓）。
