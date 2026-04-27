# x_videos-dowloads

Rust x.com 视频下载器，支持 CLI 和 GUI，具有三级视频源解析和协作下载功能。

## 功能特性

- **CLI 模式**: 直接通过命令行下载视频
- **GUI 模式**: 友好的图形界面 (Slint v1.15.1)
- **协作下载**: 多设备实时协作下载 (WebSocket + P2P)
- **三级解析链**: yt-dlp → FxTwitter API → Scrapling (Cloudflare 绕过)
- **自适应线程**: 根据网站特征和下载速度智能调整线程数
- **网站特征学习**: 记录30+热门网站的防爬机制和下载特征

## 快速开始

```bash
# CLI 模式
cargo run -- "https://x.com/user/status/123456"

# GUI 模式
cargo run -- --gui

# 发行版构建
cargo build --release
```

## 项目结构

```
x_videos-dowloads/
├── src/
│   ├── main.rs              # CLI 入口
│   ├── lib.rs                # 库入口
│   ├── config.rs            # 配置管理
│   ├── types.rs             # 类型定义
│   ├── downloader.rs        # 直接下载器 (4线程分段并行)
│   ├── yt_dlp.rs            # yt-dlp 集成
│   ├── fxtwitter.rs         # FxTwitter API 解析器
│   ├── scrapling_client.rs   # Scrapling Cloudflare 绕过
│   ├── scrapling_extractor.rs # HTML 视频 URL 提取
│   ├── website_profile.rs    # 网站特征学习（防爬机制+自适应线程）
│   ├── gui.rs               # GUI (Slint)
│   ├── history.rs           # 下载历史/书签
│   ├── theme.rs             # 主题
│   └── collaboration/       # 分布式协作模块
│       ├── client/          # WebSocket 客户端
│       ├── server/          # WebSocket 服务端
│       ├── crypto/          # 一致性哈希
│       ├── transfer/        # P2P 文件传输
│       └── types.rs         # 共享类型
├── ui.slint                 # Slint UI 定义
├── Cargo.toml
├── CLAUDE.md               # 项目指南
├── HANDOFF.md              # 交接文档
└── README.md
```

---

## 视频解析链 (Fallback)

当下载 x.com 视频时，程序按以下顺序尝试解析视频源：

```
1. yt-dlp (首选)
   └── 快速解析，支持多种格式
       ↓ 失败
2. FxTwitter API
   └── 调用 api.fxtwitter.com 获取视频地址
       ↓ 失败
3. Scrapling (Cloudflare 绕过)
   └── 使用 Python scrapling 浏览器自动化绕过 Cloudflare
```

### scrapling_client.rs

通过 Python subprocess 调用 [scrapling](https://github.com/D4Vinci/Scrapling) 框架：

- 使用 `StealthyFetcher` 绕过 Cloudflare Turnstile 挑战
- 支持 headless 浏览器自动化
- 返回原始 HTML 用于视频 URL 提取

### scrapling_extractor.rs

从 HTML 中提取视频 URL：

- 解析 `window.__config` JSON
- 提取 `<meta property="og:video">` 标签
- 直接搜索 `video.twimg.com` URL

---

## 网站特征学习系统

智能网站特征学习，自动适应不同网站的下载策略：

### 特性

- **自适应线程**: 根据下载速度自动调整线程数（2-8线程）
- **防爬感知**: 识别Cloudflare、RateLimit、验证码等防爬机制
- **持久化学习**: 记录成功/失败次数，自动调整配置
- **30+默认网站**: 预置热门网站的特征和防爬配置

### 防爬机制类型

| 类型 | 说明 | 处理策略 |
|------|------|----------|
| Cloudflare | Cloudflare保护 | UA轮换+降低线程+请求间隔 |
| RateLimit | 速率限制 | 降低线程+增加请求间隔 |
| CookieAuth | 需要Cookie | 提示用户提供认证 |
| JavaScript | JS挑战 | 使用Scrapling绕过 |

### 配置文件

`~/.config/x-video-downloader/website_profiles.json`

---

## 协作模块 (Collaboration)

多设备实时协作下载模块，支持团队创建、任务分发、点对点文件传输。

### 架构概览

```
┌─────────────────────────────────────────────────────────────────┐
│                        Collaboration Layer                       │
├─────────────────────────────────────────────────────────────────┤
│  ┌──────────────┐                    ┌──────────────────────┐     │
│  │ collaboration │                  │    collaboration     │     │
│  │    client    │◄────────────────►│       server        │     │
│  └──────────────┘    WebSocket      └──────────────────────┘     │
│         │                                        │              │
│         ▼                                        ▼              │
│  ┌──────────────┐                        ┌──────────────┐     │
│  │   Transfer   │                        │     DB       │     │
│  │   (P2P)      │                        │   SQLite     │     │
│  └──────────────┘                        └──────────────┘     │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│                      Module Structure                            │
├─────────────────────────────────────────────────────────────────┤
│  collaboration/                                                  │
│  ├── client/          WebSocket 客户端                           │
│  │   ├── ws.rs        CollaborationClient, WithFileHandler       │
│  │   ├── queue.rs     LocalQueue (本地队列管理)                   │
│  │   └── discovery.rs  设备发现                                    │
│  ├── server/          WebSocket 服务端                           │
│  │   ├── ws.rs        WsServer                                  │
│  │   ├── handler.rs   MessageHandler                            │
│  │   └── db.rs        SQLite 数据库封装                          │
│  ├── crypto/           加密模块                                  │
│  │   └── hashring.rs  一致性哈希环 (节点发现)                    │
│  ├── transfer/        文件传输模块                               │
│  │   ├── http_server.rs  HTTP 范围请求服务器                     │
│  │   └── downloader.rs   分块下载器                             │
│  ├── discovery/        NAT 穿透模块                              │
│  │   └── stun.rs      STUN 协议实现                             │
│  ├── mod.rs          模块入口                                   │
│  └── types.rs        核心类型定义                               │
└─────────────────────────────────────────────────────────────────┘
```

### 核心类型

| 类型 | 说明 |
|------|------|
| `Team` | 团队，包含 ID、名称、邀请码、创建时间 |
| `Device` | 设备，包含 ID、团队 ID、名称、IP/端口、在线状态 |
| `Task` | 下载任务，包含 URL、状态、进度、文件路径 |
| `TaskStatus` | 任务状态枚举：`New`, `Queued`, `Claimed`, `Complete`, `Failed` |

### 消息协议

**客户端消息 (ClientMessage)**:
| 变体 | 说明 |
|------|------|
| `Register` | 注册设备到团队 |
| `CreateTeam` | 创建团队 |
| `JoinTeam` | 加入团队 (通过邀请码) |
| `Heartbeat` | 心跳保活 |
| `AddTask` | 添加下载任务 |
| `ClaimTask` | 认领任务 |
| `UpdateProgress` | 更新下载进度 |
| `TaskComplete` | 任务完成 |
| `RequestFile` | 请求文件传输 |
| `LeaveTeam` | 离开团队 |

**服务端消息 (ServerMessage)**:
| 变体 | 说明 |
|------|------|
| `Registered` | 注册成功 |
| `TeamCreated` | 团队创建成功 |
| `TeamJoined` | 加入团队成功 |
| `TeamDevices` | 团队设备列表 |
| `TaskAdded` | 任务添加成功 |
| `TaskClaimed` | 任务被认领 |
| `TaskUpdated` | 任务更新 |
| `TaskReleased` | 任务释放 |
| `FileAvailable` | 文件可用 (P2P 下载通知) |
| `DeviceOnline` | 设备上线 |
| `DeviceOffline` | 设备下线 |
| `Error` | 错误消息 |

### 公共 API

```rust
// 基础协作客户端
pub struct CollaborationClient {
    pub async fn connect(server_url, team_id, device_id, device_name) -> Result<Self>
    pub async fn send(msg: ClientMessage) -> Result<()>
    pub fn subscribe() -> broadcast::Receiver<ServerMessage>
    pub async fn add_task(url) -> Result<()>
    pub async fn claim_task(task_id) -> Result<()>
    pub async fn update_progress(task_id, progress) -> Result<()>
    pub async fn task_complete(task_id, local_path, file_size) -> Result<()>
    pub async fn request_file(task_id) -> Result<()>
    pub async fn leave_team() -> Result<()>
    pub async fn create_team(name) -> Result<()>
    pub async fn join_team(invite_code) -> Result<()>
}

// 带文件传输功能的客户端
pub struct CollaborationClientWithFileHandler {
    pub async fn new(server_url, team_id, device_id, device_name, download_dir, file_server_port) -> Result<Self>
    pub async fn with_file_server(..., file_server) -> Result<Self>
    pub async fn start_file_handler()  // 自动处理 FileAvailable 消息
    pub async fn register_completed_file(task_id, local_path) -> Result<()>
    pub fn client() -> &CollaborationClient
    pub fn file_server() -> Option<&Arc<FileServer>>
}

// 一致性哈希
pub struct HashRing
pub fn get_owner(url) -> Uuid           // 获取 URL 对应的设备
pub fn add_device(node_id, url)         // 添加设备到哈希环
pub fn remove_device(node_id)           // 从哈希环移除设备

// HTTP 服务器
pub struct FileServer {
    pub async fn new(port) -> Self
    pub async fn start()                              // 启动服务器
    pub async fn register_file(task_id, path) -> () // 注册可分享文件
}

// 分块下载器
pub struct ChunkedDownloader {
    pub async fn new(chunk_size) -> Self
    pub async fn download_from_peer(ip, port, task_id, output_path, progress_cb) -> Result<u64>
}
```

### 通信流程

```
1. 设备连接注册
   Client -> Server: Register { device_id, team_id, name }
   Server -> Client: Registered { device_id }

2. 团队创建/加入
   Client -> Server: CreateTeam { name } / JoinTeam { invite_code }
   Server -> Client: TeamCreated { team } / TeamJoined { team }

3. 任务分发
   Client -> Server: AddTask { url }
   Server -> All: TaskAdded { task }

4. 任务认领 (一致性哈希决定)
   Client -> Server: ClaimTask { task_id }
   Server -> All: TaskClaimed { task_id, device_id }

5. P2P 文件传输
   - 任务完成方: register_completed_file()
   - Server -> All: FileAvailable { task_id, from_device, ip, port, ... }
   - 请求方: 自动从对等节点下载
```

---

## 依赖

- `tokio` - 异步运行时
- `reqwest` - HTTP 客户端 (rustls-tls)
- `yt-dlp` - 视频下载后端
- `slint` v1.15.1 - GUI 框架
- `tokio-tungstenite` - WebSocket
- `rusqlite` - SQLite 数据库
- `which = "6"` - Python 解释器查找
- `serde` - 序列化
- `anyhow` - 错误处理
- `chrono` - 时间处理