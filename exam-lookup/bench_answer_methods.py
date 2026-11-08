#!/usr/bin/env python3
"""bench_answer_methods —— 横向比较「从本地文档推出答案」的几种取法，看哪种最准。

每种方法共用同一份检索结果（每题只查一次库），所以跑得快、比较公平。
指标 = 与题库存量答案严格一致（多选要求集合全等）；另附多选单独的严格/交集命中。

方法:
    M1  chunk      top-5 段落整体给选项打分（最初的写法）
    M2  sentence   段落切句，取与题干最像的 top-3 句打分
    M3  top1句      只取最像的 1 句（最锐，也最易被噪声带偏）
    M4  M2+字面     在 M2 基础上，要求选项有 4 字片段真出现在那几句里
    M5  M2∧M1      两者都指向同一选项才算，否则取 M2
    M6  共识        选项的 4 字片段在 ≥2 份文档里出现则加权（文档级投票）
    M7  M2+题干权重  选项打分时乘上「与题干共有 bigram」的比例

用法: python3 bench_answer_methods.py [--sample 200]
"""
import argparse
import json
import os
import sys

PROJ = os.path.expanduser("~/x_videos-dowloads/exam-lookup")
sys.path.insert(0, PROJ)
import kb_db as KB          # noqa: E402
import kb_answer_resolve as R  # noqa: E402

BANKS = ["dtzx-safety.json", "dtzx-yungai.json", "comm-safety-50.json", "changqi-anquan.json",
         "docs-otn-transport.json", "docs-tianyi-security.json", "自测-安全数字.json",
         "保密公开题-中科院.json"]


def literal_in(text, option_text):
    t = option_text or ""
    return any(t[i:i + 4] in text for i in range(max(0, len(t) - 3)))


def score_with_question(ctx, question, options, nchunks, q_weight=0.0):
    out = {}
    qg = KB.cjk_bigrams(KB.query_terms(question))
    ctx_g = KB.cjk_bigrams(ctx)
    for o in options or []:
        g = KB.cjk_bigrams(o.get("text") or "")
        if not g:
            continue
        s = 0.0
        for x in g:
            if x in ctx_g:
                s += 1.0
        if q_weight and g:
            s *= (1.0 + q_weight * len(g & qg) / len(g))
        out[o["key"]] = s
    return out


def run_bank(db, path, sample=0):
    qs = R.bank_questions(os.path.join(PROJ, "banks", path))
    if sample and len(qs) > sample:
        qs = qs[:sample]
    nchunks = db.execute("SELECT COUNT(*) FROM chunks").fetchone()[0]
    methods = ["M1", "M2", "M3", "M4", "M5", "M6", "M7"]
    ok = {m: 0 for m in methods}
    multi_ok = {m: 0 for m in methods}
    multi_n = 0
    covered = 0
    for q in qs:
        hits = R.chunk_ctx(db, q.get("question") or "")
        if not hits:
            continue
        covered += 1
        opts = q.get("options") or []
        question = q.get("question") or ""
        multi = R.is_multi(q)
        stored = set(q.get("answerKeys") or [])
        if multi:
            multi_n += 1

        s1 = R.m1_chunk(db, hits, opts, nchunks)
        s2, sent = R.m2_sentence(db, hits, question, opts, nchunks)
        # 句子池（与 M2 同源，供 M3/M4 用）
        qg = KB.cjk_bigrams(KB.query_terms(question))
        cands = []
        for h in hits:
            for s in R.sentences(h["text"]):
                ov = len(qg & KB.cjk_bigrams(s))
                if ov:
                    cands.append((ov, s))
        cands.sort(key=lambda x: -x[0])
        ctx3 = " ".join(c[1] for c in cands[:3])
        ctx1 = cands[0][1] if cands else ""
        s3 = R.score_options(db, ctx1, opts, nchunks)
        s4 = {k: v for k, v in R.score_options(db, ctx3, opts, nchunks).items()
              if any(literal_in(ctx3, o.get("text")) for o in opts if o.get("key") == k)}
        k1 = R.pick(s1, multi)
        k2 = R.pick(s2, multi)
        k5 = k1 if (k1 and k2 and set(k1) == set(k2)) else k2
        votes = R.m3_consensus(hits, opts)
        s6 = {}
        for o in opts:
            v = votes.get(o.get("key"), 0)
            if v:
                den = max(s2.values()) if (s2 and max(s2.values()) > 0) else 1
                s6[o["key"]] = v * 1.0 + s2.get(o.get("key"), 0) / den
        s7 = score_with_question(ctx3, question, opts, nchunks, q_weight=0.5)

        preds = {"M1": k1, "M2": k2, "M3": R.pick(s3, multi), "M4": R.pick(s4, multi),
                 "M5": k5, "M6": R.pick(s6, multi) or k2, "M7": R.pick(s7, multi)}
        for m, p in preds.items():
            hit = set(p) == stored and bool(stored)
            ok[m] += hit
            if multi:
                multi_ok[m] += hit
    return len(qs), covered, ok, multi_ok, multi_n


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--sample", type=int, default=0, help="每个题库最多取多少题（0 = 全量）")
    a = ap.parse_args()
    db = KB.connect(readonly=True)
    methods = ["M1", "M2", "M3", "M4", "M5", "M6", "M7"]
    print("%-24s %5s %5s " % ("题库", "题数", "覆盖") + " ".join("%8s" % m for m in methods))
    agg = {m: [0, 0] for m in methods}
    for b in BANKS:
        if not os.path.exists(os.path.join(PROJ, "banks", b)):
            continue
        n, cov, ok, mok, mn = run_bank(db, b, a.sample)
        print("%-24s %5d %5d " % (b[:24], n, cov) + " ".join(
            "%8s" % ("%d(%.0f%%)" % (ok[m], 100.0 * ok[m] / max(1, n))) for m in methods))
        for m in methods:
            agg[m][0] += ok[m]
            agg[m][1] += n
    print("-" * 100)
    print("%-24s %5s %5s " % ("合计", "", "") + " ".join(
        "%8s" % ("%.0f%%" % (100.0 * agg[m][0] / max(1, agg[m][1]))) for m in methods))
    print("\n（口径：与题库存量答案严格一致；存量答案本身可能有错，所以这比的是「一致性」，不是绝对正确率）")
    db.close()


if __name__ == "__main__":
    main()
