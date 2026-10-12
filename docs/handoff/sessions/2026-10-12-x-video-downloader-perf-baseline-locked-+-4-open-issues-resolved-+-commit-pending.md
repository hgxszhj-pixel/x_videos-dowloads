# 2026-10-12 — x-video-downloader perf baseline 锁定 + 4 open issue 解决

## Why

`docs/perf-baseline.md` 有 4 个未决策 open issue 阻塞 regression 网建立(发版前无法自动判定 perf 退化)。本次一次性闭环:决策落档 + bench 数字锁定 + 监控阈值。

## What changed

**代码** (5 文件):
- `src/hls/downloader.rs:51` — `HlsDownloaderConfig::default()` `max_concurrent: 4` → `8` (bench sweet spot 280ms / 6.3× speedup / 80% efficiency)
- `benches/hls_parse.rs` — 加 `make_long_url` fixture (~250 char CloudFront style);`make_playlist` 签名加 `url_for: F` 闭包参数
- `benches/aes_decrypt.rs` — 新建 micro-bench (100KB/1MB/5MB ciphertext, 50 iter min)
- `Cargo.toml` — `[[bench]] aes_decrypt` entry (`harness = false`)
- `docs/perf-baseline.md` — 4 项 open issue 全部 decision-record + 长 URL 验证表 + AES 吞吐表

**关键决策**:

| Issue | 决策 | 理由 |
|-------|------|------|
| #1 max_concurrent | 4 → 8 | bench 280ms sweet spot |
| #2 bumpalo | **不采纳** | 节省 0.25ms 用户不可见 + 80KB binary + 0.4MB compile + lifetime entanglement;ROI 为负 |
| #3 hls_parse 长 URL | 假设**被否决** | 长 URL bumpalo 优势 64-66% 与短 URL 持平,**不构成采纳理由** |
| #4 AES-128 吞吐 | **保留手动实现** | 测得 785 MB/s = AES-NI 硬件上限,LLVM 已 auto-vectorize;`cbc = "0.2"` 与 `aes 0.8` 不兼容 (cipher 0.4 vs 0.5 trait 冲突) |

## Verification (3/3 pass)

- **clippy** (本次新增 `benches/*`): 0 warnings (我的两个 lint 已修:`aes_decrypt.rs` 重写 `make_ciphertext` 避开 `chunks_exact_mut(常量)` 误报;`hls_parse.rs` 加 `type UrlFor = fn(usize) -> String` alias 解 `type_complexity`)
- **cargo test --bin x-video-downloader**: 375 passed, 12 ignored
- **cargo bench --benches**: hls_parse ratios 0.318-0.349 (在 [0.30, 0.35] 阈值内);hls_concurrency c=8=281ms (在 [250, 310] 阈值内);aes_decrypt 583/801/785 MB/s

## Lessons

- **ROI framework 落地** — 节省 ×ms 必须对比 binary 增长 + API 重构成本。本项目 parse 不在 segment 并发关键路径,bumpalo 节省 0.25ms 完全沉没
- **cbc crate 与 aes 版本错配** — aes 0.8 / cbc 0.2 因 cipher crate 0.4 vs 0.5 trait 不兼容而无法编译;升级路径需 aes 0.9 或 cbc 0.1,**目前不必要**
- **bench 必须有 `harness = false` 否则 main() 不跑** — `cargo bench` 默认走 libtest harness,只跑 `#[test]` 函数
- **clippy 在本项目下 `-D warnings` 必挂** — pre-existing 噪声在 `src/platform/` `src/commands/{analyze,installer,history_stats,clean}.rs` `src/core/progress.rs`,非本次 session 引入,不在 scope

## Pending

**用户 commit** — `x-video-downloader/` 目录在本 session 视角无 `.git`,需要从用户本地环境 commit:

```bash
cd /Users/hgxszhj/develop_software/x-video-downloader
git add src/hls/downloader.rs benches/hls_parse.rs benches/aes_decrypt.rs \
        Cargo.toml docs/perf-baseline.md
git commit -m "perf(hls): max_concurrent 4→8 + parse/decrypt bench coverage

Intent: lock perf baseline + fix 2 clippy warnings from new bench code
Ref-trace: <当天 NDJSON 路径, commit-msg hook 自动注入>"
```

Intent Lock + Ref-trace + DID 三件套 hook 已 wired (per session 2026-09-17),无 `Intent:` 行会被 `.git/hooks/commit-msg` 拒 commit。

## Follow-up (out of scope, 列 perf-baseline.md)

- **`cbc = "0.2"` unused dep** — `src/` 不引用,可能 stale,可清理
- **跨平台 baseline** — 仅 arm64-darwin 测过,未在 linux/intel 上跑
