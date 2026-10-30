# 2026-10-30 — review-eval 门禁 + revert_rate 指标 + Rust 边界补测

> 来源：头条视频《Open Computer Use》(7701973957590762047) 分析 + Anthropic
> 《How the Claude Code team does code review》7 层流程的 4 条「可选后续」全部落地。

## 本轮做完了什么

| # | 项 | 落地物 | 证据 |
|---|----|--------|------|
| 1 | **改审查 prompt 必须先跑分** | `scripts/review_eval.py` + 3 个 fixture + `scripts/review_eval_smoke.sh` + commit-msg 门禁 | eval recall **1.00 (3/3)**、0 误报；冒烟 11/11；真 prompt 变异 → 门禁 rc=1 → 还原（字节一致）→ rc=0 |
| 2 | **revert_rate 指标** | `scripts/revert_rate.py` + `scripts/revert_rate_smoke.sh` | 冒烟 10/10；本仓 30 天窗 55 commit / 0 回滚 |
| 3 | **合并队列语义** | `docs/dna/review-checklist.md` 新增「合并队列语义」段 | 写进清单（非第 6 个 Phase，守住该文件自己的「不要私自加 Phase」规矩） |
| 4 | **CI 边界场景补测** | 5 个 Rust 文件 + 15 条新测试 | `cargo test --bin x-video-downloader` **394 passed / 0 failed**；`cargo clippy --all-targets -- -D warnings` **rc=0** |

## 顺带修掉的 4 个真 bug（不是「缺测试」而已）

| 文件 | bug | 症状 |
|------|-----|------|
| `src/hls/crypto.rs` | `parse_iv` 按**字节**查长度、按**字符**切片 | 多字节 IV（emoji）切在字符中间 → **panic** |
| `src/hls/parser.rs` | `is_live` 写死 `false` | HLS 规范（RFC 8216 §4.3.3.1）：无 `#EXT-X-ENDLIST` 即直播 —— 这个字段**永远**是 false。`src/` 内 0 个消费者，改动安全 |
| `src/core/segment.rs` | `parse_ytdlp_progress` 没剥 `[download]` 前缀 | yt-dlp 写成 `[download]100%`（无空格）时百分号前那截是 `"[download]100"` → parse 失败 → **整行进度丢失** |
| `src/downloader/parallel.rs` | `calculate_segment_ranges` 不夹 `start` | total=10/segments=7/chunk=2 → start=12 > last=9 → 下游 `end-start+1` **下溢 panic**；另删死代码 `calculate_optimal_threads`（全仓 0 调用点），改成纯函数 `adjust_thread_count` |

## 三个「把检测器钉死」的坑（通用，值得记住）

1. **测试脚本自己假绿灯**：`check("a" "b" "c")` 里 `(` 在命令词位置是 **bash 语法错误**，
   整段空转仍 rc=0。本轮第一版冒烟脚本用了没定义的 `check`，报「未找到命令」却仍打
   `passed: 8 failed: 0` —— 与 `[[commit-gate-chain-7-gates-2026-10-22]]` 的「漏 `$` 整段空转」
   是同一个病。
2. **baseline 必须自检**：光比 prompt 哈希不够 —— 一份 recall 0.67 的 baseline 照样过哈希检查。
   门禁要同时判「哈希一致」「recall ≥ 阈值」「误报 == 0」。
3. **失败不许覆盖 baseline**：recall 不达标时写 `*.failed.json`，旧 baseline 原样保留。
   否则一次退化会被固化成新标准，门禁从此再也拦不住东西。

## 评测跑不通的真因：**轮数不够，不是 prompt 不行**

stream-json 抓下来看到：reviewer **第 13 轮已经推理出正确结论**（用 bash 演示了
`"VAR"` vs `"$VAR"` 的区别），但轮数耗尽前**没写出报告** —— 结论对、评测却记 0 分。

- 给了 Bash 它会花一半轮数做「考古 + 写 /tmp 复现」；非交互模式写文件要审批、被拒后再换招。
- 修法：`--tools "Read,Grep,Glob"`（diff 已内联，不需要它跑 git）+ `--max-turns 20`。
- 另外 `--setting-sources ""` 隔离全局 settings —— 不隔离时子 reviewer 会继承本机 anti-slop
  Stop hook，措辞被判「AI 味」后它花好几轮解释自己为什么改措辞 → 卡 max turns 超时
  （300s / 600s 各挂一次）。
- **教训**：评测记 0 分时，先确认被测对象是不是**没机会说话**。

## 门禁接线

- `scripts/git_hooks/commit-msg` 是**源**，`.git/hooks/commit-msg` 是安装副本 ——
  改完跑 `scripts/install_git_hooks.sh` 同步（已跑）。
- 逃生舱：commit message 含 `[skip-review-eval]`。
- 实测接线：过期 baseline → hook rc=1、拒绝信息打到 stderr、commit message **未被改写**
  （在 DID/VC trailer 注入之前就退出了）。

## 附带

- 头条视频《Open Computer Use》= `iFurySt/open-computer-use`（AX 树而非像素操作，
  9 个原生 MCP 工具）→ 本机已有 3 套逆向浏览器 MCP + 本项目不操作桌面 GUI → **暂不接入**，记 watch item。
- memory：新增 `review-eval-gate-2026-10-30` + `open-computer-use-video-2026-10-30`，MEMORY.md 加 2 行。

## 下一步

- 评测集目前 3 个 fixture，全部取自本仓库历史缺陷。下一个真事故应**当天补进 fixture**，
  否则 eval 集会长成「只测已知 bug」的静态清单 —— 这正是 Anthropic 说的 incidents-as-evals 的维护成本。
- `revert_rate.py` 目前只作为独立指标存在；要不要接进发版流水线（`--max-rate` 判 rc=2）
  还没定，取决于本机发版频率。
