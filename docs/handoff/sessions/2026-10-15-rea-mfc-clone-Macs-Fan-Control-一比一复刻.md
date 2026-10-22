---
name: 2026-10-15-rea-mfc-clone-Macs-Fan-Control-1to1
description: 用 rea 逆向 Macs Fan Control 1.5.21 做一比一原生 macOS 复刻 —— Qt rcc 资源提取、AppleSMC 自实现、温控算法反编译对齐、7 个风扇键模板运行期直读、主窗口原生控件全部按量值自绘
type: session-log
---

# 2026-10-15 — rea 逆向 + Macs Fan Control 一比一复刻 (rea-mfc-clone)

## 触发

用户贴了一张 Macs Fan Control 窗口截图 + 一句「利用 rea 一比一复刻一个 mac 版本」。要求逐像素对齐，
且"数据要来自原程序自己的来源"，不接受"看着像"。

## 做什么

### 1. REA 真跑（不是样子）

- `npm i -g rea-agents` → rea 6.5.0 落 `~/.local/bin/rea`；`rea setup --yes --install-hopper` 自动装 Hopper 到 `~/Applications`
- `rea analyze` 成功：provider=hopper、8989 过程、sha256 证据落 `evidence/rea_analyze.json`
- `rea search --kind procedures` → 322 个带真实地址的符号（`evidence/rea_procs_fan.json`）
- `rea decompile` 2/3 成功：`SensorBasedControl_Simple::update` @0x100036ec0、`setFansControlModes` @0x100049a10；
  `setFanRPM` 因 Hopper 许可证对话框停在**另一个 Space**（Orca 全屏盖着）而 `provider_timeout` → 转 otool
- `capture-native-ui-scenario` 只跑通参数校验：**AX 权限未授予**（`AXIsProcessTrusted=false`，System Events 报 -1719），UI 树取不到

### 2. 不用 Hopper 的静态路线（otool）

- `otool -arch x86_64 -tvV` 全量反汇编 → `evidence/asm/full_disasm.txt`（561,557 行），抽出 4 个函数
- 写入编码定案：`setFanRPM(string,ushort)` 按 `m_smcFanKeySize` 分支 —— 键长 4 → `cvtsi2ss`/`movss`（`flt ` 浮点）；
  键长 2 → `rpm*4`（fpe2 语义）；其他 → 打日志后失败
- key 由 `rpmTypeToString(FanTpmType, idx)` + `fmt::vformat` 运行期拼出 → 解释了二进制里搜不到 `F0Tg` 字面量
- 模板是**运行期 XOR 去混淆**（`xorl imm32` + 两个 `xorb`，混淆表在文件偏移 0x200536+0x50*n）；
  用已知 imm32 反解出每组前 4 字符 `F{}A/F{}M/F{}M/F{}T/F{}S/F{}M`，尾字母静态解不出（8 段密文尾字节全同）
- 四个 SMC 方法在反汇编里**零个直接 call** → 查小端 8 字节指针，各命中一次且集中在 0x342a20–0x342a70 = **方法表虚调用**

### 3. 运行期直读（补上静态的缺口）

- attach 被拒真因**不是 TCC**：`codesign -dv` 显示 `flags=0x10000(runtime)` 且无 `get-task-allow` → 强化签名连
  "调试器自己启动"都拒（TCC 里 `com.stablyai.orca = 2` 授权确实已生效也没用）
- 解法（**原版一字节不改**）：复制 App 到 scratch → `xattr -cr` → 用 `get-task-allow` plist ad-hoc 重签**副本** →
  `lldb -b -f 副本 -s cmds.txt` 在 **main 断点**停下（静态初始化器已跑完）→ 解引用 7 个全局
- 直读结果（`evidence/lldb_templates_readback.txt`）：`F{}ID / F{}Mn / F{}Mx / F{}Ac / F{}Tg / F{}Sf / F{}Md`，**全是 len=5**
- 与硬件键空间交叉：本机 `#KEY` 1164、`F` 开头 32 个；`F0Tg/F0Sf/F0Md` 各自唯一，`A`→`Ac`（`AH/AL` 是 spf0 环境温度，
  语义排除），`M`→`Mn/Mx/Md` 三候选与 3 个 M 模板数量正对上；类型/长度与反编译两分支逐条吻合
- 一处修正：此前按 `movb $0x0, 0x6(%rax)` 推"6 字符模板"是**错的**，直读显示 5 字符

### 4. 复刻品（原生 AppKit，无 Xcode 工程）

- 窗口 821×403pt；几何全部来自截图 2× 像素测量（`evidence/original_user_screenshot.png`）
- 数据不自造：传感器表/排序/图标全部取自原版 Qt rcc 内嵌 XML（从 Mach-O 抠 gzip 段，5 个 XML + 116 张 PNG）
- 自实现 AppleSMC 客户端（两个非显然坑：内核要 C 布局 **80 字节**，Swift 同字段 struct 是 76 字节 →
  `kIOReturnBadArgument`；`key`/`data32`/`flt ` 是**本机序**，`sp78`/`ui32` 大端）
- 温控对齐反编译：低于下限 → minRPM；[下限,上限] 线性插值；60s + 中点阻尼状态机

### 5. 主窗口原生控件全部按量值自绘（本轮主要修复）

原生 `NSButton/NSPopUpButton` 与 Qt 扁平外观差得远，且**非激活窗口会把标题调暗**（实测最暗只有 198，
规格是 0），因此逐个量值后自绘：

| 元素 | 规格量值 | 自绘后 |
|---|---|---|
| 表头列分隔线 ×2 | 133pt / 266pt，1pt 灰 | 复刻分隔线 pt=[11.0, 133.0, 266.0] = 规格 |
| 风扇胶囊 `PillButton` | 边框 213、圆角收敛 15px=7.5pt、字色最暗 38、bbox 284.0–386.0pt | 边框 213 ✓ 圆角 15px ✓ 最暗 38 ✓ bbox 284.5–385.5pt |
| 预设胶囊 `QtComboPopup` | 白底 255 / 边框 216 / 字最暗 38 / 圆角 3.5pt / 箭头距左 93pt | 255 ✓ 216 ✓ 38 ✓ bbox 207.0–313.0pt（差 1pt） |
| 刷新盘 + 图标 | 填充 246 / 边框 230 / 图标最暗 183 | 同；图标实测 182 |
| 底部按钮 `QtPushButton` ×2 | 585.0–689.5pt 与 704.0–764.5pt，边框 228，字最暗 0 | 位置逐值相同、228 ✓、字最暗 198 → **0** ✓ |
| "?" 圆形 `QtCircleButton` | 字形 795.0–801.5pt | 796.0pt（差 1pt；自绘前量不到字形） |

## 验证

| 项 | 结果 |
|---|---|
| 自检 | `./build/selftest` → **SELFTEST PASSED（22/22）** |
| 去标题栏整体像素差（基准=用户截图） | 10.12 → **9.73**（此前已验证版本 9.84、本轮起点 10.12） |
| 左表区域均值 | 7.46 → **7.33** |
| 底部带均值 | 21.18 → **17.10** |
| 全窗均值 | 21.7（**不可用作结论**：差异集中在标题栏条，抓图时窗口激活态不同；块统计显示差异最集中的 10 块全在 y=0 那一行） |
| 风扇键 | 本机 `#KEY`=1164、`FNum`=2；fan0 1836–5312–5297、fan1 1700–4906–4905 |
| 写入路径 | 非 root 下 `err write F0Tg failed` 优雅失败，应用退回只读（不崩） |

## 更正（两次自我纠错，都记录在案）

1. **"复刻胶囊文字没画出来"是假警报**：根因是我按窗口 pt 反算裁剪区时**锚点反了**（那一片不是胶囊所在行），
   叠加非激活窗口调暗标题。否证它的两条硬证据：运行时探针打印 `title=自动 enabled=true alpha=1.0
   frame=(285.0,74.5,102.0,21.0)`；逐像素聚合与旧版持平（10.11 vs 9.84、左表 7.46 vs 7.41）。
2. **那条全库 grep 是跑偏的**：把 mangled 符号表当候选刷了一屏噪声，顺带捞到的 `F0md` 落在 tinyxml2 符号名里，
   是假命中；"没有明文键字面量"这一定论不变。

## 决策

- **D-2026-10-15-3**: 主窗口**所有**原生控件改为按量值自绘（不用系统 bezel）—— 理由：原生外观带渐变/强调色/焦点态调暗，
  且聚合像素差对 1pt 线、20px 控件完全无感（≤0.05% 像素）；自绘后抓图与规格可稳定逐点比对
- **D-2026-10-15-4**: `FxSf` / `FxID` 两个模板**不复刻** —— `Sf` 在可见 UI 无对应列；`ID` 对应键本机不存在；
  且"谁在什么条件下写 Sf"追不到（方法表虚调用链），按规矩记为未解决而不是凭合理性实现
- **D-2026-10-15-5**: **不装 root 特权助手**（要用户输密码）→ 应用保持只读；调速功能未启用（需用户明确授权）
- **D-2026-10-15-6**: 用**硬件键空间裁决**争议键名，而不是猜 —— 由解码首字母 ∩ 真实键空间 + 类型/长度一致性定案
- **D-2026-10-15-7**: 技能 `binary-ui-clone` 按规范拆分（正文 24,439 → 7,496 字符 + 4 个 references），
  并补齐两个**悬空引用**（正文提到但文件不存在的 `references/rea-workflows.md`、`scripts/smc-keydump.swift`）

## 落地件

| 路径 | 内容 |
|---|---|
| `/Users/hgxszhj/develop_software/rea-mfc-clone/` | 复刻工程（`build.sh` + `build/Macs Fan Control.app` + `src/*.swift`） |
| `report/复刻报告.md` (41,848 B) | 完整报告：方法、证据分级、§6.5.1–6.5.5.4 反编译/直读、§10.1–10.4 四项自绘修复、§7 已知差异 |
| `README.md` | 从零复现 + 4 套自检命令 + 已验证/未验证清单 |
| `evidence/` | `rea_analyze.json`、`rea_procs_fan.json`、`rea_decompile_*.json`、`asm/full_disasm.txt`（561,557 行）、`lldb_templates_readback.txt`、解出的 5 个 XML + 116 张 PNG |
| `shots/` | 逐轮抓图 + 对照图（`pill_compare_v10.png`、`topbar_2up_v11.png`、`bottom_2up_v13.png`、`acceptance_heatmap.png`） |
| `~/.hermes/skills/software-development/binary-ui-clone/` | 技能：SKILL.md 7,496 字符 + `references/{rea-workflows,static-analysis,pixel-verification,device-layer}.md` + `scripts/{extract_fn.py,smc-keydump.swift}` |
| 本 workspace | 无改动（复刻工程在 `develop_software/`，不在本 repo） |

## 下一步

- **需要用户动作**：装 root 特权助手（输一次管理员密码）→ 才能启用调速；不装则保持只读（写入优雅失败）
- 已知未闭合：① 设置窗口内部 —— 原版对话框在另一 Space 且强化签名，**抓不到图也量不了**，保持骨架；
  ② 右表滚动条几何与配色未逐项核；③ 传感器表条纹仅整体核过
- 已知限制（不改）：含标点表头串比原版宽 8%（纯中文串宽比 1.000 ⇒ 字号正确，差异来自 Qt 自带西文字形，
  调小字号会破坏中文匹配）；磁盘/NVMe 温度未接（原版走 SMART 需 root）
- Reason: 研究性复刻，产物在 `develop_software/rea-mfc-clone`，本 session 0 commit
- Ref-trace: 无（本 session 0 commits）
