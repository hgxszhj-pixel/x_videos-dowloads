# 2026-10-04 — moscan 模块 25/26/27：卸载面板 + guard 根因 + 复制卸载命令；Paragon/OrbStack 残留判定

## TL;DR

moscan 补齐 mole 的招牌功能「卸载 app」：把 CLI 的卸载逻辑抽成共用纯数据（`UninstallPlan` / `resolve_app` /
`uninstall_plan` / `list_apps`），GUI 加第 7 个面板；写这个模块的测试时挖出 `guard()` 的根因 bug
—— 判断「是否在家目录内」读的是**进程环境变量 `HOME`**，而调用方传的是 `Paths.home`，两个事实来源；
最后加「复制卸载命令」+ CLI `--only`，把界面上的勾选编成可**直接粘贴执行**的命令，
绕开「GUI 点击验证被辅助功能权限卡住」这个死结。测试 `46/49 → 55/59` 全绿、`0 warnings`。
副线：判定 `/Library/Application Support/Paragon Software` = **Paragon NTFS for Mac 15.9.314** 残留（24.7 MB），
并顺手发现 **OrbStack 特权助手仍挂着 root 开机项**（原清理文档漏记），两项都出了校验过的备份包，等用户 sudo。

## 交付物

| 模块 | 内容 | 关键文件 |
|------|------|---------|
| 25 | GUI「卸载 app」面板（侧栏搜 app + 后台算计划 + 命中表 + 确认框）；CLI 卸载逻辑抽成公共纯数据，`cmd_uninstall` 改为调用同一份实现 | `moscan/src/cli.rs`, `gui.rs`, `scan.rs` |
| 26 | 根因修复：`guard(home, path, …)` 显式接收 home，不再偷读 `$HOME`；前缀比较两边都 canonicalize | `moscan/src/scan.rs` |
| 27 | 「复制卸载命令」按钮 + `MOSCAN_COPY_CMD` 调试钩子 + CLI `--only`（精确复现界面勾选） | `moscan/src/cli.rs`, `gui.rs` |
| 副线 | Paragon NTFS 残留判定 + OrbStack helper 开机项发现 + 两个校验过的备份包 | `~/Desktop/*residue*.tar.gz` |

## 验证（本 session 实测）

```
cargo test --no-default-features   55 passed; 0 failed      (46 → 55)
cargo test --features gui          59 passed; 0 failed      (49 → 59)
cargo build --features gui          0 warnings
moscan selftest                    25 项断言全通过
```

- **端到端（GUI 产出的命令原样执行）**：GUI 打印
  `'/Users/hgxszhj/develop_software/moscan/target/debug/moscan' uninstall '/Applications/Clash Party.app' -y`
  → 追加 `-n` 干跑 = 9 项 / 47.5 MB / 合计 702.7 MB，与界面表格逐行一致。
- **CLI `--only` 往返四例**：不给 → 9 项（旧行为不变）；点名 2 项 → `9 → 2`；空 → `9 → 0`（只动本体）；点名不存在路径 → `9 → 0`。
- **备份校验**：`paragon-ntfs-residue-2026-10-04.tar.gz` 11 MB（sha256 `c2e26b3d…`，源/归档 124/124，抽验字节一致，`gzip -t` 通过）；`misc-root-residue-2026-10-04.tar.gz` 1.4 MB（sha256 `e238b6a8…`，18/18）。

## 坑 / 更正（按代价排序）

| # | 内容 | 教训 |
|---|------|------|
| 1 | `--only` 最初写成 `Vec<String>`：空值既表示「没给」又表示「不过滤」，界面上一项不勾会退化成**删光全部关联项** | 改成 `Option<Vec<String>>` 区分，加测试钉住 |
| 2 | 命令写死 `moscan uninstall …`，粘进终端报「未找到命令」（本机没装到 PATH）—— 真跑一遍才发现 | 用 `std::env::current_exe()` 拼真实路径 |
| 3 | `guard()` 读进程 `HOME` 判断放行范围，与传入的 `Paths.home` 不一致（新测试零命中才暴露） | 一个判断只能有一个事实来源 |
| 4 | 只 canonicalize 路径不 canonicalize home：macOS `/tmp → /private/tmp`，前缀比较落空 | 两边都 canonicalize |
| 5 | 测试现场放 `temp_dir()` 必被守卫判「系统目录」（`FORBIDDEN_ROOTS` 含 `/private`） | 现场改放家目录下临时目录，跑完删净 |
| 6 | `pkill -f 'target/debug/moscan'` 会匹配到自己那条 shell 命令行并自杀 → 新实例没起来、读到的是旧日志 | 改用 `pgrep -x` + 按 PID kill |
| 7 | 抓窗口必须按 `kCGWindowOwnerPID == 目标 PID`：机器上同时开着备忘录/Safari/Orca 等一堆同尺寸窗口，按大小选会抓到别人的窗口；`CGWindowListCreateImage` 对已死进程的残留窗口返回 `None` | 截图先验属主 |
| 8 | **未解释现象**：抓到的一帧画的是「无主残留」面板而非「卸载 app」（进程日志/环境变量都确认在 uninstall 面板、命令正确产出） | 功能侧有端到端证据；这一帧没查完，既不是已确认 bug 也没排除怀疑，用户叫停后未再追 |
| 9 | 清理文档里 ZeroTier 的 sudo 待办**已过期**（实测应用支持目录/三个命令/开机项/进程全无） | 复核旧待办要重新实测，不能照抄 |

## 决策与下步

- **root 待办三条**（备份已就绪，用户自己输密码）：Paragon 厂商卸载脚本 / OrbStack helper + 其 plist / 搜狗皮肤 QuickLook 插件；跑完由助手逐项复核（含 `sudo kmutil showloaded | grep -i paragon` 确认 kext 卸载 —— 无 sudo 的 `ioreg` 探测法经对照验证**不可靠**）。
- **moscan 真实点击验证**仍待用户开辅助功能权限；不开也能用（「复制卸载命令」已端到端验过）。
- 详细落档：`~/Documents/moscan-progress-report-2026-10-04.md`（1098 行，第十九~二十一节）、`~/Documents/mac-cleanup-2026-10-04.md`（218 行，第六/七节）。
