# 数据格式与更新机制（exam-lookup 资料库）

2026-10-26 落地。宗旨：**一个库当真相，文件只做导出口。**

## 一、三层结构

    原件层   jr/out/docs_new/              附件保持原格式（pdf/docx/pptx/xlsx/zip…），按 attachmentId 命名，不转码不改名
    主库层   jr/out/docs.sqlite            清单 + chunk 文本 + bigram 倒排，单一事务源；PRAGMA user_version 记 schema 版本
    导出门   jr/out/docs_export.json       机器读快照（带 schema_version）
             jr/out/docs_new_text/*.txt    归一化纯文本（人读、正则 grep、给别的脚本用）
             exam-lookup-rs/docs/platform-training-docs.md/.json   项目内清单（人读 + Rust 侧）

## 二、为什么换掉 pickle + 目录

同一份语料（177 篇去重后、2,772 块、194 万字）实测：

| 方案 | 体积 | 冷启动 | 查询 p50 | p90 | 增量更新 |
|---|---|---|---|---|---|
| pickle 倒排（旧） | 12.6 MB | 531 ms | 2.3 ms | 5.3 ms | 必须整库重建（6.6 s 起） |
| **SQLite（现）** | **13.6 MB** | **6 ms** | **1.5 ms** | 3.5–6.6 ms | **32 ms / 篇** |

触发换的直接证据：文件目录会漂。迁移前 `docs_new_text/` 有 235 个 txt，`docs_state.json` 只认 177 条——多出的 58 个是旧命名（`x.pdf.txt` 与 `x.pdf.pdf.txt`）留下的重复文本，没人在册，却照样进了索引。目录当数据库用，这种漂移无从发现；SQLite 靠 `att` 主键 + 单事务，重复与半更新不可能存在。

## 三、表结构（schema_version = 2）

    documents(id INTEGER PK, att TEXT UNIQUE, name, course, course_id, domain, ext,
              size, url, sha1, chars, synced_at, gone_at)
        ↑ 清单与同步状态合一：att 唯一，gone_at 非空表示平台已下架
    chunks(id INTEGER PK, doc_id, seq, text)
        ↑ 每 700 字一块，不重叠
    postings(bigram TEXT PK, chunk_ids BLOB)
        ↑ bigram → 命中的 chunk id 数组（int32），一次查询取回所有命中
    questions(bank, qid, number, type, question, options, answer_keys, answer_text,
              analysis, topic, category, volume, page, extra, sha1, updated_at)
        ↑ 题库的唯一落地点（v2 新增）：bank = 文件名（与 answers.bank 同一口径，便于 join）
          options/answer_keys 存 JSON 原文；extra 存其余字段（myWrong/examWhen/law/confidence…）
          原样 JSON，加字段不用改表；sha1 = 题面+选项+答案 的指纹，幂等判据
    answers(bank, qid, …)  web_evidence(bank, qid, url, …)
        ↑ 核对结论与网页证据（由 kb_answer_resolve / kb_mark_verified 写入）

不变式（违反任一条就是 bug）：

1. `documents` 条数 = 平台清单条数（当前 187 = 177 有正文 + 10 在册无正文）
2. `sha1` 一律以磁盘文件为准，state 里的值不可信（迁移中实测到一次坏值，已由 sync 自愈）
3. 抽不出文本的（扫描 PDF、图片、压缩包）仍进 `documents`，`chars=0`，不是 gone
4. 同一个 `att` 只允许一份正文；重复文本移入 `jr/out/docs_new_text_orphans/` 并记日志
5. `questions` 里 (bank,qid) 全局唯一；重复导入同一份 json 不产生新行（sha1 相同即跳过）
6. `answers` 的每一行都应能在 `questions` 找到同 (bank,qid) 的题——对不上说明题库改过名或删过题，
   `import-banks` 会打出 `answers 里有 N 行对不上 questions` 提示
7. `banks/*.json` 必须带 `schema_version: 1` 并通过 `validate-banks`（契约文件 `schema/bank.schema.json`）；
   不合格的题库 `import-banks` 直接拒绝，一行都不写
8. 取数方向单一：`kb_review_queue.py` 等消费者从 `questions` 表读题，`banks/*.json` 只是导入格式
   （`schema/bank.schema.json` 的字段清单与 `kb_db.REQUIRED_Q` 由 selftest 断言锁在一起，改一边不改另一边会红）

## 四、命令

    建库/重建          python3 kb_db.py migrate [--dry-run]
    题库导入            python3 kb_db.py import-banks [文件…] [--prune]
                       ↑ 省略文件=导入 banks/ 全量；--prune 才删「json 里已不存在」的题（默认只报告）
    题库格式契约        python3 kb_db.py validate-banks      # 合格打印一行；不合格逐题指名并 exit 1
    题库一致性哨兵      python3 kb_db.py check-banks        # 一致时无输出、exit 0；砖了才打印
    看库                python3 kb_db.py stats
    评估               python3 kb_db.py eval [--bank banks/xxx.json] [--cite-out x.json]
    性能               python3 kb_db.py bench
    红绿自检            python3 kb_db.py selftest            # 含导入幂等/指纹/删题断言
    导出快照            python3 kb_db.py export [--out path]
    网页端同步          python3 sync_banks.py [--check]

    平台同步            python3 kb_docs_sync.py sync [--offline] [--legacy-pickle]
    只看差量            python3 kb_docs_sync.py diff
    看状态              python3 kb_docs_sync.py status

`sync` 的三类差量：新增（新 attachmentId）、变更（size 或 url 变）、**状态漂移**（state 与主库 sha1 不一致）。第三类是这套结构的自愈能力：手改文件或上次写到一半，下一次 sync 会检出并把 sha1 纠正回磁盘真相。

定时（平台更新不快，每周一次够）：

    0 7 * * 1 cd ~/x_videos-dowloads/exam-lookup && python3 kb_docs_sync.py sync >> jr/out/docs_sync.log 2>&1

## 五、验收判据（本次实测结果）

| 判据 | 目标 | 实测 |
|---|---|---|
| 四选一答题准确率不退化 | 与 pickle 同语料一致 | 双方均 43/62 = 69% ✅ |
| 冷启动 | < 50 ms | 6 ms ✅ |
| 增量 1 篇 | < 100 ms | 32 ms ✅ |
| 写入/查询/替换/互不影响 | selftest 四项断言 | PASS ✅ |
| 漂移检出与自愈 | 检出 1 → 处理后 0 | 漂移 1 → 变动 0 → 复跑 0 ✅ |

评估口径（与 `kb_docs_index.py` 完全一致，脚本内可对照）：

- ① 定位：top-1 段落与题干内容词共享 ≥3 个 bigram — 当前 58/62 = 94%
- ② 锚定：top-5 段落里出现正确答案的 ≥4 字连续片段 — 当前 28/62 = 45%
- ③ 答题：top-5 段落给各选项 idf 加权打分取 max — 当前 43/62 = 69%

## 六、已知天花板

bigram 只做字面匹配：同义改写（选项措辞与教材说法不同）、跨词边界误配、多选被当成单选取 max —— 这三类是剩余 19 道错题的全部来源。要往上抬得加同义词表或做短语级匹配，属于下一步工作，不是格式问题。

`kb_docs_index.py` 保留为对照基线（`sync --legacy-pickle` 会顺带重建 `docs_index.pkl`），日常查询走 `kb_db.py`。
