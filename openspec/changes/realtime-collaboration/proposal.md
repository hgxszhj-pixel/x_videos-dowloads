## Why

当前 x-video-downloader 是单机使用，无法在多设备间共享下载任务。用户（2-3 人小团队）需要在不同设备上协同下载视频，并希望：
- 一个人添加任务，其他人能看到
- 任务自动分配到空闲设备下载
- 下载完成的文件能在设备间传输

## What Changes

引入实时协作系统，使多个设备能够：
1. 加入同一个团队，实时同步下载队列
2. 添加任务时自动分配到合适的设备执行下载
3. 下载完成后支持设备间 P2P 文件传输

## Capabilities

### New Capabilities

- `team-management`: 团队创建、邀请码加入、设备管理、心跳保活
- `shared-queue`: 分布式队列同步、URL 冲突合并、状态广播
- `task-distribution`: 一致性哈希任务分配、设备抢接、任务超时释放
- `p2p-transfer`: WebSocket 长连接、NAT 穿透协助、HTTP 分块文件传输
- `central-service`: 合并发现服务 + 中继服务，统一部署

### Modified Capabilities

- (无)

## Impact

- 新增服务端组件（发现+中继服务）
- 新增客户端协作模块（WebSocket、P2P传输、一致性哈希）
- 数据库：SQLite（服务端）+ 本地文件（客户端）
- 网络协议：WebSocket（设备通信）、HTTP（文件传输）
