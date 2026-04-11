# 协作下载模块 (Collaboration)

多设备实时协作下载模块，支持团队创建、任务分发、点对点文件传输。

## 架构概览

```
┌─────────────────────────────────────────────────────────────────┐
│                        Collaboration Layer                       │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌──────────────┐                    ┌──────────────────────┐  │
│  │ collaboration │                    │    collaboration     │  │
│  │    client    │◄──────────────────►│       server        │  │
│  └──────────────┘    WebSocket       └──────────────────────┘  │
│         │                                        │              │
│         │                                        │              │
│         ▼                                        ▼              │
│  ┌──────────────┐                        ┌──────────────┐     │
│  │   Transfer   │                        │     DB       │     │
│  │   (P2P)      │                        │   SQLite     │     │
│  └──────────────┘                        └──────────────┘     │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│                      Module Structure                            │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  collaboration/                                                  │
│  ├── client/          WebSocket 客户端                           │
│  │   ├── ws.rs        CollaborationClient, CollaborationClient   │
│  │   │                 WithFileHandler                           │
│  │   ├── queue.rs     LocalQueue (本地队列管理)                   │
│  │   └── discovery.rs 设备发现                                    │
│  │                                                             │
│  ├── server/          WebSocket 服务端                          │
│  │   ├── ws.rs        WsServer                                  │
│  │   ├── handler.rs   MessageHandler                            │
│  │   └── db.rs        SQLite 数据库封装                          │
│  │                                                             │
│  ├── crypto/           加密模块                                   │
│  │   └── hashring.rs  一致性哈希环 (节点发现)                    │
│  │                                                             │
│  ├── transfer/        文件传输模块                               │
│  │   ├── http_server.rs  HTTP 范围请求服务器                     │
│  │   └── downloader.rs   分块下载器                              │
│  │                                                             │
│  ├── discovery/        NAT 穿透模块                              │
│  │   └── stun.rs      STUN 协议实现                             │
│  │                                                             │
│  ├── mod.rs          模块入口                                   │
│  └── types.rs        核心类型定义                               │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

## 核心类型 (types.rs)

### 数据结构

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

## 公共 API

### 客户端 (client/ws.rs)

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
```

### 服务端 (server/mod.rs)

```rust
pub struct WsServer
pub struct MessageHandler
pub struct Database

// 服务器启动
pub async fn start_server(addr, db_path) -> Result<()>
```

### 本地队列 (client/queue.rs)

```rust
pub struct LocalQueue
```

### 一致性哈希 (crypto/hashring.rs)

```rust
pub struct HashRing
pub fn get_owner(url) -> Uuid           // 获取 URL 对应的设备
pub fn add_device(node_id, url)         // 添加设备到哈希环
pub fn remove_device(node_id)           // 从哈希环移除设备
```

### 文件传输 (transfer/)

```rust
// HTTP 服务器 (http_server.rs)
pub struct FileServer {
    pub async fn new(port) -> Self
    pub async fn start()               // 启动服务器
    pub async fn register_file(task_id, path)  // 注册可分享文件
}

// 分块下载器 (downloader.rs)
pub struct ChunkedDownloader {
    pub async fn new(chunk_size) -> Self
    pub async fn download_from_peer(ip, port, task_id, output_path, progress_cb) -> Result<u64>
}
```

## 使用示例

### 1. 启动服务器

```rust
use x_videos_dowloads::collaboration::start_server;

start_server("0.0.0.0:8080", Path::new("collab.db")).await?;
```

### 2. 连接为客户端

```rust
use x_videos_dowloads::collaboration::client::ws::CollaborationClient;
use uuid::Uuid;

let client = CollaborationClient::connect(
    "ws://server:8080",
    team_id,
    device_id,
    "My Device"
).await?;

// 订阅消息
let mut rx = client.subscribe();

// 添加任务
client.add_task("https://example.com/video.mp4").await?;

// 认领任务
client.claim_task(task_id).await?;
```

### 3. 带文件传输的客户端

```rust
use x_videos_dowloads::collaboration::client::ws::CollaborationClientWithFileHandler;

let client = CollaborationClientWithFileHandler::new(
    "ws://server:8080",
    team_id,
    device_id,
    "My Device",
    PathBuf::from("./downloads"),
    8080,
).await?;

// 启动文件处理器 (后台自动处理文件下载)
client.start_file_handler().await;

// 任务完成时注册文件
client.register_completed_file(task_id, local_path).await?;
```

## 通信流程

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

## 依赖

- `tokio-tungstenite` - WebSocket
- `rusqlite` - SQLite 数据库
- `uuid` - 唯一 ID
- `chrono` - 时间处理
- `serde` - 序列化
- `anyhow` - 错误处理
