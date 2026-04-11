## ADDED Requirements

### Requirement: task-distribution

系统 SHALL 支持一致性哈希任务分配、设备抢接和超时释放机制。

#### Scenario: 任务分配
- **WHEN** 新任务加入队列且无人认领
- **THEN** 通过一致性哈希环分配给一台设备，该设备自动抢占任务

#### Scenario: 设备抢接任务
- **WHEN** 设备收到分配通知
- **THEN** 设备发送 ClaimTask 消息，开始下载

#### Scenario: 任务超时释放
- **WHEN** 认领任务的设备 5 分钟无进度更新
- **THEN** 任务状态释放回 QUEUED，重新分配
