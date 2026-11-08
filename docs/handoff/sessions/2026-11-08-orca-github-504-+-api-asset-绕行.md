# 2026-11-08 — Orca 更新 504 根因 + 绕行（API 资产端点）+ `~/bin/orca-update`

> 来源：用户「怎么 https://github.com/stablyai/orca/releases 更新orca 出现504 Gateway Time-out」
> + 应用内截图「Update Check Failed — Could not check for updates」。
> 产物在**库外**（`~/bin/orca-update`），本仓只留记录。

## 症状（两种，别混为一谈）

1. **GitHub 自己的 504**：`github.com/stablyai/orca/releases/download/...` 返回
   `504 Gateway Time-out`，body 是 GitHub 自己的 92 字节错误页。响应头带
   `x-github-request-id` + `x-github-edge-region: westus3` + `x-frame-options: DENY`
   ⇒ **服务端边缘节点拉 blob 超时**，不是本机浏览器/代理问题。同一个 URL **3/3 复现**。
2. **连接本身不稳**：偶发 `curl (55) Failed sending data to the peer` / TLS handshake timeout，
   连的是 `release-assets.githubusercontent.com` 这个 CDN，别的仓库也会中。

**对照实验**：`api.github.com/repos/.../releases` 200 / 1.2s；`cli/cli` 的
`releases/latest/download/...` 正常 302 ⇒ **不是 GitHub 全站挂**，是这条资产路径 + 本机连接质量。

## 为什么应用内更新也不行

Orca 是 **Electron** 应用（Electron 43.8.6），不是 Tauri。electron-updater 检查更新时请求的正是
`https://github.com/stablyai/orca/releases/download/latest/latest-mac.yml` —— **就是坏掉的那个端点**。
所以点 "Re-check" 和 "Download Manually" 都会失败，不是本机 app 装坏了。

## 绕行（已落地并验证）

`https://api.github.com/repos/<repo>/releases/assets/<id>` 带 `Accept: application/octet-stream`
+ token → 302 到签名直链 `release-assets.githubusercontent.com`（支持 Range 206，可断点续传）。
实测 1.5 MB/s 下完 291 MB。

**落地物：`~/bin/orca-update`**（新工具，chmod +x，4 个模式都实跑过）

| 模式 | 行为 | 验证 |
|------|------|------|
| `orca-update` | 更新到最新版并替换 `/Applications/Orca.app` | ✅ 1.4.250 → **1.4.253** |
| `--check` | 只查本机版 vs 最新版 | ✅ |
| `--dry-run` | 解析资产 id 与官方 sha512，不下载 | ✅ |
| `--fetch <id> <path>` | 单独测下载链路 | ✅ |

内置：按架构选包（Intel → `Orca-<v>-mac.zip` / Apple Silicon → `Orca-<v>-arm64-mac.zip`）、
从官方 `latest-mac.yml` 取 **sha512 校验**（不匹配拒绝安装）、退出旧进程、
旧版备份到 `~/Downloads/Orca-<旧版>.app.bak`、`ditto` 换包、`codesign -v --deep`、
装完复读版本号对不上就报错。

## 踩过的坑（4 个）

1. **`gh api` 没有 `--arg` 标志**（那是 `jq` 的）。只能用内联 `select(.name=="...")`。
   写错时 gh 报 `unknown flag: --arg`，但**stderr 被吞**会静默返回空值 —— 看起来像「这个版本没有资产」。
2. **`gh api -i` 有时跟 302 并把 body 流出来** ⇒ 拿不到 `location:` 头，**行为不稳定**。
   取签名直链必须用 `curl -D -` 且**不要跟 302**（不跟才能看到 Location）。
3. **token 别进 argv**（`ps` 可见）：写临时 curl 配置文件（chmod 600）用 `curl -K` 传。
4. **`gh auth status` 会联网校验**，链路 flaky 时**误报「未登录」** ⇒ 改用 `gh auth token` 判断。
   同理，所有 `gh api` 调用都包了一层 3 次重试。

**自事故**：先下了 `Orca-1.4.253-arm64-mac.zip`，**没先看 `uname -m`** —— 本机是 Intel
(i7-9750H)，arm64 包白下 268 MB。正确包是 `Orca-1.4.253-mac.zip`（asset id 720383395）。
→ 下 release 资产前先 `uname -m`。

## 清理

- `/tmp` 里的下载产物已清（回收 **1.3 GB**）：arm64 白下的 zip + 291 MB 的 mac zip + 临时 curl 配置。
- `~/Downloads/Orca-1.4.250.app.bak`（**739 MB**）**故意留着** —— 是回滚副本，等用户决定删否。

## 复现本文档的检查

```bash
curl -sS -o /dev/null -w '%{http_code}\n' \
  https://github.com/stablyai/orca/releases/latest/download/latest-mac.yml   # 期望 504（本机）
orca-update --check        # 期望：本机 1.4.253  目标 <最新版>
```
