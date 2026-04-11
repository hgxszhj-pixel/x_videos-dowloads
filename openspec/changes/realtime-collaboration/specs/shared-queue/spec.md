## ADDED Requirements

### Requirement: shared-queue

系统 SHALL 支持分布式队列同步、任务添加、状态同步和 URL 冲突合并。

#### Scenario: 添加任务
- **WHEN** 用户添加视频 URL 到下载队列
- **THEN** 任务广播给所有在线设备，URL 相同则静默合并

#### Scenario: 同步队列状态
- **WHEN** 任何设备更新任务状态
- **THEN** 服务端广播更新到所有设备

#### Scenario: URL 冲突合并
- **WHEN** 设备 A 添加 URL "https://x.com/video/123"
- **AND** 设备 B 也添加相同 URL
- **THEN** 系统检测为同一任务，显示"已存在"提示，不创建重复任务
