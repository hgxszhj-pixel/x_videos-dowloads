#!/usr/bin/env python3
"""bench_retrieval —— 同一份语料上比三种检索：现有 bigram 倒排 / FTS5 trigram / LIKE 兜底。

只读主库：FTS5 索引建在临时副本上，不动 jr/out/docs.sqlite。
跑法：python3 bench_retrieval.py [题库json] [题数]
"""
import json
import os
import shutil
import sqlite3
import statistics
import sys
import tempfile
import time

PROJ = os.path.expanduser("~/x_videos-dowloads/exam-lookup")
sys.path.insert(0, PROJ)
import kb_db as KB                    # noqa: E402
import kb_answer_resolve as R         # noqa: E402


def ms(t0):
    return (time.perf_counter() - t0) * 1000


def main():
    bank = sys.argv[1] if len(sys.argv) > 1 else os.path.join(PROJ, "banks/dtzx-safety.json")
    n = int(sys.argv[2]) if len(sys.argv) > 2 else 40
    qs = [q["question"] for q in R.bank_questions(bank) if q.get("question")][:n]

    db = KB.connect(readonly=True)
    nchunks = db.execute("SELECT COUNT(*) FROM chunks").fetchone()[0]
    ndocs = db.execute("SELECT COUNT(*) FROM documents").fetchone()[0]
    size = os.path.getsize(KB.DB_PATH) / 1048576
    print("语料：%d 份文档 / %d 块 / %.1f MB，题 %d 道（%s）"
          % (ndocs, nchunks, size, len(qs), os.path.basename(bank)))

    # 1) 现状：CJK bigram 倒排
    t = []
    hit = {"bigram": [], "fts": [], "like": []}
    for q in qs:
        t0 = time.perf_counter()
        hits = KB.search_with_cite(db, q, topk=3)
        t.append(ms(t0))
        hit["bigram"].append(tuple(h["doc"] for h in hits))
    cold = []
    for _ in range(5):                       # 冷启动：断开重连（WAL 已在 page cache 里，算乐观值）
        t0 = time.perf_counter()
        d2 = KB.connect(readonly=True)
        d2.execute("SELECT COUNT(*) FROM chunks").fetchone()
        cold.append(ms(t0))
        d2.close()
    print("  现行 bigram 倒排  中位 %.2f ms  p90 %.2f ms  冷启动 %.2f ms"
          % (statistics.median(t), sorted(t)[int(len(t) * 0.9) - 1], statistics.median(cold)))

    # 2) FTS5 + trigram（临时副本上建，不碰主库）
    tmp = os.path.join(tempfile.gettempdir(), "bench_retrieval.sqlite")
    shutil.copy2(KB.DB_PATH, tmp)
    tdb = sqlite3.connect(tmp)
    try:
        tdb.execute("""CREATE VIRTUAL TABLE IF NOT EXISTS chunks_fts USING fts5(
                         text, content='chunks', content_rowid='id', tokenize='trigram')""")
        tdb.execute("INSERT INTO chunks_fts(chunks_fts) VALUES('rebuild')")
        tdb.commit()
    except sqlite3.OperationalError as e:
        print("  FTS5 trigram 不可用（%s），跳过这一项" % e)
        tdb.close()
        os.path.exists(tmp) and os.remove(tmp)
        return 0
    build = os.path.getsize(tmp) / 1048576 - size
    t2 = []
    for q in qs:
        t0 = time.perf_counter()
        # FTS5 只能当"召回过滤器"用：把题干里最稀有的几个 bigram 用 OR 连起来
        qg = KB.cjk_bigrams(KB.query_terms(q))
        rare = sorted(qg, key=lambda g: R.df_of(db, g))[:6]
        expr = " OR ".join('"%s"' % g for g in rare)
        rows = tdb.execute("""SELECT d.name FROM chunks_fts f JOIN chunks c ON c.id=f.rowid
                              JOIN documents d ON d.id=c.doc_id
                              WHERE chunks_fts MATCH ? ORDER BY rank LIMIT 3""", (expr,)).fetchall()
        t2.append(ms(t0))
        hit["fts"].append(tuple(r[0] for r in rows))
    print("  FTS5 trigram      中位 %.2f ms  p90 %.2f ms  索引额外占 %.1f MB"
          % (statistics.median(t2), sorted(t2)[int(len(t2) * 0.9) - 1], build))
    tdb.close()
    os.path.exists(tmp) and os.remove(tmp)

    # 3) 最朴素：LIKE 全表扫（只在题少时可行，用来做万级以下的对照）
    t3 = []
    for q in qs:
        key = q[:12]
        t0 = time.perf_counter()
        db.execute("SELECT id FROM chunks WHERE text LIKE ? LIMIT 3", ("%" + key + "%",)).fetchall()
        t3.append(ms(t0))
    print("  LIKE 全表扫      中位 %.2f ms  p90 %.2f ms（且只能前缀匹配，召回差）"
          % (statistics.median(t3), sorted(t3)[int(len(t3) * 0.9) - 1]))

    same = sum(1 for a, b in zip(hit["bigram"], hit["fts"]) if a and b and set(a) & set(b))
    print("\n两路召回 top3 有交集的题：%d/%d（%.0f%%）" % (same, len(qs), 100 * same / len(qs)))
    db.close()
    return 0


if __name__ == "__main__":
    sys.exit(main())
