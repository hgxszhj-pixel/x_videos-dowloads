# CLAUDE.md

@andrej-karpathy-skills:karpathy-guidelines

# Rust x.com Video Downloader

视频下载器，支持 x.com (Twitter), TikTok, 抖音, Bilibili 等平台。

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

## 1. 编码前思考 (Think Before Coding)

**不要假设。不要隐藏困惑。主动提出权衡。**

## 2. 简单优先 (Simplicity First)

**最小代码解决问题。不做投机性实现。**

## 3. 精准修改 (Surgical Changes)

**只触碰必须改的。只清理自己造成的垃圾。**

## 4. 目标驱动执行 (Goal-Driven Execution)

**定义成功标准。循环验证直到完成。**

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
