#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
kb_coverage.py —— 量「依据覆盖率」：每道题的题干拿去 kb 依据库里找官方原文，命中算有依据
=========================================================================================
  python3 kb_coverage.py              # 默认探针 ≥6 字（精确版，D-025 定的口径）
  python3 kb_coverage.py 4            # 降到 4 字窗（会出假依据，只用于对照）
  python3 kb_coverage.py 6 --bank comm-safety-50
  python3 kb_coverage.py 6 --bank baomi --include-external   # 跨门考试题库（保密教育）要显式带上
  python3 kb_coverage.py 6 --bank baomi --include-external --write   # 另存逐题法条对照

复用 webui.cite 的探针阶梯（query_of -> 12/10/8/6 字窗），逻辑不重写。
只用题库里的题干，不碰答案 —— 量的是「语料够不够」，不是「答案对不对」。
"""
import glob
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

import kb                                                     # noqa: E402
import solve                                                  # noqa: E402

CJK = re.compile(r"[\u4e00-\u9fa5]{2,}")


def probes_of(stem, min_len):
    """探针阶梯：整段截断 -> 12/10/8/…/min_len 字窗，长探针在前（宁缺勿假）。"""
    lens = [k for k in (12, 10, 8, 6, 5, 4) if k >= min_len]
    return [solve.query_of(stem, 40)] + [m[:k] for m in CJK.findall(stem) for k in lens]


def cite(stem, min_len=6, n=3, exclude_kind=None):
    """exclude_kind：按 kind 排掉某一类语料（如跨门考试的「保密法规」）。
    多取一些再过滤，否则一排就空，会假报「没依据」。"""
    seen = set()
    for p in probes_of(stem, min_len):
        p = p.strip()
        if len(p) < min_len or p in seen:
            continue
        seen.add(p)
        out = kb.search(p, 40 if exclude_kind else n)
        if exclude_kind:
            out = [d for d in out if d[0] != exclude_kind][:n]
        if out:
            return p, out
    return "", []


def main():
    argv = [a for a in sys.argv[1:]]
    min_len = 6
    if argv and argv[0].isdigit():
        min_len = int(argv[0])
        argv.pop(0)
    only = None
    stem_only = "--stem-only" in argv
    ext = "--include-external" in argv          # 默认跳过 external（跨门考试），要量就显式带上
    write = "--write" in argv
    exk = argv[argv.index("--exclude-kind") + 1] if "--exclude-kind" in argv else None
    if "--bank" in argv:
        only = argv[argv.index("--bank") + 1]

    files = sorted(glob.glob(os.path.join(HERE, "banks", "*.json")))
    total = hit_total = 0
    for f in files:
        name = os.path.basename(f)[:-5]
        if only and only not in name:
            continue
        j = json.load(open(f, encoding="utf-8"))
        if isinstance(j, dict) and ((j.get("meta") or {}).get("mergeOf") or (j.get("meta") or {}).get("generated")
                       or (j.get("meta") or {}).get("derived")
                       or ((j.get("meta") or {}).get("external") and not ext)):
            continue          # 合集是「视图」、生成题不是真题、external 是跨门考试：计入都会虚抬覆盖率
        qs = j["questions"] if isinstance(j, dict) and "questions" in j else j
        hit, miss = [], []
        for q in qs:
            # 和查题页一致：用户粘进来的通常是整题（题干 + 选项），选项里常含官方措辞
            stem = (q.get("question") or "").strip()
            if not stem:
                continue
            text = stem
            if not stem_only:
                opt = q.get("options") or []
                text += " " + " ".join((o.get("text") or o) if isinstance(o, dict) else str(o) for o in opt)
            p, docs = cite(text, min_len, exclude_kind=exk)
            (hit if docs else miss).append((q.get("number"), stem, p, docs))
        total += len(qs)
        hit_total += len(hit)
        print("\n=== %s：%d/%d 有原文依据（%d%%）" % (name, len(hit), len(qs), round(100.0 * len(hit) / max(1, len(qs)))))
        for num, stem, p, docs in miss:
            print("    ✗ 第%s题 %s%s" % (num, stem[:46], "" if not p else "  [探针 %r 无命中]" % p))
        if write:
            out = os.path.join(HERE, "crawl", "%s-法条对照.md" % name)
            with open(out, "w", encoding="utf-8") as fh:
                fh.write("# %s：每道题对应的官方原文（探针 ≥%d 字）\n\n" % (name, min_len))
                fh.write("生成：`python3 kb_coverage.py %d --bank %s --include-external --write`\n\n" % (min_len, name))
                fh.write("命中 %d/%d（%d%%）。「依据」单元格是该题能命中的**原文出处**；没命中的直接标「未命中」。\n\n"
                         % (len(hit), len(qs), round(100.0 * len(hit) / max(1, len(qs)))))
                fh.write("> 看这一列要带点心眼：探针最短到 6 字，6 字窗可能落到隔壁条款上；"
                         "命中的摘录若是「（1988年…第…次会议通过）」这种**文件开头**，基本是误命中，别当依据用。\n\n")
                fh.write("| 题号 | 题型 | 题干（截断） | 依据出处 | 原文摘录 |\n|---|---|---|---|---|\n")
                for num, stem, p, docs in hit:
                    d = docs[0]
                    kind, title, section, src, snip = d[0], d[1], d[2], d[3], re.sub(r"\s+", " ", d[4])[:90]
                    fh.write("| %s | | %s | %s ｜ %s | %s |\n" % (num, stem[:40].replace("|", "\\|"),
                                                                  src.split("/")[-1][:28].replace("|", "\\|"),
                                                                  (section or title or "")[:26].replace("|", "\\|"),
                                                                  snip.replace("|", "\\|")))
                for num, stem, p, docs in miss:
                    fh.write("| %s | | %s | 未命中 | — |\n" % (num, stem[:40].replace("|", "\\|")))
            print("    → 逐题对照已写入 crawl/%s-法条对照.md" % name)
    print("\n合计：%d/%d = %d%%（探针 ≥%d 字）" % (hit_total, total, round(100.0 * hit_total / max(1, total)), min_len))


if __name__ == "__main__":
    main()
