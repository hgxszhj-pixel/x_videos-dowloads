# 2026-09-18 — OpenCode 净室重装 (no provider/auth clean state)


### 1. 触发
用户要求"清除 opencode 的目录和相关的配置文件,重新安装一遍,保持干净的无配置的 opencode 环境" — 选择最激进选项(连 wrapper 也删) + `npm i -g opencode-ai` 同版本重装。

### 2. 操作清单(8 步)

| 步骤 | 命令 / 路径 | 说明 |
|------|------------|------|
| 1 | `kill 28683 45984` | 杀掉 2 个运行中的 opencode 主进程(避免 rm 时还占用 `~/.cache/opencode/packages`) |
| 2 | `rm -rf ~/.local/share/opencode` | 清 `auth.json` + `opencode.json` + `opencode.db` (210MB) + 11 份 `.bak*` |
| 3 | `rm -rf ~/.cache/opencode` | 清 `models.json` (4.5MB) + `bin/` + `packages/` + `skills/` |
| 4 | `rm -rf ./.opencode` | 项目级 `node_modules` + `package.json` |
| 5 | `rm ~/bin/opencode-failover.sh` | wuzuapi 主备包装脚本(对应旧 memory `opencode-wuzuapi-baseurl-failover.md`) |
| 6 | `~/.zshrc` 删 3 行 | `alias opencode='~/bin/opencode-failover.sh opencode'` + `export WUZUAPI_BASE_URL=...` + 注释 |
| 7 | `npm uninstall -g opencode-ai` | removed 3 packages in 323ms |
| 8 | `npm install -g opencode-ai` | added 13 packages in 15s,版本回 1.18.31 |

### 3. 验证
- `which opencode` → `/Users/hgxszhj/.nvm/versions/node/v22.20.0/bin/opencode` (npm 链接,不是 wrapper)
- `/Users/hgxszhj/.nvm/versions/node/v22.20.0/bin/opencode --version` → `1.18.31`
- `ls ~/.local/share/opencode` → 仅首次运行自动重建的空 `bin/log/repos` 三个运行时目录,无 `auth.json`/`opencode.json`/`opencode.db`
- `ls ~/.cache/opencode` → 首次运行重建 `bin/packages/skills`,4.5MB models.json 待首次 `opencode` TUI 启动时按需下载
- `ls ~/bin/opencode-failover.sh` → `No such file or directory`
- `grep opencode ~/.zshrc` → 无结果

### 4. 决策
- 🟢 用户选"最激进" — wrapper + alias + env var 一并删,避免半干净状态留下混淆
- 🟢 保持 npm 同版本 (1.18.31),不引入 breaking change 风险;若日后需新功能再 `npm i -g opencode-ai@latest`
- 🟢 旧三块 opencode memory (failover/wuzuapi/智谱) 保留为**参考档案**而非删除 — 双域名探测思路 / wuzuapi 不接 Anthropic 上游结论 / GLM baseURL 必须带 `/v1` 三条经验仍可复用

### 5. 已知 follow-up
- 当前 shell 进程仍缓存旧 alias — `unalias opencode` 即清,新开 terminal 自动失效
- provider/auth 全部为空,用户下次按需重接(wuzuapi / 智谱 GLM / 其他),配置写到 `~/.local/share/opencode/opencode.json`(首次运行后目录存在)
- failover 包装脚本已删 — 若日后想恢复 wuzuapi 主备双域名探测,需重写 `~/bin/opencode-failover.sh` 并在 `.zshrc` 加回 alias + `WUZUAPI_BASE_URL` env
- 新增 memory `memory/opencode-clean-reinstall-2026-09-18.md` 记录本次操作 + 标记旧 memory 状态

---

