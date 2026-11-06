# 2026-11-06 — Tendedero 扩展:接住第三方截图软件(Snipaste 等)

> 起因: 用户「本地已经安装发现有一定限制不支持其它公司的截图软件例如 snipaste 的截图,扩展一下功能吧?」→ 在 `~/Projects/tendedero`(upstream clone)实现「Capture apps」扩展,提交 `d28b0be` 于分支 `feat/capture-apps`。

## 一、为什么原来接不住

Tendedero 只捡 macOS 自己截的图。判据是扩展属性 `com.apple.metadata:kMDItemIsScreenCapture` —— macOS 截图自带,第三方 app 写的文件没有。本机 Snipaste 的三个事实叠在一起:

| 事实 | 证据 | 后果 |
|---|---|---|
| `snip` 走**剪贴板** | `~/.snipaste/splog` 里是 `sp snip -o clipboard` | Tendedero 的 `clipboardEnabled` 默认关 ⇒ 完全不挂 |
| 文件模式写 `~/Desktop/Snipaste_*.jpg` | `~/.snipaste/config.ini` 的 `quick_save_path` | Desktop watcher 要 xattr ⇒ 静默丢弃 |
| 默认存 `~/Pictures` | 同上 `last_save_folder` | 该目录根本没被 watch |

upstream 其实有「Also watch folders」(手动选目录,最多 3 个,`anyImage`),但要用户自己去点;**代码层真正缺的只有「按 app 自动认目录 + 按文件名前缀认图」**。

## 二、设计:三处扩展点

### 1. `Sources/Tendedero/CaptureApps.swift`(新增,Foundation-only)

`CaptureApp { id, name, folders, prefixes }` + `CaptureApps.known` 一张表,每行一个 app:`id / name / appNames / extraPrefixes / settings`。

- **目录不再猜**。原来给每个 app 编一个 `~/Pictures/<App>`(那些目录本机根本不存在),改成通用规则:截图本来就落下的地方(`~/Desktop` / `~/Pictures` / `~/Downloads`)+ app 自己配置里说的那个。配置读得到的(Snipaste 的 `~/.snipaste/config.ini`)照样优先读:`parseIni` 解析(跳过 `[Section]` 头与 `;`/`#` 注释)取出 `last_save_folder` / `quick_save_path` / `file_name_pattern`。
- **文件名不再猜**。通用规则 = app 自己的名字 + 一个分隔符(`Snipaste ` / `Snipaste_` / `Snipaste-`)。分隔符是名字的一部分,所以 `Kap` 不会顺手拿走 `Kaplan.jpg`。app 另有自己的叫法就写进 `extraPrefixes`(Kap → `Screen Shot `),配置里的模板前缀走 `patternPrefix`(取第一个 `$` 之前的固定部分)。
- 开关存 **`captureAppsDisabled`(存被关掉的 id)**,不是存开启列表 —— 这样以后新装的 app 默认开。

### 2. `ScreenshotWatcher.Acceptance`(取代 `anyImage: Bool`)

```swift
enum Acceptance {
    case anyImage                                   // 专用目录,来什么收什么
    case screenCapture(names: Set<String>)          // macOS 打了标记 或 文件名属于某个截图 app
    case named(Set<String>)                         // 只收这些前缀的文件
}
```

Desktop 的默认规则从「只认 xattr」变成「认 xattr **或** 启用的 app 的文件名前缀」;其余目录维持 `.anyImage`(上游行为不变)。

### 3. `AppDelegate`

`captureWatchers` 一组 watcher + 菜单里的「Capture apps / 截图应用」子菜单(每个 app 一个勾选项)。

**一个目录只 watch 一次**:`CaptureApps.watchPlan()` 把所有启用的 app 的名字按目录合并成一个集合,`startWatcher()` 逐目录建 watcher。之前是「每个 app 各开一个 watcher」,第二个 app 的目录会被 `watched` 集合挡掉 ⇒ 它的名字就丢了。

**顺带修一个真漏洞**:`watched` 集合一直假设「桌面有人在 watch」,但只有 inbox 模式(安全 watcher 在桌面)或 macOS 截图目录恰好是桌面时才成立。macOS 截图存到别处 + inbox 关 ⇒ 桌面上的第三方截图**永远挂不上**。改成只把**真的在 watch** 的目录放进去:`watcher.folder` + (inbox 模式时的 Desktop) + 用户手选的目录。红测试确认这个漏洞是真的(见下)。



## 三、验证

- `scripts/check-capture-apps.swift`(新增,因 **XCTest 不在 Command Line Tools 里**,自检只能写普通脚本):`swiftc -parse-as-library Sources/Tendedero/CaptureApps.swift scripts/check-capture-apps.swift -o /tmp/x && /tmp/x` ⇒ `capture apps: ok`。
- **变异测试证明脚本能红**:7 个变异全 KILLED。中途有 1 个 SURVIVED —— 「配置里的文件名前缀被忽略」,查下来是**测试用例自己的锅**:样本里 app 名叫 Snipaste、配置前缀也是 `Snipaste_`,和通用前缀重名 ⇒ 删掉那行结果不变。换成 `snap_` 这种不同的前缀后,KILLED。
- `swift scripts/check-strings.swift` ⇒ `67 strings, checked in az, en, es, fr, gl, hi, ja, ko, pt-BR, tr, zh-Hans`,无 missing / 无 unused。新增 2 个 key 已进 11 种语言。
- **端到端冒烟**(装新 app → 启动 → 投放 5 个文件):正例桌面 / `~/Pictures` / `~/Downloads` 三个 `Snipaste_smoke.jpg` **全部挂上**,反例 `ordinary_photo.jpg`(非前缀)与 `Kaplan.jpg`(前缀只到 `Kap`,没有分隔符)**都没挂**。判定看 app 自己的 `pegged` 状态(不用肉眼看屏幕)。
- **漏洞的红/绿测试**:把 macOS 的截图目录临时指到 `~/Pictures/Screenshots`(测试后 `defaults delete` 还原)+ inbox 关 ⇒ 桌面上的 `Snipaste_desktop.jpg`:**旧规则挂不上(red)**,新规则**挂上(green)**。
- **菜单可视验证**:`截图应用 >` 子菜单弹出 `✓ Snipaste`。
- 检测探针:本机 Snipaste → prefix `Snipaste ` / `Snipaste_` / `Snipaste-`,folder `~/Desktop` + `~/Pictures` + `~/Downloads`。

## 四、坑

1. **`~/Pictures` 不受 TCC 拦**(实测无 `cannot watch` 日志),不用加 usage description —— 这条如果靠猜会白加一个不存在的 key。但 **Desktop 和 Downloads 受拦**:第一次 watch 这两个目录时 macOS 弹「Tendedero.app 想访问"桌面/下载"文件夹中的文件」,`open()` 会**阻塞等答复**;重建后 ad-hoc 签名变了 ⇒ 每次重建都再问一遍。测试脚本不点掉这个弹窗,watcher 永远起不来 ⇒ 看起来像"功能没生效"。
2. **弹窗按钮坐标会飘**:Desktop 的提示里「允许」在 y≈464pt,Downloads 的在 y≈450pt。点歪 13pt 就点空 ⇒ 一次测试"红"了,其实是没点中按钮(不是代码问题)。解法:x 固定 1080(绝不在「不允许」那一侧,它在 x≈959),y 在 445–495 扫一遍。
3. **构建期间改源码会失败**:`error: input file '…/ScreenshotWatcher.swift' was modified during the build`。第一次构建正是这么挂的。`scripts/build-app.sh release` 要 3–6 分钟(universal 双架构 + WMO)。
4. **菜单只有点击才构建**(`NSMenuDelegate.menuNeedsUpdate`),osascript/System Events 因无辅助权限被拒(-1719)。可行办法:CGEvent 直接点状态栏图标 + 截图。**只能点一次** —— 点两次会开又关,第一次照着做时菜单刚开就被自己关掉了。
5. **剪贴板路径必须开 `clipboardEnabled`**(`defaults write app.tendedero.Tendedero clipboardEnabled -bool true`),否则 Snipaste 的主输出永远不挂。已替用户打开,菜单「挂上复制的图片」可随时关。
6. `CaptureApps.matches` 的参数从 `[String]` 改成 `Set<String>` 后,漏改一处 `Array(prefixes)` 直接编译失败 —— 改完要 `swift build -c release` 过一遍再打包。
7. **判定"挂上了没有"别看屏幕**:`pegged`(UserDefaults)就是 app 自己的挂件清单,读它比肉眼看窗口可靠得多;屏幕上会被别的窗口盖住。

## 五、交付物

- 仓库 `~/Projects/tendedero`,分支 `feat/capture-apps`:
  - `d28b0be` 初版(16 文件,+455/−12)
  - `d5c6ef3` 改成通用规则 + 修 watcher 漏洞(4 文件,+198/−87)
  - 工作树干净,**未推 upstream**。
- 新增 `docs/extensions.md`:写清「加一个新截图 app = 表里加一行」「新一类图片来源 = `Acceptance` 加一个 case + `startWatcher()` 加一个 watcher」,并给出两条自检命令。
- 安装版已覆盖 `/Applications/Tendedero.app`;旧版 1.8.1 备份在 `build/Tendedero-1.8.1-original.app`(`build/` 被 .gitignore,不会进 git)。
- 记忆:`~/memory/projects/tendedero.md`(Obsidian 项目档案)+ auto-memory `tendedero-capture-apps-extension-2026-11-06.md`。

## 六、遗留

- 分支未推;若要长期保存,需在 GitHub 上开 fork 并把 origin 指过去。
- **跨平台**:上游是 macOS-only(`platforms: [.macOS(.v14)]`、零 `#if os()`、AppKit + CFPreferences + xattr),移植 = 重写 UI 与 watcher 层,不是加开关。本次新增的 `CaptureApps.swift` 是纯 Foundation,是这里唯一可搬的部分。
- CleanShot X / Shottr / PixPin / Kap / Monosnap / Lightshot 本机没装,只验证了**规则**通用(目录 + 名字),没验证它们各自的默认目录和命名;真装上后若目录不在 `~/Desktop`/`~/Pictures`/`~/Downloads`,用菜单里的「Also watch folders」补,或给该 app 加一条 `settings` 闭包读它的配置。
- `README.md` / `docs/i18n` 的手势表没动(该表记的是「做了什么」,本次没有新手势)。
