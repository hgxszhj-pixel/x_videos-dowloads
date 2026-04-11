## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                    完整协作系统架构                              │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │              服务端 (合并: 发现 + 中继)                    │   │
│  │  • 团队管理 (创建/邀请/加入)                            │   │
│  │  • 设备注册与心跳 (TTL)                                 │   │
│  │  • 共享队列状态存储                                     │   │
│  │  • 消息广播/转发                                       │   │
│  │  • 离线消息缓冲                                        │   │
│  └─────────────────────────────────────────────────────────┘   │
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │              客户端 (各设备)                             │   │
│  │  • 本地任务队列                                         │   │
│  │  • WebSocket 长连接                                     │   │
│  │  • P2P 文件传输 (HTTP 分块)                            │   │
│  │  • 一致性哈希任务分配                                   │   │
│  │  • 冲突合并 (URL 去重)                                 │   │
│  │  • 任务超时释放                                        │   │
│  └─────────────────────────────────────────────────────────┘   │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

## Data Models

### Team (Team)
```rust
struct Team {
    id: Uuid,           // 团队唯一标识
    name: String,       // 团队名称
    invite_code: String, // 邀请码 (6位)
    created_at: DateTime,
}
```

### Device
```rust
struct Device {
    id: Uuid,           // 设备唯一标识 (首次安装生成)
    team_id: Uuid,     // 所属团队
    name: String,       // 自定义名称
    public_ip: Option<String>,   // 公网IP
    public_port: Option<u16>,    // 公网端口
    last_seen: DateTime,
    is_online: bool,
}
```

### Task
```rust
enum TaskStatus {
    New,      // 新建
    Queued,   // 队列中
    Claimed,  // 已抢占
    Complete, // 完成
    Failed,   // 失败
}

struct Task {
    id: Uuid,
    url: String,
    status: TaskStatus,
    claimed_by: Option<DeviceId>,
    claimed_at: Option<DateTime>,
    progress: f64,        // 0.0 - 1.0
    local_path: Option<PathBuf>,
    file_size: Option<u64>,
    created_by: DeviceId,
    created_at: DateTime,
    version: u64,          // 乐观锁
}
```

## Task State Machine

```
┌────────┐    添加     ┌────────┐    分配     ┌────────┐
│  NEW   │──────────▶ │ QUEUED │─────────▶ │CLAIMED│
└────────┘             └────────┘            └────────┘
    │                                              │
    │ URL冲突                                      │ 超时 (5min无进度)
    ▼                                              ▼
┌────────┐                                  ┌────────┐
│MERGED  │                                  │RELEASE │
└────────┘                                  └────────┘
                                                  │
                                                  ▼
                                            ┌────────┐
                                            │ QUEUED │
                                            └────────┘
```

## Task Distribution: Consistent Hashing

```
环结构:
  hash(设备A) ──────────── hash(设备B) ──────────── hash(设备C)
       ↑                                              ↑
       └──────────────── 任务落点 ────────────────────┘

分配算法:
  task_hash = hash(URL)
  owning_device = 环上 >= task_hash 的第一个设备
  若环为空，则任务保留在本地等待设备加入
```

## Communication Protocol

### WebSocket Messages

```rust
// 客户端 → 服务端
enum ClientMessage {
    Register { device_id: Uuid, team_id: Uuid },
    Heartbeat { device_id: Uuid },
    AddTask { url: String, device_id: Uuid },
    ClaimTask { task_id: Uuid, device_id: Uuid },
    UpdateProgress { task_id: Uuid, progress: f64 },
    RequestFile { task_id: Uuid, device_id: Uuid },
}

// 服务端 → 客户端
enum ServerMessage {
    TaskAdded { task: Task },
    TaskClaimed { task_id: Uuid, device_id: Uuid },
    TaskUpdated { task: Task },
    FileAvailable { task_id: Uuid, from_device: Uuid, ip: String, port: u16 },
    DeviceOnline { device: Device },
    DeviceOffline { device_id: Uuid },
}
```

## File Transfer Protocol

```
流程:
1. 设备A下载完成 → 广播 FileAvailable
2. 设备B请求文件 → 服务端转发 Request
3. 设备A响应 → { ip, port, filename, size }
4. 设备B 连接设备A的 HTTP 服务器
5. 分块传输，支持断点续传

HTTP API:
  GET /file/{task_id}           # 获取文件信息
  GET /file/{task_id}?from=N   # 分块下载 (从 N bytes 开始)
```

## Team Creation Flow

```
1. 用户A: 创建团队 → 生成本地 UUID + 团队邀请码 (6位)
2. 用户A: 分享邀请码给用户B、C
3. 用户B/C: 输入邀请码 → 加入团队 → 下载团队设备列表
```

## NAT Traversal Strategy

```
优先级:
1. 公网直连 (设备有公网IP)
2. UDP 打洞 (通过 STUN 服务器协助)
3. 中继转发 (最后兜底，延迟高)

STUN 服务器: 使用公共 STUN 服务
```

## Storage Strategy

```
服务端 (SQLite):
- teams 表
- devices 表
- tasks 表 (最新状态快照)
- offline_messages 表 (最多缓存100条)

客户端 (本地文件):
- 任务队列状态 (JSON)
- 下载历史记录
- 配置文件
```

## Security Considerations

```
1. 邀请码: 6位随机字母数字，定期更换
2. 设备认证: UUID + 团队ID 验证
3. 消息签名: 防止伪造消息
4. 文件传输: 仅允许已注册设备间传输
```
