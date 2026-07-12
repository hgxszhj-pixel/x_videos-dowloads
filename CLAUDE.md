# CLAUDE.md

@andrej-karpathy-skills:karpathy-guidelines

# Rust x.com Video Downloader

视频下载器，支持 x.com (Twitter), TikTok, 抖音, Bilibili 等平台。

## Commit 规则（Intent Lock）

每次 commit message 必须包含一行 `Intent: <一句话>`，否则被 `.git/hooks/commit-msg` 拒绝。

```text
fix(hls): 修复 segment index 越界

Intent: 修复 segment index 越界 bug，不重构现有 download API
```

详见 [`scripts/README_intent_lock.md`](scripts/README_intent_lock.md)。紧急情况可用 `git commit --no-verify`。

## 构建

```bash
cargo run -- "URL"        # CLI
cargo run -- --gui       # GUI (Dioxus)
cargo build --release    # 发行版
```

## 依赖

- yt-dlp, reqwest, tokio, dioxus, rfd, notify

## 项目结构

```
src/
├── main.rs              # CLI 入口
├── gui_dioxus.rs       # GUI (Dioxus, ~450行)
├── gui.rs              # GUI (Slint, 保留)
├── config.rs           # 配置管理
├── types.rs            # 类型定义
├── downloader.rs       # 下载引擎
├── yt_dlp.rs           # yt-dlp 集成
├── history.rs          # 下载历史/书签
├── platform/           # 平台解析器 (ResolverRegistry)
│   ├── tiktok.rs, douyin.rs, bilibili.rs
│   └── ...
└── core/              # 核心模块
    └── downloader, muxer, subtitle, danmaku
```

---

# 编程理念

**核心原则**: 谨慎优于速度，简单优于复杂，直接解决当前问题而非预判未来需求。


## 1. Multi-Agent 开发循环 (Anthropic 模式)

**来源**: Anthropic 工程博客 — 三 Agent 架构：Planner / Generator / Evaluator

### 三角色分工

| 角色 | 职责 | 你的项目中的应用 |
|------|------|-----------------|
| **Planner** | 产品层面高层次规划，输出规格书（不含技术细节） | 规划下载策略、Resolver 选择逻辑 |
| **Generator** | 按 sprint 增量开发，自测后交棒 | 执行下载、muxer 拼接、文件输出 |
| **Evaluator** | **独立分离**，真实验证（不只是读代码） | 校验文件完整性、实际跑下载测例 |

### 核心原则

- **Evaluator 必须独立**：同一个 agent 评自己代码一定会给好评，必须分离
- **sprint contract**：Generator 和 Evaluator 事先协商"完成定义"（如：下载成功率 ≥ 95%，文件完整可播放）
- **文件系统通信**：Agent 间通过文件传递状态，无花哨消息队列
- **主动验证**：Evaluator 不做静态审代码，直接跑实际下载流程验证

### 你的项目适配

```
URL → Planner(规划下载策略) → Generator(执行下载) → Evaluator(验证文件完整性)
                                    ↑                    ↓
                                    ←←← 循环直到通过 ←←←←
```

### 不适用场景

视频下载是**确定性任务**（有正确答案的执行），不是探索性产品开发。Planner 不碰技术细节这条**不适用**——视频下载涉及协议解析、平台差异，必须含技术上下文。

### 参考文献

- Anthropic 工程博客 (2026-03-24): Building Effective AI Agents
- Boris Cherny (Claude Code 负责人): "I don't prompt Claude anymore. I have loops running that prompt Claude. My job is to write loops."

---

# 项目特定规则

## Dioxus GUI 规则

### 进度存储格式
- 存储: `progress_arc.store((percent * 100.0) as u32)` (0-10000)
- 显示: 除以 100.0 转换为浮点数

### 错误提示结构
- ⚠️ 图标 + 错误类型标签
- 红色背景 + 解决建议

## Slint GUI 规则 (保留, gui.rs)

### 进度回调架构
- `download_segment()` 只更新 `total_downloaded`，由后台任务统一计算
- 存储: `progress_arc.store((p.percent * 100.0) as u32)`
- 终止条件: `while progress_arc.load() < 10000`

---

# Agent Skills

| Task | Location |
|------|----------|
| Issue tracker | `docs/agents/issue-tracker.md` |
| Triage labels | `docs/agents/triage-labels.md` |
| GitNexus | See `.claude/skills/gitnexus/*/` |

---

**核心洞见**: 过度复杂的代码往往遵循设计模式和最佳实践。问题在于**时机**：在需要之前添加复杂性会导致代码更难理解、引入更多 bug，花更长时间实现，更难测试。

**好代码是简单解决今天问题的代码，而非提前解决明天问题的代码。**

<!-- rtk-instructions v2 -->
<!-- 完整 RTK 命令文档已移至 docs/rtk-commands.md -->
**RTK Commands**: `rtk <cmd>` 自动过滤输出，60-90% token 节省
<!-- /rtk-instructions -->
