# 备考台 · 本地查题 + 刷题 + 官方原文依据

备考用的小站：粘一道题给答案，还给**官方原文依据**；题库能逐题刷、判对错、记错题。
全部跑在本机（127.0.0.1），不上传任何东西，只读本地文件。

---

## 启动（三种方式，选一种）

| 方式 | 怎么做 | 适合 |
|---|---|---|
| **双击**（推荐） | Finder 里双击 `查题.command` | 不敲命令；开一个终端窗口 + 自动开浏览器 |
| 命令行 | `cd exam-lookup && python3 webui.py` | 想看日志 / 指定端口 |
| 指定端口 | `python3 webui.py --port 9000` | 默认端口段被占 |

- 打开地址：`http://127.0.0.1:8765/`（被占自动顺延 **8765→8769**，不会有报错弹窗）
- **停**：关掉那个终端窗口，或在该窗口按 `Ctrl+C`（`查题.command` 的窗口一关，服务就停）
- 这是**前台进程**，不常驻：合上终端/重启电脑就没了，再双击一次即可
- 改了 `ask.html`（样式/文案）**不用重启**，浏览器刷新就行；改了 `webui.py`（后端）要重启

## 环境要求

- macOS + **python3**（零第三方依赖，不需要 `pip install`）。实测**两个解释器都能跑**：
  - 双击 `查题.command` 走登录 shell → `/usr/local/bin/python3`（Python **3.14.7**，SQLite 3.53.4）
  - 终端里 `python3` 可能是 ServBay 的别名 → Python **3.9.22**（SQLite 3.40.0）
  - 两个版本上 `webui.py selftest` / `kb.py selftest` / `solve.py stats` 均通过
- **sqlite3 带 fts5 + trigram**（上面两个版本都带；自检：`python3 kb.py selftest`）
- `pdftotext`（poppler，实测 26.09.0，`/usr/local/bin/pdftotext`）—— 只在**从 PDF 教材抽文本**时用

## 页面能做什么

**查题 tab**
- 粘题干（带不带选项都行），`查询` 或 `Ctrl/⌘ + Enter`
- 上块：题库答案（大字号 + 题型·题号·命中率·来源·选项·解析）
- 下块：**官方原文依据**（材料类型 + 文件名 + 出处路径 + `[高亮]` 摘录）
- `联网兜底` 默认**关**：勾了才走联网检索（慢，且会写 `pending.jsonl`）

**刷题 tab**
- 选库 / 题型（单选·多选·判断）/ 题量（10/20/50/全部）→ `开始 / 重开`
- 逐题作答 → `提交` 立刻判 ✅/❌，给正确项 + 解析；多选题标「全对才算对」，答错还告诉你「漏选 BC / 错选 D」
- `看原文依据`：当场查这道题的官方原文（直连 kb 依据库）
- `跳过`：不会的题先跳过，不计入成绩；`结束并看结果`：本轮正确率 + 错题回顾
- **错题本**：答错的自动记住（存浏览器 localStorage），下次 `只练错题`；结果页可 `导出错题 txt`（含题号题型/全部选项/答案/解析，便于打印）
- 顶栏：`A−` / `A+`（或 `⌘ =` / `⌘ -`）调字号、`◐` 浅色/深色 —— 都记在浏览器里，下次打开还是这个字号
- 快捷键：`1–5` 选选项、`Enter` 提交 / 下一题

## 目录里要动的文件

```
webui.py            后端：/ 页面 /api/ask /api/banks /api/quiz 四个接口（查询逻辑 import solve, kb，不重写）
ask.html            页面本体：查题 + 刷题两个 tab，样式与字号只改这里
查题.command        双击启动（cd 到本目录后 exec python3 webui.py）
kb.py / kb.sqlite3  官方原文依据库：880 块 / 540,635 字；索引可删可重建
solve.py / qb.sqlite3  题库检索：build 从 banks/*.json 重建索引
banks/*.json        题库真源（6 套 / 366 题）；新增题库都落这里
parse_*.py          各套试卷的解析器（原始 txt → banks JSON）
sync_banks.py       把 banks/*.json 同步到 shiroha 刷题端 + 重建 banks-index.json
learned.jsonl / pending.jsonl  自动学习记录 / 待确认队列（联网兜底会写）
tools/shiroha-quiz-web-*/data  shiroha 刷题端的数据目录（sync_banks.py 的输出）
```

## 常用命令

```sh
cd /Users/hgxszhj/x_videos-dowloads/exam-lookup

python3 webui.py selftest        # 查题页自检：命中/依据/取题/路径穿越（失败 exit 1）
python3 kb.py selftest           # 依据库自检：块数/无重复/回环查得回/空格式
python3 kb.py stats              # 依据库规模（按材料类型）
python3 kb.py search 安全带       # 直接查原文：出处 + [高亮]摘录（--kind 官方原件 可限定）
python3 solve.py build           # 从 banks/*.json 重建 qb.sqlite3（索引可随时删掉重建）
python3 solve.py stats           # 题库总量 + 按库分布
python3 sync_banks.py            # 同步到刷题端网页（会打印「库数 / 合计题数」）
```

## 新增一套题库（走通的 5 步）

1. 把试卷原文整份存成 `exam-lookup/<卷名>.txt`（**留档**，日后每个数字都能回原文复算）
2. 写/复用 `parse_<卷名>.py`：切出「题号 / 选项 / 答案」→ 输出 `banks/<库名>.json`；
   末尾用断言把底线（题数、题型分布、题号连续、每题都有答案、单选选项数），对不上就 `exit 1`
3. `python3 solve.py build` —— 查题端立刻能搜到
4. `python3 sync_banks.py` —— 刷题端下拉里出现新库
5. `python3 webui.py selftest` —— 回归自检

参考实现：`parse_comm_safety.py`（通信安全生产 50 题：单选 20 + 判断 30）。

## 常见问题

- **端口被占 / 打不开**：双击方式会自动顺延 8765→8769；都被占就用 `python3 webui.py --port 9000`，然后开 `http://127.0.0.1:9000/`
- **双击和终端里的 python3 不是一个**：Finder 双击走登录 shell → `/usr/local/bin/python3`（3.14.7）；交互终端里可能是 ServBay 别名 → 3.9.22。两边都实测能跑；要统一就在 `查题.command` 里把 `python3` 写成绝对路径
- **「看原文依据」说没找到条文**：这是设计选择 —— 只用 ≥6 字长探针，**宁可缺、不给假依据**。
  实测通信安全 50 题里 14/50 能对到原文，其余是通用安全常识，依据库里没有对应材料（要提覆盖率得往 kb 里补语料，不是调探针）
- **页面字太小**：右上角 `A+`（或 `⌘ +`）调大，会连按钮、下拉、选项一起缩放（全站 rem 由 `--fs` 驱动）
- **错题本不见了**：存在浏览器 localStorage（键名 `examwrong:<库文件名>`），换浏览器 / 清缓存会丢 —— 想留存就结果页 `导出错题 txt`
- **服务会不会一直开着**：不会。现在只是前台进程；要免手动可自行装 launchd（本项目**尚未**做）
- **`kb.sqlite3` / `qb.sqlite3` 能删吗**：能，都是索引 —— 前者 `python3 kb.py build`，后者 `python3 solve.py build` 重建
- **往依据库加材料**：把文件放进 `kb.py` 里 `SOURCES` 覆盖的路径（官方原件 / 教材 / 教材汇编 / 视频课件），再 `python3 kb.py build`

## 已知限制 / 待办

- 依据覆盖率：通信安全卷 14/50（语料问题，见上）；补《安全生产法》《通信建设工程安全生产操作规范》等原文可提升
- 题库 `answerKeys` 来源在每题 `analysis` 里注明（试卷自带 / 模型判定 / 已核对），**并非每套都逐题对了官方原文**
- 天翼云考试练习题-1（296 题）原件不在本机（`~/Downloads` 只有一份「自检报告」，含 44 条可疑题明细）→ 拿到原件再入库
- 服务非常驻（未装 launchd）；`banks/dtzx-safety.json.bak` 是旧备份，可删

---

改动前后请跑 `python3 webui.py selftest` 与 `python3 kb.py selftest`；
详细来龙去脉（三个真坑、决策、验证数据）见仓库根 `HANDOFF.md` 的 2026-09-21 session 段与 `solve-使用说明.txt`。
