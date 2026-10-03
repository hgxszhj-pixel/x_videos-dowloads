# 2026-09-26 — exam-lookup：代码审计修复（B1–B12）+ 源码纳入 git 基线


### 1. 触发
用户「这项目的代码有什么地方有 bug？」→ 先问范围（仓库根 Rust 下载器 vs `exam-lookup/` Python 备考台），用户选 **exam-lookup**。
交付期短指令链：`这项目的代码有什么地方有 bug?` → 范围=exam-lookup → `按照你建议执行修复工作吧` → `好的`（修 B5–B12）→ `update handoff.md`。
边界：只改 `exam-lookup/`；不动 Rust 侧；不代考/不点开始考试（沿用）；提交只入**源码**，数据/语料/浏览器 profile 一律不入 git。

### 2. 交付物（都在 `exam-lookup/`；本轮起已纳入 git）

| 交付物 | 路径 | 说明 |
|---|---|---|
| 修复：前端 | `ask.html` | B1 重开清 mock 计时器/状态；B2 错题按 `q.bankFile` 落库；B3 `lsArr()` 兜底 `localStorage` 解析；B10 `esc()` 补转义 `"`/`'` |
| 修复：后端 | `webui.py` | B6 `do_GET/do_POST` → `_route_get/_route_post` + 兜底 `try/except` → 500 JSON |
| 修复：检索 | `solve.py` | B4 `ingest` id 改 `sha1(norm(question))` + `merge_banks` 按题干去重；B11 `answerText` isinstance 兜底 |
| 修复：依据库 | `kb.py` | B9 LIKE 兜底转义 `%`/`_`/`\` + `ESCAPE '\'` |
| 修复：工具 | `sync_banks.py` | B7 `BASE=__file__` + 索引 tmp+`os.replace` 原子写 |
| 修复：小项 | `cite_law.py` / `make_collection.py` / `parse_baomi_records.py` | B8 rate 防零除；B12 断言前移；B5 `qid()` 死分支去除 |
| git 基线 | `exam-lookup/.gitignore` | 挡掉 `banks/`/`crawl/`/`jr/out/`/`jr/profile/`/`*.sqlite3`/HTML·JSON dump；提交 `6eca77a`（80 文件 / +10,568 行） |

### 3. 验证（真跑的）
- `python3 -m py_compile <7 个改过的 py>` OK；`node --check`（抽取 `ask.html` 内联脚本）OK
- `kb.py selftest` OK（5494 块 / 5/5 回环零误报）；`webui.py selftest` OK（模拟考池 365 题 / 题型过滤 / 导入切题 / 路径穿越）
- **B4 跨进程实证**：改前两次独立进程 `284071136938` / `755604882297`（漂移）→ 改后两次均 `auto-97e448369b6a`
- **B6 HTTP 冒烟**（selftest 不覆盖路由层）：`GET /`→200、`/api/banks`→200、`/api/ask`→200、POST 坏 JSON→400、**handler 抛异常→`500 {"error":"RuntimeError: boom"}`**
- **B9 功能测试**：`kb.search('%')`/`('_')`/`('a\b')` 均不报错（8/8/0 条）
- **B3/B10 静态核对**：`grep` 确认 `lsArr` 定义在使用之前、`esc` 新正则落盘

### 4. 真坑（都有实证）
1. **`hash()` 每进程随机加盐**：`solve.ingest` 的 id 想「同题同 id」，但 CPython 字符串 hash 默认随机化 → 两次 `python3 solve.py ingest` 给出不同 id（实测），`merge_banks` 按 id 去重因此失效。改用 `hashlib.sha1`（`PYTHONHASHSEED=0` 才稳定只是巧合，不能依赖）。
2. **`start()` 没清模拟考状态**：`startMock` 里 `startTimer()` 会 `clearInterval`，但普通 `start()` 不会 —— 模拟考中途点「开始/重开」（确认框点继续）后，新轮继承 `S.mock=true`（不给答案）+ 旧倒计时仍在跑，到点 `finishMock("时间到")` 打断新轮。修在**确认之后**复位（取消不能误伤）。
3. **错题跨库归属**：模拟考混抽带 `bankFile`，「立即重做这批错题」把 `S.cur` 设成第一题的库，`submit()` 再按 `S.cur` 落盘 → 其余库的错题进错本。修在 `submit()` 按每题 `q.bankFile`。
4. **`localStorage` 裸 `JSON.parse`**：`wrongBook`/`cardWrong` 无 `try/catch`，一条被截断的值就让 `showBookInfo()`→`showAllStat()` 整链抛错、刷题/背题页不可用。
5. **委派基础设施本轮不可用**：`task(category=deep)` 报 `Model not found: gpt-5.6-sol`、回退 `deepseek-v4-flash` 也不存在 → 3 个后台审计 agent 全失败，改为全程手读（本机 agent category 的模型映射是坏的，下次别再盲发委派）。
6. **门禁反复拦「每文件首次编辑」**：GateGuard 会拦下每个文件的第一次 Edit/Write（要求先陈述 facts），本轮多次「同批编辑里部分被拦、部分落盘」→ 一度差点让 `solve.py`/`ask.html` 引用未定义的 `hashlib`/`lsArr`；按门禁要求补齐 facts 后重放并 `grep` 复核全部标记。

### 5. 决策（细节见 D-070~D-073）
- **git 提交范围**：`exam-lookup/` **源码入 git，数据/语料/索引/浏览器 profile 出 git**（`.gitignore`）；理由：`jr/profile/` 是 424MB 浏览器 profile（可能含登录态），`jr/out/` 有大量教材视频，索引可重建。
- **id 可复现**：`solve.ingest` 的 id 一律用 `hashlib.sha1(norm(question))`，禁用 Python `hash()`。
- **前端兜底**：`localStorage` 解析一律 `try/catch`；模拟考状态/计时器在 `start()` 确认重开后复位。
- **检索转义**：`kb.search` 的 LIKE 兜底对 `%`/`_`/`\` 转义 + `ESCAPE`（"宁可缺、不给假依据"的延伸）。

### 6. 已知 follow-up
- **`banks/*.json` 未入 git**：clone 出来直接跑 `webui.py selftest` 会因空库失败（README 说它是「真源」）—— 待定是否随仓库分发（数据 vs 代码的取舍）。
- **`tools/` 未入 git**：第三方刷题端（含 `*.zip`），可作为 vendor 处理。
- **未跑的写盘自检**：`parse_baomi_records.py selftest`（会重写 `banks/baomi-*.json`）、`python3 solve.py merge`（会创建 `banks/auto-learned.json`）—— 有副作用，未获授权未跑。
- **B12 余项未改**（理论/无害）：`kb_coverage` 空题干题计入分母、`ask.html` 少数数值字段未 `esc`、`sync_banks` 陈旧库保留逻辑。
- **Rust 侧未碰**：本轮仅在 `exam-lookup/`；HANDOFF 的 Rust 记事项（G1/G2、trace.rs 等）不受影响。

---

