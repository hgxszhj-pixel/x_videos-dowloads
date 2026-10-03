# 2026-09-18 — 微信小程序逆向环境（WMPFDebugger @ Intel macOS）


### 1. 触发
用户「配置一下微信逆向的环境」→ 走 web-protocol-recovery skill `wechat-miniapp` route。
`miniapp-reverse-mcp` 早已注册在 `~/.config/opencode/opencode.json`，但工具链跑不起来。

### 2. 核心问题（一句话）
**上游 WMPFDebugger 的 macOS 支持只有 arm64；本机是真 Intel x86_64。** 偏移是架构相关的，
把上游 arm64 偏移打到 x86_64 slice 上会命中**函数中部/`ret`**（字节级已验证）→ 必崩。
上游 `frida/config/darwin/` 只有 1 个文件，没有 x86_64 可用偏移。

### 3. 产出
| 偏移 | 值 | 状态 |
|---|---|---|
| `LoadStartHookOffset` | `0x59a2730` | ✅ **live 验证正确**（hook 命中 + scene 解出 1101/1256/1000 + 零崩溃） |
| `CDPFilterHookOffset` | `0x9285470` | ✅ **live 验证正确**（真实 DevTools 连上后 patch 触发 7692 次）；MCP 不可用是 cdp_use 问题，与偏移无关 |
| `SceneOffsets` | `[56,1504,8,1440,16,456]` | ✅ 架构无关，复用上游 |

### 4. 做了什么
1. **同步真上游**：GitHub `evi0s/WMPFDebugger` 的 `src/platform/` + `frida/config/{darwin,linux,win32}/`
   （本地原树是 Gitee `tuling-python/web-reverse-mcps-tool` 的副本，**Windows-only，无 macOS 支持，别从它拉更新**）
2. **推导 x86_64 偏移并写入** `frida/config/darwin/addresses.269136.json`（arm64 原件另存 `.arm64.json`）
3. **`yarn install`** 重建 node_modules（此前从未跑过）
4. **注入并 live 验证**：tmux 常驻 `node_modules/.bin/ts-node src/index.ts --debug-frida`
5. **加防护层** `~/.local/share/mcp-tools/wmpf-local/`：overlay 真源 + `apply-local.sh`（幂等回装 + 漂移检测）
   + `run.sh` + `update-upstream.sh`（白名单同步，绝不用 `--delete`，不碰 `miniapp-reverse-mcp/`）

### 5. 交付物
- 备份：`~/backups/wechat-miniapp-reverse-20260918-150425/`（9.4M/182 文件 + 7.8M tar.gz，三方 sha 一致）
- 配置实录：工具树 `README.debug-zh.md`（348 行，§3 改动清单 / §4 踩坑 P1~P10 / §5 验证套路 / §9 来源对比）
- 记忆：`~/memory/projects/wechat-miniapp-reverse.md`
- 用法：`~/.local/share/mcp-tools/wmpf-local/run.sh`；DevTools 入口
  `devtools://devtools/bundled/inspector.html?ws=127.0.0.1:62000`

### 6. 更正（2026-09-18 晚）：`CDPFilterHookOffset` 是【正确】的
早前「已证伪」是**误判** —— 我用 `node ws` 合成 CDP 流量做探针，而 **send-to-client 过滤路径只有真实 `devtools://` 前端才会走**。
真实 DevTools 连上后日志立刻出现 `[cdp] CDP client connected` → `[patch] CDP filter on leave, retval+8 = 6` → `[patch] CDP filter patched`，
统计 **7692 次**（`retval+8` 7692 次全部 = 6；同期 `OnLoadStart onEnter` 48 次、CDP 客户端连接 19 次）。
→ **三个偏移全部 live 验证通过。** 教训：**负结果不能只凭「我的流量没触发」下结论**，要先问「我造的流量是否走那条路径」。
MCP 不可用是 `cdp_use` 把 future 注册在本地 id(3,4,5)、代理对 session 级命令回包用自身 id(381,382) 所致，**与偏移无关**。
→ `cdp_use` future 永不解 → `miniapp-reverse-mcp` 不可用。**DevTools 手动调试不受影响。**
下一步：签名探针（挂候选 `onLeave`，判据 `retval!=null && retval.add(8).readU32()===6`，
需真实 DevTools 流量；已排除 30 个静态候选）或装 Ghidra。
**坑**：frida CLI 的 `setTimeout` 回调在本环境不执行 → 探针汇报必须写在 `onEnter`/`onLeave` 里直接 `send`。

---

## Session: 2026-09-18 — opencode 逆向 MCP 三件套接线 + develop_software 目录盘点

### 1. 触发
用户要求「分析 `/Users/hgxszhj/develop_software` 目录的文件夹,配置逆向的 skill、mcp」,随后批准按建议执行(本地构建 js-reverse + 清理重复副本)。

### 2. 目录盘点 — 3 份副本
| 路径 | 角色 | 处置 |
|---|---|---|
| `~/develop_software` | `web-reverse-mcps-tool` 工作副本(skills/scripts/tests + Camoufox/CloakBrowser/WMPFDebugger),**非独立 git**(`rev-parse` 顶层=`~`;仅 `WMPFDebugger/.git` 嵌套) | MCP 源码主路径 |
| `~/web-reverse-mcps-tool` | **唯一带 gitee origin 的正规克隆**(`origin=gitee.com/tuling-python/web-reverse-mcps-tool.git`,含完整提交史) | **保留**(删则丢版本控制) |
| `~/develop_software/WMPFDebugger.old-polluted` | 175M 旧/污染副本(含独立 `miniapp-reverse-mcp`+.venv) | 移入 `~/.Trash/WMPFDebugger.old-polluted-20260918-133632` |
| `~/.local/share/mcp-tools/` | `local_skill_sync.py` 同步镜像(MCP 稳定路径) | 复用 |

### 3. 操作清单
1. 备份 `~/.config/opencode/opencode.json` → `opencode.json.bak-2026-09-18-131913-reverse-wire`
2. mcp 块由「只有 miniapp 且指向失效 venv 路径」改为 3 项:
   | server | command |
   |---|---|
   | `camoufox-reverse` | `uv run --directory ~/develop_software/Camoufox/camoufox-reverse-mcp camoufox-reverse-mcp` + env `CAMOUFOX_BROWSER_ROOT=~/camoufox-reverse`、`CAMOUFOX_EXECUTABLE_PATH=~/camoufox-reverse/Camoufox.app/Contents/MacOS/camoufox` |
   | `js-reverse-mcp` | `node ~/develop_software/CloakBrowser/js-reverse-mcp-local-cloak/build/src/index.js` |
   | `miniapp-reverse-mcp` | `uv run --directory ~/.local/share/mcp-tools/WMPFDebugger/miniapp-reverse-mcp miniapp-reverse-mcp` |
3. `develop_software/CloakBrowser/js-reverse-mcp-local-cloak`:`npm install`(461 包)+ `npm run build`(PATH 用 nvm node v22.20.0)
4. `WMPFDebugger.old-polluted` → Trash

### 4. 验证
- opencode.json `json.load` OK;4 个目标路径均存在
- `js-reverse`:`node --check` 通过 + **真实 MCP initialize 握手**回 `serverInfo.name=js-reverse` v3.0.9
- `camoufox_reverse_mcp` import OK(uv run)
- `miniapp` import OK(uv run)
- `web-protocol-recovery` skill 本会话可见(`~/.agents/skills/skills/`),无需改动

### 5. 决策
- 🟢 服务名沿用 `camoufox-reverse`/`js-reverse-mcp`/`miniapp-reverse-mcp`(与 `~/.claude.json` 及本会话工具名一致),避免重复注册
- 🟢 js-reverse 配置指向 develop_software 本地 build;镜像 `~/.local/share/mcp-tools` 仍由 local_skill_sync 维护
- 🟢 不删 `~/web-reverse-mcps-tool` — 唯一有 `origin` 的克隆;develop_software 无自身 repo
- 🟢 `.old-polluted` 走 Trash(可恢复)非 `rm -rf`(符合安全规则)

### 6. 已知 follow-up
- **需重启 opencode** 新 MCP 才生效
- `miniapp` 使用前须 `npx ts-node src/index.ts` 起 WMPFDebugger + 打开微信小程序(`ws://127.0.0.1:62000`)
- `develop_software` 不自包含 `miniapp-reverse-mcp`(仅镜像有);如需自包含,从镜像复制过去并改配置
- 改 js-reverse 源码后须在 develop_software 重跑 `npm run build`(配置已指向其 build)

---

