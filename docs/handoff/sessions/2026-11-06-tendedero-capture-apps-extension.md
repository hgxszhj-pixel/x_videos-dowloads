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

`CaptureApp { id, name, folders, prefixes }` + `CaptureApps.known` 一张表,每行一个 app:`id / name / appNames / folders / prefixes / settings`。

- `settings` 闭包读 app 自己的配置。Snipaste 的 `~/.snipaste/config.ini` 用 `parseIni` 解析(跳过 `[Section]` 头与 `;`/`#` 注释),取出 `last_save_folder` / `quick_save_path` / `file_name_pattern`。
- `patternPrefix` 取文件名模板里**第一个 `$` 之前的固定部分**:`Snipaste_$yyyy-MM-dd_HH-mm-ss$.jpg` → `Snipaste_`。
- 目录 = 配置里的 `last_save_folder` + `quick_save_path` 的 dirname;前缀 = 模板前缀 ∪ `quick_save_path` 文件名前缀。
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

`captureWatchers` 一组 watcher + 菜单里的「Capture apps / 截图应用」子菜单(每个 app 一个勾选项)。已经在 watch 的目录(截图目录 / Desktop / 用户手选的目录)不再重复挂 watcher。

## 三、验证

- `scripts/check-capture-apps.swift`(新增,因 **XCTest 不在 Command Line Tools 里**,自检只能写普通脚本):`swiftc -parse-as-library Sources/Tendedero/CaptureApps.swift scripts/check-capture-apps.swift -o /tmp/x && /tmp/x` ⇒ `capture apps: ok`。
- **变异测试证明脚本能红**:6 个变异,5 KILLED;唯一 SURVIVED 的是「ini 解析保留 section 头」—— 查证为**等价变异**(section 头没有 `=`,已被 `firstIndex(of: "=")` 那条守卫挡掉,行为不变),不是测试漏洞。补测后又加了「value 去空格」用例(原样本 ini 全是 `key=value` 无空格,trim 根本没被覆盖 ⇒ 补上后该变异 KILLED)。
- `swift scripts/check-strings.swift` ⇒ `67 strings, checked in az, en, es, fr, gl, hi, ja, ko, pt-BR, tr, zh-Hans`,无 missing / 无 unused。新增 2 个 key 已进 11 种语言。
- **端到端冒烟**(装新 app → 启动 → 投放):桌面 `Snipaste_smoke.jpg`、`~/Pictures/Snipaste_smoke.jpg`、剪贴板一张图 ⇒ **屏幕上同时出现 3 个挂件**;app 日志出现 `Screenshot landed on the Desktop despite inbox mode: Snipaste_smoke.jpg`(说明走的是 capture-app 前缀这条路,不是 xattr)。
- **菜单可视验证**:`截图应用 >` 子菜单弹出 `✓ Snipaste`。
- 检测探针:本机 Snipaste → prefix `Snipaste_`,folder `~/Pictures` + `~/Desktop`。

## 四、坑

1. **`~/Pictures` 不受 TCC 拦**(实测无 `cannot watch` 日志),不用加 usage description —— 这条如果靠猜会白加一个不存在的 key。
2. **构建期间改源码会失败**:`error: input file '…/ScreenshotWatcher.swift' was modified during the build`。第一次构建正是这么挂的。`scripts/build-app.sh release` 要 3–6 分钟(universal 双架构 + WMO)。
3. **菜单只有点击才构建**(`NSMenuDelegate.menuNeedsUpdate`),osascript/System Events 因无辅助权限被拒(-1719)。可行办法:CGEvent 直接点状态栏图标 + 截图。**只能点一次** —— 点两次会开又关,第一次照着做时菜单刚开就被自己关掉了。
4. **剪贴板路径必须开 `clipboardEnabled`**(`defaults write app.tendedero.Tendedero clipboardEnabled -bool true`),否则 Snipaste 的主输出永远不挂。已替用户打开,菜单「挂上复制的图片」可随时关。
5. `CaptureApps.matches` 的参数从 `[String]` 改成 `Set<String>` 后,漏改一处 `Array(prefixes)` 直接编译失败 —— 改完要 `swift build -c release` 过一遍再打包。

## 五、交付物

- 仓库 `~/Projects/tendedero`,分支 `feat/capture-apps`,提交 `d28b0be`(16 文件,+455/−12),工作树干净,**未推 upstream**。
- 新增 `docs/extensions.md`:写清「加一个新截图 app = 表里加一行」「新一类图片来源 = `Acceptance` 加一个 case + `startWatcher()` 加一个 watcher」,并给出两条自检命令。
- 安装版已覆盖 `/Applications/Tendedero.app`;旧版 1.8.1 备份在 `build/Tendedero-1.8.1-original.app`(`build/` 被 .gitignore,不会进 git)。
- 记忆:`~/memory/projects/tendedero.md`(Obsidian 项目档案)+ auto-memory `tendedero-capture-apps-extension-2026-11-06.md`。

## 六、遗留

- 分支未推;若要长期保存,需在 GitHub 上开 fork 并把 origin 指过去。
- `CaptureApps.known` 里 CleanShot X / Shottr / PixPin / Kap 的**默认目录是猜的**(`~/Pictures/<App>`),只有 Snipaste 读了真实配置。这几个 app 本机没装,无法端到端验证;装上后若目录不对,补一条 `settings` 闭包读它们的配置即可。
- `README.md` / `docs/i18n` 的手势表没动(该表记的是「做了什么」,本次没有新手势)。
