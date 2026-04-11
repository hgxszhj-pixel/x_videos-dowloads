## ADDED Requirements

### Requirement: central-service

系统 SHALL 提供合并的发现+中继服务，统一管理设备注册、消息转发和离线缓冲。

#### Scenario: 设备注册
- **WHEN** 设备加入团队时
- **THEN** 服务端注册设备信息（ID、IP、端口、能力）

#### Scenario: 消息转发
- **WHEN** 设备间无法直连
- **THEN** 消息通过服务端中继转发

#### Scenario: 离线缓冲
- **WHEN** 目标设备离线时收到消息
- **THEN** 服务端缓存消息（最多 100 条），设备上线后转发
