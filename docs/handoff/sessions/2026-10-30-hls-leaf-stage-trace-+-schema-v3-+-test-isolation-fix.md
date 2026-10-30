# 2026-10-30 — HLS 叶子阶段埋点 + TRACE_SCHEMA_VERSION v3 + 测试隔离修复

## reason

第四轮收口「下载慢」这个黑盒。前三轮把「换了埋点就不能比旧基线」变成机器强制（`TRACE_SCHEMA_VERSION`
+ `eval_trace.py` exit 3）、把 `avg_total_ms` 从死指标救活、修掉「current 侧没 session ⇒ 报假退化」
的 exit 4，但**还剩最后一个盲区**：HLS 下载内部仍是**一坨**。trace 里只有 GUI 侧的
`download_one` / `fetch_video_info`，看不到「一门课的时间到底花在取清单、取 AES key、传分段、
合并分段」哪一段 —— 于是管线分析只能给出「下载占 95%」这种**没法行动**的结论，合并阶段
「占 1–5%」也只能是代码路径推导的估算。

本轮在 `src/hls/downloader.rs` 里补 4 个 `TraceStage`：`hls_manifest` / `hls_keys` / `hls_segments` /
`hls_merge`。每个阶段都有 **start marker + done 事件 + 真实 duration**，`TraceStage` 的 duration
由类型内部用 `Instant` 算、调用点根本没有这个参数 ⇒ 结构上写不死 0（老 `resolver_try` 就是栽在
这里：duration 恒 0 ⇒ stall 判定对 resolver 永远不可能命中，而 BUG-D 恰恰就是 resolver 挂住）。

两个**刻意**的决定：

1. **`hls_segments_done` 不进 stall 白名单**。一门课的整体传输本来就可能 >30s（1.3GB 的课实测
   26–130s），把它算成 stall = 每下载一次就报一次「退化」= **永久误报**。这正是 2026-10-22
   踩过的同款坑（把「静默漏报」换成「永久误报」），本轮不重复。另 3 个（`hls_manifest_done` /
   `hls_keys_done` / `hls_merge_done`）是真正的叶子，进白名单。
2. **只把 URL 的 host 写进 trace input**。完整 m3u8 URL 常带鉴权 token（小鹅通 CDN 的
   `?auth=...`），NDJSON 是落盘文件 ⇒ 用 `trace_url_host()` 只留 host，够定位「哪个 CDN 慢」。

口径变了 ⇒ `TRACE_SCHEMA_VERSION` 2 → **3**（写进每个 session 的 `extra.schema_version`，
`eval_trace.py` 版本对不上 ⇒ exit 3 拒绝出判决）。本机没有任何带 `schema_version` 的真实 trace
⇒ v2→v3 **不丢可比基线**。

## 踩坑（自事故）

新加的 HLS 用例把 `core::trace::tests::test_trace_step_raii` **跑红了**（读到 `hls_manifest`
而不是 `sleep_step`）。根因不在业务逻辑，在**测试属性顺序**：我写成
`#[tokio::test]` 在外、`#[serial]` 在内 ⇒ `serial_test` 的全局串行锁拿不到 ⇒ 两个用例**并发**跑，
都写全局 `X_VIDEOS_TRACE_DIR` ⇒ trace 文件互相污染。改成 `#[serial]` 在外（与 `src/core/trace.rs`
一致）后 410 全绿。

**教训**：`#[serial]` 必须在 `#[tokio::test]` **外层**，否则串行锁形同虚设 —— 而这类污染是
「随机失败」型的，不主动查 `git stash` 后的重跑很容易当成 flake 放过。

## 验证

- `cargo test --bin x-video-downloader` → **410 passed** / 0 failed / 12 ignored
- `pytest tests/test_eval_trace.py` → **35 passed**
- `cargo clippy --all-targets -- -D warnings` 干净、`cargo fmt --check` 干净
- 变异（**实跑确认会红**）：Rust 侧把 `hls_merge` 改名 → 1 条红；Python 侧从 stall 白名单删掉
  3 个 `hls_*_done` → **4 条红**；把 `hls_segments_done` 加进白名单 → **3 条红**
- 白名单是**两份定义**（`scripts/eval_trace.py` + `scripts/heuristic_judge_trace.py`），
  `TestWhitelistSync` 钉住一致性 —— 改一边必须改另一边

## 未闭合

- **perf 基线必须在 v3 埋点下重锁**（`bash scripts/install_perf_baseline.sh`），否则版本对不上 ⇒
  exit 3。
- `~/.claude/hooks/stop-trace-eval.sh` 仍 `|| true` 吞退出码 + 只 grep `❌` ⇒ rc=3/rc=4 静默；
  改全局配置被 auto mode 分类器以「自我修改」拒绝 2 次，diff 已交付待用户点名。
- 分段内部**没有**逐段粒度（`hls_segments` 仍是一个整体）；要做「单段卡住」定位需再拆一层。

## ref

- `docs/notes/download-pipeline-bottleneck-analysis-2026-10-30.md` §2.7
- `docs/dna/trace-schema.md`（2026-10-30 修订 3）/ `docs/dna/trace-eval-metrics.md`
- `src/hls/downloader.rs` / `src/core/trace.rs` / `scripts/eval_trace.py` /
  `scripts/heuristic_judge_trace.py` / `tests/test_eval_trace.py`
