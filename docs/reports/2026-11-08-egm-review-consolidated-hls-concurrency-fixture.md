# Consolidated Review Report — EGM Cross-Stage Synthesis

## 1. Header

**Target document:** `/Users/hgxszhj/x_videos-dowloads/.dev/docs/egm-review-fixture.md` (lines 1–82)
**Synthesis date:** 2026-11-08
**Inputs:** 5 stage reports (below) — no new review was performed; every finding traces to a source report.

| Stage | Report file | Reviewer lenses |
|---|---|---|
| S5 — Initial Zoom Out | `zoom_out_report.md` | `egm-zoom-out`, `egm-inventive-catalyst` |
| S6 — Comprehension | `comprehension_report.md` | `egm-comprehension` |
| S7 — Critic | `critic_report.md` | `egm-design`, `egm-structural`, `egm-style` |
| S8 — Intermediate Zoom Out | `intermediate_zoom_out_report.md` | `egm-zoom-out` |
| S9 — Implementation Readiness | `implementation_readiness_report.md` | `egm-implementation-readiness`, `egm-codebase-details`, `egm-style` |

**Lenses run:** 9 distinct (2 + 1 + 3 + 1 + 3, with `egm-style` serving both S7 and S9).

**Overall verdict:** 🔴 **NOT APPROVED — Blocker.** All five stages returned a not-approved verdict independently; the document's factual baseline is stale, its scope boundary is contradicted by its own proposal *and* its own plan, the feature's core policy is prose rather than a rule, and the success criterion names a file that does not exist and a metric no tool in this repository computes.

---

## 2. Consolidated Findings

31 findings survive deduplication across 107 raw findings: **16 Blocker · 8 Major · 7 Nit**.
Severity is the highest severity any source assigned to that defect (see §5 for the three places this changed the outcome).

### 🔴 Blockers

---

#### C-01 — Current-state baseline is stale: the concurrency cap is 8, not 4

- **Severity:** 🔴 Blocker
- **Sources:** S5/egm-zoom-out + egm-inventive-catalyst (S5-1) · S7/egm-design + egm-structural + egm-style (S7-1) · S9/egm-codebase-details + egm-style + egm-implementation-readiness (S9-M-4) · S9 context finding (S9-C-1, promoted)
- **Scope:** fixture line 13; everything calibrated against it — lines 6, 8, 16–19, 66–67; plan item 2 (line 75)

Line 13 states the current segment concurrency cap is **4**. `HlsDownloaderConfig::default().max_concurrent` is **8** (`src/hls/downloader.rs:58`), asserted by `test_default_config` (`:798`); both production call sites hard-code `.max_concurrent(8)` (`src/downloader/mod.rs:124`, `src/commands/download/strategy_direct.rs:102`). The only `max_concurrent: 4` occurrences in the repo are test fixtures. The value was raised by commit `44dbc77` ("perf(hls): AES key 缓存 + 分段并发 4→8") on 2026-10-12, a benched decision whose comment ("c=8 是 sweet spot") the document silently overturns. The document also contradicts itself: its own measurement uses concurrency **8** as the baseline (line 18). Every derived number is affected — the headroom argument reads as 4× when it is 2×, and the ≥15 % criterion has no baseline (against 4 or against 8?).

**Required action:** Correct 现状 to 8; cite `44dbc77` / the 2026-10-12 bench decision as the State of the Art; re-derive every dependent number; re-anchor the ≥15 % target to c=8.

---

#### C-02 — The scope boundary ("one file, no config-format change") contradicts the proposal *and* the change list, with no precedence rule

- **Severity:** 🔴 Blocker
- **Sources:** S5/egm-zoom-out + egm-inventive-catalyst (S5-2) · S6/egm-comprehension (S6-F1, S6-D1) · S7/egm-design + egm-structural + egm-style (S7-3) · S8/egm-zoom-out (S8-Z8-04) · S9/egm-implementation-readiness + egm-codebase-details + egm-style (S9-B-1)
- **Scope:** fixture lines 58–62 (`## 范围边界`) vs. lines 25, 40–41, 48 (`## 方案概述`) vs. lines 71–78 (`### 改动清单`)

Line 60 promises "只改 `src/hls/downloader.rs` 一个文件；不改 CLI、不改 GUI、不**不改配置格式**". The proposal violates this on every axis — line 25 adds `src/hls/concurrency_guard.rs`, line 48 adds `src/hls/retry.rs`, lines 40–41 add `HlsPolicyConfig` plus per-site overrides in a JSON profile file (which *is* a config-format change), and the plan adds `src/config.rs` and `src/core/trace.rs` (five files total). Nothing establishes which statement governs. Because the scope boundary is exactly what a Phase-5 write-allowlist gate is derived from, the ambiguity makes the real blast radius unknowable and the gate unenforceable. S6's reading is the sharpest: the plan is the artifact an implementer works from, so a plan that overrides the spec's scope without saying so is a blocking ambiguity, not a stylistic one.

**Required action:** Rewrite `范围边界` as an accurate changed-files table derived from the change list (the change list is the internally coherent one), state explicitly that the config format does change, and give an explicit precedence rule between the two sections.

---

#### C-03 — Under the stated scope the feature is inoperative: both production call sites hard-code `.max_concurrent(8)`

- **Severity:** 🔴 Blocker
- **Sources:** S7/egm-design + egm-structural + egm-style (S7-9) · S8/egm-zoom-out (S8-Z8-04)
- **Scope:** fixture lines 43–44 vs. line 60; `src/downloader/mod.rs:124`, `src/commands/download/strategy_direct.rs:102`

S5 and S7 found the one-file/five-file contradiction; S8 states the consequence neither stated plainly: **there is no implementable path.** Both production call sites hard-code the value — `src/downloader/mod.rs:124` `.max_concurrent(8)` and `src/commands/download/strategy_direct.rs:102` `.max_concurrent(8).retry_count(3)`. Touching the call sites violates the scope boundary and "不改 CLI"; staying inside `src/hls/downloader.rs` leaves the shipped value at 8 and the policy inert. S8 additionally establishes (Coverage Gaps #7) that the GUI reaches HLS *through* `src/downloader/mod.rs` (`:1453` `.with_strategy(DownloadStrategy::Hls)`), so "不改 GUI" as a file constraint does not mean "no GUI behaviour change" — the GUI's HLS concurrency is set at `src/downloader/mod.rs:124`.

**Required action:** Put `src/downloader/mod.rs` and `src/commands/download/strategy_direct.rs` in the change list, state the precedence between `max_concurrent` and the new policy (e.g. `max_concurrent` = ceiling, policy may only tune below it), and rename the API so ownership is visible.

---

#### C-04 — The policy — the entire substance of the feature — is unimplementable as written

- **Severity:** 🔴 Blocker
- **Sources:** S5/egm-zoom-out + egm-inventive-catalyst (S5-6) · S6/egm-comprehension (S6-F3, S6-F4) · S7/egm-structural + egm-style (S7-20), S7/egm-design + egm-structural + egm-style (S7-10), S7/egm-structural (S7-32) · S9/egm-implementation-readiness + egm-codebase-details + egm-style (S9-B-7)
- **Scope:** fixture lines 25–44 (`### 1. 并发策略`) and plan items 1–2 (lines 73–75)

Two sub-parts, both fatal:
**(a) Contract.** `base` and `ceiling` have no units, defaults, valid ranges, clamp rule, or stated relationship to the existing `HlsDownloaderConfig::max_concurrent` (8). `LinearPolicy` ("按分片数线性放大") has no formula — slope, offset, rounding, clamp points, and behaviour at `segment_count` 0 or 1 are implementer choices that decide whether the feature hits its own ≥15 % target. No default policy is named, no construction path for `Arc<dyn ConcurrencyPolicy>` is given, no floor is stated (a returned `0` deadlocks the lesson: `Semaphore::new(0)` blocks, proven by `test_semaphore_zero_permits_blocks`, `src/hls/downloader.rs:854`), and the struct carries no derives.
**(b) Feedback channel.** `AdaptivePolicy` ("按观测 RTT 反馈调整") has no rule at all: no threshold, step size, hysteresis, definition of which RTT (segment fetch? manifest fetch? key fetch?), window, or aggregation. The source of `observed_rtt_ms` is **structurally undetermined**: the limit is computed "在每个课节开始时" (line 44), when that lesson has produced zero RTT samples — so the value can only come from a previous lesson, requiring cross-lesson state the plan places nowhere. `fn next_limit(&self, …)` behind `Arc<dyn ConcurrencyPolicy>` offers no mutable channel, so the implementer must independently choose interior mutability or change a public trait signature the plan does not authorise. `observed_rtt_ms == 0` (the first lesson of every course) is undefined. The limit's *scope* is also unstated: the semaphore is created per playlist (`src/hls/downloader.rs:314`) while a second per-site concurrency axis already exists (`WebsiteProfile.max_threads` via `src/downloader/parallel.rs`) — the two can multiply.

**Required action:** Give both formulas with worked examples against the document's own data points (30 / 90 / 160 segments, unknown RTT); name the RTT measurement point, its window, its storage, and the state owner; assign clamp ownership; specify defaults, ranges, and a floor of 1 for `base`/`ceiling`; state which policy is the default and how config selects policy identity; state the limit's scope and how it composes with the existing per-site thread axis.

---

#### C-05 — The retry design is a third, unwireable implementation whose 4xx rule regresses shipped 429 / Retry-After handling

- **Severity:** 🔴 Blocker
- **Sources:** S5/egm-zoom-out + egm-inventive-catalyst (S5-3), S5/egm-inventive-catalyst (S5-10) · S6/egm-comprehension (S6-F7, S6-D3) · S7/egm-design + egm-structural + egm-style (S7-7), S7/egm-design + egm-structural + egm-style (S7-4, sub-point) · S9/egm-implementation-readiness + egm-codebase-details + egm-style (S9-B-5), S9/egm-codebase-details (S9-N-2)
- **Scope:** fixture lines 46–50 (`### 2. 重试策略`), plan item 3 (line 76), and lines 13–14 (`## 现状`)

The codebase already ships **two** retry implementations, and the plan names neither. `HlsDownloader::download_segment_with_retry` (`src/hls/downloader.rs:478`) implements every element the proposal describes and more: `retry_count` (10), `base_delay * 2^attempt` capped at `retry_max_delay_ms` (20 s), `is_retryable_error()` for the 4xx-vs-network split, `Retry-After` taking priority over exponential backoff (`:518`), a timeout branch, a cancel flag, and `failed_count` accounting. `core::segment::download_segment_with_retry` (`src/core/segment.rs:99`) is the second, parallel implementation. The proposed `RetryPolicy { max_attempts, base_delay_ms, multiplier }` is the same mechanism renamed, with the `Retry-After` branch and the max-delay cap dropped, and **no stated consumer** — no plan item touches either existing loop. Worse, "HTTP 4xx 直接失败" makes **429** fatal: 429 is a 4xx and is precisely the status the code retries today. The proposal's core action is to *raise* concurrency — the change most likely to induce 429 — and then converts the rate-limit response into an unrecoverable lesson failure. The policy is also under-specified: field types, defaults for `max_attempts` / `base_delay_ms` / `multiplier`, attempt indexing (0-based or 1-based), a maximum-delay cap, jitter, and the error taxonomy are all unstated.

**Required action:** Delete `src/hls/retry.rs`. Name the exact call site; map `RetryPolicy` fields onto the existing `retry_count` / `retry_base_delay_ms` / `retry_max_delay_ms` with defaults; keep the 429/503 `Retry-After` branch and the max-delay cap; fix the types, attempt indexing, cap and jitter policy; publish the error taxonomy; and state the intended 4xx/429 semantics explicitly. If the error classifier is rewritten, rewrite `is_retryable_error` **in place** by HTTP status (see §5-3), not beside it in a new file.

---

#### C-06 — The per-site config item targets the wrong module, names a file that does not exist, duplicates the existing `WebsiteProfile` mechanism, and breaks serde back-compat

- **Severity:** 🔴 Blocker
- **Sources:** S5/egm-zoom-out + egm-inventive-catalyst (S5-5) · S6/egm-comprehension (S6-F6) · S7/egm-design + egm-structural + egm-style (S7-8) · S9/egm-implementation-readiness + egm-codebase-details + egm-style (S9-B-4)
- **Scope:** fixture lines 40–41, line 60, plan item 4 (line 77)

Four defects in one line. **(1) Wrong module** — `src/config.rs` is the TOML `AppConfig` with no per-site concept; per-site config lives in `src/website_profile.rs` (`WebsiteProfile` `:184`, `WebsiteProfileStore` `:479`, `get_profile_store()` `:615`, `get_recommended_config()` `:635`). **(2) Non-existent filename** — the persisted artifact is `website_profiles.json` (plural, `:609`, `:625`); no `website_profile.json` exists anywhere. **(3) The mechanism already exists** — `WebsiteProfile` already carries `adaptive_threads`, `min_threads` / `max_threads`, `speed_threshold_bps`, `record_success` / `record_failure` (failure halves `max_threads`, floor 2), `recommended_config()`, and `recommended_segments_for_size()` (already size-adaptive: <50 MB→1, 50–500 MB→4, >500 MB→max). `docs/plans/performance-improvement-plan.md` already adjudicated this exact shape and listed "与现有 website_profile 重复" as a *reason to reject* it. **(4) Backward compatibility broken** — `WebsiteProfile` derives `Deserialize` with **no `#[serde(default)]`** on its fields, so adding a required field breaks deserialization of every existing `website_profiles.json`; the codebase already does this correctly for `AppConfig.concurrent_downloads` (`src/config.rs:43`).

**Required action:** Retarget item 4 to `src/website_profile.rs` and the real `website_profiles.json`; publish the JSON schema, defaults, merge semantics and policy discriminator; add serde defaults so existing profile files still deserialize; and state explicitly whether `WebsiteProfile` is reused or why it cannot be.

---

#### C-07 — The change list is not an enforceable write allowlist: at least ten required paths are missing

- **Severity:** 🔴 Blocker
- **Sources:** S7/egm-design + egm-structural (S7-17) · S8/egm-zoom-out (S8-Z8-08) · S9/egm-implementation-readiness + egm-codebase-details + egm-style (S9-B-2), S9/egm-codebase-details + egm-style + egm-implementation-readiness (S9-M-2), S9/egm-implementation-readiness (S9-N-3)
- **Scope:** fixture lines 71–78 (`### 改动清单`)

Missing from the list: `src/hls/mod.rs` (currently `mod crypto; mod downloader; mod parser;` — the two new modules need `mod` declarations *and* a `pub use` for the public trait); the two production call sites `src/downloader/mod.rs:124` and `src/commands/download/strategy_direct.rs:102`; `src/website_profile.rs`; `src/core/segment.rs` (the existing retry path); `benches/hls_concurrency.rs` (which inlines the whole HLS tree via `#[path = "../src/hls/mod.rs"]` and stubs `core::trace::TraceStage` with the comment "那边改了 API,这里要同步(否则 bench 编译不过)"); `docs/dna/trace-schema.md`; `scripts/eval_trace.py` + `tests/test_eval_trace.py`; the trace test at `src/hls/downloader.rs:1227`; and `src/hls/crypto.rs` if the AES path is touched. **The build-breaking consequence:** without the `mod` declarations the two new files sit on disk uncompiled — `cargo build` stays green and the new policy and retry code are silently dead, surfacing only as a feature that does nothing. Items 1 and 3 say 新增; items 2, 4 and 5 are bare paths with no verb, so it is unstated whether they are created or modified.

**Required action:** Replace the numbered list with a table of `path → what changes → why it is in scope`, mark every row Create / Modify / Delete, and mark every other path read-only. State the intended module visibility and the exact `pub use` list.

---

#### C-08 — The test plan cannot fail: it repeats an existing ceiling test, names no assertion, and cannot distinguish "the limit was computed" from "the limit is in effect"

- **Severity:** 🔴 Blocker
- **Sources:** S5/egm-zoom-out (S5-9) · S6/egm-comprehension (S6-D4) · S7/egm-design (S7-21), S7/egm-style (S7-29) · S8/egm-zoom-out (S8-Z8-10) · S9/egm-implementation-readiness + egm-codebase-details + egm-style (S9-B-8), S9/egm-codebase-details + egm-style (S9-N-1)
- **Scope:** fixture lines 80–82 (`### 测试计划`)

The entire test plan is one sentence: "用 mock HTTP server 起 8 条目课节，验证并发上限确实生效". It names no test, no file, no observed quantity, no bound, and no assertion; the mock server is unnamed; the fixture is ambiguous — "8 条目课节" mixes 条目 / 课节 / 分片 and could mean 8 segments in one lesson, 8 lessons, or 8 playlist entries. **8 is not a discriminating value**: the current default cap is already 8 and `base`/`ceiling` are unspecified, so an 8-segment fixture cannot make the cap bite nor show the limit varying with segment count. The plan also restates `test_max_concurrent_e2e_mock_server` (`src/hls/downloader.rs:868`), which already asserts server-observed peak concurrency ≤ `max_concurrent` (and `test_semaphore_bounds_concurrency` at `:810` covers the same ground). S8 supplies the decisive point: **a ceiling test cannot detect the failure mode that killed the previous implementation** — a controller that computes a per-lesson limit and never applies it to the semaphore still passes `peak ≤ max_concurrent`. A mock server is also not bandwidth-bound, so it is structurally incapable of validating the ≥15 % wall-clock goal.

**Required action:** Replace with named cases carrying fixture shape, observed quantity, assertion and expected limit: a **policy unit test** (limits for 30 / 90 / 160 segments, clamped to ≥1 and ≤ `ceiling`) plus **equality** assertions in the e2e test (observed peak *equals* the computed limit; two lessons with different segment counts receive different peaks). Keep the existing ceiling test unchanged as a regression guard. State the test placement and what is *new* relative to `test_max_concurrent_e2e_mock_server`.

---

#### C-09 — The success criterion is not executable: the cited baseline file does not exist, and the repo's tooling computes no such metric

- **Severity:** 🔴 Blocker
- **Sources:** S5/egm-zoom-out + egm-inventive-catalyst (S5-8) · S6/egm-comprehension (S6-F10, S6-F12) · S7/egm-design + egm-structural + egm-style (S7-5), S7/egm-design + egm-structural + egm-style (S7-12), S7/egm-structural (S7-31) · S8/egm-zoom-out (S8-Z8-05, S8-Z8-12) · S9/egm-codebase-details + egm-implementation-readiness + egm-style (S9-M-6) · S9 context finding (S9-C-2, promoted)
- **Scope:** fixture lines 8, 64–67 (`## 成功判据`)

Three compounding failures. **(a) The cited baseline does not exist.** `docs/perf-baseline.md` is absent from the repo, from `HEAD`, and from every branch's history; S8 additionally establishes *why* — it was a promised deliverable of the 2026-10-12 baseline session that was never committed (`git log --all -- 'docs/perf-baseline.md'` is empty). The real machinery is `scripts/install_perf_baseline.sh` (installs an NDJSON trace directory under `~/.local/share/x_videos-dowloads-trace/<label>/`) plus `scripts/eval_trace.py --baseline <label> --current <label>`, which **refuses to issue a verdict (exit 3)** when baseline and current `schema_version` differ. The current `TRACE_SCHEMA_VERSION` is 3 (2026-10-30, HLS leaf stages) while the locked baseline dates from 2026-10-12 (v2) — a schema skew sits behind the comparison. The reference is also a bare string where every other source path is a relative Markdown link, so from `.dev/docs/` it is ambiguous between repo-root `docs/` and a sibling `.dev/docs/docs/`.
**(b) The primary metric has no instrument.** "wall-clock 中位数下降 ≥ 15%" names a quantity **no tool in this repo computes**. `scripts/eval_trace.py` produces `avg_total_ms` — an *average* over sessions of a per-session proxy (`session_close.output["total_ms_proxy"]`) — plus stall counts and a schema check. There is no median in the script and no course-level wall-clock quantity at all. The median's population is unstated, there is no run count and no dispersion requirement, and "同一门课" and "中位数" contradict each other in one sentence.
**(c) Two metrics, no precedence.** The criterion requires both the ≥15 % median and "`hls_segments_done` 的总时长下降"; the second has no threshold and neither is declared authoritative.

**Required action:** Name the real baseline artifact, its label and the script invocation; freeze a schema-v3 baseline **before** any code change so before/after is like-for-like; define the metric as something the tooling produces (or instrument course-level wall-clock and accept a baseline re-lock); state N runs, the time window, alternating arm order, and the defined median; give the second metric a threshold and a precedence rule; and cite `docs/perf-baseline.md` only as the lost 2026-10-12 artifact — or drop it.

---

#### C-10 — No global concurrency budget: aggregate concurrency to one host is unbounded, `ceiling` has no derivation, and there is no kill-switch

- **Severity:** 🔴 Blocker
- **Sources:** S5/egm-zoom-out + egm-inventive-catalyst (S5-11) · S6/egm-comprehension (S6-F2) · S7/egm-design + egm-structural + egm-style (S7-4 sub-point, S7-16) · S8/egm-zoom-out (S8-Z8-09)
- **Scope:** fixture lines 43–44, 58–62; `src/hls/downloader.rs:314`, `src/config.rs:85`, `src/gui_dioxus/handlers.rs:1344`, `src/http_client.rs:19`

The document's framing — "所有课节用同一套并发上限" — treats the per-lesson cap as if it were global. It is not: the per-lesson semaphore is created fresh per playlist (`src/hls/downloader.rs:314`), and the GUI batch path wraps item downloads in an **outer** semaphore sized by `cfg.concurrent_downloads` (default 3, `src/config.rs:85`, consumed at `src/gui_dioxus/handlers.rs:1344`). Aggregate in-flight requests to one CDN = lessons-in-flight × per-lesson cap = 3 × 8 today, 3 × 16 under this plan. The same codebase encodes the opposite concern elsewhere: `WebsiteProfile` pins `max_threads = 1` for some domains and flags Cloudflare domains `thread_limit: true`, and the repo's own plan records "CDN 通常限制 6-12 并发连接" (`performance-improvement-plan.md:77`). `pool_max_idle_per_host(64)` (`src/http_client.rs:19`) is an idle-keep-alive setting, **not** a concurrency cap. `ceiling` has no value and no derivation. No global budget, no per-host fairness, no kill-switch, and the repo's own DoS-risk record (`performance-improvement-plan.md` Phase 5: "引入 DoS 攻击面") is not engaged. S8 additionally records that raising caps in this repo has already produced a panic once: `src/downloader/parallel.rs:294` carries a latent assumption `segments ≤ 8` ("一旦某个网站特征把 max_threads 提到 >8，这里会**越界 panic**"). Because a global budget must live in the GUI batch path, the "不改 GUI" clause and the missing global budget are mutually exclusive.

**Required action:** State the aggregate-concurrency product (lessons-in-flight × per-lesson cap ≤ host budget), say explicitly whether `ceiling` is per-lesson or global, derive its value from the per-domain thread-limit knowledge, treat 429 / `Retry-After` as a down-pressure signal on the budget, give the change a single revertible kill-switch, and widen the scope to the GUI batch path if a global budget is required.

---

#### C-11 — No `备选方案` section, and the document's own data already meets the ≥15 % goal with a constant cap

- **Severity:** 🔴 Blocker
- **Sources:** S5/egm-zoom-out + egm-inventive-catalyst (S5-4) · S6/egm-comprehension (S6-F9) · S7/egm-design + egm-structural + egm-style (S7-2), S7/egm-style (S7-14)
- **Scope:** whole document (the section would sit between line 21 and line 58); evidence at lines 16–19

The Feature Spec Standard states that a spec "fails this standard if it proposes a single path without proving that other viable designs were considered and rejected for specific, documented reasons". The document proposes one path (new controller + trait + two implementations + a new per-site config layer) and evaluates no alternatives — an explicit fail condition. The omission is material because the alternatives are not hypothetical: they exist in this repo. The only evidence offered is a *uniform-cap* experiment (8 → 6m12s, 16 → 4m48s, 23 % faster) which already exceeds the ≥15 % target with a **constant**, and therefore isolates no per-lesson adaptive increment; there is no segment-count-vs-duration distribution to motivate adaptivity. The document never explains what the adaptive policy buys over simply fixing the cap at 16. The connection pool was already provisioned for 16+ (`.pool_max_idle_per_host(64) // 支持 16+ 并行分段 (翻倍)`).

**Required action:** Add an Alternatives section confronting at least four alternatives, starting with the fixed 8→16 bump that the document's own data supports; either record concrete rejection reasons or adopt the alternative. If a constant 16 already satisfies the goal, the 立项 rationale for the entire `ConcurrencyPolicy` abstraction needs rewriting.

---

#### C-12 — The causal premise is contradicted by the repo's own mutation-verified analysis of the same pipeline, and the per-lesson evidence does not exist

- **Severity:** 🔴 Blocker
- **Sources:** S7/egm-design + egm-style (S7-13), S7/egm-structural + egm-style (S7-28) · S8/egm-zoom-out (S8-Z8-03)
- **Scope:** fixture lines 5–8, 16–19, line 39

`docs/notes/download-pipeline-bottleneck-analysis-2026-10-30.md` — written 9 days before the document's own measurement date — is a full-chain analysis of *this exact pipeline*, explicitly produced by "读代码 + 变异测试验证，不是拍脑袋排序", and carrying a §5 titled "哪些是实测、哪些是估算". Its §0 conclusion: the bulk of download time is network transfer itself (95 %+ of the whole chain), and the two pure wastes on the transport path were (1) re-fetching the same AES key per segment and (2) segment concurrency capped at 4 — noting that 8 was benched as the sweet spot on 2026-10-12 and that decision had not landed in that checkout. Its stage table lists 分段传输 with 并发性 = "信号量 c=8", status "✅ c 4→8". The repo's most recent rigorous analysis of the same pipeline therefore found a **constant** cap deficit, not a per-lesson one. Independently, the document asserts "实测长尾课节（分片数多、单片小）明显慢于平均" but its measurement block contains only course-level wall-clock totals and no per-lesson distribution; nothing establishes that segment count is the right predictor, that the relationship is linear rather than sqrt/log, or that the "长尾" claim holds. The parenthetical also mixes two orthogonal properties ("many segments" and "small segments") that imply opposite mechanisms.

**Required action:** Show the per-lesson segment-count-vs-duration distribution that motivates per-lesson adaptivity and reconcile it with the 2026-10-30 analysis, or accept the constant-cap intervention the repo's own analysis supports. State the mechanism (segment count → queue depth → latency hiding → throughput), give a counter-example check for the small-segment case, disclose the mock-vs-real conditions of any bench relied on, and split the parenthetical into two named properties or drop the second.

---

#### C-13 — The repo already built *and deleted* a non-functional adaptive HLS concurrency mechanism; the proposal re-proposes it without citing the deletion

- **Severity:** 🔴 Blocker
- **Sources:** S8/egm-zoom-out (S8-Z8-01)
- **Scope:** fixture lines 28–44, 73–75

Commit `8c8711c` — *"refactor(hls): remove non-functional adaptive concurrency sampler"* (2026-07-11, `src/hls/downloader.rs`, 13 insertions / 119 deletions) — removed `HlsDownloaderConfig.adaptive_concurrency` (bool, default `true`), `min_concurrent`, `speed_threshold_bps`, a background sampler task, a `current_optimal AtomicUsize`, and the `downloaded` / `total_size` atomic counters. The commit message states the reason verbatim: the sampler was "purely logging theater — 'downloaded' atomic counter was never …". The proposal re-introduces the same shape (a background sampler plus an `observed_rtt_ms` feedback channel) without citing the deletion. Re-proposing a mechanism the repo already built, found non-functional, and deleted is the canonical architectural dead end.

**Required action:** Cite `8c8711c` in 现状 and then either (a) specify the learning channel as a contract — who writes the RTT sample, at what point in the segment loop, over what window, with what state owner, lifetime, floor/ceiling, and decay, plus an explicit statement that the computed value gates the semaphore — or (b) delete `AdaptivePolicy` and ship `LinearPolicy` only.

---

#### C-14 — The repo already adjudicated "dynamic concurrency adjustment" as Phase 5: 放弃 (Abandoned), with an expected benefit of 5–10 %

- **Severity:** 🔴 Blocker
- **Sources:** S8/egm-zoom-out (S8-Z8-02)
- **Scope:** fixture line 8, lines 23–44

`docs/plans/performance-improvement-plan.md` is an in-repo decision record for exactly this class of change, and it rejected it. `:83` — "### 瓶颈 5: 动态线程调整风险 ⚠️ LOW (建议放弃或限制)", reasons: 网络波动导致不必要的线程调整 / 引入 DoS 攻击面 (人为制造慢速网络) / 与现有 website_profile 重复. `:325–332` — "### Phase 5: P3 - 放弃或限制 (NOT RECOMMENDED)", reasons: 层耦合问题 / DoS 攻击面 / 实际改善有限 (5-10%) / 与 website_profile 重复; 建议: 保持当前静态配置，由 website_profile 提供 per-domain 最优值. `:344` — the priority-matrix row: `| **Phase 5** | P3 动态线程 | 5-10% | 高 | 中 | **放弃** |`. The document proposes one instance of this rejected shape, with a ≥15 % target, without engaging the record. The burden is on the new proposal to explain why the earlier adjudication is now wrong.

**Required action:** Cite `performance-improvement-plan.md:83, 325-332, 344` and either overturn the 放弃 decision with new evidence or drop the dynamic layer. Reconcile the ≥15 % figure with the recorded 5–10 % expectation for this class.

---

#### C-15 — One component carries three-to-four names, and the APIs the plan depends on do not exist

- **Severity:** 🔴 Blocker
- **Sources:** S5/egm-zoom-out + egm-inventive-catalyst (S5-13) · S6/egm-comprehension (S6-D2) · S7/egm-design + egm-structural + egm-style (S7-22) · S8/egm-zoom-out (S8-Z8-13) · S9/egm-implementation-readiness + egm-codebase-details + egm-style (S9-B-3)
- **Scope:** fixture lines 25, 28, 34, 43, 73–75

One thing is named four ways: file `concurrency_guard.rs` (line 25), struct `ConcurrencyController` (lines 28, 43), `ConcurrencyGuard::acquire()` (plan item 1), trait `ConcurrencyPolicy` (lines 34, 73–74). `ConcurrencyGuard` occurs exactly once in the whole document, and the architecture section never mentions it. The plan re-lists the trait as a deliverable of a file where the architecture section already defines it. Separately, **`HlsDownloader::with_dynamic_limit(n)` matches nothing in the codebase** — the actual constructors are `new` and `with_config`, the actual builders are `max_concurrent`, `retry_count`, `skip_unavailable`; a repo-wide grep returns no match (S5 Coverage Gap). The API is presented as if it were an existing entry point. And `base` / `ceiling` are never given values, defaults, or a relationship (is `base` today's 8? is `ceiling` 16? is `ceiling >= base` enforced?). S8 adds the level vocabulary: the repo already has three named concurrency levels — `AppConfig.concurrent_downloads` (task level, default 3), `HlsDownloaderConfig.max_concurrent` (segment level, 8), `WebsiteProfile.max_threads` (per-domain) — and the document introduces a fourth without saying which level it occupies.

**Required action:** Pick one component name and use it in the file name, the struct, the trait and the plan; publish the full public surface (fields with defaults, constructor, `acquire()` signature, derives); state whether `Semaphore` is wrapped or removed and what happens to the `max_concurrent` builder; give `base`/`ceiling` explicit defaults and assert `base <= ceiling`; and state which concurrency level the new limit occupies.

---

#### C-16 — The new trace step is specified by name only: no payload, emitter, cardinality, registry membership, schema-version decision, or reader — and it breaks the trace test

- **Severity:** 🔴 Blocker
- **Sources:** S5/egm-zoom-out + egm-inventive-catalyst (S5-14) · S6/egm-comprehension (S6-D6, S6-F11) · S8/egm-zoom-out (S8-Z8-07, S8-Z8-14) · S9/egm-implementation-readiness + egm-codebase-details + egm-style (S9-B-6)
- **Scope:** fixture lines 62, 66–67, plan item 5 (line 78)

Plan item 5 names one string and nothing else: not the step name, not the `input` / `output` / `extra` payload, not the emission site or cardinality. `TraceStage::start(step, input, extra)` (`src/core/trace.rs:561`) auto-derives the done-event name as `<step>_done` (`:564`), so `concurrency_adjust` silently also emits `concurrency_adjust_done` — unstated. The work is assigned to the wrong layer: the emitter is the call site, where the other three `hls_*` stages are started (`src/hls/downloader.rs:264, 285, 319, 450`), yet the item points at `src/core/trace.rs`. `scripts/eval_trace.py` classifies steps through two explicit frozensets — `STALL_ELIGIBLE_STEPS` and `AGGREGATE_STEPS` — and a step in *neither* set is silently neither stall-counted nor surfaced as a metric: an event in neither registry is exactly what rots, because a later reader cannot tell whether it was deliberately excluded or simply forgotten. No consumer is named. `docs/dna/trace-schema.md` (which records the step vocabulary) is not in the change list. Adding the step also breaks `src/hls/downloader.rs:1227` `test_hls_download_emits_four_leaf_stage_events()`, which asserts a hardcoded list `["hls_manifest", "hls_keys", "hls_segments", "hls_merge"]` (`:1335`) and checks each `<stage>_done`'s `duration_ms` is a `u64`. Finally, the schema-version decision the document asserts ("不改 trace 的 NDJSON schema 版本号") is never shown, and `eval_trace.py` refuses a verdict (exit 3) when baseline and current `schema_version` differ — so a wrong choice makes the document's own success criterion silently un-runnable.

**Required action:** State in one sentence: "`concurrency_adjust` is a marker (not stall-eligible), with a `finish()`-emitted `_done`, outside `STALL_ELIGIBLE_STEPS` and `AGGREGATE_STEPS`; `TRACE_SCHEMA_VERSION` stays 3 **because** it does not enter the stall whitelist; if it ever enters it, +1 is mandatory and the perf baseline must be re-locked under v4." Then specify the payload keys and types, the emitter and cardinality, the registry membership, the reader, the `docs/dna/trace-schema.md` row, and the update to the trace test (exempt the marker from the `duration_ms` assertion if it carries no duration).

---

### 🟡 Majors

---

#### C-17 — `with_dynamic_limit(n)`: the argument has no contract, the call site cannot see the segment count, and the sentence contradicts itself about who computes the limit

- **Severity:** 🟡 Major
- **Sources:** S5/egm-inventive-catalyst + egm-zoom-out (S5-12) · S6/egm-comprehension (S6-F5) · S7/egm-design + egm-structural + egm-style (S7-9, second half) · S9/egm-implementation-readiness + egm-style (S9-M-5)
- **Scope:** fixture lines 43–44, plan item 2 (line 75)

"调用侧改成 `HlsDownloader::with_dynamic_limit(n)`，由 `ConcurrencyController` 在每个课节开始时算出上限" reads two ways: the caller passes a limit `n` (a per-lesson constructor call) or the controller derives the cap per lesson. If the controller computes the cap, an `n` argument at construction time is unexplained; if the caller supplies it, the controller's per-lesson computation is unexplained. The API also takes a number, yet the two call sites hold a URL and nothing else — the segment count only exists after the manifest is parsed inside `HlsDownloader::download` (`:272`), and the semaphore is created at `:314` *after* that parse, so a builder argument at the call site cannot implement per-lesson adaptivity. The API also breaks the type's existing builder convention (`max_concurrent(n)` already covers the constant case).

**Required action:** Compute the limit inside `HlsDownloader::download` after the manifest parse, using quantities already in hand (segment count; per-segment durations already measured on the segment path and in the trace); pick one mechanism, state where the computation lives, and state the fate of `max_concurrent` (base / ceiling / override / removed) together with its two call sites.

---

#### C-18 — 现状 misattributes the HLS segment retry path to `src/core/segment.rs`

- **Severity:** 🟡 Major
- **Sources:** S7/egm-design + egm-structural + egm-style (S7-6) · S9 context finding (S9-C-3, promoted)
- **Scope:** fixture lines 13–14

`src/hls/downloader.rs` never imports `crate::core::segment`; its imports are `super::crypto`, `super::parser`, `crate::core::trace`, `crate::types` (`:20-23`). The HLS segment path is `HlsDownloader::download_segment_with_retry` (`src/hls/downloader.rs:478`), with its own backoff loop, its own `is_retryable_error`, and its own AES / `SegmentKeys` / `spawn_blocking` handling. `src/core/segment.rs:99` is a *different*, parallel `download_segment_with_retry`. Two independent retry loops exist today; the document names the wrong one, so a reader would design against — or edit — a file that is not on the HLS route.

**Required action:** Point lines 12–14 at `src/hls/downloader.rs:478` and note that the two implementations are parallel, independent systems.

---

#### C-19 — The failure-mode section contradicts the shipped semantics and contradicts itself

- **Severity:** 🟡 Major
- **Sources:** S5/egm-zoom-out + egm-inventive-catalyst (S5-7) · S6/egm-comprehension (S6-F8) · S7/egm-design + egm-structural + egm-style (S7-11) · S9/egm-codebase-details (S9-M-7) · S9 context findings (S9-C-4, S9-C-5, promoted)
- **Scope:** fixture lines 52–56 (`### 3. 失败模式`), line 53

Three defects. **(a) The shipped tolerance path is omitted** — "重试耗尽后，整个课节失败并向上返回错误" ignores `max_failed_segments` (default 5, `src/hls/downloader.rs:67`, abort at `:414`) and `skip_unavailable` (default `false`, `:61`, branch at `:434`), where failed segments are *skipped and merged*, producing a silently truncated output file — the most dangerous path, and not covered by the change list. **(b) The third bullet is self-contradictory** — "服务器返回的 manifest 分片数比预期少：忽略多余分片": if the server returned *fewer* segments there are no extra segments to ignore, so the rule is either a typo for "多" or describes a case that cannot occur. The genuinely dangerous case (a short manifest silently yielding a truncated lesson reported as success) is unaddressed. **(c) The second bullet is unreachable** — "课节中途 manifest 变化": the playlist is fetched exactly once before the segment loop (`M3u8Parser::fetch_and_parse`, `:268`) and the proposal adds no re-fetch; S7 recommends dropping it as a defensive caveat with no reachable operator decision. S6 also notes that bullet 1's "向上返回错误" never names the recipient of that error.

**Required action:** Rewrite all three bullets against the real semantics: name `max_failed_segments` and `skip_unavailable` and state whether they survive; correct the direction of the segment-count bullet and say what happens on a genuinely short manifest; drop the manifest-change bullet or restate it as a design change with a trigger; and name the error recipient and propagation path.

---

#### C-20 — Two competing configuration surfaces for the same knobs, with no precedence rule

- **Severity:** 🟡 Major
- **Sources:** S9/egm-codebase-details + egm-implementation-readiness + egm-style (S9-M-3)
- **Scope:** fixture line 77 (plan item 4) against `src/hls/downloader.rs:28-49`

`HlsDownloaderConfig` already owns `max_concurrent`, `retry_count`, `retry_base_delay_ms`, `retry_max_delay_ms`, `skip_unavailable` and `max_failed_segments`. The plan adds `HlsPolicyConfig` in a different layer and never states which one wins when they disagree: does `max_concurrent` become a ceiling, a base, a fallback, or is it deleted? Do `RetryPolicy`'s `max_attempts` / `base_delay_ms` / `multiplier` supersede `retry_count` / `retry_base_delay_ms` / `retry_max_delay_ms`, or coexist with them?

**Required action:** One owner per knob, plus a precedence table stating which field wins when both are present and what the fallback is when a site has no override.

---

#### C-21 — The implementation plan specifies no phases, ordering, ownership, lifetime, or acceptance criteria

- **Severity:** 🟡 Major
- **Sources:** S6/egm-comprehension (S6-D5) · S8/egm-zoom-out (S8-Z8-08, S8-Z8-11) · S9/egm-implementation-readiness + egm-codebase-details + egm-style (S9-M-1)
- **Scope:** fixture lines 71–78; also lines 43–44

The five items are an unordered list with no dependency order, no per-item acceptance criteria, no verification commands (`cargo test`, `cargo clippy`, `cargo bench --bench hls_concurrency`, `pytest tests/test_eval_trace.py`) and no phases leaving the tree stable, buildable and testable. The dependency order is real but unstated: item 1 (trait + module) plus the `src/hls/mod.rs` update must land before item 2 compiles; item 4's config and item 5's trace point must exist before items 1–2 can use them; the bench stub must be reconciled. No owner or lifetime is stated for `ConcurrencyController` (per downloader? per course? per lesson?) or for the state it must hold — and the ownership question is the same one raised by `with_dynamic_limit(n)` (C-17) and by the per-lesson-vs-global topology (C-10). S8 confirms it in the working tree: `HlsDownloader` holds an `Arc<HlsDownloaderConfig>` and is cloned per call site; the semaphore is per-`download()`-call — so a controller carrying cross-lesson feedback state would need a longer-lived scope than the document's "one file".

**Required action:** Add an explicit, dependency-ordered phase list with per-phase verification commands and acceptance criteria; name the owner and lifetime of each proposed type and pick the one matching the state it must hold; and say which phase may not begin until `src/hls/mod.rs` and the bench stub are reconciled.

---

#### C-22 — Heilmeier gaps: Impact / Cost / Timeline absent, and the Objectives are written in unglossed jargon

- **Severity:** 🟡 Major
- **Sources:** S7/egm-design + egm-structural + egm-style (S7-15), S7/egm-design + egm-structural + egm-style (S7-18)
- **Scope:** whole document; objectives at lines 5–8

Three of Heilmeier's eight questions get no answer: **Impact** (who cares — the operator downloading 80–150-lesson courses? a batch script? the maintainer — and what difference it makes; the document gives only its own performance numbers), **Cost** (implementation effort, runtime compute, added connection and bandwidth pressure on a third-party paid CDN, and the new maintenance surface of two modules + a trait + a config schema), and **Timeline** (no phased plan, no measurement checkpoint). Separately, Heilmeier requires Objectives to be "written with absolutely no jargon": the goal sentence (line 8) uses "wall-clock", "并发上限", "自适应", "中位数" with no gloss, for a reader assumed to be new to the project; unglossed terms across the scope include wall-clock, manifest, policy, trait, RTT, NDJSON, mock HTTP server, plus `LinearPolicy` / `AdaptivePolicy` / `ConcurrencyPolicy` / `RetryPolicy` / `HlsPolicyConfig` / `ConcurrencyController`.

**Required action:** Add all three sections — minimum viable cost statement: implementation days, the change in concurrent connections/bandwidth against the CDN, whether the user bears a higher rate-limit risk, plus a phased plan with a measurement checkpoint after each phase. Open with a one-line plain-language statement of what a course recording is and what "downloading it faster" means; add a short 术语 note or gloss each term at first use.

---

#### C-23 — The repo's own concurrency bench and the locked c=8 decision are never cited as prior art

- **Severity:** 🟡 Major
- **Sources:** S7/egm-design + egm-structural + egm-style (S7-19)
- **Scope:** fixture lines 10–19, 21–44

`benches/hls_concurrency.rs` exists precisely to answer this document's question — its header reads "Benchmark: find optimal `max_concurrent` for HLS segment downloads … across {1, 2, 4, 8, 16, 32} concurrency" — and its result is already recorded as a locked decision (2026-10-12 perf session: "max_concurrent 4 → 8 (bench sweet spot 280ms / 6.3× speedup / 80% efficiency)"). A proposal that would push some lessons to 16 mentions neither, so a fresh reader cannot tell whether the new 8→16 observation confirms, contradicts, or is orthogonal to the bench's latency-bound result. The bench also sweeps only a *latency-bound* mock, "one sample per config" — a bandwidth-bound mock would be needed if it is to be used as evidence.

**Required action:** Cite the bench and its locked result in 现状, and state whether the 8→16 observation confirms, contradicts, or is orthogonal to it.

---

#### C-24 — The nominated reuse target for feedback-driven concurrency (`adjust_thread_count`) is display-only by the repo's own documentation

- **Severity:** 🟡 Major
- **Sources:** S8/egm-zoom-out (S8-Z8-06) — corrects S5's reuse-inventory row
- **Scope:** fixture lines 39–44, plan item 1 (line 73)

S5's Verified Reuse Inventory nominates `adjust_thread_count(speed_bps, threshold, min, max, current)` (`src/downloader/parallel.rs:44`) as *the* established in-repo pattern for feedback-driven concurrency, and S5's `AdaptivePolicy` finding recommends reusing it. The repository's own doc comment says the opposite: "影响仅限显示：`active_threads` 在 `src/` 内无读取方，且 **`current_threads` 从不参与分段 spawn**（所有分段在 `parallel_download` 里一次性起完）", and its predecessor `calculate_optimal_threads` is 死代码. Reusing it would reproduce, in new code, the exact defect that got `8c8711c` deleted — a number computed and reported but never wired to the semaphore. The one mechanism that genuinely changes concurrency is `recommended_segments_for_size()` (its value becomes the spawn count) — but `src/hls/` consumes no website profile today, so even the correct target is not free: it requires plumbing a domain key and a profile lookup into the HLS path.

**Required action:** Do not reuse `adjust_thread_count`. If per-domain adaptivity is wanted, target `recommended_segments_for_size()` and budget the plumbing (domain key derivation + profile lookup) into the scope.

---

### 🟢 Nits

---

#### C-25 — The scope exclusion names `src/core/crypto.rs`, which does not exist

- **Severity:** 🟢 Nit
- **Sources:** S5/egm-inventive-catalyst (S5-15) · S7/egm-design + egm-structural + egm-style (S7-23)
- **Scope:** fixture line 62

`src/core/` contains only cache.rs, constants.rs, mod.rs, progress.rs, segment.rs, trace.rs. The AES-128 path is `src/hls/crypto.rs` (`fetch_key`, `parse_iv`; the in-line decryptor is `HlsDownloader::decrypt_segment` at `src/hls/downloader.rs:723`). A verifier grepping the stated path finds nothing and cannot confirm the constraint targets the right module. *The exclusion's substance is sound* — S5, S7 and S9's context check (S9-C-6) all agree the AES and trace-schema exclusions are consistent with the codebase; only the path string is wrong.

**Required action:** Correct the path to `src/hls/crypto.rs`.

---

#### C-26 — The design section drops below design altitude

- **Severity:** 🟢 Nit
- **Sources:** S5/egm-zoom-out (S5-16)
- **Scope:** fixture lines 27–37, line 48

`方案概述` inlines a full Rust struct definition, a trait definition, field names, and a config-field triple — implementation detail inside a section that should carry architecture, interfaces and data flow. These fields duplicate existing ones almost one-for-one (`max_attempts` ≈ `retry_count`, `base_delay_ms` ≈ `retry_base_delay_ms`, and a `multiplier` that duplicates the existing hard-coded `2u64.pow(attempt)`).

**Required action:** At design altitude the section should read as "one knob, one policy, one placement" with the formula left to implementation. Publish the concrete fields once, in the Implementation Plan (see C-15), after the reuse question is settled.

---

#### C-27 — The test-plan sentence is colloquial and mixes three of the document's own units

- **Severity:** 🟢 Nit
- **Sources:** S7/egm-style (S7-29) · S9/egm-codebase-details + egm-style (S9-N-1)
- **Scope:** fixture line 82

"起 8 条目课节" — 起 is spoken register for "spin up", and 条目 (entry/item) has no referent in this codebase, whose units are 分片 (segment), 课节 (lesson) and playlist entry. Elsewhere the document uses 课节 for a lesson and 分片 for a segment, so the sentence contradicts the document's own terminology, and it is the test plan for the whole feature.

**Required action:** Write `构造 1 个含 8 个分片的课节`.

---

#### C-28 — The measurement evidence is below the repository's own standard

- **Severity:** 🟢 Nit
- **Sources:** S7/egm-structural + egm-style (S7-24), S7/egm-design + egm-style (S7-25), S7/egm-design + egm-style (S7-26), S7/egm-style (S7-27) · S8/egm-zoom-out (S8-Z8-15)
- **Scope:** fixture lines 5, 16–19

The document's headline arithmetic is *correct* (6m12s = 372 s, 4m48s = 288 s, (372−288)/372 = 22.6 %, consistent with "快 23 %"), but the evidence around it is thin: one course, one run per arm, no repetition count, no variance, no time-of-day control, and the course is unnamed — against a repo whose own pipeline analysis carries a §5 titled "哪些是实测、哪些是估算" and whose bench header states "one sample per config". The two segment-count ranges ("60–120 个分片" at line 5 vs. "分片数 30–160 不等" at line 17) are presumably two different courses, but the document does not say so, so they read as a contradiction between background and measurement. "同一门课" and "中位数" collide in one sentence unless the median is over repeated runs, which is not stated. And the measured 23 % is never reconciled with the 15 % target.

**Required action:** Label each figure as measured vs. estimated (the repo already has that vocabulary), name the course, report N runs with min/median/max and the time window, label each segment-count range with its source, say "N 次重复运行的中位数" explicitly, and state the relationship between the measured 23 % and the ≥15 % threshold in one sentence.

---

#### C-29 — The document register is inconsistent

- **Severity:** 🟢 Nit
- **Sources:** S6/egm-comprehension (S6-D7) · S7/egm-structural (S7-33)
- **Scope:** fixture lines 69, 23/46/52

Six H2 siblings are in Chinese (背景与目标, 现状, 方案概述, 范围边界, 成功判据) and then one is English ("Implementation Plan") with two Chinese H3 children underneath; only 方案概述's children carry numeric prefixes, so cross-references ("see §2") stay ambiguous. Separately, the plan is the only English section and carries no "not doing" line, so the reader must carry the spec's 范围边界 over from lines 58–60 — where, on cross-reference, it turns out to contradict the plan (C-02).

**Required action:** Pick one register for the H2 layer and one numbering convention, and add a one-line in-scope/out-of-scope restatement inside the plan.

---

#### C-30 — Inconsistent emphasis

- **Severity:** 🟢 Nit
- **Sources:** S7/egm-style (S7-30)
- **Scope:** fixture lines 6, 8, 13, 17, 19, 66

Bold is applied to `**固定并发**`, `**4**` and `**快 23%**`, but not to the equally load-bearing `8` (line 17), `≥ 15%` (line 8), `60–120 个分片` (line 5) or `30–160` (line 17).

**Required action:** Adopt one convention (bold the single number the reader must not miss) and apply it consistently.

---

#### C-31 — The new files do not follow the repo's own conventions

- **Severity:** 🟢 Nit
- **Sources:** S9/egm-implementation-readiness (S9-N-4)
- **Scope:** plan items 1 and 3 (lines 73, 76)

Every module in `src/hls/` opens with a `//!` Chinese doc header (`src/hls/mod.rs:1-3`) and uses Chinese inline comments. Relatedly, this repo distinguishes `#[expect(dead_code)]` (errors out once the expectation is unfulfilled) from `#[allow(dead_code)]` (required for test-only items) — `src/hls/downloader.rs` carries `#[allow(dead_code)]` on the config struct (`:26`) and `#[expect(dead_code)]` on the impl block (`:143`). Any new config field read only by the new module needs the right marker.

**Required action:** Note the doc-header and comment-language convention and the `expect`/`allow` choice for any newly-unused field.

---

## 3. Traceability Matrix

108 raw finding rows (107 findings + 1 no-finding context row) → 31 consolidated findings.
**Disposition legend:** `merged` = folded into a consolidated entry · `promoted` = raised out of a context/parked section into the findings set · `merged+promoted` = both · `dropped` = not carried.

| Raw ID | → Consolidated | Disposition | Note |
|---|---|---|---|
| S5-1 | C-01 | merged | |
| S5-2 | C-02 | merged | |
| S5-3 | C-05 | merged | |
| S5-4 | C-11 | merged+promoted | Major → Blocker |
| S5-5 | C-06 | merged+promoted | Major → Blocker |
| S5-6 | C-04 | merged+promoted | Major → Blocker; its *recommendation* is corrected by C-24 |
| S5-7 | C-19 | merged | |
| S5-8 | C-09 | merged+promoted | Major → Blocker |
| S5-9 | C-08 | merged+promoted | Major → Blocker |
| S5-10 | C-05 | merged+promoted | Major → Blocker |
| S5-11 | C-10 | merged+promoted | Major → Blocker |
| S5-12 | C-17 | merged | |
| S5-13 | C-15 | merged+promoted | Nit → Blocker |
| S5-14 | C-16 | merged+promoted | Nit → Blocker |
| S5-15 | C-25 | merged | |
| S5-16 | C-26 | merged | |
| S6-F1 | C-02 | merged | |
| S6-F2 | C-10 | merged | Blocker preserved |
| S6-F3 | C-04 | merged | Blocker preserved |
| S6-F4 | C-04 | merged+promoted | Major → Blocker |
| S6-F5 | C-17 | merged | |
| S6-F6 | C-06 | merged+promoted | Major → Blocker |
| S6-F7 | C-05 | merged+promoted | Major → Blocker |
| S6-F8 | C-19 | merged | |
| S6-F9 | C-11 | merged+promoted | Major → Blocker |
| S6-F10 | C-09 | merged+promoted | Major → Blocker |
| S6-F11 | C-16 | merged+promoted | Nit → Blocker |
| S6-F12 | C-09 | merged+promoted | Nit → Blocker |
| S6-D1 | C-02 | merged | |
| S6-D2 | C-15 | merged | Blocker preserved |
| S6-D3 | C-05 | merged+promoted | Major → Blocker |
| S6-D4 | C-08 | merged+promoted | Major → Blocker |
| S6-D5 | C-21 | merged | |
| S6-D6 | C-16 | merged+promoted | Major → Blocker |
| S6-D7 | C-29 | merged | |
| S7-1 | C-01 | merged | |
| S7-2 | C-11 | merged | Blocker preserved |
| S7-3 | C-02 | merged | |
| S7-4 | **C-10** (primary) · C-05 · C-04 | merged+promoted | Bundle finding split across three entries: global-cap → C-10; 429-fatal → C-05; zero-permit floor + `ceiling` derivation → C-04 |
| S7-5 | C-09 | merged | Blocker preserved |
| S7-6 | C-18 | merged | |
| S7-7 | C-05 | merged+promoted | Major → Blocker |
| S7-8 | C-06 | merged+promoted | Major → Blocker |
| S7-9 | **C-03** · C-17 | merged+promoted | Split: "feature cannot take effect" → C-03; "API ownership contradiction" → C-17 |
| S7-10 | C-04 | merged+promoted | Major → Blocker |
| S7-11 | C-19 | merged | |
| S7-12 | C-09 | merged+promoted | Major → Blocker |
| S7-13 | C-12 | merged+promoted | Major → Blocker |
| S7-14 | C-11 | merged+promoted | Major → Blocker |
| S7-15 | C-22 | merged | |
| S7-16 | C-10 | merged+promoted | Major → Blocker |
| S7-17 | C-07 | merged+promoted | Major → Blocker |
| S7-18 | C-22 | merged | |
| S7-19 | C-23 | merged | |
| S7-20 | C-04 | merged+promoted | Major → Blocker |
| S7-21 | C-08 | merged+promoted | Major → Blocker |
| S7-22 | C-15 | merged+promoted | Major → Blocker |
| S7-23 | C-25 | merged | |
| S7-24 | C-28 | merged | |
| S7-25 | C-28 | merged | |
| S7-26 | C-28 | merged | |
| S7-27 | C-28 | merged | |
| S7-28 | C-12 | merged+promoted | Nit → Blocker |
| S7-29 | **C-08** · C-27 | merged+promoted | Nit → Blocker (fixture ambiguity half) + Nit (wording half) |
| S7-30 | C-30 | merged | |
| S7-31 | C-09 | merged+promoted | Nit → Blocker |
| S7-32 | C-04 | merged+promoted | Nit → Blocker |
| S7-33 | C-29 | merged | |
| S8-Z8-01 | C-13 | merged | |
| S8-Z8-02 | C-14 | merged | |
| S8-Z8-03 | C-12 | merged | Blocker preserved |
| S8-Z8-04 | **C-02** · C-03 | merged | Blocker preserved; split: contradiction → C-02, inoperability → C-03 |
| S8-Z8-05 | C-09 | merged | Blocker preserved |
| S8-Z8-06 | C-24 | merged | |
| S8-Z8-07 | C-16 | merged+promoted | Major → Blocker |
| S8-Z8-08 | **C-07** · C-21 | merged+promoted | Major → Blocker (allowlist half) + Major (ordering half) |
| S8-Z8-09 | C-10 | merged+promoted | Major → Blocker |
| S8-Z8-10 | C-08 | merged+promoted | Major → Blocker |
| S8-Z8-11 | C-21 | merged | |
| S8-Z8-12 | C-09 | merged+promoted | Nit → Blocker |
| S8-Z8-13 | C-15 | merged+promoted | Nit → Blocker |
| S8-Z8-14 | C-16 | merged+promoted | Nit → Blocker |
| S8-Z8-15 | C-28 | merged | |
| S9-B-1 | C-02 | merged | Blocker preserved |
| S9-B-2 | C-07 | merged | Blocker preserved |
| S9-B-3 | C-15 | merged | Blocker preserved |
| S9-B-4 | C-06 | merged | Blocker preserved |
| S9-B-5 | C-05 | merged | Blocker preserved |
| S9-B-6 | C-16 | merged | Blocker preserved |
| S9-B-7 | C-04 | merged | Blocker preserved |
| S9-B-8 | C-08 | merged | Blocker preserved |
| S9-M-1 | C-21 | merged | |
| S9-M-2 | C-07 | merged+promoted | Major → Blocker |
| S9-M-3 | C-20 | merged | |
| S9-M-4 | C-01 | merged+promoted | Major → Blocker |
| S9-M-5 | C-17 | merged | |
| S9-M-6 | C-09 | merged+promoted | Major → Blocker; its bench-invalidation sub-point is carried by C-07 |
| S9-M-7 | C-19 | merged | |
| S9-N-1 | **C-08** · C-27 | merged+promoted | Nit → Blocker (fixture ambiguity half) + Nit (wording half) |
| S9-N-2 | C-05 | merged+promoted | Nit → Blocker |
| S9-N-3 | C-07 | merged+promoted | Nit → Blocker |
| S9-N-4 | C-31 | merged | |
| S9-C-1 | C-01 | promoted | Context row (🔴 Blocker) — anchors C-01's severity |
| S9-C-2 | C-09 | promoted | Context row (🔴 Blocker) |
| S9-C-3 | C-18 | promoted | Context row (🔴 Blocker) — also referenced by C-05 |
| S9-C-4 | C-19 | promoted | Context row (🟡 Major) |
| S9-C-5 | C-19 | promoted | Context row (🟡 Major) — outside the S9 range, carried anyway |
| S9-C-6 | — | dropped | "No finding" clean probe; `范围之外` verified consistent with the codebase |

---

## 4. Stage-by-Stage Summary

### S5 — Initial Zoom Out (`zoom_out_report.md`)

| | |
|---|---|
| Lenses | `egm-zoom-out`, `egm-inventive-catalyst` |
| Findings | 16 (3 Blocker · 9 Major · 4 Nit) |
| Distinctive contribution | The only stage that produced a **Verified Reuse Inventory** — a table of every proposed artifact that already exists in the repo (`WebsiteProfile` / `recommended_segments_for_size()`, `adjust_thread_count`, `download_segment_with_retry` + `is_retryable_error` + `Retry-After`, `max_concurrent` + per-playlist `Semaphore`, `test_max_concurrent_e2e_mock_server`, `benches/hls_concurrency.rs`, `install_perf_baseline.sh` / `eval_trace.py`, and the prior plan's duplicate rejection). It also produced the **Minimal Viable Alternative** (change three literals 8→16, zero new lines) that both lenses converged on, and was the only stage to record the trace-schema disagreement as *unresolved* and to log a synthesizer-verified coverage gap. |

### S6 — Comprehension (`comprehension_report.md`)

| | |
|---|---|
| Lenses | `egm-comprehension` |
| Findings | 19 (5 Blocker · 11 Major · 3 Nit) — F1–F12 (FeatureRange, lines 1–68), D1–D7 (DetailedRange, lines 69–82) |
| Distinctive contribution | The only stage that read the document **without repository access by design** ("document-only, zero-context read"). It therefore found the questions an implementer cannot answer from the document at all: the unit of concurrency (per-lesson limit vs. one shared pool), the policy math as prose rather than a rule, the undefined argument `n`, the unlocated and unshaped `website_profile.json`, the `RetryPolicy` with no consumer, and the plan's missing ordering/ownership/acceptance criteria. S8 explicitly warns these are **not** duplicates of the repo-verified stages and must not be dropped in synthesis. |

### S7 — Critic (`critic_report.md`)

| | |
|---|---|
| Lenses | `egm-design`, `egm-structural`, `egm-style` |
| Findings | 33 (5 Blocker · 17 Major · 11 Nit) |
| Distinctive contribution | The only stage that applied the two **standards that impose structural gates**: the Feature Spec Standard's mandatory Alternatives section (its explicit fail condition) and Heilmeier's Impact / Cost / Timeline and no-jargon Objectives. It also supplied the **Cross-Verification Notes** (every load-bearing factual claim independently re-verified) and the **"Correct as Written"** section that cleared the trace-schema and AES clauses, and it produced the most granular nit inventory (evidence standard, register, emphasis, terminology). |

### S8 — Intermediate Zoom Out (`intermediate_zoom_out_report.md`)

| | |
|---|---|
| Lenses | `egm-zoom-out` |
| Findings | 15 (5 Blocker · 6 Major · 4 Nit) |
| Distinctive contribution | The only stage that read the repository's **history and prior decision records**, not just the working tree — surfacing the deleted non-functional adaptive sampler (`8c8711c`), the 放弃 adjudication of dynamic concurrency (`performance-improvement-plan.md:83, 325-332, 344`), and the mutation-verified bottleneck analysis of the same pipeline written 9 days earlier. It was also the only stage that read the **actual output** of `scripts/eval_trace.py` and found that the primary metric has no instrument, and the only stage to resolve the S5↔S7 trace-schema disagreement. It explicitly attributes its findings to which earlier stage each extends or corrects. |

### S9 — Implementation Readiness (`implementation_readiness_report.md`)

| | |
|---|---|
| Lenses | `egm-implementation-readiness`, `egm-codebase-details`, `egm-style` |
| Findings | 19 in-range (8 Blocker · 7 Major · 4 Nit) + 6 context rows (5 findings + 1 clean probe) |
| Distinctive contribution | The only stage that treated the Implementation Plan as an **enforceable write allowlist**, proving that without `src/hls/mod.rs` the two new modules sit on disk uncompiled while `cargo build` stays green — the feature "does nothing" with a green build. It also produced the **Review Integrity Note** (a contaminated lens self-disclosed), the **Minimum Bar to Pass** (an ordered, consolidated release-condition list), and the **Context Findings** table that parked out-of-range defects rather than dropping them. |

---

## 5. Reviewer Disagreements

Eight conflicts were surfaced across the reports. Six are substantive; two are citation-level.

1. **Trace-schema version bump for a new HLS leaf step — S5 vs. S7. Resolved in favour of S7.**
   S5 (zoom-out) held that the repo convention is to bump `TRACE_SCHEMA_VERSION` when HLS leaf steps are added; S7 read `src/core/trace.rs:33-35` ("加新步骤、加可选字段 **不用** +1") as meaning a new marker step needs no bump. S5's triage notes recorded this as unresolved. **The source supports S7.** `docs/dna/trace-schema.md:155-156` states the rule as written, and `:194-195` shows why the 2026-10-30 round bumped anyway: those steps entered the **stall whitelist**, changing `stall_count`'s 口径. The trigger is whitelist membership, not novelty. Stage 5's underlying concern is nevertheless valid in substance and is folded into C-16 — `scripts/eval_trace.py:26` makes a wrong choice produce **no verdict (exit 3)** rather than a wrong number, i.e. the document's own success criterion would silently become un-runnable.

2. **The reuse target for feedback-driven concurrency — S5 vs. S7 vs. S8. S5's recommendation is the weakest and is corrected.**
   S5's reuse inventory nominates `adjust_thread_count(...)` as *the* established in-repo pattern; S7 Major 16 points at `WebsiteProfile` instead. The repository's own doc comment (`src/downloader/parallel.rs:31-34`) settles it against S5: `current_threads` "从不参与分段 spawn", its only consumer is `progress.update(...)`, and the predecessor `calculate_optimal_threads` was 死代码 — so `adjust_thread_count` is **display-only**, and reusing it would reproduce the exact defect that got `8c8711c` deleted. The one mechanism that genuinely changes concurrency is `recommended_segments_for_size()`. **S8 does not fully side with S7 either** — S7 said "reuse `WebsiteProfile`" without noting that `src/hls/` consumes no website profile today. Carried as **C-24**.

3. **The 4xx rule — S5/S7 call it a regression; S8 records a nuance that no prior stage made fairly.**
   S5 and S7 treat "HTTP 4xx 直接失败" as a regression. That is right for 429 and 408, which are currently retryable. But `is_retryable_error` is a **substring blocklist** on the error message, not a status classifier (blocklist: `connection refused`, `invalid url`, `parse error`, `unauthorized`, `forbidden`, `not found`, `400 bad request`, `404`; everything else retryable) — so a status-code classifier would be *strictly better* than what ships, which is the one fair argument for a new module. **S8's conclusion is unchanged** (do not add a third retry implementation; extend the existing function in place), but if the author converts the classifier it must be a rewrite of `is_retryable_error` by status code **in place**, not a new file beside it. Carried in **C-05**.

4. **Severity of the stale baseline — S5/S7 Blocker vs. S9 Major. Escalated to the highest.**
   S5-1 and S7-1 rate the 4-vs-8 defect 🔴 Blocker; S9-M-4 rates it 🟡 Major; S9's own context table then rates the same defect 🔴 Blocker (S9-C-1). Under the highest-severity rule the consolidated entry is **C-01 = Blocker**.

5. **Severity of the global concurrency budget — S5/S7/S8 Major vs. S7's Blocker bundle. Escalated to the highest.**
   S5-11, S7-16 and S8-Z8-09 rate the missing global budget 🟡 Major. S7-4 rates it inside a 🔴 Blocker finding ("Aggregate concurrency has no global cap" is one of its four named risk bullets). Taking the highest, **C-10 = Blocker**. The escalation is on the packaging, not on the substance — all four sources describe the same defect and the same fix.

6. **Severity of the policy specification — S6/S9 Blocker vs. S5/S7 Major. Escalated to the highest.**
   S5-6, S7-10 and S7-20 rate the unspecifiable policy 🟡 Major; S6-F3 and S9-B-7 rate it 🔴 Blocker. **C-04 = Blocker.**

7. *(citation-level)* **`src/downloader/mod.rs` line reference — S5 `:123` vs. S7 `:124`. S7 is correct.**
   Verified: `:123` is `HlsDownloader::new(client)`; `.max_concurrent(8)` is on `:124`. Immaterial to the finding, but the citations must be exact.

8. *(citation-level)* **`AppConfig.concurrent_downloads` line reference — S7 is internally inconsistent.**
   S7 Blocker 4 and Major 16 cite `src/config.rs:43` for "default 3"; S7's own Cross-Verification section cites `:85`. Verified: `:43` is the `#[serde(default = …)]` attribute, `:44` the field, `:85` the default function, `:219` the assertion. The value (3) is right in both.

**Refinement, not a conflict.** S7-5 ("`docs/perf-baseline.md` is absent from the repo, from HEAD, and from every branch's history") is verified exactly true; S8 adds the *why* — it was a promised deliverable of the 2026-10-12 baseline session that was never committed. This changes the recommendation from "correct the path" to "cite what exists and recover the decided thresholds from the session record", and sharpens C-09, because the real substitute tooling computes a different quantity than the document's metric.

---

## 6. Integrity Notes

### 6.1 Stage 9 — Review Integrity Note (carried forward in full)

`egm-style` disclosed, unprompted, that a repo-wide grep for `website_profile*.json` surfaced `.dev/docs/egm-review-answerkey.md` — a file explicitly marked `评审方不可见` (not visible to reviewers) that enumerates planted defects `D1–D9` and two clean probes `C1–C2`. The reviewer read it before completing its analysis and stated that every finding had already been derived from source, in its own words, with its own citations, and asked that the run's independence be discounted on that basis.

Assessment and disposition as recorded by the S9 synthesizer:

- The synthesizer did **not** read the answer key. It was confirmed to exist and to carry the `评审方不可见` marker, but its contents were never ingested into that consolidation.
- `egm-implementation-readiness` and `egm-codebase-details` did not report reading it, and their findings were independently derived from source.
- **Corroboration is strong.** The two uncontaminated lenses and the contaminated one converge on the same eight blockers from three different audit vectors — missing `src/hls/mod.rs`, the wrong config file and non-existent `website_profiles.json` name, three names for one concept, the duplicated retry implementation, the unspecifiable `observed_rtt_ms`, the unusable test plan. The style lens also independently surfaced two facts the uncontaminated lenses caught: the cap is 8 not 4, and the `benches/hls_concurrency.rs` harness that the ≥15 % criterion depends on.
- Two planted defects the style lens named (`D1` "cap is 4", `D6` the self-contradictory failure-mode bullets) fall outside its assigned range and appear in the S9 report only as C-1 and C-5.
- **Recommendation for future runs of this fixture:** exclude `.dev/docs/egm-review-answerkey.md` from the reviewers' search scope — either by placing it outside the repository tree, or by instructing reviewers to skip paths under `.dev/docs/`. A clean pass would then be scoring-eligible without a caveat.

*Independence impact on this consolidation: none on the finding set.* Every finding in §2 traces to at least one uncontaminated source (S5, S6, S7, S8, or S9's `egm-implementation-readiness` / `egm-codebase-details` lenses), and no consolidated entry rests on `egm-style`'s S9 contribution alone. The integrity caveat is carried forward, not resolved.

### 6.2 Stage 5 — Coverage Gap note (synthesizer-verified, deliberately not attributed to a lens)

> Neither S5 reviewer explicitly reported that `src/hls/concurrency_guard.rs` and `src/hls/retry.rs` do not exist as paths in the working tree (both confirmed missing), nor that `HlsDownloader::with_dynamic_limit` has no counterpart in the builder API (grep across `src/` returns no match). Both files are *new* by design, so their absence is expected — but `with_dynamic_limit` is presented as if it were an existing entry point.

Recorded in S5's triage notes rather than in its Prioritized Findings, to keep reviewer attribution clean. In this consolidation the substance is carried by **C-07** (the two new files must be registered in `src/hls/mod.rs` or the build stays green while the code is dead) and **C-15** (the plan's APIs do not exist).

### 6.3 Stage 5 — other attribution notes

- **Clean probes:** the background figures (30–160 segments, 3.2 MB average) were flagged by neither lens; the AES / trace-schema scope exclusion was raised by one lens **only** as a path-typo nit, which does not amount to a design gap against that clause. (Carried as **C-25**.)
- **Isolation:** both S5 reviewers were spawned with zero conversation history and were given no project context beyond their persona file, the target standard, the target document, and the two GoldfishContext source files. The grading answer key was not passed to either reviewer.
- **Fan-out:** 2 reviewer subagents were spawned and completed (the caller requested 2 reviewers; no additional instances were requested).

### 6.4 Stage 8 — corrections to earlier stages

- **Z8-06 corrects Stage 5's Verified Reuse Inventory** row that nominated `adjust_thread_count` as the reuse target for feedback-driven concurrency. See §5-2 and **C-24**.
- **S8's Reviewer Disagreements §3** corrects Stage 5's line reference `src/downloader/mod.rs:123` → `:124`. See §5-7.
- **S8's Reviewer Disagreements §4** flags Stage 7's internal inconsistency on `src/config.rs:43` vs. `:85`. See §5-8.
- **S8's Reviewer Disagreements §5** refines Stage 7's `docs/perf-baseline.md` finding from "absent" to "never existed; a promised 2026-10-12 deliverable that was never committed". See §5, refinement note.
- **S8's Reviewer Disagreements §6** records that Stage 6's document-only findings (F2, F3, F5, F6, D3, D5) are **not** duplicates of Stages 5/7 and must not be dropped in synthesis — honoured here: F2 → C-10, F3 → C-04, F5 → C-17, F6 → C-06, D3 → C-05, D5 → C-21.
- **Attribution hygiene preserved:** S8's findings self-declare which prior stage each one "extends", "sharpens", "credits", or "corrects"; that provenance is reproduced verbatim in the "Sources" line of each consolidated entry where S8 is a source.

### 6.5 Stage 7 — "Correct as Written" (not findings; carried so they are not re-raised)

- Line 62's "不改 trace 的 NDJSON schema 版本号" is **correct** — `docs/dna/trace-schema.md:158` and `src/core/trace.rs:36-40` both say a new step or optional field needs no `TRACE_SCHEMA_VERSION` bump. The problem is only that the change list under-scopes the other files (C-07).
- `hls_segments_done` is a real trace event name (`TraceStage::start("hls_segments")` emits `<step>_done` with a real `duration_ms`, `src/core/trace.rs:561-564`).
- Markdown structure is sound — heading levels correctly nested, the single `rust` fence balanced, list markers consistent, and the two relative links at lines 12 and 14 resolve to real files. Every S7 issue is a content or internal-consistency issue, not a syntax one.
- The scope boundary's *intent* (don't touch CLI/GUI) is sound; only its file list and the config-format clause need reconciling.

### 6.6 Stage 8 — Coverage Gaps (synthesizer-verified; not attributed to a lens)

Eight verified gaps recorded in S8. Four are folded into consolidated findings above: #1 (`src/hls/mod.rs` must be edited) → **C-07**; #2 (`src/hls/` has zero references to `website_profile`) → **C-24**; #3 (`eval_trace.py` computes `avg_total_ms`, an average of a proxy) → **C-09**; #4 (`docs/perf-baseline.md` never existed in git history) → **C-09**. The remaining four are carried as evidence inside findings: #5 (`is_retryable_error` classifies by substring match, not HTTP status) → **C-05**; #6 (`TraceStage::start` only produces a `_done` event via `finish()`, so a marker without `finish()` breaks the trace-test invariant) → **C-16**; #7 (the GUI reaches HLS through `src/downloader/mod.rs`, so "不改 GUI" as a file constraint ≠ no GUI behaviour change) → **C-03**; #8 (`src/downloader/parallel.rs:294` carries a latent `segments ≤ 8` assumption that has already produced a panic once) → **C-10**.

### 6.7 Stage 8 — Clean Probes (verified correct; carried so they are not re-raised)

Ten probes, including: both source links resolve; `hls_segments_done` is real and deliberately excluded from stall counting (making it the document's *only* criterion with a working instrument); the AES scope exclusion's substance is sound; the per-lesson semaphore is the right placement for a per-lesson knob (created at `:314` *after* the manifest parse at `:272`); the headline arithmetic is correct (22.6 % ≈ "快 23 %"); `LinearPolicy`'s *concept* is not novel — `recommended_segments_for_size()` already solves it on the downloader path; `pool_max_idle_per_host(64)` shows a constant 16 is not blocked by the connection-pool configuration; the `Semaphore` concurrency model is right in kind; and the document is readable end-to-end with clear prose — its failures are structural and factual, not linguistic (S6 reached the same judgment independently).

---

## 7. Release Conditions

Merged from the release-condition lists proposed by S5 (5 items), S7 (9 items), S8 (10 items) and S9 (11 items) into one non-duplicated, dependency-ordered sequence. C-numbers reference §2.

1. **Correct the factual baseline and the wrong references** — cap is 8, not 4, and cite `44dbc77` / the 2026-10-12 bench decision; the HLS retry path is `src/hls/downloader.rs:478` (`download_segment_with_retry`), not `src/core/segment.rs`; the AES module is `src/hls/crypto.rs`, not `src/core/crypto.rs`; the profile file is `website_profiles.json` (plural); the baseline artifact is `scripts/install_perf_baseline.sh` + `scripts/eval_trace.py`, not `docs/perf-baseline.md`; and state the three conflicting `retry_count` values (10 / 5 / 3). Re-anchor the ≥15 % target to c=8. *(C-01, C-18, C-25)*
2. **Rewrite `范围边界` as an accurate changed-files table** derived from the change list, state explicitly that the config format *does* change, and give the precedence rule between the scope boundary and the change list. *(C-02)*
3. **Put the two production call sites in the change list** (`src/downloader/mod.rs:124`, `src/commands/download/strategy_direct.rs:102`), state the precedence between `max_concurrent` and the new policy, and rename the API so ownership is visible. *(C-03, C-17)*
4. **Publish an enforceable file allowlist** — `path → what changes → why it is in scope`, with Create / Modify / Delete on every row — covering at minimum `src/hls/mod.rs`, the two call sites, `src/website_profile.rs`, `src/core/segment.rs`, `benches/hls_concurrency.rs`, `docs/dna/trace-schema.md`, `scripts/eval_trace.py` + `tests/test_eval_trace.py`, and the trace test in `src/hls/downloader.rs`; mark every other path read-only. *(C-07)*
5. **Add `## 备选方案` and answer the repo's prior records** — at least four alternatives, starting with the fixed 8→16 bump; cite and either overturn or accept `8c8711c` (the deleted non-functional adaptive sampler) and `performance-improvement-plan.md:83, 325-332, 344` (the 放弃 decision, 5–10 % expected benefit). *(C-11, C-13, C-14)*
6. **Produce the per-lesson evidence, or accept the constant** — the segment-count-vs-duration distribution that motivates per-lesson adaptivity, reconciled with the 2026-10-30 bottleneck analysis; if the curve is flat, the deliverable is three literals changed from 8 to 16. *(C-12)*
7. **Specify the policy behaviourally** — both formulas with worked examples at 30 / 90 / 160 segments; defaults, ranges and an asserted `base <= ceiling` for `base`/`ceiling`; clamp ownership; a floor of 1; the default policy and its construction path; the RTT measurement point, window, storage and state owner; whether the computed value gates the semaphore; and the limit's scope relative to the existing per-site thread axis. *(C-04)*
8. **Choose one retry path and retire the others** — delete `src/hls/retry.rs`; extend `HlsDownloaderConfig.retry_*` and `is_retryable_error` in place (if the classifier is rewritten, rewrite it by status code in place, not in a new file); preserve the 429/503 `Retry-After` branch and the max-delay cap; fix the field types, attempt indexing, cap and jitter; publish the error taxonomy and the intended 4xx/429 semantics. *(C-05)*
9. **Retarget the per-site config** — `src/website_profile.rs` + `website_profiles.json`; publish the JSON schema, defaults, serde back-compat, the policy discriminator, and a precedence table over `HlsDownloaderConfig`; state how the new policy composes with `WebsiteProfile`'s existing adaptive-thread logic. Do not reuse `adjust_thread_count`; if per-site adaptivity is wanted, target `recommended_segments_for_size()` and budget the HLS-path plumbing. *(C-06, C-20, C-24)*
10. **Pick one name and publish the full API** — one component name across file, struct, trait and plan; fields with defaults, constructor, `acquire()` signature and derives; whether `Semaphore` is wrapped or removed and what happens to the `max_concurrent` builder; which of the repo's existing concurrency levels the new limit occupies; and move the inlined struct/trait definition out of `方案概述` to design altitude. *(C-15, C-26)*
11. **State the trace decision in one sentence, and add its table row** — `concurrency_adjust` is a marker with a `finish()`-emitted `_done`, outside `STALL_ELIGIBLE_STEPS` and `AGGREGATE_STEPS`; `TRACE_SCHEMA_VERSION` stays 3 **because** it does not enter the stall whitelist; entering it later mandates +1 and a baseline re-lock. Plus the payload keys and types, the emitter and cardinality, the reader, the `docs/dna/trace-schema.md` row, and the update to `test_hls_download_emits_four_leaf_stage_events`. *(C-16)*
12. **Make the success criterion runnable** — name the real baseline artifact and label; define the metric as something the tooling produces (or instrument course-level wall-clock and accept a schema-v3 baseline re-lock); state N runs, the time window, alternating arm order and the defined median; give the second metric a threshold and a precedence rule; update the bench so its labels stay truthful; cite `docs/perf-baseline.md` only as the lost 2026-10-12 artifact, or drop it. *(C-09, C-28)*
13. **Rewrite `### 3. 失败模式` against the shipped semantics** — name `max_failed_segments` and `skip_unavailable` and state whether they survive; correct the direction of the "fewer segments → ignore extras" bullet and say what happens on a genuinely short manifest; drop the unreachable manifest-change bullet or restate it as a design change with a trigger; name the error recipient and propagation path. *(C-19)*
14. **Add `## 风险与缓解` and name the system-level constraints** — the aggregate-concurrency product (lessons-in-flight × per-lesson cap) against the CDN's recorded 6–12 connection limit; whether `ceiling` is per-lesson or global and how it is derived; its anti-scraping role; the zero-permit floor; 429 / `Retry-After` as a down-pressure signal; a single revertible kill-switch; and widen the scope to the GUI batch path if a global budget must live there. *(C-10)*
15. **Replace the duplicate mock-server test with falsifying assertions** — a policy unit test (limits for 30 / 90 / 160 segments, clamped to ≥1 and ≤ `ceiling`) plus e2e assertions that the observed peak *equals* the computed limit and that two lessons with different segment counts receive different peaks; keep the existing ceiling test unchanged as a regression guard; name fixture shape, observed quantity, assertion, expected limit and test placement; and write the sentence in the document's own units (`构造 1 个含 8 个分片的课节`). *(C-08, C-27)*
16. **Phase the work** — dependency-ordered phases, each leaving the codebase stable, buildable and testable, with per-phase acceptance criteria and verification commands (`cargo test`, `cargo clippy`, `cargo bench --bench hls_concurrency`, `pytest tests/test_eval_trace.py`); name the owner and lifetime of each proposed type; and state which phase may not begin until `src/hls/mod.rs` and the bench stub are reconciled. *(C-21)*
17. **Close the standards gaps** — add Impact / Cost / Timeline (implementation days, the change in concurrent connections and bandwidth against the CDN, the user's rate-limit risk, and a phased plan with a measurement checkpoint); de-jargon the Objectives with a plain-language opener and a 术语 note; and fix the document register (one H2 language, one numbering convention, an in-scope/out-of-scope line inside the plan, consistent emphasis). *(C-22, C-29, C-30)*
18. **Note the repo conventions for the new files** — `//!` Chinese doc headers and Chinese inline comments; and the correct `#[expect(dead_code)]` vs `#[allow(dead_code)]` marker for any newly-unused config field. *(C-31)*

---

## 8. Verdict

**NOT APPROVED.** The document cannot be reviewed, scheduled, estimated, or implemented as written. Its factual baseline is stale by a full benched decision (C-01), which silently invalidates every derived number including the ≥15 % target; its scope boundary is contradicted three times over by its own proposal and plan, with no precedence rule, and under that boundary the feature is literally inoperative because both production call sites hard-code the value (C-02, C-03); the substance of the feature — the policy — is prose rather than a rule, with no formulas, defaults, bounds, clamp ownership, default policy, RTT source, or state channel (C-04); the retry design would be a third, unwireable implementation that makes 429 fatal on exactly the axis the change pushes (C-05); and the test plan and success criterion cannot fail or cannot be executed at all, naming a baseline file that has never existed and a metric no tool in this repository computes (C-08, C-09). Beyond the internal defects, Stages 7 and 8 established that the proposal's central premise was already adjudicated twice in the repository — a mechanism built and deleted in `8c8711c`, and a 放弃 decision with a 5–10 % expected benefit — and that the document's own measurement already clears its target with a constant cap (C-11 through C-14). All five stages returned a not-approved verdict independently, from five different audit vectors, and the one point where the stages conflicted on severity was resolved by the highest-severity rule, not by averaging. The release conditions in §7 are the path to re-review; the minimal viable outcome, per S5 and S8, is three literals changed from 8 to 16 with zero new lines of code, and the current design should not survive that comparison unless it can show the per-lesson distribution it has never produced.
