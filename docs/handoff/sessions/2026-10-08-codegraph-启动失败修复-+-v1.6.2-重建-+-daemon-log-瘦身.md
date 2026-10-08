# codegraph 启动失败修复 + v1.6.2 全量重建 + daemon.log 瘦身 — 2026-10-08

## 触发

Claude Code MCP 选择器显示 codegraph 标 **✗**(Local MCPs 段)。`~/.claude.json` 里 command 指向
`/Users/hgxszhj/.npm/_npx/febab48ff478372d/node_modules/@colbymchenry/codegraph-darwin-x64/bin/codegraph serve --mcp`,
但 stderr 一片空白,`exit=137` (SIGKILL)。

## 根因

`_npx/.../codegraph-darwin-x64/lib/dist/` **缺失**。`bin/codegraph` 脚本期望:

```sh
exec "$DIR/node" --liftoff-only --disable-warning=ExperimentalWarning \
  "$DIR/lib/dist/bin/codegraph.js" "$@"
```

但 `_npx/.../codegraph-darwin-x64/` 实际只残留:
- `bin/`(wrapper 脚本)
- `node`(60 MB 裸 bundled node 二进制)
- `lib/node_modules/`(子依赖)
- `lib/kernel/`(`codegraph-kernel.node` 32.6 MB native 模块)
- `package.json`

`lib/dist/bin/codegraph.js` 这个 JS 入口根本不存在 → serve 一启动就 SIGKILL。

`.omo/codegraph/` 还有一份 1.5.0 完整副本(v1.5.0 没有 MCP `serve --mcp` 子命令,作废)。

## 修复路径

1. **清半残缓存** — `rm -rf _npx/.../codegraph-darwin-x64` + `.codegraph-darwin-x64-cGqkHyWZ`(pack cache),
   加 `codegraph/`(主包) + `.codegraph-kAkwI98p`(临时 rename 残骸)。每删一个都得再来一次,npm
   `ENOTEMPTY` 报了一轮才通。
2. **npx repull** — `npx --yes @colbymchenry/codegraph --version` → `1.6.2`,`lib/dist/` 齐了。
3. **MCP handshake 验证** — `codegraph serve --mcp` 接 `initialize` 请求 → `serverInfo: {name: codegraph, version: 1.6.2}`,`exit=0`。

## v1.6.2 重建

旧索引是 v1.5.0 时代留下的(`Index was built by v1.5.0; re-index to pick up this engine's improvements`)。
`codegraph index .` 全量重建:**198 文件 / 4,067 nodes / 8,578 edges / 1.2 s**。
边数 8,837 → 8,578 是引擎去重/重构的正常差,不是丢失。

## daemon.log 瘦身

旧 daemon.log **65.4 KB / 828 行**,前 11+ 条全是 `RuntimeError: Aborted(Error: ENOENT...)` +
`failed to asynchronously prepare wasm`,都是 2026-09-02..09-06 codegraph 半残期的失败痕迹。
当前运行零错误。

清理流程:`SIGTERM 67486` → `: > daemon.log` → MCP host 自动 respawn → 新 daemon pid 63193 → log
3 行 bootstrap,共 **398 B**。

## 文件/配置影响

- `~/.claude.json` 里 `command` 路径不动(已指对)
- `~/.mcp.json` project-level 无变化
- 项目本地 `.codegraph/codegraph.db`(11.78 MB)+ WAL(0.01 MB)无变化,索引数据保留
- `codegraph status .` 现在报 `Index is up to date` ✓,`Files: 198, Nodes: 4,067, Edges: 8,578`

## 验证

```
$ codegraph serve --mcp → initialize handshake → serverInfo.codegraph 1.6.2 ✓
$ codegraph status .     → Index is up to date ✓
$ ps: daemon pid 63193 (v1.6.2) alive, writing daemon.log ✓
$ wc -c daemon.log       → 398 bytes (was 65,634) ✓
```

## 教训 / 改进

- npm npx cache 半残状态很难追:`codegraph-darwin-x64/lib/dist/` 缺失但其他 lib 子目录在,
  错误现象是 SIGKILL 而非 ENOENT,排查要直接看脚本 `$DIR/lib/dist/bin/codegraph.js` 假设
- `lsof -p <PID>` 确认进程活/绑定的文件比 `ps -p` 更可靠(rtk 包装层对某些 ps 调用会抛
  `No such file or directory`,但 lsof 直接走内核表)
- 半残缓存 vs pack cache (`*.codegraph-darwin-x64-cGqkHyWZ/`) vs rename 残骸(`*.codegraph-kAkwI98p/`)
  三类都要清,npm ENOTEMPTY 报哪一个就删哪一个,共 3 轮才通