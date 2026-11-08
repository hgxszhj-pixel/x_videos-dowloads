#!/usr/bin/env python3
"""doc_adjudicate —— 只依据原文判定答案（不采信题库/文件里的答案）。

做法：给每道题准备候选原文句（两路取：① 与题干稀有词重合最高 ② 含任一选项措辞片段），
把「题干 + 选项 + 候选原句」按批导出成人读清单，由人或模型逐题判定：

    supported   原文直接说了该选项的内容
    excluded    原文明说与该选项相反 / 把该选项排除在外
    unknown     原文没有相关信息

导出：
    jr/out/adjudicate/batch_XX.md    每批 12 题
"""
import argparse
import json
import os
import re
import sys

PROJ = os.path.expanduser("~/x_videos-dowloads/exam-lookup")
OUT = os.path.join(PROJ, "jr/out")
BATCH_DIR = os.path.join(OUT, "adjudicate")
sys.path.insert(0, PROJ)
import kb_db as KB          # noqa: E402
import compare_exam_txt as C  # noqa: E402

SENT_SPLIT = re.compile(r"[。；;！!？?\n]")


def norm_sents(db):
    out = []
    for text, name in db.execute("""SELECT c.text, d.name FROM chunks c JOIN documents d ON d.id=c.doc_id
                                    WHERE d.gone_at IS NULL"""):
        for s in SENT_SPLIT.split(text or ""):
            s = KB.normalize(s)
            if 10 <= len(s) <= 300:
                out.append((s, name, KB.cjk_bigrams(s)))
    return out


def pick_candidates(q, sents, k=4):
    qbig = KB.cjk_bigrams(KB.query_terms(q["stem"]))
    scored = []
    for s, name, sg in sents:
        if not sg:
            continue
        ov = len(qbig & sg) / max(1, len(qbig))
        if ov < 0.25:
            continue
        scored.append((ov, s, name))
    scored.sort(key=lambda x: -x[0])
    picks, seen = [], set()
    for ov, s, name in scored[:40]:
        key = s[:40]
        if key in seen:
            continue
        seen.add(key)
        picks.append((s, name, ov))
        if len(picks) >= k:
            break
    return picks


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--txt", default="/Users/hgxszhj/Documents/考试.txt.txt")
    ap.add_argument("--per-batch", type=int, default=12)
    a = ap.parse_args()

    qs = C.parse(os.path.expanduser(a.txt))
    db = KB.connect(readonly=True)
    sents = norm_sents(db)
    db.close()
    os.makedirs(BATCH_DIR, exist_ok=True)
    print("语料句 %d 条，题 %d 道" % (len(sents), len(qs)))

    for i in range(0, len(qs), a.per_batch):
        chunk = qs[i:i + a.per_batch]
        idx = i // a.per_batch + 1
        path = os.path.join(BATCH_DIR, "batch_%02d.md" % idx)
        with open(path, "w", encoding="utf-8") as fp:
            fp.write("# 原文判定清单 第 %02d 批（题 %d–%d）\n\n" % (idx, chunk[0]["no"], chunk[-1]["no"]))
            fp.write("只依据下面的原文句判定；原文没说的写 unknown，不要猜。\n\n")
            for q in chunk:
                fp.write("## #%d　%s\n\n" % (q["no"], q["type"]))
                fp.write("**题干**：%s\n\n" % q["stem"])
                for k in sorted(q["options"]):
                    fp.write("- **%s** %s\n" % (k, q["options"][k]))
                fp.write("\n**文件给出的答案**：%s（仅作对照，不作依据）\n\n" % (
                    q["ans"] if not isinstance(q["ans"], list) else ",".join(q["ans"])))
                fp.write("**候选原文**：\n\n")
                for j, (s, name, ov) in enumerate(pick_candidates(q, sents), 1):
                    fp.write("%d. （%s，重合 %.2f）%s\n" % (j, name[:30], ov, s[:220]))
                fp.write("\n**判定**：＿\n\n---\n\n")
        print("  →", path)
    return 0


if __name__ == "__main__":
    sys.exit(main())
