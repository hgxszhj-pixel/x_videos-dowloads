# 项目交接文档 (Handoff)

**项目**: x_video_downloader (Rust x.com 视频下载器)
**生成时间**: 2026-04-14
**当前分支**: main
**Git 状态**: 干净 (领先 origin/master)

---

## 项目概述

基于 Rust 的 x.com 视频下载器，使用 yt-dlp 作为后端，支持 CLI 和 GUI 模式，具有协作下载功能。

**核心技术栈:**
- Rust (tokio, reqwest)
- yt-dlp 视频下载
- iced GUI 框架
- WebSocket 协作通信
- SQLite (rusqlite) 数据持久化

---

## 项目架构

```
┌─────────────────────────────────────────────────────────────────┐
│                         CLI/GUI 入口                            │
│                    main.rs / gui.rs                              │
└─────────────────────┬───────────────────────────────────────────┘
                      │
┌─────────────────────▼───────────────────────────────────────────┐
│                     核心模块 (lib.rs)                            │
│  config.rs │ types.rs │ history.rs │ downloader.rs │ yt_dlp.rs  │
└─────────────────────┬───────────────────────────────────────────┘
                      │
        ┌──────────────┴──────────────┐
        │                             │
┌───────▼──────────┐      ┌──────────▼──────────┐
│   直接下载器      │      │   yt-dlp 下载器     │
│  downloader.rs   │      │    yt_dlp.rs        │
└──────────────────┘      └─────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│                    协作模块 (collaboration/)                     │
├─────────────┬─────────────┬─────────────┬─────────────┬───────────┤
│   client/  │   server/  │   crypto/  │  discovery/ │  transfer/│
│             │             │             │             │           │
│  ws.rs     │  handler.rs│  hashring  │  stun.rs    │ http_server│
│  queue.rs  │  db.rs     │  auth.rs   │             │ downloader │
└─────────────┴─────────────┴─────────────┴─────────────┴───────────┘
```

---

## 模块职责

| 模块 | 文件 | 职责 |
|------|------|------|
| **CLI** | main.rs | 命令行参数解析，下载流程控制 |
| **配置** | config.rs | 配置文件加载/保存 (~/.config/) |
| **类型** | types.rs | DownloadRequest, VideoInfo, VideoFormat |
| **历史** | history.rs | 下载历史和书签 (JSON 文件) |
| **下载器** | downloader.rs | 直接 HTTP 下载，支持分段并行 |
| **yt-dlp** | yt_dlp.rs | yt-dlp 集成，视频信息解析 |
| **GUI** | gui.rs | iced 框架图形界面 |
| **协作** | collaboration/ | 分布式下载协作 |

### 协作模块 (collaboration/)

| 子模块 | 文件 | 职责 |
|--------|------|------|
| **client/ws** | CollaborationClient | WebSocket 客户端，自动重连 |
| **client/queue** | LocalQueue | 本地任务队列 |
| **server/handler** | MessageHandler | 消息处理，12 种消息类型 |
| **server/db** | Database | SQLite WAL 模式存储 |
| **server/ws** | WsServer | WebSocket 服务器 |
| **crypto/auth** | AuthToken | HMAC-SHA256 认证 |
| **crypto/hashring** | HashRing | 一致性哈希，URL 路由 |
| **discovery/stun** | STUN Client | NAT 类型检测 |
| **transfer/http_server** | FileServer | HTTP 范围请求文件服务 (默认绑定 127.0.0.1) |
| **transfer/downloader** | ChunkedDownloader | 分块下载 |

---

## 消息协议 (WebSocket)

```
客户端 → 服务器:
├── Register { device_id, team_id, name }
├── CreateTeam { name }
├── JoinTeam { invite_code }
├── Heartbeat { device_id }
├── AddTask { url }
├── ClaimTask { task_id }
├── UpdateProgress { task_id, progress }
├── TaskComplete { task_id, local_path, file_size }
├── RequestFile { task_id }
└── LeaveTeam { device_id }

服务器 → 客户端:
├── Registered { device_id }
├── TeamCreated { team }
├── TeamJoined { team }
├── TeamDevices { devices }
├── TaskAdded { task }
├── TaskClaimed { task_id, device_id }
├── TaskUpdated { task }
├── TaskReleased { task_id, reason }
├── FileAvailable { task_id, from_device, ip, port, filename, size }
├── DeviceOnline { device }
├── DeviceOffline { device_id }
└── Error { message }
```

---

## 数据模型

```rust
Team { id, name, invite_code, created_at }
Device { id, team_id, name, public_ip, public_port, last_seen, is_online }
Task { id, team_id, url, status, claimed_by, progress, local_path, file_size, ... }
```

---

## 已解决问题 (2026-04-14)

| # | 问题 | 优先级 | 状态 | 修复提交 |
|---|------|--------|------|----------|
| 1 | URL TOCTOU 竞争条件 | 🔴 CRITICAL | ✅ 已修复 | f746632 |
| 2 | 0.0.0.0 绑定暴露公网 | 🔴 CRITICAL | ✅ 已修复 | f746632 |
| 3 | WebSocket 失败状态无通知 | 🟠 HIGH | ✅ 已修复 | f746632 |
| 4 | RateLimiter cleanup 未使用 | 🟡 MEDIUM | ✅ 已修复 | f746632 |
| 5 | 分块下载重试不足 (仅3次) | 🟡 MEDIUM | ✅ 已修复 | f746632 |
| 6 | WebSocket Channel 已关闭 | 🟠 HIGH | ✅ 已修复 | db8fa87 |

---

## 快速开始

```bash
# 构建
cargo build --release

# 下载视频
./target/release/x-video-downloader "https://x.com/user/status/123"

# GUI 模式
./target/release/x-video-downloader --gui

# 启动协作服务器
./target/release/x-video-downloader --start-server

# 创建团队
./target/release/x-video-downloader --create-team "团队名称"

# 加入团队
./target/release/x-video-downloader --join-team "邀请码"
```

---

## 构建和测试

```bash
# 构建
cargo build

# 运行测试 (121 tests)
cargo test

# Clippy 检查
cargo clippy -- -W warnings

# 发布版本构建
cargo build --release

# 运行发布版本
./target/release/x-video-downloader "URL"
```

---

## 测试结果

| 测试项 | 命令 | 结果 |
|--------|------|------|
| 帮助信息 | `--help` | ✅ 正常显示 |
| 版本 | `--version` | ✅ 0.1.0 |
| 协作服务器 | `--start-server` | ✅ 监听 0.0.0.0:9000 |
| 创建团队 | `--create-team` | ✅ 连接成功 |
| 下载历史 | `--history` | ✅ 暂无历史 |
| 书签列表 | `--bookmarks` | ✅ 暂无书签 |
| 搜索历史 | `--search` | ✅ 未找到匹配 |
| 初始化配置 | `--init-config` | ✅ 配置文件已存在 |
| 单元测试 | `cargo test` | ✅ 121 passed |
| 代码检查 | `cargo clippy` | ✅ No issues found |

---

## 最近提交

```
db8fa87 - fix: resolve WebSocket channel closed issue in CollaborationClient
f746632 - fix: resolve 5 critical and medium priority issues from HANDOFF
97f1080 - docs: add project handoff document with architecture and tasks
e365a01 - Merge: add WebSocket auto-reconnect
2718cc2 - Merge: add CORS validation
```

---

## 项目规范

### 提交信息格式
```
<类型>: <描述>

类型: feat, fix, refactor, docs, test, chore, perf, ci
```

### 代码质量标准
- 函数 < 50 行
- 文件 < 800 行
- 嵌套层级 < 4 层
- 使用 `?` 操作符进行错误处理
- 禁止硬编码 (使用常量或配置)

### 安全准则
- 禁止硬编码 secrets (使用环境变量)
- 所有用户输入验证
- 所有端点限速
- 错误信息不泄露敏感数据

---

## 文件结构

```
src/
├── main.rs              # CLI 入口
├── lib.rs               # 库入口
├── config.rs            # 配置管理
├── types.rs             # 类型定义
├── downloader.rs        # 直接 HTTP 下载器
├── yt_dlp.rs            # yt-dlp 集成
├── gui.rs               # GUI (iced)
├── history.rs           # 下载历史/书签
└── collaboration/       # 分布式协作
    ├── client/          # WebSocket 客户端
    │   └── ws.rs         # CollaborationClient, auto-reconnect
    ├── server/          # 服务器端
    │   ├── handler.rs    # MessageHandler
    │   └── db.rs         # SQLite Database
    ├── crypto/          # 加密相关
    │   ├── auth.rs       # HMAC-SHA256 认证
    │   └── hashring.rs   # 一致性哈希环
    ├── discovery/       # NAT 检测
    │   └── stun.rs       # STUN 客户端
    ├── transfer/        # 文件传输
    │   ├── http_server.rs # HTTP 文件服务器 (绑定 127.0.0.1)
    │   └── downloader.rs  # 分块下载器
    └── types.rs         # 共享类型
```

---

## 技术参考

- [yt-dlp](https://github.com/yt-dlp/yt-dlp) - 视频下载后端
- [iced](https://iced.rs/) - Rust GUI 库
- [tokio-tungstenite](https://github.com/snapview/tokio-tungstenite) - WebSocket
- [rusqlite](https://github.com/rusqlite/rusqlite) - SQLite 绑定

---

## 联系方式

- CLAUDE.md - 项目详细说明
- README.md - 使用文档
