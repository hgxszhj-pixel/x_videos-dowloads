---
name: 2026-10-16-airgorah-macos-port
description: 用户问「我的 Mac 能不能跑 Airgorah」→ root 下实测内置 Broadcom 卡不能监听/注入 → 顶「可以源码编译」后把 Linux 专用的特权 agent 移植到 macOS（AF_PACKET→/dev/bpf、SO_PEERCRED→LOCAL_PEERCRED、pkexec→osascript、iw/sysfs→networksetup/system_profiler），9 文件 810 行 cfg 分支，GUI 与 agent 双双跑起来，点网卡给出诚实的「监听模式在 macOS 不存在」硬错误
type: session-log
---

# 2026-10-16 — Airgorah macOS 移植（GUI 能跑 + agent 能起 + 监听模式诚实的硬错误）

## 触发

用户贴来一条今日头条微头条（`散修程序员小鹏` 推荐 Airgorah：Rust 写的 WiFi 审计工具，只支持 Linux），
先问「内容？」，再贴上游仓库 <https://github.com/martin-olivier/airgorah>，然后问「看看我的 mac 可否运行？」。
我给出「跑不了 + 三条路」后，用户顶了一句「可以源码编译」——只能拿编译结果说话，不能复述 README。
用户随后选了 A（先不装 VM，实测内置卡能否进监听模式），再一句「按照你建议执行」= 采纳「移植 agent 到 macOS」。

## 做什么

### 1. 上游核对（不抓渲染页，取原始文件）

- 浅克隆 `b24e463`（2026-10-14，v0.9.0）；MIT、4228 star / 750 fork、101 commit
- 外部依赖真源 = `crates/common/src/deps.rs` 的 `TOOLS` 表：必需 `sh/iw/xterm`(GUI) + `ip/macchanger`(agent)；
  按需 `aircrack-ng/crunch/systemctl/pkexec`；打包元数据 `.fpm` 的 `--depends bash xterm iw macchanger adwaita-icon-theme`
- 那篇文章两处不准：① 「底层核心逻辑借用 aircrack-ng 库」——是外部调用命令行，且 0.9.0 changelog 已写
  `deps: get rid of aircrack-ng suite and mergecap dependencies`；② 配图里的「艾尔戈拉」中文页不是仓库自带（仓库只有英文 README）

### 2. 机器事实（一次采齐）

MacBookPro16,1 / i7-9750H / 16 GB / macOS 15.8.1 (24H32) / x86_64 / `kern.hv_support=1` / 根盘可用 87 GB；
内置无线网卡 Broadcom `(0x14E4, 0x7BF)`，驱动 Apple 私有 `IO80211Family 1200.13.1`；
`command -v` 查 docker/orb/lima/colima/qemu/utmctl/prlctl/VBoxManage/vmrun 全无（Docker Desktop.app 已卸载，
只剩 `~/Library/Application Support/Docker Desktop` 残件）；brew 已有 gtk4 4.22.4；无任何外置 USB 无线网卡。

### 3. 硬件侧实测（root，系统管理员对话框，密码用户自输）

- `brew install aircrack-ng` → 1.7_2 瓶装 9.7 秒；**Homebrew 包不含 airmon-ng**（Linux 专有，不是装坏）
- `airport -I` / `airport -s` 在 root 下**只打印废弃警告、零输出**；裸跑 `airport` 也没有用法说明 → 该 CLI 在 15.8.1 已是空壳
- `airport en0 sniff 6` 跑 14 秒 → `/tmp` 下**没有任何 `.cap`**，`en0` 状态不变
- `airodump-ng en0` → `Failed initializing wireless card(s): en0`；`aireplay-ng --test en0` → exit=1
- 结论：内置卡**不能进监听模式、也不能注入，root 下同样如此** → 不是权限问题；测完 `networksetup -setairportpower` 复原，Wi-Fi 正常

### 4. 编译 probe：先把结论分层，再动代码

- 卡点是 pkg-config 链：需把 **201 个 brew keg 的 pkgconfig 目录** + Homebrew 自带的系统库 `.pc` 目录
  （`…/Homebrew/Library/Homebrew/os/mac/pkgconfig/15`）拼进 `PKG_CONFIG_PATH`，并 `brew install bzip2`
- `cargo check -p airgorah` 通过（55.52 s）；`cargo build -p airgorah` 出 **21 MB** 二进制（1m25s）
- 真跑 → 只弹 `could not locate the 'airgorah-agent' binary next to the GUI`：**卡点全在 agent**
  （8 个编译错误：`libc::AF_PACKET`/`sockaddr_ll`/`ETH_P_ALL`、`nix::…::PeerCredentials`、u64/i64 一处）

### 5. 移植（9 文件 / 810 行，全部按平台 cfg，**Linux 原路径一行没删**）

| Linux 原物 | macOS 替代 | 文件 |
|---|---|---|
| `AF_PACKET`/`sockaddr_ll` | `/dev/bpf*` + `BIOCSETIF`/`BIOCSRTIMEOUT`/`BIOCSHDRCMPLT`（ioctl 码按 `_IOC` 规则现算） | agent/backend/raw_socket.rs（508 行） |
| `SO_PEERCRED` | `LOCAL_PEERCRED`（nix `LocalPeerCred`，`xucred.uid()`） | agent/server.rs |
| `/sys/class/net` 枚举 | `networksetup -listallhardwareports`（`Hardware Port: Wi-Fi` 块的 `Device:`） | gui/backend/iface.rs |
| `iw phy … info` 判 5 GHz | `system_profiler SPAirPortDataType` 找 `(5GHz)` | gui/backend/iface.rs |
| `pkexec` + `PKEXEC_UID` | `osascript … with administrator privileges`，uid/实例号**内联进命令行** | gui/backend/client.rs |
| `getppid()` 当实例号 | 显式传 `AIRGORAH_INSTANCE`（osascript 的父进程不是 GUI） | agent/main.rs |
| `/run`、`/var/lib` | `/var/run`、`/var/db` | common/ipc.rs、agent/globals.rs |
| 工具必需性检查 | 清单加 `linux_only` 标记，非 Linux 跳过 `iw/xterm/ip/macchanger/systemctl/pkexec` | common/deps.rs |
| 网卡监听模式 | 无解 → 硬错误 `MonitorUnsupported`（文案带实测依据），**不假成功** | agent/backend/interface.rs |

关键实现细节（都是踩过才知道的）：

- **Darwin 的 `libc` crate 没有 BPF 绑定**：ioctl 请求码用 `const fn ioc(dir, group, num, len)` 现算，
  数值先写 C 探针从 `MacOSX15.sdk/usr/include/net/bpf.h` 取出来对齐：
  `BIOCSBLEN=0xc0044266`、`BIOCGBLEN=0x40044266`、`BIOCSETIF=0x8020426c`、`BIOCIMMEDIATE=0x80044270`、
  `BIOCSHDRCMPLT=0x80044275`、`BIOCSRTIMEOUT=0x8010426d`
- **`struct bpf_hdr` 在 64 位 Darwin 是 20 字节**（时间戳是 `timeval32` 两个 u32，不是 `timeval`），
  `bh_caplen` 偏移 8、`bh_hdrlen` 偏移 16 —— 用 `const _: () = assert!(…)` + `offset_of!` 编译期锁住
- **BPF 一次 `read()` 返回一批记录**，每条前面有 `bpf_hdr` → 后端自己按 `bpf_hdr` 切帧再交给调用方

### 6. 补完剩下两处（本轮最后一段）

- **解密终端 `xterm` → Terminal.app**：`osascript` 的 `do script` 开窗口（等价 `-hold`），
  `printf '\033]0;%s\007'` 设标签标题（等价 `-T`）；两个平台共用一条 shell 命令行（参数经 `sh_quote`）
- **PMKID 读本机 MAC `/sys/…/address` → `ifconfig <iface>` 的 `ether` 行**

## 验证

| 项 | 结果 |
|---|---|
| 整体构建（补完两处前） | `cargo build --workspace` 通过，1m31s；`airgorah` 21,233,328 B / `airgorah-agent` 13,333,840 B |
| workspace check（补完两处后） | 通过，1m00s |
| agent 单测 | **27 passed / 0 failed**（新增：ioctl 常量、bpf_hdr 布局、BPF 抓包冒烟、ifconfig 读 MAC） |
| GUI 单测 | **3 passed / 0 failed**（真机断言 `get_interfaces()` 含 `en0`、`is_5ghz_supported("en0")==true`；+2 个引号/转义） |
| BPF 抓包（root） | **en0 上 1 帧 / 157 字节**，5.10s，exit=0 |
| 端到端 | GUI(root) 起 → agent 起（日志 `listening on /var/run/airgorah/0-67106.sock for uid 0`，socket `srw------- root:daemon`）→ 点网卡 → 弹 **Monitor mode failed**（截图 `report/evidence/acceptance-monitor-mode-failed.png`，215,418 B），日志里两条错误对应用户点两次 |
| Terminal.app 机制 | 同样的 osascript 调用（echo 替身）`exit=0`，窗口标题实测 `WPA Decryption (selftest)`，随后按标题只关掉这一个窗口 |
| 收尾 | 无 airgorah 进程；`/var/run/airgorah`、`/var/db/airgorah` 已删；Wi-Fi `en0` UP / Power On |

## 坑（都已写进技能）

1. **先前判断过重**：我说过「要重写整个抓包/注入层才能移植」，实际每一项都有对等物
   （`AF_PACKET`→BPF 设备、`SO_PEERCRED`→`LOCAL_PEERCRED`），只有监听模式这一项是真无解。
2. **`pkill -f "<全路径>/target/debug/<app>"` 匹配不上以相对路径启动的进程** → 第一次收尾误报「已清理」，
   实际两个 root 进程还活着。
3. **`sudo cargo test` 会让 root 拥有 `target/`** → 改为编译用普通用户、测试直接跑已编好的
   `target/debug/deps/<crate>-<hash>`。
4. **SIGTERM 打不动这个 GTK GUI**（4 秒后仍在），最后 `pkill -9` 收场 → 因此 **agent 靠 socket EOF 自清这条仍未验证**
   （也解释了上游 `main.rs` 开头为何要清「上次崩留下的陈旧 socket」）。
5. **macOS 的 `airport` CLI 不能当判据**（空壳），要用 `system_profiler` + 真实产物（是否生成 `.cap`、airodump 的报错原文）。
6. **zsh 的后台包装里 `$(find … -exec dirname {} \;)` 可能拿到空变量** → 路径拼装写进脚本文件再 `bash` 它最稳。

## 决策

- **D-2026-10-16-1**：macOS 上监听模式**不做假成功**，直接返回带实测依据的硬错误 —— GUI 必须看到真实原因，
  而不是「成功了但什么都扫不到」
- **D-2026-10-16-2**：**Linux 路径零改动**，全部用 `cfg(target_os)` 分支 —— 保留可上游 PR 的形状，不做不可回退的重写
- **D-2026-10-16-3**：非 root 提权走 `osascript` 系统对话框（密码只进系统弹窗、不经我手），不引入 sudo 包装或 root 助手
- **D-2026-10-16-4**：Darwin 缺绑定的内核常量（BPF ioctl、`bpf_hdr` 布局）**先用 C 探针从 SDK 头取真值**，
  再用编译期 `assert!` 钉住 —— 不靠记忆、不靠猜
- **D-2026-10-16-5**：所有需要 root 的实测统一走「系统管理员对话框 + 脚本文件 + 日志重定向 + 恢复步骤」，密码不经手

## 落地件

| 路径 | 内容 |
|---|---|
| `/Users/hgxszhj/develop_software/airgorah-macos/` | 移植工作副本（9 个源文件改动 + 构建脚本 + 报告） |
| `report/进度报告.md` (155 行) | 逐模块改动、实测数字、未解决项、复现命令、回退方式 |
| `report/evidence/acceptance-monitor-mode-failed.png` (215,418 B) | 验收截图：`Monitor mode failed` 对话框 |
| `scripts/build-macos.sh` | 自动拼 `PKG_CONFIG_PATH` + 先 `pkg-config --exists gtk4` 快速失败 |
| `scripts/ipc_probe.py` | 直连 agent socket 的探针（4 字节大端长度 + JSON） |
| `~/.hermes/skills/software-development/macos-tool-feasibility/` | 技能新增「移植路线」一节（Linux→macOS 对照表 + BPF 细节）与 3 条 pitfalls |
| 本 repo | 仅本 session 文件 + `HANDOFF.md` 索引行 |

## 下一步

- **需要用户动作**（各一次）：① 在那个 airgorah 窗口按 Cmd-Q 优雅退出 → 才能验证 agent 的 socket 自清；
  ② 以**普通用户**启动 GUI、点网卡 → 在系统对话框输一次密码，验证 `osascript` 提权分支
- 已验机制但未验整条流程：解密终端（xterm→Terminal.app）——GUI 内需要真实握手包才能点到
- 结构上不可能到达：`EnableMonitor` 之后的扫描页/踢人/抓握手包 —— 内置卡不支持监听模式（第 3 节实测）
- 可回退：`brew uninstall aircrack-ng bzip2`；scratch 克隆在 `~/.hermes/cache/scratch/airgorah/`（闲置 24h 自动清）
- Reason: 用户追问「我的 Mac 能不能跑 Airgorah」并顶回「可以源码编译」，本轮把结论做到硬证据级（硬件实测 + 源码移植 + 端到端验收）
- Ref-trace: 无（本 session 本 repo 0 commits）
