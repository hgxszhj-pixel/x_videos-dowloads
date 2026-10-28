# 2026-10-28 — 可验证演化体系 (soul) + 收编页「题库已有」判定 + 每日自检 cron

> 项目：exam-lookup（`~/x_videos-dowloads/exam-lookup`）+ `~/.hermes` 工具层。**本轮 0 commit**（改动都在工作区与本机工具目录）。

## 一、背景 / 触发

1. 上一轮收尾时 `soul.py` / `check_skill.py` 已建好但只有手动用法 → 用户「继续」→ 把它接成每天自动跑的一次自检。
2. 收编页仍积压 201 条待收编，其中 192 条被判 `unusable`（"只有题干、没有选项"），看起来必须人工补题面。

## 二、做了什么

### A. soul 每日自检 cron（`7db708b9d608`）

- 新脚本 `~/.hermes/scripts/soul_due_daily.py`：读 `soul.sqlite` 里 `support >= THRESHOLD` 的经验 + 跑 `check_skill.py --all` 的不合格项。
- **安静模式**：没东西可说 → 无 stdout → cron 不发消息；同一条经验提醒后 **3 天内不再念**（状态 `~/.hermes/soul/notify.json`）。
- 任务：每天 09:00，`no_agent` + `script=soul_due_daily.py`，`deliver=qqbot:D7928693F01EC750928F5A20C0FB888B`（用户 QQ 私聊）。
- 实测：手动 `action=run` → `last_status=ok`、`last_delivery_error=null`、QQ 实际收到；紧接着再跑一遍无输出（安静态生效）；`notify.json` 写入 `{"3": 1793170939.74}`。

### B. 收编页：「只有题干」先用题库兜住

- `plugins/harvest.py::_bank_lookup(stem, floor=0.86)`：无选项的记录先跑 `solve.lookup`，命中就把 `bank_id / bank_answer / bank_answer_text / bank_src / bank_score` 挂到卡片上。
- 新增 `exists(h)` / `exists_all(limit)` + 路由 `POST /api/harvest/exists`、`POST /api/harvest/exists_all`：只记 `state.done`，**不写库、不改题**。
- `ask.html`：卡片显示「题库已有答案 X / 题号 / 相似度 / 来源」+「已有，从待收编里划掉」；页首批量卡「一次划掉这 N 条」。`stats` 增 `left`（还要人处理的数），徽标从「待收编」改成「待处理」。
- **实测结论：192 条"只有题干"里 175 条题库本来就有答案**（相似度多为 1.0）—— 它们缺的不是题面，是「已存在」这个判断。

| 指标 | 改前 | 改后 |
|---|---|---|
| 待收编（还要人处理） | 201 | **23**（7 条有选项 + 16 条真需补题面） |
| 收编页首屏 | 10.7 s | **2.2 s**（含切 tab） |
| `list_items(20)` 稳态 | 10.7 s（遍历 201 条边查边算） | **1.4 s**（先排序切片再富化） |
| 本地原文依据（17 条） | 7.6 s | **1.4 s**（gram 按 mtime 缓存） |

### C. 三个真坑（都已修在根因处）

1. **`list_items` 全表富化**：原来每条都跑 `_local_evidence`（~0.45 s/条）才切片。改成「先排序切片，只给可见条目做昂贵查询」，再加进程内按题干缓存。
2. **`ask_ctx.file_hits` 每问重算**：每个问题都把全部原件重读、重算 3-gram。改成按 `(mtime, size)` 缓存文本与 gram 集合（`_FILE_CACHE`），7.6 s → 1.4 s。
3. **`esc()` 把整页渲染打挂**：`ask.html` 的 `esc = s => (s || "").replace(...)`，遇到数字（页码 `x.page`）抛 `replace is not a function`，页面空白、只报一行 TypeError。改成 `String(s ?? "")`，一处修好全类。

> 调试法：页面空白但接口正常时，在浏览器里手动 `hvRender()` 并 `try/catch` 取 `e.stack`，比翻服务端日志快得多（堆栈直接指到模板那一行）。

### D. 走自己的门禁改 skill（不直接改）

| 步骤 | 结果 |
|---|---|
| 候选 | `cand-a653a0c039`（exam-lookup-ops 5703 → 6498 字） |
| 验收尺子 | `~/.hermes/cache/scratch/check_harvest_skill.py`：文档写了 `exists_all/_bank_lookup/先切片/String(s ??` **且** `list_items(20)` 稳态 < 2.5 s **且** 能识别「题库已有」 |
| 基线 | 红 `[1, 1]`（缺 4 条文案；门槛 2.5 s 是把「改回全表富化」判红的界线，附理由） |
| 候选 | 绿 `[0, 0]`（2 轮交替顺序） |
| 转正 | `exam-lookup-ops` 1.3.0 → **1.4.0**，备份 `SKILL.md.bak-soul-20261028T153510`，可 `soul.py rollback --cand cand-a653a0c039` |

### E. 回归

`webui selftest` · `python3 -m plugins selftest` · `kb_db.py selftest` · `solve.py search-selftest` · `answer_chain.py --selftest` · `ask_ctx.py --selftest` · `sync_banks.py --check`（无漂移）全过；`check_skill.py --strict` PASS。

## 三、未闭合

- **待收编 23 条**：7 条有选项（点一下就能收编）+ 16 条只有题干（题库与本地语料都没原文，必须找卷面/教材补题面）。
- 收编页 `_local_evidence` 仍是串行（17 条 ≈ 1.4 s）；再快需要并行，但 SQLite 连接跨线程有风险，暂不动，已写成 skill 里的已知天花板。
- `soul_due_daily.py` 只覆盖「够阈值的经验 + skill 尺子」，**没覆盖**作业性质的待办（如上面 23 条）。

## 四、refs

- 代码：`plugins/harvest.py`、`ask_ctx.py`、`webui.py`（启动预热 `_warm_cache`）、`ask.html`
- 工具：`~/.hermes/tools/soul/{soul,check_skill}.py`、`~/.hermes/scripts/soul_due_daily.py`
- 账本：`~/.hermes/soul/soul.sqlite`（5 观察 / 1 够阈值 / 6 候选全转正）、`~/.hermes/soul/notify.json`
- 技能：`exam-lookup-ops` 1.4.0、`soul-evolve` 1.1.0
