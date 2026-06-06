# CLAUDE.md

@andrej-karpathy-skills:karpathy-guidelines

# Rust x.com Video Downloader

视频下载器，支持 x.com (Twitter), TikTok, 抖音, Bilibili 等平台。

## 构建

```bash
cargo run -- "URL"        # CLI
cargo run -- --gui       # GUI
cargo build --release    # 发行版
```

## 依赖

- yt-dlp, reqwest, tokio, slint, rfd

## 项目结构

```
src/
├── main.rs              # CLI 入口
├── lib.rs               # 库入口
├── config.rs            # 配置管理
├── types.rs             # 类型定义
├── downloader.rs        # 直接下载器
├── yt_dlp.rs            # yt-dlp 集成
├── gui.rs               # GUI (slint)
├── history.rs           # 下载历史/书签
└── resolvers/           # 平台解析器
    ├── mod.rs           # ResolverRegistry
    ├── tiktok.rs        # TikTok 解析器
    ├── douyin.rs        # 抖音解析器
    ├── bilibili.rs      # Bilibili 解析器
    ├── douyin_api.rs    # 抖音 API 客户端
    ├── xbogus.rs        # X-Bogus 签名
    ├── zhixueyun.rs     # 智学云解析器
    └── cookie_pool.rs   # Cookie 池
```

---

# 编程理念

**核心原则**: 谨慎优于速度，简单优于复杂，直接解决当前问题而非预判未来需求。

## 1. 编码前思考 (Think Before Coding)

**不要假设。不要隐藏困惑。主动提出权衡。**

实现前：
- 明确陈述假设。如果不确定，直接问。
- 如果存在多种解释，提出它们——不要静默选择。
- 如果存在更简单的方案，说出来。
- 如果有不清楚的地方，停下来。说出困惑点并询问。

## 2. 简单优先 (Simplicity First)

**最小代码解决问题。不做投机性实现。**

- 不添加需求之外的功能。
- 单次使用的代码不抽象。
- 不添加"灵活性"或"可配置性"除非被要求。
- 不处理不可能发生的错误场景。
- 如果200行可以写成50行，重写。

自问："高级工程师会觉得这过于复杂吗？"如果会，简化。

## 3. 精准修改 (Surgical Changes)

**只触碰必须改的。只清理自己造成的垃圾。**

编辑代码时：
- 不要"改进"相邻代码、注释或格式。
- 不要重构没坏的东西。
- 匹配现有风格，即使你可能用不同方式写。

你的修改造成的孤儿代码：
- 移除因你的修改而未使用的 import/变量/函数。
- 不要移除之前就存在的死代码，除非被要求。

检验标准：每行修改都能追溯到用户需求。

## 4. 目标驱动执行 (Goal-Driven Execution)

**定义成功标准。循环验证直到完成。**

将任务转化为可验证的目标：
- "添加验证" → "为无效输入写测试，然后让它们通过"
- "修复 bug" → "写一个复现 bug 的测试，然后修复它"
- "重构 X" → "确保重构前后测试都通过"

多步骤任务应声明简要计划：
```
1. [步骤] → 验证: [检查方式]
2. [步骤] → 验证: [检查方式]
3. [步骤] → 验证: [检查方式]
```

强有力的成功标准让你能独立循环。弱标准("让它工作")需要不断确认。

---

# 项目特定规则

## Slint GUI 进度条禁则

### 进度回调架构

| 错误做法 | 正确做法 |
|----------|----------|
| `download_segment()` 直接发送回调 | 只更新 `total_downloaded`，由后台任务统一计算 |
| 回调中使用 `percent * 10000.0` 存储 | 使用 `percent * 100.0` 存储到 `AtomicU32` |
| UI 线程用 `10000` 作为终止条件 | UI 线程用 `100` 作为终止条件 |
| 进度计算用 `segment_size * 4` | 后台任务用 `total_downloaded` 和 `total_size` |

### Slint UI 编写

| 错误做法 | 正确做法 |
|----------|----------|
| `font-family: "PingFang SC, Microsoft YaHei"` | `font-family: "PingFang SC"` (单个字体) |
| 同时使用 `width: 700px` 和 `min_width: 500px` | 只用 `min_width` 或只改 `width` |
| `Rectangle { width: root.download-progress * parent.width }` | 用 `HorizontalLayout { alignment: start }` 包裹 |

### 验证清单

修改 GUI 进度代码后必须验证：
1. `download_segment()` 只更新 `total_downloaded`，不发回调
2. 存储格式：`progress_arc.store((p.percent * 100.0) as u32)`
3. UI 更新：`progress_arc.load() as f32 / 100.0`
4. 终止条件：`while progress_arc_for_update.load(...) < 100`
5. 进度条容器使用 `HorizontalLayout { alignment: start }`

---

## Agent Skills

### Issue tracker

GitHub Issues via `gh` CLI. See `docs/agents/issue-tracker.md`.

### Triage labels

Standard labels: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context layout. See `docs/agents/domain.md`.

### GitNexus 集成

| Task | Read this skill file |
|------|---------------------|
| Understand architecture / "How does X work?" | `.claude/skills/gitnexus/gitnexus-exploring/SKILL.md` |
| Blast radius / "What breaks if I change X?" | `.claude/skills/gitnexus/gitnexus-impact-analysis/SKILL.md` |
| Trace bugs / "Why is X failing?" | `.claude/skills/gitnexus/gitnexus-debugging/SKILL.md` |
| Rename / extract / split / refactor | `.claude/skills/gitnexus/gitnexus-refactoring/SKILL.md` |
| Tools, resources, schema reference | `.claude/skills/gitnexus/gitnexus-guide/SKILL.md` |
| Index, status, clean, wiki CLI commands | `.claude/skills/gitnexus/gitnexus-cli/SKILL.md` |

---

**核心洞见**: 过度复杂的代码往往遵循设计模式和最佳实践。问题在于**时机**：在需要之前添加复杂性会导致代码更难理解、引入更多 bug、花更长时间实现、更难测试。

**好代码是简单解决今天问题的代码，而非提前解决明天问题的代码。**

<!-- rtk-instructions v2 -->
# RTK (Rust Token Killer) - Token-Optimized Commands

## Golden Rule

**Always prefix commands with `rtk`**. If RTK has a dedicated filter, it uses it. If not, it passes through unchanged. This means RTK is always safe to use.

**Important**: Even in command chains with `&&`, use `rtk`:
```bash
# ❌ Wrong
git add . && git commit -m "msg" && git push

# ✅ Correct
rtk git add . && rtk git commit -m "msg" && rtk git push
```

## RTK Commands by Workflow

### Build & Compile (80-90% savings)
```bash
rtk cargo build         # Cargo build output
rtk cargo check         # Cargo check output
rtk cargo clippy        # Clippy warnings grouped by file (80%)
rtk tsc                 # TypeScript errors grouped by file/code (83%)
rtk lint                # ESLint/Biome violations grouped (84%)
rtk prettier --check    # Files needing format only (70%)
rtk next build          # Next.js build with route metrics (87%)
```

### Test (60-99% savings)
```bash
rtk cargo test          # Cargo test failures only (90%)
rtk go test             # Go test failures only (90%)
rtk jest                # Jest failures only (99.5%)
rtk vitest              # Vitest failures only (99.5%)
rtk playwright test     # Playwright failures only (94%)
rtk pytest              # Python test failures only (90%)
rtk rake test           # Ruby test failures only (90%)
rtk rspec               # RSpec test failures only (60%)
rtk test <cmd>          # Generic test wrapper - failures only
```

### Git (59-80% savings)
```bash
rtk git status          # Compact status
rtk git log             # Compact log (works with all git flags)
rtk git diff            # Compact diff (80%)
rtk git show            # Compact show (80%)
rtk git add             # Ultra-compact confirmations (59%)
rtk git commit          # Ultra-compact confirmations (59%)
rtk git push            # Ultra-compact confirmations
rtk git pull            # Ultra-compact confirmations
rtk git branch          # Compact branch list
rtk git fetch           # Compact fetch
rtk git stash           # Compact stash
rtk git worktree        # Compact worktree
```

Note: Git passthrough works for ALL subcommands, even those not explicitly listed.

### GitHub (26-87% savings)
```bash
rtk gh pr view <num>    # Compact PR view (87%)
rtk gh pr checks        # Compact PR checks (79%)
rtk gh run list         # Compact workflow runs (82%)
rtk gh issue list       # Compact issue list (80%)
rtk gh api              # Compact API responses (26%)
```

### JavaScript/TypeScript Tooling (70-90% savings)
```bash
rtk pnpm list           # Compact dependency tree (70%)
rtk pnpm outdated       # Compact outdated packages (80%)
rtk pnpm install        # Compact install output (90%)
rtk npm run <script>    # Compact npm script output
rtk npx <cmd>           # Compact npx command output
rtk prisma              # Prisma without ASCII art (88%)
```

### Files & Search (60-75% savings)
```bash
rtk ls <path>           # Tree format, compact (65%)
rtk read <file>         # Code reading with filtering (60%)
rtk grep <pattern>      # Search grouped by file (75%). Format flags (-c, -l, -L, -o, -Z) run raw.
rtk find <pattern>      # Find grouped by directory (70%)
```

### Analysis & Debug (70-90% savings)
```bash
rtk err <cmd>           # Filter errors only from any command
rtk log <file>          # Deduplicated logs with counts
rtk json <file>         # JSON structure without values
rtk deps                # Dependency overview
rtk env                 # Environment variables compact
rtk summary <cmd>       # Smart summary of command output
rtk diff                # Ultra-compact diffs
```

### Infrastructure (85% savings)
```bash
rtk docker ps           # Compact container list
rtk docker images       # Compact image list
rtk docker logs <c>     # Deduplicated logs
rtk kubectl get         # Compact resource list
rtk kubectl logs        # Deduplicated pod logs
```

### Network (65-70% savings)
```bash
rtk curl <url>          # Compact HTTP responses (70%)
rtk wget <url>          # Compact download output (65%)
```

### Meta Commands
```bash
rtk gain                # View token savings statistics
rtk gain --history      # View command history with savings
rtk discover            # Analyze Claude Code sessions for missed RTK usage
rtk proxy <cmd>         # Run command without filtering (for debugging)
rtk init                # Add RTK instructions to CLAUDE.md
rtk init --global       # Add RTK to ~/.claude/CLAUDE.md
```

## Token Savings Overview

| Category | Commands | Typical Savings |
|----------|----------|-----------------|
| Tests | vitest, playwright, cargo test | 90-99% |
| Build | next, tsc, lint, prettier | 70-87% |
| Git | status, log, diff, add, commit | 59-80% |
| GitHub | gh pr, gh run, gh issue | 26-87% |
| Package Managers | pnpm, npm, npx | 70-90% |
| Files | ls, read, grep, find | 60-75% |
| Infrastructure | docker, kubectl | 85% |
| Network | curl, wget | 65-70% |

Overall average: **60-90% token reduction** on common development operations.
<!-- /rtk-instructions -->