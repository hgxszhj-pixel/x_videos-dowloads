#!/usr/bin/env python3
"""compare_exam_txt —— 拿「文件自带的答案」对账：库内答案、本地文档推导。

三种题型都支持：
    选择题（单选/多选）  【答案】A 或 A、B、C   → 比集合
    判断题              【答案】正确 / 错误     → 比 正确/错误
    排序题              【答案】CBDAFE          → 有序比较（不是集合）

用法:
    python3 compare_exam_txt.py "/Users/hgxszhj/Documents/考试.txt.txt"
    python3 compare_exam_txt.py <file> --bank banks/dtzx-yungai.json --out jr/out/x.md
"""
import argparse
import json
import os
import re
import sys

PROJ = os.path.expanduser("~/x_videos-dowloads/exam-lookup")
OUT = os.path.join(PROJ, "jr/out")
sys.path.insert(0, PROJ)
import kb_db as KB           # noqa: E402
import kb_answer_resolve as R  # noqa: E402

Q_RE = re.compile(r"^\s*(\d+)\s*[、.．]\s*(.+)$")
OPT_RE = re.compile(r"^\s*([A-H])\s*[.、．)]\s*(.+?)\s*$")
ANS_RE = re.compile(r"【答案】\s*(.+)")
TYPE_RE = re.compile(r"[（(]\s*(单选题|多选题|判断题|排序题|不定项)[^）)]*[）)]")
JUDGE_MAP = {"正确": "正确", "错误": "错误", "对": "正确", "错": "错误", "√": "正确", "×": "错误"}


def parse(path):
    qs, cur = [], None
    for raw in open(path, encoding="utf-8", errors="ignore"):
        line = raw.rstrip("\n")
        m = Q_RE.match(line)
        if m and not OPT_RE.match(line):
            if cur:
                qs.append(cur)
            full = m.group(2)
            tm = TYPE_RE.search(full)
            cur = {"no": int(m.group(1)), "stem": TYPE_RE.sub("", full).strip(),
                   "type": tm.group(1) if tm else "", "options": {},
                   "kind": "", "ans": None, "ans_raw": ""}
            continue
        if cur is None:
            continue
        mo = OPT_RE.match(line)
        if mo:
            cur["options"][mo.group(1)] = mo.group(2).strip()
            continue
        ma = ANS_RE.search(line)
        if ma:
            raw_ans = ma.group(1).strip()
            cur["ans_raw"] = raw_ans
            letters = re.findall(r"[A-H]", raw_ans)
            judge = next((v for k, v in JUDGE_MAP.items() if raw_ans.startswith(k)), None)
            if cur["type"] == "排序题" or ("→" in raw_ans and len(letters) >= 3):
                cur["kind"], cur["ans"] = "order", "".join(letters)
            elif judge:
                cur["kind"], cur["ans"] = "judge", judge
            elif letters:
                cur["kind"], cur["ans"] = "choice", sorted(set(letters))
    if cur:
        qs.append(cur)
    return qs


def stem_sim(a, b):
    ga, gb = KB.cjk_bigrams(a), KB.cjk_bigrams(b)
    if not ga or not gb:
        return 0.0
    return len(ga & gb) / len(ga)


def match_bank(q, bank_qs):
    best, best_s = None, 0.0
    for b in bank_qs:
        s = stem_sim(q["stem"], b.get("question") or "")
        if s > best_s:
            best, best_s = b, s
    return (best, best_s) if best_s >= 0.62 else (None, best_s)


def bank_answer(bq, kind):
    """把库内答案转成和文件同一口径。"""
    keys = [str(x).upper() for x in (bq.get("answerKeys") or [])]
    if not keys:
        return None
    if kind == "judge":
        for o in bq.get("options") or []:
            if str(o.get("key")).upper() in keys:
                t = str(o.get("text") or "").strip()
                if t in JUDGE_MAP:
                    return JUDGE_MAP[t]
        return "正确" if keys == ["A"] else "错误"
    if kind == "order":
        return "".join(keys)
    return sorted(set(keys))


def fmt(v):
    if v is None:
        return "—"
    if isinstance(v, list):
        return ",".join(v) if v else "—"
    return str(v)


def derive_local(db, stem, opts, kind, nchunks, multi=False):
    if kind == "judge" and not opts:
        opts = [{"key": "A", "text": "正确"}, {"key": "B", "text": "错误"}]
    if not opts:
        return None, "", 0.0
    hits = R.chunk_ctx(db, stem)
    if not hits:
        return None, "", 0.0
    cov, _ = R.rare_coverage(db, stem, " ".join(h["text"] for h in hits[:3]), nchunks)
    s2, sent = R.m2_sentence(db, hits, stem, opts, nchunks)
    picked = R.pick(s2, multi)
    doc = sent[1] if sent else hits[0]["doc"]
    if kind == "judge":
        texts = [o["text"] for o in opts if o["key"] in picked]
        return (JUDGE_MAP.get(texts[0]) if texts else None), doc, cov
    return (sorted(picked) if picked else None), doc, cov


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("path")
    ap.add_argument("--bank", default=os.path.join(PROJ, "banks/dtzx-yungai.json"))
    ap.add_argument("--out", default=os.path.join(OUT, "exam_txt_compare.md"))
    a = ap.parse_args()

    qs = parse(os.path.expanduser(a.path))
    bank_qs = R.bank_questions(os.path.expanduser(a.bank))
    db = KB.connect(readonly=True)
    nchunks = db.execute("SELECT COUNT(*) FROM chunks").fetchone()[0]

    rows = []
    for q in qs:
        bq, sim = match_bank(q, bank_qs)
        bank_ans = bank_answer(bq, q["kind"]) if bq else None
        opts = [{"key": k, "text": v} for k, v in q["options"].items()]
        multi = (q["type"] == "多选题") or (isinstance(q["ans"], list) and len(q["ans"]) > 1)
        derived, doc, cov = derive_local(db, q["stem"], opts, q["kind"], nchunks, multi)
        rows.append({
            "no": q["no"], "stem": q["stem"], "type": q["type"], "kind": q["kind"],
            "file_ans": q["ans"], "ans_raw": q["ans_raw"], "bank_ans": bank_ans,
            "same_bank": (bank_ans == q["ans"]) if (bank_ans is not None and q["ans"] is not None) else None,
            "derived": derived,
            "same_doc": (derived == q["ans"]) if (derived and q["ans"] is not None) else None,
            "doc": doc, "cov": round(cov, 2), "sim": round(sim, 2), "n_opt": len(q["options"])})

    n = len(rows)
    by_type = {}
    for r in rows:
        by_type.setdefault(r["type"] or "未标注", []).append(r)
    cmp_rows = [r for r in rows if r["same_bank"] is not None]
    agree_b = [r for r in cmp_rows if r["same_bank"]]
    diff_b = [r for r in cmp_rows if not r["same_bank"]]
    doc_rows = [r for r in rows if r["same_doc"] is not None and r["cov"] >= 0.5]
    agree_d = [r for r in doc_rows if r["same_doc"]]
    no_bank = [r for r in rows if r["same_bank"] is None]

    def pct(x, d=n):
        return "%.0f%%" % (100.0 * x / d) if d else "—"

    L = ["# 考试.txt 答案对账", ""]
    L.append("文件：`%s`　共 **%d** 题（%s）" % (
        a.path, n, "，".join("%s %d" % (k, len(v)) for k, v in sorted(by_type.items()))))
    L.append("")
    L.append("对账对象：`banks/%s`（%d 题）＋ 本地 187 份平台文档的检索推导。" % (
        os.path.basename(a.bank), len(bank_qs)))
    L.append("")
    L.append("## 一、总览")
    L.append("")
    L.append("| 项目 | 数量 | 占全文件 |")
    L.append("|---|---|---|")
    L.append("| 文件题数 | %d | 100%% |" % n)
    L.append("| 库内找到同题（题干相似度≥0.62） | %d | %s |" % (len(cmp_rows), pct(len(cmp_rows))))
    L.append("| **库内答案与文件一致** | **%d** | **%s** |" % (len(agree_b), pct(len(agree_b))))
    L.append("| **库内答案与文件冲突** | **%d** | **%s** |" % (len(diff_b), pct(len(diff_b))))
    L.append("| 库内无同题 | %d | %s |" % (len(no_bank), pct(len(no_bank))))
    L.append("| 本地文档强依据（稀有词覆盖≥50%%） | %d | %s |" % (len(doc_rows), pct(len(doc_rows))))
    L.append("| 强依据且与文件一致 | %d | %s |" % (len(agree_d), pct(len(agree_d))))
    L.append("| 强依据但与文件冲突 | %d | %s |" % (len(doc_rows) - len(agree_d), pct(len(doc_rows) - len(agree_d))))
    L.append("")
    L.append("> 库内答案来自本地题库；文档推导来自 187 份平台文档的检索 + 选项措辞打分。两条独立路径都对不上文件的题，才需要外部裁决。")
    L.append("")

    def section(title, rs):
        L.append(title)
        L.append("")
        L.append("| # | 题型 | 题干 | 文件答案 | 库内答案 | 文档推导 | 覆盖 | 依据文档 |")
        L.append("|---|---|---|---|---|---|---|---|")
        for r in rs:
            L.append("| %d | %s | %s | %s | %s | %s | %s | %s |" % (
                r["no"], r["type"] or "—", r["stem"][:34].replace("|", "/"),
                fmt(r["file_ans"]), fmt(r["bank_ans"]), fmt(r["derived"]),
                r["cov"], (r["doc"] or "—")[:24]))
        L.append("")

    section("## 二、库内答案与文件冲突（%d 道）" % len(diff_b), diff_b)
    section("## 三、文档强依据但与文件冲突（%d 道）" % (len(doc_rows) - len(agree_d)),
            [r for r in doc_rows if not r["same_doc"]])
    section("## 四、库内没有同题（%d 道）" % len(no_bank), no_bank)

    L.append("## 五、全表")
    L.append("")
    L.append("| # | 题型 | 题干 | 文件答案 | 库内 | 一致 | 文档推导 | 一致 | 覆盖 |")
    L.append("|---|---|---|---|---|---|---|---|---|")
    for r in rows:
        L.append("| %d | %s | %s | %s | %s | %s | %s | %s | %s |" % (
            r["no"], r["type"] or "—", r["stem"][:38].replace("|", "/"),
            fmt(r["file_ans"]), fmt(r["bank_ans"]),
            "Y" if r["same_bank"] else ("N" if r["same_bank"] is False else "—"),
            fmt(r["derived"]),
            "Y" if r["same_doc"] else ("N" if r["same_doc"] is False else "—"),
            r["cov"]))

    md = "\n".join(L)
    open(a.out, "w", encoding="utf-8").write(md)
    json.dump(rows, open(os.path.join(OUT, "exam_txt_compare.json"), "w", encoding="utf-8"),
              ensure_ascii=False, indent=1)
    print("\n".join(L[:36]))
    print("...\n报告 → %s" % a.out)
    db.close()
    return 0


if __name__ == "__main__":
    sys.exit(main())
