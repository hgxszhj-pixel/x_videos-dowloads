# 2026-10-12 — x-video-downloader 2 carry-over fix (E0433 bench + CLAUDE.md cargo test)

承接 [4-commit landing session](2026-10-12-x-video-downloader-perf-baseline-locked-+-4-open-issues-resolved-+-commit-pending.md) 的「pre-existing tech debt 待后续清理」段,本轮 2 commit 闭环:

## Commit 1 (205ccf0): fix(bench): stub drission_client mod for bin-only bench compilation

**根因**(记忆错位):E0433 不在 bench 文件,在 `src/hls/downloader.rs:177`。
bench 通过 `#[path = "../src/hls/mod.rs"]` 拉入 src 代码,src 内
`#[cfg(target_os="macos")]` 调 `crate::drission_client::decrypt_ego_lite_cookies_for_host(...)`,
bench crate 是独立编译单元,看不到 main.rs 的 private mod。

**修复**:bench 顶部加 14 行 stub mod,只补函数签名返 `Err(anyhow!(...))`,
原调用点 `if let Ok(...)` 走 Err 分支,runtime 行为不变。

**验证**:
- `cargo check --benches` 0 errors (4 pre-existing unfulfilled_lint_expectations 警告保留)
- 之前所有 bench 数字都是上次 perf session 残留,新 bench 改动无法本地验证 — 此 fix 恢复 `cargo bench` 路径

**Why stub 而不提 lib.rs**:Rule 3 surgical change。stub 14 行 vs
"提 drission_client 到 lib.rs"(影响 ~20 个 crate 引用点)或
"加 trait 抽象 downloader 的 cookie 源"(新增 trait + impl + bench stub,
且所有其他模块都需改调用形式)。

## Commit 2 (42fe3ad): docs(claude): cargo test --lib → --bins

**根因**:项目 0 `[lib]` + 3 `[[bin]]`(main, hls_concurrency, hls_parse),
`cargo test --lib` 直接报错 "no library targets"。

**修复**:CLAUDE.md 启动命令清单第 9 行 `cargo test --lib` → `cargo test --bins`
并加注释 `# 项目 bin-only,无 [lib]`。

**验证**:`cargo test --bins` → 375 passed, 12 ignored(与 HANDOFF.md 基线一致)

**Why**:CLAUDE.md 启动命令是"新会话第一眼必跑",写错的命令比没有命令更糟
— agent 撞 3 次才会换(Matt Pocock `/retro` 实测)。lint 这类静态值
不可在 commit 时 lint,只能靠 review。

## Push 阶段

`git push origin gitbutler/workspace` → 978492c..42fe3ad 一次性 push 通过,
无需 force(普通增量 commit)。

## 项目状态

- `gitbutler/workspace` HEAD: `42fe3ad` (本轮 2 commit)
- master 未动
- untracked: .claude/(runtime,gitignored)
- pre-existing test flake `core::trace::tests::test_integration_4_fault_points` 仍 pre-existing,
  本轮 scope 外不处理
