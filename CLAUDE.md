# CLAUDE.md

@andrej-karpathy-skills:karpathy-guidelines

Rust x.com 视频下载器，使用 yt-dlp 后端，支持 CLI 和 GUI。

## 构建

```bash
cargo run -- "URL"        # CLI
cargo run -- --gui       # GUI
cargo build --release    # 发行版
```

## 依赖

- yt-dlp, reqwest, tokio, iced, rfd

## 项目结构

```
src/
├── main.rs              # CLI 入口
├── lib.rs               # 库入口
├── config.rs            # 配置管理
├── types.rs             # 类型定义
├── downloader.rs        # 直接下载器
├── yt_dlp.rs            # yt-dlp 集成
├── gui.rs               # GUI (iced)
├── history.rs           # 下载历史/书签
├── theme.rs             # 主题
└── collaboration/       # 分布式协作
    ├── client/          # WebSocket 客户端 (ws.rs, queue.rs, discovery.rs)
    ├── server/          # 服务端 (handler.rs, db.rs, ws.rs)
    ├── crypto/          # 一致性哈希 (hashring.rs)
    ├── transfer/        # 文件传输 (http_server.rs, downloader.rs)
    └── types.rs         # 共享类型
```

---

# 编程理念 (Karpathy Guidelines)

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
- 如果发现无关的死代码，提出来——不要删除。

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

# 模块特定规则

## 核心下载模块 (downloader.rs, yt_dlp.rs)

- 使用 `reqwest` 进行 HTTP 请求，`tokio::process::Command` 执行 yt-dlp
- 下载进度通过回调报告，使用 `Arc<dyn Fn(DownloadProgress) + Send + Sync>`
- 直接下载模式用于 video.twimg.com 等直接 URL
- yt-dlp 模式用于复杂页面，自动选择最佳格式

## GUI 模块 (gui.rs)

- 使用 `iced` 库构建 GUI
- 状态通过 `Subscription` 和 `Command` 管理
- 不会直接修改 `Message` 枚举的变体，除非添加新功能

## 配置模块 (config.rs)

- 使用 `toml` 序列化配置
- 配置文件路径通过 `AppConfig::config_path()` 获取
- `AppConfig::load()` 在启动时调用，失败则退出

## 历史模块 (history.rs)

- 使用 JSON 文件持久化 `history.json` 和 `bookmarks.json`
- `History::new()` 加载历史，`save()` 持久化
- `#[derive(Serialize, Deserialize)]` 用于 JSON 序列化

## 协作模块 (collaboration/)

### 客户端 (client/)

- WebSocket 客户端使用 `tokio-tungstenite`
- `CollaborationClient` 通过 `connect()` 创建
- `subscribe()` 返回广播接收器用于消息监听
- `#[allow(dead_code)]` 保留公共 API

### 服务端 (server/)

- SQLite 数据库使用 `rusqlite`，WAL 模式
- `Database` 封装连接，方法都是 public API
- `MessageHandler` 处理 WebSocket 消息
- `#[allow(dead_code)]` 保留未使用的方法

### 加密模块 (crypto/hashring.rs)

- 一致性哈希环使用 `BTreeMap<u64, Uuid>` 存储节点
- `get_owner(url)` 返回 URL 对应的设备 ID
- `add_device()` / `remove_device()` 管理环中设备
- `#[allow(dead_code)]` 保留公共 API

### 传输模块 (transfer/)

- `FileServer` 提供 HTTP 范围请求服务
- `ChunkedDownloader` 支持分块下载
- `#[allow(dead_code)]` 保留未使用的传输功能

## 类型定义 (types.rs)

- `DownloadRequest`, `DownloadResult`, `DownloadProgress` 用于下载流程
- `VideoInfo` 包含视频元数据
- `format_bytes()` 将字节数转为人类可读格式

---

# 测试要求

- 所有新功能必须添加单元测试
- 使用 `#[cfg(test)]` 模块编写测试
- 运行 `cargo test` 验证所有测试通过
- 运行 `cargo check` 确保无编译警告

---

**反模式警示**:
| 原则 | 反模式 | 正确做法 |
|------|--------|----------|
| 编码前思考 | 静默假设文件格式、字段、范围 | 列出假设并请求澄清 |
| 简单优先 | 为单一折扣计算使用策略模式 | 一个函数解决问题直到确实需要复杂性 |
| 精准修改 | 修复 bug 时改格式、加类型提示 | 只改修复问题所必需的行 |
| 目标驱动 | "我会审查并改进代码" | "为 bug X 写测试 → 修复 → 验证无回归" |

**核心洞见**: 过度复杂的代码往往遵循设计模式和最佳实践。问题在于**时机**：在需要之前添加复杂性会导致代码更难理解、引入更多 bug、花更长时间实现、更难测试。

**好代码是简单解决今天问题的代码，而非提前解决明天问题的代码。**
