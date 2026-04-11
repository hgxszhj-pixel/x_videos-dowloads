## ADDED Requirements

### Requirement: team-management

系统 SHALL 支持设备注册、心跳和团队创建/加入功能。

#### Scenario: 创建设备唯一标识
- **WHEN** 用户首次启动应用
- **THEN** 系统生成 UUID 作为设备唯一标识，存储到本地配置

#### Scenario: 创建团队
- **WHEN** 用户选择创建新团队
- **THEN** 系统生成 6 位字母数字邀请码，用户可分享给其他人

#### Scenario: 加入团队
- **WHEN** 用户输入有效的邀请码
- **THEN** 设备注册到团队，下载当前设备列表

#### Scenario: 设备心跳
- **WHEN** 设备保持在线
- **THEN** 每 30 秒发送一次心跳到服务端

#### Scenario: 设备离线
- **WHEN** 设备超过 60 秒未发送心跳
- **THEN** 服务端标记设备为离线，广播给其他设备
