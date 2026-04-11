# 项目接手文档 (Handoff)

**项目**: x_video_downloader (Rust x.com 视频下载器)
**生成时间**: 2026-04-11
**当前分支**: main (e365a01)
**领先 origin/master**: 18 commits

---

## 项目架构图

```
┌─────────────────────────────────────────────────────────────────┐
│                         CLI/GUI 入口                              │
│                    main.rs / gui.rs                              │
└─────────────────────┬───────────────────────────────────────────┘
                      │
┌─────────────────────▼───────────────────────────────────────────┐
│                     核心模块 (lib.rs)                             │
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
│  queue.rs  │  db.rs     │  auth.rs   │             │ downloader│
│  discovery │  ws.rs     │             │             │           │
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
| **transfer/http_server** | FileServer | HTTP 范围请求文件服务 |
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

## 待处理问题

### 🔴 高优先级 (需优先处理)

| # | 问题 | 文件 | 影响 | 建议 |
|---|------|------|------|------|
| 1 | URL TOCTOU 竞争条件 | `handler.rs:273-283` | 可能创建重复任务 | 数据库唯一约束 |
| 2 | Failed 状态无通知 | `client/ws.rs` | 用户不知连接断开 | 添加事件回调 |
| 3 | 0.0.0.0 绑定暴露公网 | `http_server.rs:62` | 安全风险 | IP 白名单 |

### 🟡 中优先级

| # | 问题 | 文件 | 说明 |
|---|------|------|------|
| 4 | RateLimiter cleanup 未用 | `http_server.rs:57` | 预留 API |
| 5 | ParallelDownload 分段无重试 | `downloader.rs:213-224` | 失败分段仅重试 3 次 |
| 6 | 任务超时无通知 | `handler.rs:502-508` | 超时释放无事件 |

---

## 高效开发提示词

### 问题修复提示词

**TOCTOU 竞争条件修复:**
```
修复 src/collaboration/server/handler.rs 的 URL TOCTOU 问题：
1. 在 Database::add_task 表添加 UNIQUE(url) 约束
2. 在 handle_add_task 中处理 rusqlite UNIQUE 约束冲突错误
3. 返回合适的错误消息给客户端
4. 写 2 个测试：正常添加、重复 URL 错误
```

**WebSocket Failed 状态通知:**
```
修复 src/collaboration/client/ws.rs 的连接失败通知：
1. 当 ConnectionState 变为 Failed 时触发通知
2. 添加回调机制：pub fn on_connection_failed<F>(&self, f: F) where F: Fn() + Send + Sync
3. 在 main.rs 或 gui.rs 中处理该回调，显示错误给用户
```

**IP 白名单安全:**
```
修复 src/collaboration/transfer/http_server.rs 的安全风险：
1. FileServer 仅绑定本地 IP (127.0.0.1) 而非 0.0.0.0
2. 或添加配置项允许指定绑定地址
3. 添加注释说明为何选择该方案
```

### 功能实现提示词

**分段下载重试增强:**
```
增强 src/downloader.rs 的分段下载重试机制：
1. 将重试次数从 3 次增加到 10 次
2. 当所有分段都失败时，标记任务为 Failed 并返回错误
3. 写单元测试验证：模拟部分分段失败场景
```

**任务超时事件:**
```
在 src/collaboration/server/handler.rs 添加超时事件：
1. check_task_timeouts 释放任务时广播 TaskReleased 消息
2. 客户端收到后显示通知给用户
3. 在 handler.rs 的 check_task_timeouts 末尾添加广播逻辑
```

### 代码审查提示词

**审查 handler.rs:**
```
审查 src/collaboration/server/handler.rs，重点：
1. 所有 unwrap/expect 改为错误处理
2. handle_add_task 的 URL 重复检测逻辑
3. 数据库事务是否正确使用
4. 输出：问题列表 + 修复建议
```

---

## 开发工作流

### 单任务流程

```
1. 选择一个问题 (从待处理列表)
2. 创建 worktree: git worktree add ../fix-xxx -b fix/xxx
3. 修复问题
4. 测试: cargo test
5. 提交: git add -A && git commit -m "fix: ..."
6. 合并: git checkout main && git merge fix/xxx
7. 清理: git worktree remove ../fix-xxx
```

### 时间盒

```
每个问题: 最多 30 分钟
超时则暂停，下次继续
```

---

## 构建和测试

```bash
cargo build
cargo test        # 119 passed
cargo clippy -- -W warnings
cargo run -- "URL"
cargo run -- --gui
```

---

## 提交记录 (18 commits)

```
e365a01 - Merge: WebSocket auto-reconnect
2718cc2 - Merge: CORS validation
7fd1d03 - WebSocket exponential backoff reconnect
2ac549f - Invite code 16 chars + special chars
b9bbd36 - Error handling + memory leak fixes
97026f3 - 30 new unit tests
395f704 - CRITICAL: shared secret + rate limiter
1ac89da - STUN fix + CLI integration
```

---

## 联系方式

- CLAUDE.md - 项目详细说明
- README.md - 使用文档
