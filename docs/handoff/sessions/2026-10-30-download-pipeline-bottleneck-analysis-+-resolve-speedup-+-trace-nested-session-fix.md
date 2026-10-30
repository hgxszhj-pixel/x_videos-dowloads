# 下载管线瓶颈分析 + 解析阶段提速 + trace 嵌套 session 修复

> 日期:2026-10-30
> 触发:用户「分析影响视频的产出的速度的环节在哪儿,进行改进和升级」
> 落地:`44dbc77`(HLS) + `ddae06e`(trace) + `10a2dc4`(resolve) + `272cb55`(eval_gate H4 修复)
> 分析文档:`docs/notes/download-pipeline-bottleneck-analysis-2026-10-30.md`

---

## 1. 结论(一句话)

下载阶段的大头是网络传输本身(95%+),但旧实现在传输路径上有两处纯浪费;
解析阶段则是**另一类**问题 —— 整段串行,25 条课节 ≈ 15–20s 全花在还没开始下载之前。

## 2. 落地了什么

### 2.1 下载阶段(commit `44dbc77`)

| 改动 | 效果 |
|------|------|
| AES key 按 key_url 去重预取(`SegmentKeys::prefetch`) | 加密流从「每分段取一次 key」降到「每个不同 key URL 一次」;**取 key 的 await 原本占着信号量 permit**,移出后每段关键路径从 2 个 RTT 降到 1 个 ≈ **2×**(延迟瓶颈的流) |
| 分段并发 4 → 8 | 2026-10-12 已 bench:c=4 → 491ms / c=8 → 281ms = **1.75×**;那次决策只写在 session 文档里,代码没落地,本次同步三处配置 |

### 2.2 解析阶段(commit `10a2dc4`)

- **cookie 解密缓存(TTL 60s)**:原来每条课节都 spawn `security` 读 Keychain(实测 30–40ms)+ PBKDF2 1003 轮,25 条 = 25 次 Keychain 访问(还可能弹授权窗)。缓存**原始条目列表**(host 过滤是纯内存操作,真正贵的是解密那步,所有 host 共享)。失败不缓存。
- **共享 HTTP client**:原来每个调用点 `Client::builder().build()` = 新建连接池 = 重新 TLS 握手,N 条课节 = N 次握手。
- **三段式并发**:旧实现整段串行,理由「CDP 同一 profile 必须串行」—— 但该约束**只对 Drission 浏览器路径成立**,API fast-path 是纯 HTTP。拆成 前置检查(串行)→ API fast-path 并发(`buffer_unordered(8)`)→ Drission fallback(串行)。为此拆出 `try_api_video_info()`(只走 API 不碰浏览器)。
- **结果顺序**:从 `results.push()` 的执行顺序保证 → 改为按下标填 + 展平,顺序由数据结构保证(新测试钉住)。

### 2.3 trace 管线(commit `ddae06e`)

- **嵌套 session 隔离**:旧实现只有一个全局 `SESSION_ID` + 一个全局文件句柄,batch session 被内层 item session 直接顶掉 ⇒ batch 文件只剩 `session_open` ⇒ eval 判「有 open 无 close」→ **整个 batch session 被丢弃**。改成 session 栈 + 按 id 写回。
- **`TraceStage`**:marker(开始,duration 恒 0)+ done(**真实**耗时)。旧代码 `trace_event("resolver_try", ..., 0, ...)` 把 duration 写死 0 ⇒ stall 判定(`>= 30_000`)对 resolver **永远不可能命中** —— 而 BUG-D 恰恰就是 resolver 链把会话挂住。**耗时由类型内部用 `Instant` 算,调用点没有 duration 参数** ⇒ 结构上写不死 0。
- **CLI 路径此前零 trace 事件** → 现在有 `cli_download_run` / `cli_resolve` / `cli_resolve_attempt` 分段耗时。
- **stall 白名单**:`eval_trace.py` + `heuristic_judge_trace.py` 改用叶子步骤白名单,避免聚合步骤与叶子步骤重复计数把 `stall_count` 虚高。

## 3. 刻意没改(以及为什么)

| 不改的东西 | 理由 |
|-----------|------|
| HLS 合并阶段 | 顺序读 + 写 = 全文件 2× 磁盘 IO;1.3GB 课在 NVMe 约 1.3s,下载本身 26–130s ⇒ **占比 1–5%**,重写收益 < 2%,属投机性重构 |
| 临时目录(分段先落盘再合并) | 是**空间**问题不是速度问题(8.5GB 课要 17GB);要解决得预先知道每段字节数(加密后长度 ≠ 清单长度)⇒ 引入偏移量簿记,留作单独鲁棒性任务 |
| Drission 浏览器复用 | 收益大(25 课节 = 25 次浏览器启动)但风险高(动 `DrissionClient` 生命周期 + 同 profile 约束要重验);主路径走 API fast-path,浏览器只是 fallback ⇒ **列为待办** |

> 曾经把「合并串行 + 全量读进 RAM」排在瓶颈第一位 —— 那是错的。

## 4. 验证

- `cargo test --bin x-video-downloader` —— **406 passed / 0 failed / 12 ignored**
- `pytest tests/test_eval_trace.py` —— **18 passed**
- `cargo clippy --all-targets -- -D warnings` —— 干净;`cargo fmt --check` —— 干净
- **变异测试(实跑确认会红)**:key 缓存改坏 → `left: 7, right: 1`;cookie 缓存改坏 → `left: 2, right: 1`;stall 白名单退回旧三元组 → 4 条红;`TraceStage::finish` 的 duration 写死 0 → trace stage 测试红
- **HEAD 树单独验过**:把工作区改回 HEAD 跑 clippy + test(401 passed)⇒ 每个 commit 都独立可编译(第一次 amend 就是因为这个)

## 5. 踩的坑(本轮)

1. **commit-msg 的 eval_gate 查的是**工作区**不是 staged** —— 第一次 commit 时 staged 的 trace.rs 没有 `#[expect(dead_code)]`、工作区有 ⇒ 门禁绿灯但**提交的树编译不过**(`without_marker`/`discard` 无人调用 → dead_code)。这正是 [[git-workflow]] H4「pre-commit 检查对象 = staged」那条规则。修法:amend 补上,并**把工作区改回 HEAD 实跑一遍 clippy** 验提交树本身。**门本身也已修**(`272cb55`):`eval_gate.py` 新增 `_git_unstaged_build_files()`,fmt+clippy 通过后若工作区有未暂存的 `.rs`/`Cargo.*`/`build.rs` 改动 ⇒ 报**警告**说明「验的是工作区,不是这次提交」(不拦 —— 门的三态是拦/警告/静默,硬拦会逼人 `--no-verify`,门就成装饰品);冒烟新增 `unstaged-build-file-warns` 用例,**变异验证实跑确认能红**(`_git_unstaged_build_files` 改 `return []` ⇒ 44/1 红,还原 45/0 绿)。
2. **commit 拆分受文件依赖图约束**:handlers.rs 同时用到 `try_api_video_info`(在 tulingpython.rs)和 `TraceStage`(在 trace.rs),而 trace.rs 的两个新方法又只有 handlers.rs 用 ⇒ 死循环依赖,只能 2 个 commit:`feat(trace)` 带 `#[expect(dead_code)]` 占位,下一个 commit 去掉。**不要为了"每个 commit 独立可编译"而过度拆**。
3. **嵌套 session 的 session 栈是全局的,不是 task-local 的** —— 并发 3 个 item 时普通 `trace_event()` 可能落进另一个 item 的文件。彻底解决要 `tokio::task_local!`,本轮没做(已在 `docs/dna/trace-schema.md` 标为遗留)。

## 6. 未闭合

- **真实课程端到端复测** —— 解析阶段的提速是**代码路径推导 + 单测**,没有真实课程的端到端数字。需要已登录的 ego-lite profile + 一门真实课程。
- **perf 基线要重锁** —— 埋点换了(`resolver_try` 从「duration 恒 0」变成「marker + 真实耗时的 done」),`stall_count` 口径变了。基线与 current 必须来自同一套埋点版本,否则 delta 是「测得更准」不是「性能变差」。
- **`avg_total_ms` 仍是死指标** —— 读 `total_ms_proxy` 而该 key 在 `src/` 里从没被写过。修它要先定义「一次下载的总耗时」怎么算。
- **CLI 的叶子阶段(manifest/segments/merge)没埋点** —— 在 `src/commands/download/**`,本轮未碰。

## 7. 相关文件

- `docs/notes/download-pipeline-bottleneck-analysis-2026-10-30.md` —— 完整分析(阶段耗时表、实测 vs 估算、度量盲区、刻意不改的三项)
- `src/hls/downloader.rs` / `src/drission_client.rs` / `src/platform/tulingpython.rs` / `src/gui_dioxus/handlers.rs` / `src/core/trace.rs`
- `docs/dna/trace-schema.md` —— step 清单 + 已知边界 + 变异验证表
- `docs/dna/trace-eval-metrics.md` —— 指标口径 + 白名单理由
