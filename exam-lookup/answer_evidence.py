#!/usr/bin/env python3
"""answer_evidence —— 逐题回锚：这道题的答案在原文哪里？

判定分三档（都可核）：
    直接命中  答案选项的 ≥6 字连续片段出现在原文某句里            → 有依据（强）
    主题命中  题干与某句稀有词覆盖率 ≥50%，但答案片段未原文出现     → 有依据（弱，需人看）
    无依据    两者都不满足（或语料里根本没这道题的主题）            → 无依据

判断题特殊处理：答案为「正确」时找最贴近题干的句子；答案为「错误」时，
标注「原文与题干相悖」——因为错误题的依据是原文说了另一回事，机器只能给出最接近的原文句，
矛盾点需要人确认（脚本会把它标成「需人工确认」）。

用法:
    python3 answer_evidence.py --txt "/Users/hgxszhj/Documents/考试.txt.txt"
"""
import argparse
import json
import os
import re
import sys

PROJ = os.path.expanduser("~/x_videos-dowloads/exam-lookup")
OUT = os.path.join(PROJ, "jr/out")
sys.path.insert(0, PROJ)
import kb_db as KB                       # noqa: E402
import kb_answer_resolve as R            # noqa: E402
import compare_exam_txt as C             # noqa: E402

SENT_SPLIT = re.compile(r"[。；;！!？?\n]")


def build_sentences(db):
    """句子索引：(句子, 文档名, bigram 集合)。bigram 预先算好，否则 6 万句 × 61 题会跑死。"""
    out = []
    rows = db.execute("""SELECT c.text, d.name FROM chunks c JOIN documents d ON d.id=c.doc_id
                         WHERE d.gone_at IS NULL""").fetchall()
    for text, name in rows:
        for s in SENT_SPLIT.split(text or ""):
            s = KB.normalize(s)
            if 8 <= len(s) <= 400:
                out.append((s, name, KB.cjk_bigrams(s)))
    return out


def frags(text, n=6):
    """答案文本的连续片段（n 字窗口），用于判断「原文是否原样说过这句话」。"""
    t = KB.normalize(text or "")
    t = re.sub(r"[，。；：、（）()\"'“”‘’\s]", "", t)
    return [t[i:i + n] for i in range(0, max(0, len(t) - n + 1))]


def best_sent_by_answer(answer_text, sents):
    """找包含答案片段最多的句子。片段匹配用 str.find（C 速度），命中 3 个就提前收工。"""
    fs = frags(answer_text)
    if not fs:
        return None, 0, ""
    best, best_hit, best_frag = None, 0, ""
    for s, name, _ in sents:
        hit = sum(1 for f in fs if f in s)
        if hit > best_hit:
            best, best_hit, best_frag = (s, name), hit, ""
            if hit >= 3:
                break
    return best, best_hit, best_frag


def best_sent_by_stem(stem, sents, qbig):
    best, best_score = None, 0.0
    if not qbig:
        return None, 0.0
    for s, name, sg in sents:
        if not sg:
            continue
        score = len(qbig & sg) / len(qbig)
        if score > best_score:
            best, best_score = (s, name), score
    return best, best_score


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--txt", default="/Users/hgxszhj/Documents/考试.txt.txt")
    ap.add_argument("--out", default=os.path.join(OUT, "answer_evidence.md"))
    a = ap.parse_args()

    qs = C.parse(os.path.expanduser(a.txt))
    db = KB.connect(readonly=True)
    sents = build_sentences(db)
    print("句子索引：%d 句（来自 %d 份文档）" % (len(sents), len({s[1] for s in sents})))

    rows = []
    for q in qs:
        ans = q["ans"]
        ans_texts = []
        if q["kind"] == "choice":
            ans_texts = [q["options"].get(k, "") for k in (ans or [])]
        elif q["kind"] == "judge":
            ans_texts = []          # 判断题不靠选项文本，走题干匹配
        qbig = KB.cjk_bigrams(KB.query_terms(q["stem"]))

        direct = None
        direct_hits = 0
        for t in ans_texts:
            s, hits, frag = best_sent_by_answer(t, sents)
            if hits > direct_hits:
                direct, direct_hits = s, hits
        stem_sent, stem_score = best_sent_by_stem(q["stem"], sents, qbig)

        if q["kind"] == "judge":
            if stem_score >= 0.5:
                verdict = "有依据（弱）" if q["ans"] == "正确" else "需人工确认"
                ev, doc = stem_sent
            else:
                verdict, ev, doc = "无依据", "", ""
        elif direct and direct_hits >= 2:
            verdict, ev, doc = "有依据（强）", direct[0], direct[1]
        elif ans_texts and all(len(re.sub(r"\s", "", x)) < 6 for x in ans_texts):
            # 答案本身太短（如「AI」「5G」），6 字片段策略锚不住，硬锚会张冠李戴
            verdict, ev, doc = "答案过短需人工确认", (stem_sent[0] if stem_sent else ""), (
                stem_sent[1] if stem_sent else "")
        elif stem_score >= 0.5:
            verdict, ev, doc = "有依据（弱）", stem_sent[0], stem_sent[1]
        else:
            verdict, ev, doc = "无依据", "", ""

        rows.append({"no": q["no"], "type": q["type"], "stem": q["stem"],
                     "ans": ans if not isinstance(ans, list) else ",".join(ans),
                     "ans_text": " ｜ ".join(ans_texts)[:90],
                     "verdict": verdict, "evidence": ev, "doc": doc,
                     "direct_hits": direct_hits, "stem_score": round(stem_score, 2)})

    db.close()
    n = len(rows)
    cnt = {}
    for r in rows:
        cnt[r["verdict"]] = cnt.get(r["verdict"], 0) + 1

    L = ["# 考试.txt 逐题依据回锚", ""]
    L.append("语料：187 份平台文档（177 份有正文）／句子索引 %d 句" % len(sents))
    L.append("")
    L.append("| 判定 | 题数 | 占比 |")
    L.append("|---|---|---|")
    for k in ("有依据（强）", "有依据（弱）", "需人工确认", "答案过短需人工确认", "无依据"):
        if cnt.get(k):
            L.append("| %s | %d | %.0f%% |" % (k, cnt[k], 100.0 * cnt[k] / n))
    L.append("")
    L.append("判定标准：答案选项文本的 6 字连续片段在原文出现 ≥2 处 → 强；"
             "只在题干层面与原文稀有词重合 ≥50% → 弱；判断题答「错误」时机器只能给出最接近的原文句，需人确认矛盾点。")
    L.append("")
    L.append("## 逐题明细")
    L.append("")
    L.append("| # | 题型 | 答案 | 判定 | 原文依据（截 70 字） | 出处 |")
    L.append("|---|---|---|---|---|---|")
    for r in rows:
        L.append("| %d | %s | %s | %s | %s | %s |" % (
            r["no"], r["type"] or "—", r["ans"] or "—", r["verdict"],
            (r["evidence"] or "—").replace("|", "/")[:70], (r["doc"] or "—")[:26]))

    md = "\n".join(L)
    open(a.out, "w", encoding="utf-8").write(md)
    json.dump(rows, open(os.path.join(OUT, "answer_evidence.json"), "w", encoding="utf-8"),
              ensure_ascii=False, indent=1)
    print("\n".join(L[:20]))
    print("...\n报告 → %s" % a.out)
    return 0


if __name__ == "__main__":
    sys.exit(main())
