# 2026-10-29 — 内网共用 + 三个「看起来是数据坏了」的前端故障 + 免费资源工具

> 项目：exam-lookup（`~/x_videos-dowloads/exam-lookup`）+ `~/.hermes/tools/ffd`。**本轮 0 commit**。

## 一、局域网共用（`--lan`）

- `python3 webui.py --lan` → 绑 `0.0.0.0`，启动时打印本机内网地址（实测 `http://192.168.1.3:8765/`）；不加参数仍只绑 `127.0.0.1`。
- **准入闸写在服务端**：`webui.peer_allowed(ip)` 只放行本机与私有网段（`10./172.16–31./192.168./169.254./fe80::`），`GET`/`POST` 都先过；公网地址一律 403。理由：库里是 1768 道带答案带出处的题，路由器做一次端口映射就等于挂到公网。
- 自检：7 条应放行 + 5 条应拒绝（含 `8.8.8.8`、`172.32.0.1`、`2001:db8::1`）。
- 实测：内网地址首页 6 tab 正常、`/api/ask` 返回 `answers=1 C(30) 依据 6 条`、`/api/banks` 200；本机防火墙 disabled 无需额外放行。

## 二、三个「数据看着坏了，其实是前端」的故障

1. **刷题点「开始」没反应**（用户：「选择了题库，不能显示题库的题目」）
   - 根因：`ask.html:391 function isOrd(q){ ... || isOrd(q) }` **自我递归**（排序题改成 `multiple+ordered` 时的笔误），一点就 `Maximum call stack size exceeded`；`/api/quiz` 返回 133 题全正常、`webui/plugins selftest` 全绿 —— 坏的是渲染。
   - 修：`q.ordered === true || q.type === "ordering"`。ask.html 每次请求现读，改完即生效。
   - 守卫：`webui.py selftest` 读 ask.html 断言「isOrd 体内不出现自调用」「认得出 ordering」+ 4 条语义断言。
2. **复习页只显示「(题干缺失)」**
   - 根因：`srs.json` 的卡只有 `{due,reps,lapses}`，题面是每次请求再去 `questions` 表补；补不到就只有空壳。
   - 修：`due()` 给这类卡打 `missing:true`；`stats()` 增 `orphans`；新增 `POST /api/srs/prune`（只删查不到原题的卡）；卡片上给「跳过这条」「清掉全部失效卡」，徽标标红「失效卡 N」。
   - 自检：注入一张不存在的卡 → `missing=True`、`orphans>=1`；`prune()` 清干净；**真卡一张不许误删**（用库里真实 bank+qid 做对照）。
3. **复习只有保密库有题**（用户：「其他类型的题库无法加载」）
   - 真相：复习只显示**被装载过**的题 —— 之前只有 `baomi-2024 / baomi-2026 / baomi-我的错题` 装了 50 张，其余 14 个库 0 张，选中必然「今天没有到期题」。
   - 修：`stats()` 增 `by_bank`（每库卡片数/到期数）；下拉标「（47 题 · 排期 30 / 到期 27）」或「（321 题 · 未装载）」、有排期的排前面；按钮拆成**装载本库错题** / **装载本库 50 题**；空队列文案说清「没排期卡就永远不会有到期题」。
   - 实测：选「教材原题·线路安全」→ 装载本库 50 题 → 队列 50→58、第 1/8 道正常出题。之后各库陆续装到 **382 张 / 到期 368 张 / 失效 0**。

## 三、背题：5 个 → 22 个

- 原来只吃 `crawl/*问答*.json`（5 份），选择题全在刷题页；用户问「背题怎么只有 5 个」。
- 改：`card_books()` 追加 17 个题库（`bank:` 前缀，标题标「（题库 · 看题看答案）」）；`cards()` 对 `bank:` 前缀生成「题干 + 全部选项逐行 + 答案字母与选项原文」，只背不判分。
- 两个显示修复：答案行曾渲染成 Python 列表串 `['智惠共生、云网智融']` → 按 answerKeys 匹配选项原文拼；题干换行被折叠 → `white-space:pre-wrap`。
- selftest 跟着升级（原断言要求每份材料都有章节，题库没有章节 → 会误判）：问答材料必须有章节，题库背书必须「题面带选项行 + 答案不许出现 `[`」。

## 四、免费资源工具（`~/.hermes/tools/ffd`）

- 起因：头条微头条提到 `ripienaar/free-for-dev`（GitHub API 实测 **140,562★**，README 1,775 行 / 1,365 条 / 57 分类）。
- 落地：`ffd.py` → `ffd.sqlite`（1,365 条），CLI 支持 `--refresh / --if-changed / --html / --stats / --cats / --selftest`；抓取走 `curl -sk`（本机 https 链有自签证书，urllib 会 SSL 失败）。
- 网页：`--html` 生成单文件 `~/free-for-dev.html`（318 KB，数据内嵌、浏览器端搜索）；**放弃常驻 `--serve`** —— 后台包装器起长驻 python 时三次只拉起 zsh 壳、端口无监听，静态单文件反而零守护、离线可用。
- 每周一 08:00 cron `50324646b62c`（`ffd_refresh_weekly.py`）：README 没变**零输出**，变了报增删并重写网页。实测一次 `action=run` → `Status: silent (empty output)`。
- exam-lookup 第六个 tab「免费资源」= `webui.py` 的 `/ffd.html` 路由（读磁盘静态页，缺文件给一行提示不 500）+ `ask.html` 的 `pane-ffd` iframe。

## 五、技能与工具链

| 项 | 结果 |
|---|---|
| `exam-lookup-ops` | 1.3.0 → 1.5.x：新增「局域网共用（-lan）」与「复习页 (题干缺失) = 排期卡接不回原题」两节（都走 soul 门禁，基线红/候选绿） |
| `local-data-browser-ui` | 1.1.0 → 1.2.0：新增「改完前端把每个 tab 都点一遍」（isOrd 事故教训）与 esc/pre-wrap 等坑 |
| `soul` 每日自检 cron | `7db708b9d608` 每天 09:00，安静模式 + 3 天去重，已实测投递 QQ |
| 门禁尺子 | `check_lan_skill.py` / `check_frontend_skill.py` / `check_orphan.py`，都在 `~/.hermes/cache/scratch/` |

- **发现并发写风险**：`exam-lookup-ops` 的 version 被推到 1.5.1、`local-data-browser-ui` 到 1.2.0，而 soul/check_skill 都不改 version 字段 —— 是本机另一个会话在改同一批技能文件。soul 的 base_sha 校验能挡住「基线已变」，但挡不住两个会话各自读→各自写。未决定加锁还是约定单会话。

## 六、未闭合

- 复习队列 382 张 / 到期 368 张：一天复习不完，已建议按库 + 10 道分批。
- 待收编 8 条（其中 16 条缺题面的记录已并入 `jr/out/缺题面清单-20261028.md`）。
- 技能目录并发写仍无机制（见上）。
- `ffd --serve` 常驻模式在本机后台包装器里不可用，只有静态页可靠。

## 七、refs

- 代码：`webui.py`（`--lan`、`peer_allowed`、`/ffd.html`、`card_books`、`cards`）、`plugins/srs.py`（`missing`/`orphans`/`prune`/`by_bank`）、`ask.html`（`isOrd`、`pane-ffd`、复习装载、背题 pre-wrap）
- 工具：`~/.hermes/tools/ffd/ffd.py`、`~/.hermes/scripts/ffd_refresh_weekly.py`、`~/free-for-dev.html`
- 清单：`exam-lookup/jr/out/缺题面清单-20261028.md`、`exam-lookup/make_gap_list.py`
- cron：`50324646b62c` 周刷新（local 静默）、`7db708b9d608` soul 自检（QQ）
