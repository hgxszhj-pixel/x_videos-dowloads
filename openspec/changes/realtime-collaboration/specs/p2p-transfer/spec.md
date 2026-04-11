## ADDED Requirements

### Requirement: p2p-transfer

系统 SHALL 支持 P2P 文件传输，包括 HTTP 分块传输和断点续传。

#### Scenario: 下载完成广播
- **WHEN** 设备完成下载
- **THEN** 广播 FileAvailable 消息，包含 IP、端口、文件信息

#### Scenario: 请求文件
- **WHEN** 其他设备需要获取已下载文件
- **THEN** 发送 RequestFile 消息到服务端转发

#### Scenario: 分块传输
- **WHEN** 设备 B 请求设备 A 的文件
- **THEN** 设备 A 启动 HTTP 服务器，设备 B 分块下载

#### Scenario: 断点续传
- **WHEN** 文件传输中断
- **THEN** 设备 B 可以从断点继续下载（记录已接收字节）
