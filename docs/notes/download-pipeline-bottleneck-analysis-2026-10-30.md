# 下载管线各环节耗时分析 —— 瓶颈定位与提速

> **日期**：2026-10-30
> **范围**：`x-video-downloader` 从「拿到 m3u8」到「落盘 mp4」的全链路（批量课程路径 = 真实主路径）
> **触发**：用户提问「分析影响视频的产出的速度的环节在哪儿，进行改进和升级」
> **方法**：读代码 + 变异测试验证，不是拍脑袋排序。见 §5「哪些是实测、哪些是估算」

---

## 0. 一句话结论

**下载阶段的大头是网络传输本身（占全链路 95%+），但旧实现在传输路径上有两处纯浪费**：

1. 加密 HLS 的**每个分段都重新取同一把 AES key** —— 一条 200 段的课 = 200 次重复 HTTP 往返，
   而且这次往返**占着并发信号量的许可**（permit），等于把每段的关键路径拉长到 2 个 RTT。
2. **分段并发被限制在 4** —— 2026-10-12 就已 bench 出 8 是甜点（1.75×），但那次决策没在本 checkout 落地。

**解析阶段（resolve）是另一类问题**：整段串行，每条课节都要付出「Keychain 解密 cookie + 新建 TLS 连接 + 2 次 RTT」
的固定开销 —— 25 条课节 ≈ 15–20s **全花在还没开始下载之前**。

---

## 1. 管线分段

| # | 阶段 | 代码位置 | 并发性 | 单次成本 | 说明 |
|---|------|---------|--------|---------|------|
| 0 | 登录态预检 + 课程列表 | `fetch_course_collection_impl` | 一次 | 1–2s | 浏览器/API，固定开销 |
| 1 | **逐课节解析** | `resolve_items_serially` | **整段串行** | ~0.5–0.8s/课节 | Keychain 解密 + TLS 握手 + 2 RTT |
| 2 | 下载：取播放列表 | `M3u8Parser::fetch_and_parse` | — | 1 RTT | |
| 2 | 下载：取 AES key | `SegmentKeys::prefetch` | — | **旧：每段 1 RTT** | ✅ 已修 |
| 2 | **下载：分段传输** | `src/hls/downloader.rs` | 信号量 c=8 | **主耗时** | ✅ c 4→8 |
| 3 | 合并分段 | `src/hls/downloader.rs:407-424` | 串行 | 全文件 2× 磁盘 IO | 见 §3.1 |
| 4 | 清理临时目录 | `remove_dir_all` | — | | 见 §3.2 |

「解析」阶段指的是：从课节页 URL 拿到那个 `m3u8` 地址。这一步在**下载开始之前**，用户看到的是「转圈但没有速度」。

---

## 2. 改了什么

### 2.1 AES key 缓存（commit `44dbc77`）

**问题**：`#EXT-X-KEY` 定义的是一把**覆盖整个 key block** 的密钥。旧实现每个分段都去取一次。
更糟的是取 key 的 await **发生在信号量 permit 之内**，所以每段的关键路径 = key RTT + segment RTT。

**改法**：下载前按 key URL 去重预取（`SegmentKeys::prefetch`），分段只查缓存；未命中回落实时取。
预取是 **best-effort** —— key 服务抖动只会让分段退回旧行为，不会让整条列表失败。

**效果**：
- 加密流（= 小鹅通主路径）：每条流从「每段一次 key 请求」降到「每个不同 key URL 一次」。
- 延迟瓶颈的流（分段小、RTT 高）：分段阶段关键路径**从 2 个 RTT 降到 1 个 ≈ 2×**。
- 带宽瓶颈的流：基本无变化（key 那点字节量可忽略）。

**验证**：新测试 `test_aes_key_fetched_once_per_distinct_key_url` 用 mock TCP server 断言
「同一把 key 只取一次」。**变异验证过** —— 把缓存查表改成 `match None::<[u8;16]>` 后测试红：

```
assertion `left == right` failed: 同一把 key 只该取一次(旧实现每分段取一次 = 6 次)
  left: 7
 right: 1
```

### 2.2 分段并发 4→8（commit `44dbc77`）

2026-10-12 的 bench 实测：**c=4 → 491ms，c=8 → 281ms（1.75×）**，判定 8 为甜点。
那次决策只写在 session 文档里，**代码没落地**（当时在另一个没有 `.git` 的 checkout 里）。
本次同步改了三处配置：`HlsDownloaderConfig::default()` / `src/downloader/mod.rs` / `strategy_direct.rs`。

### 2.3 ego-lite cookie 解密缓存

**问题**：`decrypt_ego_lite_cookies_raw()` 每次调用都要 spawn `security` 子进程读 Keychain
（**实测 30–40ms**）+ PBKDF2 1003 轮 + 打开 SQLite 解密全部 cookie。
而它被调用的位置是**每条课节一次**（`try_api_fetch_lesson_m3u8` 内部）—— 25 课节 = 25 次 Keychain 访问，
25 次可能弹权限授权窗。

**改法**：进程内缓存原始解密结果，TTL 60s。缓存的是**原始条目列表**（不过滤 host）——
过滤是纯内存操作，真正贵的是解密那一步，所有 host 共享。

**为什么 TTL 而不是「进程期不变」**：用户重新扫码登录后旧 cookie 会在 TTL 内继续被用。
60s 足够覆盖一个批次的解析窗口，又不至于让「登录了但还是旧 cookie」持续到重启。

**失败不缓存**：失败通常是「ego-lite 没装 / 没登录过」，让用户登录后重试就该重新算。

### 2.4 小鹅通 API 共享 HTTP client

**问题**：每个调用点都 `reqwest::Client::builder().build()` —— 新建连接池，
于是每次调用都要重新 TLS 握手。批量解析 N 条课节 = N 次握手。

**改法**：模块级 `OnceLock<reqwest::Client>` 共享。连接可复用（keep-alive），
并发解析时还能真正复用同一条连接。

### 2.5 解析阶段三段式并发

**问题**：`resolve_items_serially` 整个 for-loop 串行。函数名和注释里的理由是
「CDP 同一 profile，必须串行」—— 但这个约束**只对 Drission 浏览器路径成立**，
API fast-path 是纯 HTTP，彼此无共享状态。

**改法**拆成三段：

1. **前置检查**（skip / 已下载 / 覆盖删旧文件）—— 纯本地 IO，串行最省事；
2. **API fast-path 并发**（`buffer_unordered(8)`）—— 纯 HTTP，可并发；
3. **Drission fallback 串行** —— 同一个浏览器 profile 不能并发开多个 Chromium 实例。

为此在 `TulingPythonResolver` 上拆出 `try_api_video_info()`：**只走 API、不碰浏览器**，
返回 `None` 表示「需要走 Drission fallback」。混在 `get_video_info` 里就区分不了这两种并发语义。

**结果顺序**：旧实现靠 `results.push()` 的执行顺序保证与 `state.items` 对齐。
改成按下标填 + 最后展平后，顺序由数据结构保证。新测试 `resolve_results_stay_aligned_with_items` 钉住这个契约。

### 2.6 解析阶段的真实耗时埋点（commit `5e98539` + 本次）

改动前**测不出**解析阶段有多慢：`trace_event("resolver_try", ..., 0, ...)` 的
`duration_ms` 是写死的 0，而 `eval_trace.py` 的 stall 判定是 `duration_ms >= 30_000`
⇒ resolver 这一步**永远**不可能被判成 stall —— 而 BUG-D 恰恰就是 resolver 链把会话挂住。

新增 `TraceStage`（`src/core/trace.rs`）：

| API | 行为 |
|-----|------|
| `TraceStage::start(step, ...)` | 立刻写 marker 事件（`duration_ms=0`, `output={"phase":"start"}`） |
| `TraceStage::without_marker(done_step, ...)` | 不写 marker，只写 done（聚合 stage 用） |
| `TraceStage::finish(output)` | 写 done 事件，`duration_ms` = **真实**耗时（类型内部用 `Instant` 算） |
| `TraceStage::discard()` | 放弃这次 stage，**一条事件都不写** |
| `Drop` 兜底 | 没 finish 就 drop → 写 done 并标 `status="abandoned"` |

**耗时由类型内部计算，调用点没有 `duration` 参数** —— 结构上就写不死 0
（类型签名挡住手滑，不靠 code review 记住）。

现在解析阶段的每个课节都会产出 `resolver_try_done`（真实耗时 + `backend: api|drission`），
下一次跑真实课程就能直接读出「25 条课节各花了多久、哪条卡住、API 命中多少」。

---

## 3. 刻意没改的（以及为什么）

### 3.1 HLS 合并阶段 —— 重写收益 < 2%

合并是**顺序**读每个分段再写到输出文件 = 全文件 2× 磁盘 IO。
1.3GB 的课在 NVMe 上约 1.3s；下载本身 26–130s。**占比 1–5%**。

> 曾经把「合并串行 + 全量读进 RAM」排在瓶颈第一位 —— 那是错的。
> 合并不是瓶颈，重写它（pwrite 直写、免临时目录）属于「投机性重构」，
> 收益抵不上引入的复杂度。

### 3.2 临时目录 —— 是空间问题不是速度问题

分段先落 `.tmp/` 再合并，等于磁盘占用翻倍（8.5GB 课程要 17GB）。
真要解决应该直接 pwrite 写输出文件 —— 但那要求预先知道每段字节数（加密后长度 ≠ 清单长度），
得引入偏移量簿记。**留作单独的鲁棒性任务**，不在速度任务里顺手改。

### 3.3 Drission 浏览器复用 —— 收益大、风险高

`DrissionClient::fetch` **每条课节都 launch 一次 Chromium 再 quit**。
真走浏览器路径（API fast-path 失败时）时，25 课节 = 25 次浏览器启动。

复用浏览器（launch 一次、开 N 个 tab）是结构性收益，但：
- `capture()` 已经在 `fetch()` 里被单独拆出来了（好事），改成「外部持有 browser」不算大改；
- 但要动 `DrissionClient` 的生命周期，且有「同一 profile 不能并发」的约束要重新验证。

**没在本次做**：主路径（小鹅通 `l_xxx` 直播回放）走的是 API fast-path，浏览器是 fallback。
把 fallback 路径优化好，等于优化一个不常走的分支。**列为待办**。

---

## 4. 测不出来的部分（度量盲区）

仓库有 NDJSON trace 管线（`src/core/trace.rs`）+ `scripts/eval_trace.py）。

**本次已经关掉的盲区**（commit `5e98539`）：

| 盲区 | 处置 |
|------|------|
| CLI 路径零埋点 | ✅ CLI 现在有 `cli_download_run` / `cli_resolve` / `cli_resolve_attempt` 分段耗时 |
| `resolver_try` 时长恒为 0 | ✅ 新增 `resolver_try_done`（真实耗时）；`resolver_try` 保留为开始标记 |
| 嵌套 session 互相覆盖 | ✅ session 栈 + 按 id 写回；batch session 不再被 eval 丢弃 |

**仍然存在的盲区**：

| 盲区 | 现状 |
|------|------|
| `avg_total_ms` 是死指标 | 读 `total_ms_proxy` 键，而该键在 `src/` 里从没被写过 → 恒为 0，报告里显示「⚪ 不可测」 |
| CLI 的叶子阶段没埋点 | manifest / segments / merge 在 `src/commands/download/**`，本轮未埋点 ⇒ CLI 侧只有 run 级聚合 |
| 课节池流程本身没有 session | 事件落进 `no-session/<date>.ndjson` fallback，不计入 success_rate 分母 |
| 机器上没有真实 trace 数据 | `~/.local/share/x_videos-dowloads-trace/` 是空的 |

**这意味着**：本次的解析阶段提速是**基于代码路径推导 + 单测**，
**没有**用真实课程跑出来的端到端数字支撑。下载阶段的两个改动有 bench 数字（2.2）和变异测试（2.1）。
但 2.6 的埋点落地后，**下一次跑真实课程就能拿到解析阶段的真实耗时** —— 这正是把
「估算」变成「实测」的入口。

`eval_trace.py` 里 `total_ms_proxy` 那个死指标是已知 open loop，本次**故意没动** ——
修它要先定义「一次下载的总耗时」到底怎么算（session 级 sum？单个 download_one？），那是另一个设计决策。

> **换埋点后必须重新锁基线**：改之前 resolver 阶段永远不可能被判成 stall，
> 改之后同样的运行会多出 `resolver_try_done` 带来的 stall。
> 基线与 current 必须来自同一套埋点版本，否则 `stall_count` 的 delta 是「测得更准」而不是「性能变差」。
> 重锁：`bash scripts/install_perf_baseline.sh`。

---

## 5. 哪些是实测、哪些是估算

**实测**：
- `security find-generic-password` = 30–40ms（2026-10-30，本机）
- 分段并发 c=4 → 491ms / c=8 → 281ms（2026-10-12 bench）
- key 取值次数：旧 7 → 新 1（变异验证：改坏后测试红 `left: 7, right: 1`）

**估算（基于代码路径推导，无真实课程端到端数字）**：
- 解析阶段每课节 ~0.5–0.8s（Keychain 40ms + TLS 握手 ~100–200ms + 2 RTT ~200–400ms）
- 合并阶段占全链路 1–5%
- 分段阶段在延迟瓶颈下 ≈ 2×（key RTT 从关键路径上移除）

---

## 6. 验证清单

- [x] `cargo test --bin x-video-downloader` —— 406 passed / 0 failed / 12 ignored
- [x] `pytest tests/test_eval_trace.py` —— 18 passed
- [x] `cargo clippy --all-targets -- -D warnings` —— 干净
- [x] `cargo fmt --check` —— 干净
- [x] 变异测试：key 缓存测试改坏后**确认红**（`left: 7, right: 1`）
- [x] 变异测试：cookie 缓存 3 条断言（命中不重算 / 过期重算 / 失败不缓存）
- [x] 变异测试：stall 白名单退回旧三元组 → 4 条红
- [x] 变异测试：`TraceStage::finish` 的 duration 写死 0 → trace stage 测试红
- [ ] **真实课程端到端复测** —— 需要已登录的 ego-lite profile + 一门真实课程（未做）

---

## 7. 相关文件

- `src/hls/downloader.rs` —— key 缓存、并发度
- `src/drission_client.rs` —— cookie 解密缓存 + `cached_with_ttl`
- `src/platform/tulingpython.rs` —— 共享 client、`try_api_video_info`
- `src/gui_dioxus/handlers.rs` —— 解析阶段三段式并发 + `resolver_try_done` 埋点
- `src/core/trace.rs` —— `TraceStage`、session 栈
- `scripts/eval_trace.py` —— 叶子步骤白名单
- `docs/handoff/sessions/2026-10-12-x-video-downloader-perf-baseline-locked-+-4-open-issues-resolved-+-commit-pending.md` —— 2.2 的原始决策记录
