# CLI Flag 完整清单 (2026-10-04)

> 来源: `src/cli.rs` — 25 个 `long = "..."` flag。补齐 M4 `HANDOFF 顶部仅举例 3 个` 缺口。

## 按用途分组

### 下载流程 (5)

| Flag | 说明 |
|------|------|
| `--audio-only` | 仅下载音频 |
| `--subs` | 下载字幕 |
| `--list-formats` | 列出可用格式 |
| `--direct` | 直链下载 (跳过 resolver) |
| `--gui` | 启动 GUI |

### 团队协作 (3)

| Flag | 说明 |
|------|------|
| `--create-team` | 创建协作团队 |
| `--join-team` | 加入协作团队 |
| `--server <URL>` | 协作服务器地址 (默认 `ws://localhost:9000`) |
| `--device-name <NAME>` | 本机设备名 (默认 `MyDevice`) |

### 本地历史 (4)

| Flag | 说明 |
|------|------|
| `--init-config` | 初始化配置文件 |
| `--history` | 查看下载历史 |
| `--bookmark` | 收藏当前任务 |
| `--bookmarks` | 列出全部收藏 |
| `--clear-history` | 清空历史 |

### 搜索 (1)

| Flag | 说明 |
|------|------|
| `--search <QUERY>` | 搜索历史任务 |

### 服务器模式 (2)

| Flag | 说明 |
|------|------|
| `--start-server` | 启动 headless server |
| `--port <PORT>` | 监听端口 (默认 `9000`) |

### SHANHU SaaS 集成 (8)

| Flag | 说明 |
|------|------|
| `--shanhu-add-hash <FILE_ID:HASH>` | 手动添加文件 hash |
| `--shanhu-cache-info` | 查询本地缓存信息 |
| `--shanhu-session-info` | 查询当前 session 状态 |
| `--shanhu-login <STATE_JSON>` | 用 state JSON 登录 |
| `--shanhu-login-cookie <COOKIE>` | 用 cookie 字符串登录 |
| `--shanhu-login-token <TOKEN>` | 用 token 登录 |
| `--shanhu-login-ego-lite` | 用 ego-lite 浏览器登录 |
| `--shanhu-login-org-id <ORG_ID>` | 指定登录组织 ID |

## 维护

- 增删 flag 时同步更新本表
- 出参 `/--help` 已自动反映最新 clap derive
- 本表与源码不一致时以源码为准
