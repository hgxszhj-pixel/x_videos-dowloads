# Handoff Document

## Session: 2026-06-26

### 已完成

| 改进 | 文件 | 说明 |
|------|------|------|
| Claude Code 安全配置改进 | `~/.claude/settings.json` | 添加 22 条 allow/deny 权限规则 |
| PreToolUse Hooks | `~/.claude/hooks/pre-tool-use/block/` | 新增 3 个危险命令拦截 Hook |
| CLAUDE.md 安全规则 | `~/.claude/CLAUDE.md` | 添加安全规则和 Sub-Agent 规范 |
| 第二大脑 Obsidian vault 建设 | `~/obsidian/second-brain/` | llm-wiki 分层结构 + 13 篇文档 |

**改动详情:**

1. **settings.json permissions**:
   - allow: 22 条（git/cargo/read 常用操作）
   - deny: 22 条（rm -rf, git push --force, .env/*.key 读取等）

2. **PreToolUse Hooks**:
   - `curl-bash.json`: 拦截 `curl | bash` 管道命令
   - `dangerous-rm.json`: 拦截 `rm -rf` 等危险删除
   - `dangerous-git-force.json`: 拦截 `git push --force` 等危险 git 操作

3. **CLAUDE.md 新增内容**:
   - 安全规则：禁止读取敏感文件、拦截危险 git 操作
   - Sub-Agent 规范：Explore/Plan 只读、权限最小化、任务完成后汇报结论

**备份位置:**
- `~/.claude/settings.json.bak.20260626_*`
- 回滚：`cp ~/.claude/settings.json.bak.20260626_* ~/.claude/settings.json`

---

### 第二大脑 / Obsidian vault 建设

**Vault**: `~/obsidian/second-brain/`（Obsidian 配置路径）

**llm-wiki 分层结构**:
```
wiki/
├── concepts/      # 消化的概念页
│   ├── dioxus-gui.md
│   ├── rust-downloader-pattern.md
│   └── yt-dlp-integration.md
├── entities/      # 实体页（暂无）
├── raw/
│   ├── articles/  # toutiao 文章（3篇）
│   └── papers/     # PDF 转 Markdown（10篇网络安全培训教材）
├── index.md        # Wiki 索引
└── log.md          # append-only 操作日志
sources/
├── learnings/      # 原始学习笔记（4篇）
└── projects/       # 项目笔记
```

**安装的 Obsidian 插件**:
- `share-to-save`: 云朵按钮保存网页到 vault（已配置 `outputFolder: wiki/raw/articles`）
- `fast-note-sync`: 即时同步 vault 文件变更

**Poppler 已安装**: `brew install poppler` → 可用 `pdftotext` 提取 PDF 文字

**存入 vault 的内容**:

*Toutiao 文章（3篇）*:
- `wiki/raw/articles/toutiao-claude-code-keng-7-core-tips.md` — Claude Code 7个核心心法
- `wiki/raw/articles/toutiao-omnigent-agent-framework.md` — omnigent AI Agent 统一框架
- `wiki/raw/articles/toutiao-cua-driver-hermes-agent.md` — Cua Driver 让 AI 自己操作鼠标

*网络安全培训 PDF（10篇）*:
- `wiki/raw/papers/4A系统介绍和管理要求.md` — 账号/授权/认证/审计
- `wiki/raw/papers/IT资产基线治理培训.md`
- `wiki/raw/papers/安全资产管理培训.md`
- `wiki/raw/papers/定级备案交流-张若澜.md`
- `wiki/raw/papers/关注网络信息安全，提高风险防范意识（全省培训）v3.md`
- `wiki/raw/papers/湖北电信2021年度网络安全管理与技能培训课件0725.md`
- `wiki/raw/papers/互联网暴露面管理和探雷行动.md`
- `wiki/raw/papers/日志采集.md`
- `wiki/raw/papers/渗透测试准备-信息收集.md`
- `wiki/raw/papers/网络安全工单管控(田丹).md`

**MemPalace 知识图谱**: 659 drawers，新增 triples:
- `x_videos-dowloads → has_learning → Dioxus GUI onclick 须用 tokio::spawn`
- `x_videos-dowloads → has_learning → 进度条存储格式 0-10000`
- `claude-code-best-practices → source → toutiao 7个心法`
- `omnigent → has_capability → AI Agent统一接口框架`

**INGEST 的项目经验**:
- `sources/learnings/dioxus-gui-onclick-crash-root-cause.md` — onclick 不能直接 await
- `sources/projects/x-videos-downloads.md` — 新增 2 条经验
- `wiki/concepts/dioxus-gui.md`、`yt-dlp-integration.md`、`rust-downloader-pattern.md`
- `wiki/index.md`、`wiki/log.md`

**参考文章**:
- [用Claude Code写代码不踩坑：7个核心实操心法](https://www.toutiao.com/article/7654537249421378090/)
- [omnigent开源：一个框架打通所有Agent](https://www.toutiao.com/article/7654543242981835298/)
- [Cua Driver让AI自己点鼠标](https://www.toutiao.com/article/7655255272239252008/)

---

## Session: 2026-06-25

### 已完成

| 优化 | 文件 | 说明 |
|------|------|------|
| HeadCache 简化 | `src/downloader.rs`, `Cargo.toml` | ~45行 → ~5行，用 `lru` crate 替代手动 LRU 实现 |

**改动详情:**
- 删除: 手动 Vec + RwLock LRU 实现
- 新增: `lru = "0.10"` crate
- 类型: `HeadCache = lru::LruCache<String, u64>`
- 100条目 LRU 自动管理

**Skills 状态:**
- ✅ ponytail AGENTS.md 已安装到项目根目录
- ✅ OpenCLI v1.8.4 已全局安装 (`npm install -g @jackwener/opencli`)
- ✅ OpenCLI Chrome 扩展已安装 (`~/Downloads/opencli-extension`)

**Tools 验证**:
- `opencli hackernews top --limit 5` ✅
- `opencli bilibili hot --limit 5` ✅

**验证**: `cargo check` ✅ `cargo clippy` ✅

---

## Session: 2026-06-22

### 已完成

| 修复 | 文件 |
|------|------|
| Resolver 逻辑修复（x.com 走 FxTwitterResolver） | `src/fxtwitter.rs`, `src/gui_dioxus.rs` |
| `try_resolve_all` 添加 `default_resolver` 兜底 | `src/platform/mod.rs` |
| GUI 注册 `YouTubeResolver` | `src/gui_dioxus.rs` |
| YouTube 直接 URL 提取（从 `formats[]`） | `src/yt_dlp.rs` |
| YouTube 下载添加 yt-dlp 合并策略（`download_strategy`） | `src/types.rs`, `src/downloader.rs` |

**验证**: `cargo build --release` ✅ `cargo test` ✅ 349 passed

### Skills 安装状态
- ✅ 已安装 60+ skills
- ❌ 以下 10 个 skills **不存在**于 skills.sh 生态：Agent Swarm, Playwright Scout, Terminal Sense, CI Fixer, Release Notes, Data Cleaner, Screenshot QA, Changelog Miner, Dependency Guard, Nightly Runner

---

## Session: 2026-06-21

| 修复 | 文件 |
|------|------|
| `do_download` 进度回调节流（1% 阈值 + 100ms 间隔） | `src/downloader.rs` |
| douyin.rs nested Runtime 修复（`spawn_blocking`） | `src/platform/douyin.rs` |
| STUN 公网 IP 检测集成 | `src/collaboration/{types,server/db,server/handler,client/ws}.rs` |
| 补充测试：`test_resolver_registry_fallback`, `test_calculate_backoff_default` | — |
| yt-dlp JSON `url` 字段提取 | `src/yt_dlp.rs` |

**验证**: `cargo build --release` ✅ `cargo test` ✅ 349 passed, GUI 下载 x.com ✅
