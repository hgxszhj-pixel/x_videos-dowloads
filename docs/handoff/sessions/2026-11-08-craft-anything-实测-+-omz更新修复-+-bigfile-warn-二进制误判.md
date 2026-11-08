# 2026-11-08 — Craft Anything 实测（头条文核实）+ oh-my-zsh 更新修复 + big-file-warn 二进制误判

> 来源：用户贴 `toutiao.com/article/7704341664197870131`（《REA之后，Rust又造了个"复刻神器"》）→
> 「look …」→ 复核数字后用户选「直接完整实测：装好用一个小网站跑一遍复刻，报真实耗时和 token 量」。
> 顺手修了两个环境 bug（本机 claude hook / oh-my-zsh 更新）。仓库：`x_videos-dowloads`（本轮无 Rust / 无 exam-lookup 改动）。

## 本轮做完了什么

| # | 项 | 落地物 | 证据 |
|---|----|--------|------|
| 1 | **文章数字复核** | 6 条说法逐条对源（GitHub API + README + Cargo.toml） | Star 15,778 / 工具 321 / `rust-version = "1.97"` |
| 2 | **Craft Anything 端到端实测** | CLI 0.4.0（26.7 MB release 包，不编译）+ 一个真站点复刻 | 74 分钟、39.0 M token、`checks: []` |
| 3 | **修 `~/.claude/hooks/big-file-warn.py`** | 加 4 行 NUL 嗅探跳过二进制 | PNG → exit 0；1500 行文本 → exit 2（原意不变） |
| 4 | **修 oh-my-zsh 更新失败** | 移出嵌套 clone + `git pull --ff-only origin master` | 131 commit，`9112b53f` → `cca3bdfb` |
| 5 | **.zshrc 去重（方案 A）** | plugins 数组删两项，只留 brew 版加载 | 新 shell：autosuggest / highlight 各加载一次 |

## 一、文章数字复核

| 文章说法 | 实测 | 判定 |
|---|---|---|
| 开源几天冲到「近万 Star」 | **15,778 ★**（仓库 2026-10-31 建，6 天） | 属实但**低估 60%** |
| 314 个分析工具 | 二进制 `commands` 实测 **321**；README 写 320（v0.4.0 当天发） | 版本漂移 |
| 需 Rust 1.97+ | `Cargo.toml` `rust-version = "1.97"`，edition 2024 | 属实 |
| 默认净室 / Deep 才读源码 | README 原文一致，Deep 可联动 Ghidra | 属实 |
| Codex 适配器"没在真装验证过" | README 原文 "built, not yet run live" | 属实 |
| 团队是"重写 Photoshop/AutoCAD 那拨人" | ArtCraft：`artcraft` 39k★ / `cadcraft` 7.7k★ / `deckcraft` 4.6k★ | 属实 |
| （文章没写）| **297 open issue / 2,766 fork**，6 天发 3 个版本 | 新冷水 |

## 二、实测：真跑一遍网页复刻

```bash
craft-anything-cli rebuild https://news.ycombinator.com "Recreate this website" \
  --backend "Claude Code" --rounds 1 --unattended --allow-run \
  --projects-dir <scratch>/ca/projects --format jsonl
```

时间线（本机 Intel i7-9750H，claude 走 **MiniMax-M3.1-Flash**）：

| 阶段 | 耗时 | 产出 |
|---|---|---|
| Study | 7 min | 5 路由 × 3 档宽度截图 + 菜单态 + design token（橙色 `#ff6600` / 底 `#f6f6ef`）+ 30 条列表结构 |
| Build | **74 min 未交回合** | `app/dist/{index,news,newest,front,newcomments}.html` + `style.css` + 自写对比工具 `tools/{sbs.py,compare.py,shoot.js}` |
| Check | **0** | `craft.json` → `"checks": []`，`status: "stopped"`（第 75 分钟我被手动 kill） |

token（从 `~/.claude/projects/…/7b86a745….jsonl` 逐条 `usage` 累加，232 条 assistant 消息）：
输入 2,696,813 + 缓存读 35,895,194 + 输出 413,317 = **39,005,324**；折 Sonnet 4.5 价目 ≈ **$25.06**。

成品相似度（用它自带的 `web-pixels-compare --common-area`，不是我目测）：

| 宽度 | layout_similarity | perceptual_similarity | 像素差 |
|---|---|---|---|
| 1440 | **0.9999** | 0.9648 | 7.7% |
| 768 | 1.0000 | 0.9485 | 12.3% |
| 390 | 0.9176 | **0.4098** | 27.9% |

**结论**：1440/768 是真做到了（header/login/30 条点数作者时长评论/More/页脚 Search 逐项在），390 移动端没吃透。
工具本身可用，但**一轮的成本与时长在弱模型上不可控**：零个官方 Check 产出，`--rounds 1` 管不住单回合长度。

## 三、坑 1（真 bug，已修）：`big-file-warn.py` 把 PNG 当文本

净室模式的核心证据是**看截图**，而本机 hook 直接掐断 Read：

```
[big-file-warn] capture/screenshot-1440.png 有 975 行 (277.4 KB),超过 800 行阈值
```

根因：`subprocess.run(["wc","-l",file_path])` —— `wc -l` 数的是**换行符**，二进制里随机字节凑出 975 个。
builder 于是去缩图（jpg 237 KB）/ 再缩（`/tmp/hn2` `/tmp/hn3` 反复），又是 10 分钟白烧。

修法（`~/.claude/hooks/big-file-warn.py`，读前 4 KB 找 NUL）：

```python
with open(file_path, "rb") as fh:
    if b"\x00" in fh.read(4096):
        sys.exit(0)
```

自检（红/绿都在场）：PNG → exit 0；`seq 1 1500` 的文本 → exit 2。
⇒ 阈值语义不变，只是**不再对二进制生效**；图片由 Claude Code 按图像处理，不按文本膨胀。

## 四、坑 2：它自己的记账是坏的

`usage.json` 全程 `input_tokens: 0 / output_tokens: 0 / cost_usd: 0 / priced: false`。
`work_ms` 在涨，但 token 与钱一分没记 ⇒ **想知道花了多少只能自己翻 claude 会话 jsonl**（上面那串数就是这么来的）。
别信它界面上/JSON 里的 cost 字段。

## 五、坑 3：单回合无预算上限

- `--rounds 1` 限制的是「build+check 的轮数」，**不限制 builder 单回合内自我打磨的时长**：74 分钟里它一直在 `sbs.py` 截图比对。
- MiniMax 端点出现过一次 **10 分钟完全无响应**（进程活着，48 分钟只用了 1 分 10 秒 CPU），不是卡死是接口慢。
- 想要可控：换强模型 + 自己加时间闸（`timeout` 包一层），别指望它自己停。

## 六、oh-my-zsh 更新失败（同轮修的）

现象：`omz update` 报「工作区中下列未跟踪的文件将会因为合并操作而被覆盖：plugins/zsh-autosuggestions/*」并中止。

根因两层：
1. `plugins/zsh-autosuggestions` 是一个**独立 clone**（`git://github.com/zsh-users/…`，`a411ef3`，无本地改动），而上游 oh-my-zsh 现在**自带**同名插件（15 个文件）⇒ 未跟踪文件挡住合并。
2. 本仓 `master` **没设上游追踪** ⇒ 裸 `git pull` 直接报 `There is no tracking information`。

修法：

```bash
mv ~/.oh-my-zsh/plugins/zsh-autosuggestions ~/omz-untracked-backup/zsh-autosuggestions-clone-20261108
git -C ~/.oh-my-zsh pull --ff-only origin master   # 131 commit，9112b53f → cca3bdfb
```

验证：`git ls-files plugins/zsh-autosuggestions | wc -l` = 15（由上游提供）；`zsh -ic` 无报错，`_zsh_autosuggest_start` / `_zsh_highlight` 都在。
备份没删，留在 `~/omz-untracked-backup/`，要搬回随时。

## 七、.zshrc 双载去重（用户选 A）

原状：`plugins=(… zsh-autosuggestions … zsh-autosuggestions …)`（**重复两次**），且第 235/236 行又用 brew 的版本 source 一遍
⇒ autosuggest 与 syntax-highlighting 各加载两次，实际生效的是 brew 那份（0.7.1 / 0.8.0）。

按用户选择 A（保留 brew，`brew upgrade` 即更新）：

```zsh
plugins=(
	   git
	   gitignore
	   python
	   autojump
	   fzf
   )
```

验证：新 shell `plugins=5`，两个插件入口函数各在，无报错。

## 诚实边界

- **这一轮没有跑完**：是我在第 75 分钟主动 kill 的（一个 Check 都没产出，再等没有新信息）。相似度数字是拿它自己最后一张 `mine-1440.png` 回算的，**不是**官方 Check 输出。
- 真实单价没测出来：跑在 MiniMax-M3.1-Flash 上（用户本机 claude 配置），$25 是**按 Sonnet 价目折合**的估算，不是账单。
- 相似度只覆盖 Clean room 能看到的（截图/布局/颜色/字体/可见文本），**交互与行为未测**。
- CLI 与产物都在 `~/.hermes/cache/scratch/ca/`，没写任何全局配置；删目录即净。
- oh-my-zsh 那 8 个残留未跟踪 plugin 目录（cargo/gh/go/incr/rustup…）本轮没动 —— 这 131 个 commit 里没冲突，留着不影响以后更新，真撞上了照上面同一招处理。

## 复现

```bash
# 实测
~/.hermes/cache/scratch/ca/craft-anything-cli-0.4.0-macos-universal/craft-anything-cli doctor

# 成品（双击即看）
open ~/.hermes/cache/scratch/ca/projects/news-ycombinator-com/app/dist/index.html

# hook 自检
printf '{"tool_input":{"file_path":"…/screenshot-1440.png"}}' | python3 ~/.claude/hooks/big-file-warn.py; echo $?   # 0
```
