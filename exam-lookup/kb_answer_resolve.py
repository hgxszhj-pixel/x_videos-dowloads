#!/usr/bin/env python3
"""kb_answer_resolve —— 答案动态求解器：每次语料更新后重算答案，与存量答案对账。

为什么需要：题库里的 answerKeys 是「某次存下来的」，可能来自试卷自带、也可能来自我早前的推导；
语料更新了（或当初就抄错）它不会自己变。这个脚本把「答案」变成**语料的函数**：

    derive  对每题用三种互不依赖的方法从 docs.sqlite 推答案
            M1 chunk 级：top-5 段落 → 选项 bigram 的 idf 加权和
            M2 句子级：top-5 段落切句 → 按题干相似度取 top-3 句 → 只在句内给选项打分（更锐）
            M3 多文档共识：选项的 4 字片段若在不同文档的段落里出现，计数投票
    对账    与存量 answerKeys 比：agree / conflict / unknown（语料覆盖不到）
    落库    answers 表（含证据文档 + 片段 + 置信度 + 方法一致数 + 时间戳）
    输出    冲突清单 answer_conflicts.md，供人工裁决——这是「避免错误答案」的入口

用法:
    python3 kb_answer_resolve.py resolve [--bank banks/xxx.json ...]
    python3 kb_answer_resolve.py report              # 只看库里的对账结果
    python3 kb_answer_resolve.py eval --bank banks/dtzx-safety.json   # 用存量答案当基准比方法
"""
import argparse
import glob
import json
import os
import re
import sys
import time

PROJ = os.path.expanduser("~/x_videos-dowloads/exam-lookup")
OUT = os.path.join(PROJ, "jr/out")
sys.path.insert(0, PROJ)
import kb_db as KB  # noqa: E402

MULTI_HINT = ("多选", "多项", "不定项")


def bank_questions(path):
    d = json.load(open(path, encoding="utf-8"))
    return d if isinstance(d, list) else (d.get("questions") or d.get("items") or [])


def is_multi(q):
    t = (q.get("typeLabel") or q.get("type") or "")
    return any(h in str(t) for h in MULTI_HINT) or len(q.get("answerKeys") or []) > 1


def sentences(text, minlen=6):
    return [s.strip() for s in re.split(r"[。；;！!？?\n]", text) if len(s.strip()) >= minlen]


def score_options(db, ctx_text, options, nchunks):
    """选项打分：选项 bigram ∩ 上下文 bigram，按 idf 加权（越稀有的 bigram 权重越高）。"""
    ctx = KB.cjk_bigrams(ctx_text)
    out = {}
    for o in options or []:
        g = KB.cjk_bigrams(o.get("text") or "")
        if g:
            out[o["key"]] = sum(KB._idf(db, x, nchunks) for x in g if x in ctx)
    return out


def chunk_ctx(db, query, topk=5):
    return KB.search_with_cite(db, query, topk)


def m1_chunk(db, hits, options, nchunks):
    return score_options(db, " ".join(h["text"] for h in hits), options, nchunks)


def m2_sentence(db, hits, question, options, nchunks, top_sent=3):
    qs = KB.cjk_bigrams(KB.query_terms(question))
    cands = []
    for h in hits:
        for s in sentences(h["text"]):
            ov = len(qs & KB.cjk_bigrams(s))
            if ov:
                cands.append((ov, h["doc"], s))
    cands.sort(key=lambda x: -x[0])
    top = cands[:top_sent]
    if not top:
        return {}, None
    return score_options(db, " ".join(c[2] for c in top), options, nchunks), top[0]


def m3_consensus(hits, options):
    """选项里能整段落在几份不同文档里 → 该选项有旁证。返回 {key: 文档数}。"""
    votes = {}
    for o in options or []:
        text = o.get("text") or ""
        wins = {text[i:i + 4] for i in range(max(0, len(text) - 3))}
        if not wins:
            continue
        docs = {h["doc"] for h in hits if any(w in h["text"] for w in wins)}
        votes[o["key"]] = len(docs)
    return votes


def pick(scores, multi):
    if not scores:
        return []
    if not multi:
        return [max(scores, key=scores.get)]
    top = max(scores.values())
    return [k for k, v in scores.items() if v >= 0.75 * top] if top > 0 else []


_DF_CACHE = {}


def df_of(db, g):
    if g in _DF_CACHE:
        return _DF_CACHE[g]
    row = db.execute("SELECT LENGTH(chunk_ids)/4 FROM postings WHERE bigram=?", (g,)).fetchone()
    _DF_CACHE[g] = row[0] if row else 0
    return _DF_CACHE[g]


def rare_coverage(db, question, text, nchunks, df_ratio=0.05):
    """题干里的稀有 bigram 有多少比例出现在证据段落里。

    这是「证据是不是真在讲这道题」的硬指标：不相关文档只会共享「工作」「安全」这类
    到处都有的 bigram，稀有词（涉军、脚扣、掏挖）几乎不会同时出现。
    """
    qg = KB.cjk_bigrams(KB.query_terms(question or ""))
    limit = max(3, int(nchunks * df_ratio))
    rare = [g for g in qg if df_of(db, g) <= limit]
    if not rare:
        return 1.0, 0
    tg = KB.cjk_bigrams(text or "")
    cov = sum(1 for g in rare if g in tg) / len(rare)
    return cov, len(rare)


def resolve_one(db, q, nchunks):
    """M2（句级）为推导主答案，M1 作交叉校验，M3 作旁证票。

    取舍依据（3 个题库实测，严格口径）：M2 在 3 个题库上 ≥ M1，1 个持平，1 个（保密公开题）
    略低——那里存量答案来自平台自带答案，措辞差异大。所以：M2 主判，M1 只用来判置信度，
    绝不因为「推导与存量不一致」就自动改写存量答案。
    """
    question = q.get("question") or ""
    options = q.get("options") or []
    hits = chunk_ctx(db, question)
    if not hits:
        return {"derived": [], "status": "unknown", "confidence": "none", "method": None,
                "evidence": None, "agree": 0}
    multi = is_multi(q)
    s1 = m1_chunk(db, hits, options, nchunks)
    s2, sent = m2_sentence(db, hits, question, options, nchunks)
    v3 = m3_consensus(hits, options)
    k1, k2 = pick(s1, multi), pick(s2, multi)
    derived = k2 or k1
    stored = list(q.get("answerKeys") or [])

    agree_m1 = bool(k1) and set(k1) == set(derived)
    ev_text = " ".join(h["text"] for h in hits[:3])
    cov, nrare = rare_coverage(db, question, ev_text, nchunks)
    # 只在「证据真的在讲这道题」时才给高置信；旁证票（v3）不再单独抬级，
    # 因为通用选项措辞（「加强管理」这类）在任何文档里都能凑出 2 份旁证。
    if agree_m1 and cov >= 0.5:
        confidence = "high"
    elif agree_m1 and cov >= 0.3:
        confidence = "medium"
    elif cov >= 0.5:
        confidence = "medium"
    else:
        confidence = "low"

    if not stored:
        status = "derived-only"
    elif set(derived) == set(stored):
        status = "agree"
    else:
        status = "conflict"
    ev = (sent[1], sent[2]) if sent else (hits[0]["doc"], hits[0]["text"][:200])
    return {"derived": derived, "status": status, "confidence": confidence,
            "method": "M2主判 M1%s 稀有词覆盖%.0f%%(%d词)" % (
                "一致" if agree_m1 else "不一致", 100 * cov, nrare),
            "evidence": {"doc": ev[0], "snippet": ev[1][:300]}, "agree": agree_m1}


def ensure_table(db):
    db.executescript("""
    CREATE TABLE IF NOT EXISTS answers(
      bank TEXT, qid TEXT, question TEXT, stored TEXT, derived TEXT, status TEXT,
      confidence TEXT, method TEXT, evidence_doc TEXT, evidence_text TEXT, updated_at TEXT,
      PRIMARY KEY(bank, qid));
    """)
    if db.execute("PRAGMA user_version").fetchone()[0] < 2:
        db.execute("PRAGMA user_version=2")
    db.commit()


def cmd_resolve(a):
    db = KB.connect()
    ensure_table(db)
    # 已经有网络补答结论的题不再被本地重算覆盖（除非 --refresh-web）
    keep_web = set()
    if not getattr(a, "refresh_web", False):
        cols = {r[1] for r in db.execute("PRAGMA table_info(answers)")}
        if "provenance" in cols:
            keep_web = {(b, q) for b, q in db.execute(
                "SELECT bank, qid FROM answers WHERE provenance IN ('web','kaoshibao','manual')")}
    nchunks = db.execute("SELECT COUNT(*) FROM chunks").fetchone()[0]
    banks = a.bank or sorted(f for f in glob.glob(os.path.join(PROJ, "banks/*.json"))
                             if not f.endswith(".bak"))
    rows, conflicts, totals = [], [], {"agree": 0, "conflict": 0, "unknown": 0, "derived-only": 0}
    t0 = time.time()
    for path in banks:
        name = os.path.basename(path)
        for q in bank_questions(path):
            if (name, q.get("id") or q.get("number")) in keep_web:
                continue
            r = resolve_one(db, q, nchunks)
            totals[r["status"]] = totals.get(r["status"], 0) + 1
            rec = {"bank": name, "qid": q.get("id") or q.get("number"),
                   "question": q.get("question"), "stored": "/".join(q.get("answerKeys") or []),
                   "derived": "/".join(r["derived"]), "status": r["status"],
                   "confidence": r["confidence"], "method": r["method"],
                   "evidence_doc": (r["evidence"] or {}).get("doc", ""),
                   "evidence_text": (r["evidence"] or {}).get("snippet", ""),
                   "updated_at": time.strftime("%Y-%m-%d %H:%M:%S")}
            rows.append(rec)
            if r["status"] == "conflict":
                conflicts.append(rec)
    with db:
        db.executemany("""INSERT OR REPLACE INTO answers(bank,qid,question,stored,derived,status,
                          confidence,method,evidence_doc,evidence_text,updated_at)
                          VALUES(?,?,?,?,?,?,?,?,?,?,?)""",
                       [(r["bank"], r["qid"], r["question"], r["stored"], r["derived"], r["status"],
                         r["confidence"], r["method"], r["evidence_doc"], r["evidence_text"],
                         r["updated_at"]) for r in rows])
    db.close()

    n = len(rows)
    print("求解 %d 题 / %d 个题库，%.1fs" % (n, len(banks), time.time() - t0))
    print("  一致 agree        %4d (%.0f%%)" % (totals["agree"], 100.0 * totals["agree"] / max(1, n)))
    print("  冲突 conflict     %4d (%.0f%%)  ← 待人工裁决" % (totals["conflict"], 100.0 * totals["conflict"] / max(1, n)))
    print("  语料无覆盖 unknown %4d (%.0f%%)" % (totals["unknown"], 100.0 * totals["unknown"] / max(1, n)))
    print("  仅推导 derived-only %3d" % totals["derived-only"])

    md = os.path.join(OUT, "answer_conflicts.md")
    with open(md, "w", encoding="utf-8") as fp:
        fp.write("# 答案冲突清单（库存答案 vs 语料推导）\n\n")
        fp.write("生成时间：%s　共 %d 条冲突。左列=题库里存的答案，右列=从 docs.sqlite 推出的答案，"
                 "证据是该选项措辞所在的原文片段。\n\n" % (time.strftime("%Y-%m-%d %H:%M"), len(conflicts)))
        for c in conflicts:
            fp.write("## %s · %s\n\n" % (c["bank"], c["qid"]))
            fp.write("- 题目：%s\n" % c["question"])
            fp.write("- 库存答案：**%s** ／ 推导答案：**%s**（置信 %s，%s）\n"
                     % (c["stored"], c["derived"], c["confidence"], c["method"]))
            fp.write("- 出处：%s\n" % c["evidence_doc"])
            fp.write("- 片段：%s\n\n" % c["evidence_text"].replace("\n", " ")[:260])
    json.dump(rows, open(os.path.join(OUT, "answers.json"), "w", encoding="utf-8"),
              ensure_ascii=False, indent=1)
    print("  冲突清单 → %s（%d 条）\n  全量 → %s" % (md, len(conflicts), os.path.join(OUT, "answers.json")))
    return 0


def cmd_report(a):
    db = KB.connect(readonly=True)
    try:
        rows = db.execute("SELECT bank, status, COUNT(*) FROM answers GROUP BY bank, status").fetchall()
    except Exception as e:
        print("还没有 answers 表，先跑 resolve：", e)
        return 1
    banks = {}
    for bank, status, n in rows:
        banks.setdefault(bank, {})[status] = n
    print("%-32s %6s %8s %8s" % ("题库", "总数", "一致", "冲突"))
    for bank, st in sorted(banks.items()):
        tot = sum(st.values())
        print("%-32s %6d %8d %8d" % (bank[:32], tot, st.get("agree", 0), st.get("conflict", 0)))
    db.close()
    return 0


def cmd_eval(a):
    """把存量答案当基准（可能本身有错，只作横向比较），比 M1 / M2 / 合并 三种取法。"""
    db = KB.connect(readonly=True)
    nchunks = db.execute("SELECT COUNT(*) FROM chunks").fetchone()[0]
    qs = bank_questions(os.path.expanduser(a.bank))
    stats = {"M1": 0, "M2": 0, "M1+M2+M3": 0}
    for q in qs:
        hits = chunk_ctx(db, q.get("question") or "")
        if not hits:
            continue
        options = q.get("options") or []
        multi = is_multi(q)
        s1 = m1_chunk(db, hits, options, nchunks)
        s2, _ = m2_sentence(db, hits, q.get("question") or "", options, nchunks)
        v3 = m3_consensus(hits, options)
        merged = {}
        for k in set(s1) | set(s2):
            n1 = s1.get(k, 0) / max(s1.values()) if s1 and max(s1.values()) > 0 else 0
            n2 = s2.get(k, 0) / max(s2.values()) if s2 and max(s2.values()) > 0 else 0
            merged[k] = n1 + n2 + 0.25 * v3.get(k, 0)
        stored = set(q.get("answerKeys") or [])
        stats["M1"] += set(pick(s1, multi)) == stored and bool(stored)
        stats["M2"] += set(pick(s2, multi)) == stored and bool(stored)
        stats["M1+M2+M3"] += set(pick(merged, multi)) == stored and bool(stored)
    n = len(qs)
    print("题库 %s：%d 题" % (os.path.basename(a.bank), n))
    for k, v in stats.items():
        print("  %-10s %d/%d = %.0f%%" % (k, v, n, 100.0 * v / max(1, n)))
    db.close()
    return 0


def main():
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    r = sub.add_parser("resolve")
    r.add_argument("--bank", nargs="*", help="指定题库（默认 banks/*.json 全部）")
    r.add_argument("--refresh-web", action="store_true", help="连网络补答过的题也重算本地结论")
    r.set_defaults(fn=cmd_resolve)
    p = sub.add_parser("report"); p.set_defaults(fn=cmd_report)
    e = sub.add_parser("eval"); e.add_argument("--bank", required=True); e.set_defaults(fn=cmd_eval)
    a = ap.parse_args()
    return a.fn(a)


if __name__ == "__main__":
    sys.exit(main())
