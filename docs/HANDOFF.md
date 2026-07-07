# Handoff Document

## Session: 2026-07-07

### 重构计划执行 (REFACTOR_PLAN.md)

按 `docs/REFACTOR_PLAN.md` 推进，完成 Phase 1.1 / 2.1 / 1.2，全部编译+测试验证通过。

| Phase | 内容 | 结果 |
|-------|------|------|
| 1.1 | 修复 `core/mod.rs` 循环依赖 | ✅ |
| 2.1 | 删除孤儿模块 `gui_backup/` | ✅ |
| 1.2 | 拆分 `main.rs` (1062→126 行) | ✅ |
| 3.1 | 拆分 `downloader.rs` (1137→4 模块) | ✅ |
| 1.3 | 拆分 `gui_dioxus.rs` (1188→6 模块) | ✅ |

**Phase 1.1 — core/mod.rs 循环依赖修复：**
- 删除 `core/mod.rs` 中对 `crate::downloader`/`muxer`/`subtitle`/`danmaku`/`queue` 的 re-export（24-32 行）
- 经 grep 确认全代码库无任何调用方使用这些 `core::` re-export —— 纯死代码
- `core` 现在只保留真正的核心原语：`constants`/`cache`/`progress`/`segment`

**Phase 2.1 — 删除 gui_backup/：**
- `src/gui_backup/` 全代码库零引用、未注册为模块（lib.rs 只有 `pub mod gui;`）
- 已删除（明确路径，无通配符）

**Phase 1.2 — 拆分 main.rs (1062 → 126 行，-88%)：**

| 新文件 | 行数 | 职责 |
|--------|------|------|
| `src/main.rs` | 126 | 模块声明 + `main()` 命令路由 |
| `src/cli.rs` | 132 | `Cli` 参数结构 + `DownloadMode` |
| `src/resolver_chain.rs` | 165 | `get_video_info`/`fetch_with_ytdlp`/`is_x_com_url` |
| `src/commands/download.rs` | 567 | 下载主流程 + 路径 helper |
| `src/commands/history.rs` | 88 | 历史与书签命令 |
| `src/commands/serve.rs` | 100 | 协作服务器 + 团队管理 |
| `src/commands/mod.rs` | 9 | 子命令模块导出 |

**验证：**
- `cargo check`: ✅ 0 errors，新文件零 warning
- `cargo clippy`: 新代码仅 1 处预存 warning（`download.rs:307` 的 `.clone()` on Copy，源自原 `main.rs:636` 的 DEBUG 代码，忠实搬运）
- `cargo test --lib`: ✅ 184 passed

**⚠️ 发现预存 flaky 测试（非本次引入）：**
- `i18n::tests::test_font_family_chinese` 偶发失败（断言系统字体含 "PingFang SC"）
- 根因：`i18n::CURRENT_LANG` 是全局静态 Mutex，`test_font_family_english` 与 `test_font_family_chinese` 并发执行时存在竞态（english 把全局设成 "en"，chinese 读到 "en" → "System" → 不含 "PingFang SC"）
- 证明：单独跑 ✅ / 单线程 ✅ / 多线程连跑 2 次 ✅ —— 纯调度竞态
- 修复方向：测试内用临时语言设置 + 恢复，或加 `--test-threads=1`（未修，超重构范围）

**⚠️ 架构发现（未修，超出 Phase 1.2 范围）：**
- Cargo.toml 只有 `[[bin]]` 无 `[lib]`，但 `src/lib.rs` 存在 → **lib 和 bin 都被编译**，所有模块编译两次（lib 7 warnings + bin 20 warnings，大量重复）
- `main.rs` 用 `crate::` 路径，完全无视 lib —— lib.rs 实质是死代码（80 行注释 + `pub mod` 声明）
- 建议：后续让 main.rs 依赖 lib crate（`x_video_downloader::`），删除 main.rs 的 `mod` 重复声明

**剩余 Phase（未做）：**
- 2.2 统一类型命名（Platform/DownloadStatus/HistoryItem 多处重复）
- 2.3 提取 `db.rs` SQL → schema.sql
- 2.4 移除 40+ `#[allow(dead_code)]`

**Phase 1.3 — 拆分 gui_dioxus.rs (1188 → 6 模块):**

按**真实代码缝隙**拆分（非计划理想化的 components/state/handlers 目录结构）。`AppComponent`（~700 行 RSX）整体搬到 `app.rs` 不拆子组件 —— 因 Dioxus RSX 闭包捕获 `Signal` 语义微妙，后台 CLI 只能验证"窗口出现"无法逐按钮测交互，拆子组件有"编译过但按钮失效"的静默回归风险。

| 新文件 | 行数 | 职责 |
|--------|------|------|
| `src/gui_dioxus/mod.rs` | 58 | `run_gui()` 入口 + 模块声明 |
| `src/gui_dioxus/app.rs` | 723 | `AppComponent`（RSX 视图，逐字保留） |
| `src/gui_dioxus/handlers.rs` | 177 | `fetch_video_info_impl`/`start_download_impl`/`batch_download_impl` |
| `src/gui_dioxus/types.rs` | 129 | 显示用数据结构 + `AppState`/`BatchState` 等 + `format_file_size` |
| `src/gui_dioxus/store.rs` | 101 | 全局 History / `CONFIG_VERSION` / 配置热重载 / osascript 通知 |
| `src/gui_dioxus/theme.rs` | 49 | `AppTheme` 枚举 |

关键点：
- 结构字段用 `pub(super)` 供兄弟模块（app.rs 构造 `BatchItemState{}` 等、handlers.rs 构造 `VideoInfoDisplay`、store.rs 构造 `HistoryItem`）访问
- `mod.rs` 保留 `#![allow(dead_code)]` 内层属性，子模块继承 → 无新 dead_code warning
- `run_gui()` 为唯一 `pub`（main.rs 调用 `gui_dioxus::run_gui()`），其余 `pub(super)`
- `launch(app::AppComponent)` —— `AppComponent` 标 `pub(super)` 供 mod.rs 引用

**验证（含运行时）：**
- `cargo check`: ✅ 0 errors，gui_dioxus 新文件**零 warning**，bin 20 + lib 7 与拆分前完全一致
- `cargo test --lib`: ✅ 184 passed
- `cargo run --release -- --gui`: ✅ release 编译 2m31s 成功，GUI 启动，`list_windows` 确认窗口出现（`x-video-downloader` pid, `title: "Dioxus App"`, 800×628, `is_on_screen: true`），无 panic/error
- ⚠️ 仅验证窗口启动，未逐按钮测交互（后台 CLI 限制）

**Phase 3.1 — 拆分 downloader.rs (1137 → 4 模块):**

利用 Rust 多 `impl` 块 + 子模块可访问父结构体私有字段（descendant 可见性）的特性拆分。

| 新文件 | 行数 | 职责 |
|--------|------|------|
| `src/downloader/mod.rs` | 675 | `VideoDownloader` 结构体 + 构造器 + `download` 调度 + `do_download`/`get_content_length`/`resume_download`/`download_with_ytdlp`/`calculate_backoff` + tests |
| `src/downloader/config.rs` | 58 | `DownloaderConfig` + `Default` |
| `src/downloader/parallel.rs` | 311 | `parallel_download` + `calculate_segment_ranges`(pub(super)) + 自适应线程/进度 helper |
| `src/downloader/merge.rs` | 137 | `merge_segments`(pub(super)) |

关键点：
- `mod.rs` 保留 `#![allow(dead_code)]`，子模块继承 → 无新 dead_code warning
- 跨模块调用的方法用 `pub(super)`（`parallel_download`/`merge_segments`/`calculate_segment_ranges`），其余私有
- `mod.rs` 用 `pub use config::DownloaderConfig;` 对外重导出，外部调用方（main.rs/commands）无需改

**验证：**
- `cargo check`: ✅ 0 errors，bin/lib warning 计数与拆分前完全一致（零新增）
- `cargo clippy`: downloader 仅 1 处预存 warning（`parse_size/parse_speed` test 模块，原 `downloader.rs:979` 移位）
- `cargo test --lib`: ✅ 184 passed（含 14 个 downloader 测试全过）

---

## Session: 2026-07-07 (续)

### Phase 2.2/2.3/2.4 重构执行

按用户确认计划（`~/.claude/plans/robust-stargazing-shell.md`），完成剩余三个中优先级清理阶段，全部编译+测试验证通过。

**⚠️ 修订 REFACTOR_PLAN.md 错误（基于全代码库 grep 验证）：**
- "保留 `crate::types::Platform`" → 实际 `types.rs` 完全没有 `Platform` enum，仅 `history.rs` 定义
- "`HistoryItem` 重复" → 实际 `gui_dioxus::HistoryItem` 是 `HistoryEntry` 的 UI 字符串投影（5 字段 vs 10 字段），不是重复
- "40+ dead_code" → 实际 **128 个**，32 个文件（计划低估 3 倍）

#### Phase 2.2 — 类型命名统一

**方案：C 方案（最保守）** — 删除 dead 变体即可，不改名

| 改动 | 文件 | 内容 |
|------|------|------|
| 删除 `DownloadStatus::InProgress` 和 `Bookmarked` | `src/history.rs` | grep 验证全项目零使用 |

**未做（基于事实修订）：**
- `Platform`：无需统一（types.rs 没有）
- `HistoryItem` vs `HistoryEntry`：不同抽象，保留
- 历史 `DownloadStatus` 与 `types::DownloadStatus` 关系：变体名（Completed/Failed）已语义对齐，保持分离

**验证：** `cargo check`: ✅ 0 errors，`cargo test --lib`: ✅ 184 passed

---

#### Phase 2.3 — 提取 db.rs SQL 到 schema.sql

| 新文件 | 行数 | 内容 |
|--------|------|------|
| `src/collaboration/server/schema.sql` | 48 | 4 个 CREATE TABLE（teams/devices/tasks/offline_messages）+ 3 个 CREATE INDEX |

| 改动 | 文件 | 效果 |
|------|------|------|
| `init_schema()` 改写 | `src/collaboration/server/db.rs` | 53 行内联 `r#"..."#` → 1 行 `include_str!("schema.sql")` |
| WAL pragma 保留 | 同上 | 运行时配置，独立于 schema |

**关键点：**
- 严格按 db.rs 现有 L54-99 内容提取（`last_seen TEXT NOT NULL`、`url TEXT NOT NULL UNIQUE`、`version INTEGER NOT NULL DEFAULT 1` 等约束逐字保留）
- 编译期嵌入，无运行时 IO 路径问题

**验证：** `cargo check`: ✅ 0 errors，`cargo test --lib`: ✅ 184 passed

---

#### Phase 2.4 — 死代码清理（Round 1）

**关键发现：** 原计划"46 个安全可删"基于跨文件 grep，但实际验证后：
- 大量看似 dead 的结构体（`FxTweet`/`FxRawText`/`CookieEntry`/`VideoDetailResponse` 等）被 serde 反序列化或同文件内方法使用
- 看似 dead 的方法（`mark_complete`/`obfuscate`/`invalidate_by_account` 等）被 `#[cfg(test)]` 测试代码引用

**实际安全可删项（验证双重确认）：**

| 文件 | 删除项 |
|------|--------|
| `src/i18n.rs` | `load_from_file`, `get_current_lang`, `get_available_languages` (+ `std::path::PathBuf` import) |
| `src/collaboration/client/discovery.rs` | `get_public_endpoint`, `try_direct_connect` (+ `anyhow::Result` import) |
| `src/collaboration/client/queue.rs` | `get_all_tasks`, `sync_from_server` |
| `src/collaboration/discovery/mod.rs` | `NatType` 整个 enum |
| `src/collaboration/transfer/downloader.rs` | `download_parallel` (83 行) |
| `src/danmaku.rs` | `save_danmaku` (+ `std::path::Path` import) |
| `src/platform/douyin_api.rs` | `VideoDetailResponse::get_first_video` |
| `src/platform/zhixueyun.rs` | `set_token` |

**合计：14 项，删除 203 行（8 文件）**

**保留（被测试/serde/同文件方法引用）：**
- `fxtwitter.rs` 全部 7 个结构体 — serde 反序列化必需
- `cookie_pool::CookieEntry` / `douyin_api::VideoDetailResponse` 等 — 同文件内 impl 使用
- `queue.rs::mark_complete` / `update_status_to_queued` — tests 用（L220/249/204）
- `history.rs::remove_bookmark` / `history_count` / `bookmark_count` / `get_all` — tests 用
- `token_manager::obfuscate` / `deobfuscate` / `set_tiktok_ms_token` — 内部 + tests 用
- `cookie_pool::invalidate_by_account` / `valid_count` — tests 用
- `types.rs` 11 个 DownloadRequest builder 方法 — 预留 API
- 平台 SDK 主要方法（zhixueyun 下载、tulingpython 等） — 活跃使用

**验证：** `cargo check`: ✅ 0 errors，`cargo check` warnings: **20（与 baseline 持平）**，`cargo test --lib`: ✅ 184 passed

**⚠️ 经验教训：**
- 跨文件 `grep uses_elsewhere=0` 对 `#[allow(dead_code)]` 标记**强误导**——serde 派生结构体在同文件内被宏读取
- `#[allow(dead_code)]` 在 Rust 中对方法 vs 字段的判断准确性不同：方法级判断准确，字段级常因 serde 误报
- 删除前必须双重验证：grep + `#[cfg(test)]` 代码扫描 + cargo check

---

### 三 Phase 提交

```
ae9911a refactor(dead_code): 删除 14 个零使用 dead_code 方法/类型 (Phase 2.4)
b0791d8 refactor(db): 提取 SQLite schema 到独立 schema.sql (Phase 2.3)
2b63bea refactor(history): 删除 DownloadStatus 未使用变体 InProgress/Bookmarked (Phase 2.2)
```

**剩余 Phase（未做）：** 无（REFACTOR_PLAN.md 中标注的 2.2/2.3/2.4 全部完成）

---

## Session: 2026-07-04 (续)

### 代码结构分析与重构计划

**分析结果:**
| 文件 | 行数 | 问题 |
|------|------|------|
| `src/gui_dioxus.rs` | 1188 | 单体 GUI，UI + 业务逻辑混杂 |
| `src/downloader.rs` | 1137 | 5 种职责混在一起 |
| `src/main.rs` | 1062 | CLI + 下载编排 + resolver 链混杂 |
| `src/collaboration/server/db.rs` | 891 | 530+ 行 SQL 内联 |
| `src/types.rs` | 806 | 40+ dead_code，Builder 模式不一致 |

**架构问题:**
- `core/mod.rs` 循环依赖 (re-exports 造成)
- `Platform`, `DownloadStatus`, `HistoryItem` 命名冲突
- `gui_backup/` 孤儿模块
- `collaboration/` 最深 4 层嵌套

**重构计划:** 已写入 `docs/REFACTOR_PLAN.md`

**计划概要:**
| Phase | 内容 | 优先级 |
|-------|------|--------|
| Phase 1.1 | 修复 `core/mod.rs` 循环依赖 | 高 |
| Phase 1.2 | 拆分 `main.rs` (1062→3模块) | 高 |
| Phase 1.3 | 拆分 `gui_dioxus.rs` (1188→组件化) | 高 |
| Phase 2.1-2.4 | 清理、死代码、类型统一 | 中 |
| Phase 3.1 | 拆分 `downloader.rs` | 长期 |

---

### Agent-Reach 安装

**项目：** https://github.com/Panniantong/Agent-Reach
**状态：** ✅ 已安装 (v1.4.0)

**可用渠道 (8/16)：**
| 渠道 | 状态 |
|------|------|
| GitHub 仓库和代码 | ✅ |
| YouTube 视频和字幕 | ✅ |
| Reddit 帖子和评论 | ❌ 需安装 rdt-cli |
| 微信公众号文章 | ✅ |
| V2EX | ✅ |
| RSS/Atom 订阅源 | ✅ |
| 全网语义搜索 (Exa) | ✅ |
| 任意网页 (Jina Reader) | ✅ |
| B站视频、字幕和搜索 | ✅ |

**可选（未安装）：** Twitter、小红书、微博、小宇宙播客、雪球、抖音、LinkedIn

**安装命令：** `pipx install agent-reach && agent-reach install --env=auto`
**Skill 位置：** `~/.claude/skills/agent-reach/`

---

### Caveman 默认启用配置

**状态：** ✅ 已配置 auto-start

**已完成：**
1. 下载 caveman hooks → `~/.claude/hooks/`
   - `caveman-activate.js`
   - `caveman-config.js`
   - `caveman-mode-tracker.js`
   - `caveman-stats.js`
2. SessionStart hooks 已添加到 `~/.claude/settings.json`
3. Skill 已存在于 `~/.claude/skills/caveman/SKILL.md`

**验证：** 重启 Claude Code 后自动启用。可用 `cat ~/.claude/.caveman-active` 检查（应为 `full`）。

---

### GSD Hooks 清理

**问题：** `~/.claude/settings.json` 中引用了已删除的 gsd hooks，启动时报错

**已清理：**
- `gsd-validate-commit.sh` ❌
- `gsd-phase-boundary.sh` ❌
- `gsd-context-monitor.js` ❌
- `gsd-prompt-guard.js` ❌
- `gsd-read-guard.js` ❌
- `gsd-workflow-guard.js` ❌
- `gsd-check-update.js` ❌
- `gsd-session-state.sh` ❌

---

### HLS 下载器 referer 支持

**新增：**
- `--referer` CLI 参数 → 绕过 CDN 防盗链
- `HlsDownloaderConfig.referer` 字段
- `HlsDownloader::referer()` builder 方法

**用法：**
```bash
cargo run -- --direct --referer "https://vip.tulingpyton.cn" 'https://...m3u8?sign=...'
```

**验证：** 小鹅通 m3u8 仍 403（sign 时效），但 referer 参数已生效

---

## Session: 2026-07-04 (续)

### Caveman 项目调研 + 本地配置

**项目：** https://github.com/juliusbrussee/caveman
**Stars:** 83k | **作用：** Claude Code skill，压缩输出 65%（1214→294 tokens 平均）

**命令：**
- `/caveman [lite|full|ultra|wenyan]` — 压缩级别
- `/caveman-commit` — ≤50 字提交信息
- `/caveman-review` — 单行 PR 评论
- `/caveman-stats` — token 节省统计
- `/caveman-compress <file>` — 重写内存文件，永久省 tokens

**Benchmark：**
| 任务 | 正常 | Caveman | 节省 |
|-----|-----|---------|-----|
| Explain React re-render bug | 1180 | 159 | 87% |
| Fix auth middleware token expiry | 704 | 121 | 83% |
| Docker multi-stage build | 1042 | 290 | 72% |
| **平均** | **1214** | **294** | **65%** |

**本地状态：**
- Skill 已安装：`~/.claude/skills/caveman/`
- 未默认启用，需用户说 "caveman mode" 激活
- 要默认启用需在 `settings.json` 加 permission rules + SessionStart hook

**⚠️ 注意：** 只压缩输出 tokens，输入不动。Skill 本身每轮加 ~1-1.5k 输入。真正赢的是可读性和速度，不是成本。

**待完成：** 用户手动在 `settings.json` 的 `permissions.rules` 加两条规则后，才能自动启用 caveman

---

## Session: 2026-07-04 (续)

### HLS 下载器改进（GitHub 调研后实现）

**指数退避重试机制：**
- `src/hls/downloader.rs` — 添加 `HlsDownloaderConfig` 配置结构体
- 重试次数：`retry_count` 默认 10 次（原固定 3 次）
- 指数退避：`delay = min(100ms × 2^attempt, 20s)`
- 新增 `skip_unavailable` 选项：跳过失败分段继续合并

**代理支持：**
- `HlsDownloader::with_config(config)` — 使用配置创建带代理的下载器
- 支持 `socks5://`, `http://`, `https://` 代理协议
- 自动添加 `http://` 前缀（如果未指定）

**错误分类：**
- 可重试：TLS 超时、连接 reset、临时不可用
- 不可重试：404、401、400、403、connection refused

**验证：** `cargo test: 374 passed ✅`

---

## Session: 2026-07-04

### 文件名自动提取 + 课程视频列表

**文件名自动提取 (`--direct` 模式):**
- `src/main.rs` — 添加 `extract_filename_from_url()` 函数
- 从 URL 路径自动提取有意义的文件名
- 示例：`/playlist_eof.m3u8` → `playlist_eof`
- 特殊字符自动转换为下划线

**TulingPython 课程视频列表:**
- `src/platform/tulingpython.rs` — 增强 `guidance()` 显示课程视频列表
- 添加 `extract_video_list()` 函数提取所有课程
- 添加 `/live_pc/pc/(l_\w+)` URL 模式支持
- 显示用户已购买的所有课程列表（最多 20 个）

**测试输出：**
```
课程视频列表 (共 9 个):
  - Python全套实战项目班
  - 图灵Python2025 爬虫+AI+全栈实战项目班
  - 爬虫基础
  - PC爬虫-2026
  ...
当前课程: Python全套实战项目班
```

---

### TulingPythonResolver + CatCatch 扩展

**实现完成：**
- `src/platform/tulingpython.rs` — 小鹅通平台解析器（TulingPython）
- `scripts/cat-catch/` — Chrome 扩展（m3u8 URL 拦截器）
- 修复 `src/scrapling_client.rs` — 移除 `quote()` 包裹 URL 的 bug，添加 `nonlocal` 闭包变量
- `src/platform/mod.rs` — 注册 `TulingPythonResolver`
- `src/platform/url_matcher.rs` — 添加 `vip.tulingpyton.cn` URL 模式
- `src/history.rs` — 添加 `Platform::TulingPython` 变体
- `src/main.rs` — `ResolverError::Detailed` 直接展示给用户，不再 fallback
- `src/lib.rs` — 导出 `scrapling_client`, `scrapling_extractor` 模块

**验证结果：**
```
cargo test: 374 passed ✅
cargo build: 0 errors ✅
```

**测试输出：**
```
cargo run -- 'https://vip.tulingpyton.cn/p/t_pc/course_pc_detail/column/p_6981db02e4b0694c34d6c8dc'
→ ERROR: [图灵Python] 小鹅通课程需要用 CatCatch 扩展拦截视频 URL
提示: 1. 安装 CatCatch 扩展: chrome://extensions/ > 开发者模式 > 加载已解压
     2. 选择 scripts/cat-catch 目录
     3. 浏览器打开 https://vip.tulingpyton.cn/p/course/member/p_628ef20ee4b0cedf38b6e660 并登录账号
     4. 播放视频，扩展自动拦截 m3u8 URL
     5. 将 m3u8 URL 粘贴到下载器
当前课程: Python全套实战项目班
```

**CatCatch 扩展文件：**
```
scripts/cat-catch/
├── manifest.json      # manifest v3 配置
├── background.js     # webRequest.onCompleted 拦截 m3u8 URL
├── popup/
│   ├── popup.html   # 粉红暗色主题 UI
│   └── popup.js     # 列表展示 + 复制/下载按钮
├── icons/icon*.png   # 粉红猫咪图标
└── README.md        # 使用说明
```

**核心发现：**
- Scrapling URL 解析 bug：原 `quote(&url_owned)` 包裹导致 URL 被引号包住 → 已修复
- Python 闭包 `nonlocal` 对 module-level var 无效 → 改用 dict wrapper `_state`
- `xe.course.business.middle.page.data.get` API 无 Cookie 时返回空列表，课程数据需要登录 session
- Handoff 前记录：小鹅通 sign 签名有时效，需实时拦截

**CatCatch 使用流程：**
1. `chrome://extensions/` → 开启开发者模式 → 加载已解压 → 选 `scripts/cat-catch`
2. 浏览器访问 `vip.tulingpyton.cn` 并登录
3. 进入课程播放视频，扩展自动捕获 m3u8 URL
4. 将 m3u8 URL 粘贴到 x-video-downloader 下载

---

### HLS 下载器完成 + 小鹅通调研

**HLS 下载器实现 (✅ 完成)：**
- `src/hls/` 模块：parser.rs, crypto.rs, downloader.rs, mod.rs
- 支持 Master Playlist → Media Playlist 自动解析
- AES-128-CBC 解密
- 并发分段下载 + 进度回调
- Bug 修复：`main.rs` HLS 输出路径 `%(ext)s` → `.mp4`

**download_direct m3u8 自动检测 (✅ 完成)：**
- URL 含 `.m3u8` 自动走 HlsDownloader
- 文件：`src/main.rs` download_direct() 函数

**nnyy.in HLS 测试：**
- Scrapling m3u8 拦截 ✅
- Master Playlist 解析 ✅
- 分段下载 ❌ TLS builder error（hnts.ymuuy.com 需要特殊 TLS 配置）
- 原因：reqwest TLS 握手失败，需代理或 native-tls 配置

**vip.tulingpyton.cn (小鹅通) 分析：**
- Cookie: `/Users/hgxszhj/Downloads/fd2dc2d6-2bc8-4e85-8965-b19b0fb181fb.txt`
- 课程列表 API: `xe.course.business_go.column.items.get` ✅
- 视频详情 API: `xe.goods.detail.get` ✅
- 视频是直播回放（resource_type: 4），无 DRM 加密
- cat-catch 扩展方案：chrome webRequest API 拦截播放器请求
- m3u8 URL 示例（含 sign 签名）：
  ```
  https://v-vod-k.xiaoeknow.com/{id}/drm/main.m3u8?sign=...&t=...&us=...&time=...&uuid=...
  ```
- 签名有效期问题：sign 有时效，需实时拦截
- HLS 解析成功：4137 分段，8272.6 秒

**结论：**
- HLS 解析链路 ✅ 完全可用
- 小鹅通 CDN 下载需要实时 sign（cat-catch 拦截方案）
- nnyy.in TLS 问题需配置代理解决

---

## Session: 2026-07-03

### 视频下载测试

**1. x.com 视频下载：**
- URL: `https://x.com/ask_your99357/status/2071801495159452092`
- 结果：FxTwitter 解析出视频 URL，直接下载 `video.twimg.com` 成功
- 输出：`~/Movies/chudai_test.mp4` (895.7MB)

**2. nnyy.in 电视剧下载：**
- URL: `https://nnyy.in/dianshiju/202552712.html`
- 发现：12 个备用源，均为 HLS m3u8 流
- 方式：用 yt-dlp 直接下载 m3u8 源
- 输出：`~/Movies/wuzhi.mp4` (381.9MB, 49:08)

**HLS 流信息：**
- 源：`hn.bfvvs.com` (HN 线路)
- 格式：HLS AES-128 加密
- 分辨率：1920x1080
- yt-dlp 自动处理加密，直接下载成功

---

### 调研：headroom 上下文压缩工具

**结论：** headroom 安装成功但功能受限。

| 组件 | 状态 |
|------|------|
| headroom 版本 | 0.2.0（pip 包 0.2.2） |
| 安装路径 | `~/.virtualenvs/headroom/bin/headroom` |
| 支持命令 | 仅 `proxy` |
| minimax 兼容 | ❌ `x-api-key` 认证格式不兼容 |
| 标准 API 兼容 | ✅ |

**安装命令：**
```bash
python3.13 -m venv ~/.virtualenvs/headroom
~/.virtualenvs/headroom/bin/pip install "headroom-ai[proxy,mcp,code,memory]"
```

**使用方式：**
```bash
~/.virtualenvs/headroom/bin/headroom proxy --port 8787
```

**未完成：** Docker 版可获完整功能（doctor/wrap/dashboard/learn），但用户选择不安装。

---

### 新功能：专用 m3u8 解析器

**状态：** ✅ 实现完成，待实际下载验证

**新增文件：**
```
src/hls/
├── mod.rs      # 模块导出
├── parser.rs   # m3u8 解析器（M3u8Parser）
├── crypto.rs   # AES-128 解密（AesDecryptor）
└── downloader.rs # HLS 下载器（HlsDownloader）
```

**新增解析器：**
```
src/platform/nnyy.rs  # nnyy.in 专用解析器
```

**依赖 (Cargo.toml)：**
```toml
m3u8 = "0.1"
aes = "0.8"
cbc = "0.2"
generic-array = "0.14"
typenum = "1.17"
```

**功能：**
- m3u8 播放列表解析（Master Playlist + Media Playlist）
- AES-128-CBC 解密（支持 IV）
- 并发分段下载
- 自动合并分段
- PKCS7 padding 移除

**nnyy.in 测试结果：**
- ❌ 页面是 JavaScript 渲染的，静态 HTML 不包含 m3u8 URL
- ✅ NnyyResolver 正确注册并尝试解析
- ✅ Scrapling 降级处理 JS 渲染页面

**新增功能：Scrapling m3u8 拦截**
- `scrapling_client.rs`: 添加 `m3u8_urls` 字段到响应
- Python 脚本启用 Playwright 网络响应拦截
- 捕获所有包含 `m3u8` 的网络请求
- `scrapling_extractor.rs`: 添加 `extract_m3u8_url()` 函数

**工作流程：**
```
JS 渲染页面 → Scrapling 拦截网络请求 → 捕获 m3u8 URL → HlsDownloader 下载
```

**HLS 下载测试 (2026-07-04)：**
- nnyy.in 电视剧 `https://nnyy.in/dianshiju/202552712.html`
- ✅ Scrapling 拦截捕获 3 个 m3u8 URL
- ✅ Master Playlist → 子播放列表自动解析
- ✅ HlsDownloader 正确调用并解析分段
- ❌ TLS `builder error` — `hnts.ymuuy.com:65` 需要特殊 TLS 配置
- ❌ 部分分段连接失败（可能需要代理）
- **结论**：HLS 解析链路完全可用，下载层需调整 TLS 或使用代理

**待完成：**
- TLS 配置调优（reqwest TLS 版本/cipher）
- Master Playlist 变体选择逻辑

---

### 新功能：专用 m3u8 解析器（规划中）

**用户需求：** 添加不依赖 yt-dlp 的专用 HLS/m3u8 解析器

**参考方案：**
- `m3u8-rs`（sdroege）— Rust m3u8 解析库，基于 nom
- N_m3u8DL-RE — C# 成熟方案，支持 AES-128 解密、并发下载
- yt-dlp 内部 HLS 处理

**规划中：**
- 新建 `src/hls/` 模块
- 集成 m3u8 解析 crate
- 支持 AES-128 解密
- 并发分段下载

---

## Session: 2026-06-30

### 已完成

| 改进 | 文件 | 说明 |
|------|------|------|
| downloader.rs 拆分 | `src/core/` | constants.rs, cache.rs, progress.rs, segment.rs |
| gui 模块拆分 | `src/gui/` | mod.rs, theme.rs, types.rs, helpers.rs |
| 测试导入修复 | `src/downloader.rs` | parse_speed/parse_size 导入 |
| Multi-Agent 开发方法写入规则 | `CLAUDE.md` | Anthropic 三 Agent 模式：Planner/Generator/Evaluator |

**文件变更:**
```
src/core/: constants.rs, cache.rs, progress.rs, segment.rs
src/gui/: mod.rs, theme.rs, types.rs, helpers.rs
src/downloader.rs: 1322 → 1110 行 (-16%)
CLAUDE.md: 新增第 5 条 Multi-Agent 开发循环
```

**验证:**
- `cargo check`: 0 errors ✅

**未完成:** gui_dioxus.rs 组件拆分（Dioxus 耦合）

### 调研：Unlimited-OCR 集成

**调研结论：** baidu/Unlimited-OCR 是长视野 OCR 模型（12k ⭐），但本地运行有障碍。

| 方案 | 状态 | 说明 |
|------|------|------|
| HF Space API | ❌ 网络不稳定 | 代理环境下 TLS 握手超时 |
| 本地 CLI | ❌ torch 2.2.2 不够 | 需要 torch 2.4+（未发布） |
| ModelScope | ⏸️ 模型已下待用 | 等 torch 升级后可用 |

**已创建脚本:**
- `scripts/unlimited_ocr_cli.py` — 本地模型版（torch 2.4+）
- `scripts/unlimited_ocr_api.py` — HF Space API 版（重试健壮版）
- `scripts/unlimited_ocr_mscope.py` — ModelScope 版

**关键依赖（conda env `unlimited-ocr`）:**
```
transformers==4.44.0
gradio_client
pysocks + httpx[socks]  # SOCKS5 代理支持
modelscope
datasets, simplejson
```

---

## Session: 2026-06-29

| 改进 | 文件 |
|------|------|
| 测试编译错误修复 | `tests/types_test.rs` |
| Clippy 警告修复 | `src/yt_dlp.rs` |

**验证:** build + test + clippy 全通过 ✅

---

## Session: 2026-06-26

| 改进 | 文件 |
|------|------|
| HeadCache 简化（~45行 → ~5行） | `src/downloader.rs` |
| `lru = "0.10"` crate | `Cargo.toml` |

---

## Session: 2026-06-22

| 修复 | 文件 |
|------|------|
| x.com 走 FxTwitterResolver | `src/fxtwitter.rs` |
| try_resolve_all 兜底 | `src/platform/mod.rs` |
| YouTube URL 提取 | `src/yt_dlp.rs` |
