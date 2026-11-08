# 插件系统（含热加载）与借鉴来源

2026-10-28 落地。目标：**新功能以插件形式长在 exam-lookup 上，且任何插件都拖不垮主程序。**

## 一、借鉴了哪些成熟项目

| 项目 | 星标/许可 | 借了什么 | 落点 |
|---|---|---|---|
| [open-spaced-repetition/py-fsrs](https://github.com/open-spaced-repetition/py-fsrs) | 519★ / MIT / 2026-08 仍活跃 | FSRS-6 调度公式与 21 个默认参数（`Scheduler`、`_next_interval`、`_next_recall_stability`、`_next_forget_stability`、`_short_term_stability`、`next_difficulty`） | `plugins/srs.py`（原文照译，不引依赖） |
| [Zafer-Liu/Practice-Learning_Platform](https://github.com/Zafer-Liu/Practice-Learning_Platform) | 34★ / 无 LICENSE | 功能取舍参考：题库练习 / 成绩统计 / 错题本复练；它把「错题复练」做成了独立入口 | 复习队列的入口设计（`enroll` 默认只装错题） |
| [CiE-XinYuChen/EXAM-MASTER](https://github.com/CiE-XinYuChen/EXAM-MASTER) | Flask + SQLite | 功能面参考：登录、练习、考试、统计、CSV 题库导入 | 仅作对照，未取代码（我们已有更严的题库契约） |

为什么 FSRS 值得借：它是 Anki 现在用的算法，比「艾宾浩斯固定间隔」准得多——按你的实际答题表现
（again/hard/good/easy）推 `稳定度 S` 和 `难度 D`，下次到期日 = `next_interval(S)`。手工调间隔的时代结束了。

**正确性证据**：`plugins/srs_golden.json` 里 7 组基准向量，是用参考实现（`fsrs` 包，`learning_steps=()`、
关闭抖动）实跑出来的 `S/D/间隔`。`plugins/srs.py` 必须逐项对上（误差 < 1e-4），`python3 -m plugins selftest`
每次都会验。这是「移植对不对」唯一可信的判据，别用「跑起来没报错」当验证。

## 二、插件契约

`plugins/<name>.py`，模块级一个字典：

```python
PLUGIN = {
    "name": "srs",                                                   # 必须与文件名一致
    "desc": "错题复习调度（FSRS-6 移植；数据在 jr/out/srs.json）",
    "routes": {"/api/srs/due": handler},                             # GET；handler(query: dict) -> 可 JSON 序列化
    "selftest": fn,                                                  # 可选；失败抛 AssertionError
}
```

- 路由必须以 `/api/` 开头（加载器会拒），冲突时先加载者胜出并在 `errors` 留痕
- 状态文件自己管，**不要**去动 `docs.sqlite` 的表结构（插件卸载要能做到「删文件即下线」）
- 文件名以 `_` 开头的不加载（`__main__.py`、临时文件）

## 三、热加载与崩溃隔离（硬性要求）

    改插件文件        → 不用重启 webui：每次请求 stat 一次（1 秒 TTL），mtime/大小变了就重新 exec
    新代码写坏了      → 记住错误，继续用上一次能跑的版本（回滚），路由照常响应
    插件路由抛异常    → 转成 {"error": ...}，HTTP 500，只毁这一次请求
    插件导入失败      → 记进 errors，其它插件与主程序照常
    并发             → 一把 RLock，多线程同时发现变更只重载一次

已实测（2026-10-28，真 HTTP，服务不重启）：

    v1 上线 → curl /api/zz_probe = {"v":"v1"}
    改成 v2 → 不重启，curl 立刻 = {"v":"v2","hot":true}
    写成语法错误 → curl 仍 = v2（回滚生效），/api/plugins 显示 SyntaxError，服务不倒
    删除文件 → /api/plugins 里消失，errors 清空

## 三点五、已装插件：srs（复习）/ harvest（联网收编）

**harvest —— 把「联网找到、人工确认」的题收进题库**（动态扩容的入口）

    GET /api/harvest/list?limit=20        待收编清单（题干 / 选项 / 候选 / 检索摘要）
    GET /api/harvest/commit?h=<hash>&key=C 确认入库
    GET /api/harvest/ignore?h=<hash>      不收这条

边界（这些是硬规矩，不是建议）：

    只有人工点选才写库          搜到的答案永远不自动进题库（solve.ask 的 auto_learn 已默认关）
    落到独立题库                banks/web-learned.json，带 schema_version，过 kb_db.validate_bank
    一次写入四处同时生效        banks 文件 → docs.sqlite 的 questions 表 → qb.sqlite3 查题索引 → 刷题端网页
    只收选择题                  题面切不出 ≥2 个选项的（判断题/简答）直接标 unusable，不写半成品
    幂等                       id = WEB-<题干 sha1 前 12 位>，同题重复确认不会造出两条

实测（2026-10-28，真 HTTP）：清单 197 条待收编 → 点 D 确认「有限空间作业前应通风多长时间？」
→ validate ok / questions 新 1 / qb 已重建 / 刷题端已同步 → 再查该题：本地命中，来源 web-learned.json，相似度 1.0。

## 四、命令

    python3 -m plugins list            # 谁在线、版本时间、有没有报错
    python3 -m plugins selftest        # 插件自检 + 加载器自检（含热加载/回滚演练）
    python3 -m plugins due --limit 10  # 今天该复习哪些（带题干选项答案）
    python3 -m plugins review --bank 自测-安全数字.json --qid GEN-J0001 --rating good
    python3 -m plugins enroll --limit 50          # 只装答错过的题
    python3 -m plugins enroll --limit 50 --all   # 全库都装

HTTP（webui 起来后）：

    GET /api/plugins                         插件状态（在线/报错/重载次数）
    GET /api/srs/due?limit=10&bank=xxx.json  到期卡 + 统计
    GET /api/srs/review?bank=…&qid=…&rating=good|hard|easy|again
    GET /api/srs/stats

## 五、加一个新插件（四步）

1. 新建 `plugins/<name>.py`，写 `PLUGIN` 字典 + 处理函数（自己做 try/except，返回值必须可 JSON 序列化）
2. 跑 `python3 -m plugins list` 确认它在线
3. 写 `selftest`（断言要能变红：故意造一个坏输入，确认被拦住）
4. 跑 `python3 -m plugins selftest` 与 `python3 webui.py selftest`，两条都过再交付

## 六、已知天花板

- 只支持 GET 路由（webui 的 POST 面还没挂），写操作走 query 参数
- 插件跑在同一进程里：插件里写死循环，主程序一样会卡住（隔离的是异常，不是 CPU）
- `srs.json` 单文件、无锁并发写：单人本地使用够，多人并发需要换 SQLite 表 + 事务
- FSRS 未做分钟级 learning steps（考前复习按天排），要这个粒度得补 `step` 逻辑
