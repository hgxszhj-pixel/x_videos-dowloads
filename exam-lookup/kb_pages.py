#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
kb_pages.py —— 教材「页码索引」+「考点 → 原文页码」对照表
============================================================
零依赖（标准库；PDF 走系统 pdftotext -layout，按 \\f 切页）。

  python3 kb_pages.py index     每本 PDF 的 章节标题 → 页码  -> crawl/教材页码索引.md
  python3 kb_pages.py map [S] [R]  题库每道题 → 教材文件+页码+原文 -> crawl/考点页码对照.md / .csv
                                   默认 S=1.2 R=1（调大更准、命中更少）
  python3 kb_pages.py all       两个都跑
  python3 kb_pages.py selftest  自检（FAIL 会 exit 1）

为什么：kb.py 解决「按关键词回原文」，但不知道在第几页；刷题时要能直接翻书对照。
"""
import glob
import hashlib
import json
import os
import re
import sqlite3
import subprocess
import sys

ROOT = os.path.dirname(os.path.abspath(__file__))
CACHE = os.path.join(ROOT, "jr/out/.pdftext")
QB = os.path.join(ROOT, "qb.sqlite3")
PDF_GLOBS = ["jr/out/pdf/*.pdf", "jr/out/sub-*/files/*.pdf", "jr/out/官方原件/*.pdf",
             "crawl/attach/*/*.pdf", "crawl/attach/*/*.pptx"]   # 官方原件=命题主依据，必须有页码
OUT_INDEX = os.path.join(ROOT, "crawl/教材页码索引.md")
OUT_MAP = os.path.join(ROOT, "crawl/考点页码对照.md")
OUT_CSV = os.path.join(ROOT, "crawl/考点页码对照.csv")

HEAD = re.compile(r"^\s*(第[一二三四五六七八九十百\d]{1,3}[篇章节]|[0-9]{1,2}(\.[0-9]{1,2}){0,2}[\s、.]|（[一二三四五六七八九十]{1,2}）)")


def flat(t):
    return re.sub(r"\s+", "", t)


def pages(pdf):
    """PDF -> [每页文本]；缓存 json，避免每次重抽。"""
    os.makedirs(CACHE, exist_ok=True)
    out = os.path.join(CACHE, "pages-" + hashlib.sha1(pdf.encode()).hexdigest()[:12] + ".json")
    if os.path.exists(out) and os.path.getsize(out) > 50:
        try:
            return json.load(open(out, encoding="utf-8"))
        except Exception:
            pass
    if not subprocess.run(["which", "pdftotext"], capture_output=True).stdout:
        return []
    raw = subprocess.run(["pdftotext", "-layout", pdf, "-"], capture_output=True).stdout.decode("utf-8", "ignore")
    ps = [re.sub(r"[ \t]+", " ", x).strip() for x in raw.split("\f")]
    json.dump(ps, open(out, "w", encoding="utf-8"), ensure_ascii=False)
    return ps


def books():
    """(相对路径, [页文本])，非 PDF（真 pptx）跳过。"""
    import kb as _kb                       # 复用 kb.py 的「hash 文件名 -> 可读标题」映射
    nm = _kb.names()
    for pat in PDF_GLOBS:
        for f in sorted(glob.glob(os.path.join(ROOT, pat))):
            try:
                if open(f, "rb").read(4) != b"%PDF":
                    continue
            except Exception:
                continue
            ps = pages(f)
            if ps and sum(len(x) for x in ps) > 500:
                stem = os.path.splitext(os.path.basename(f))[0]
                ch, title = nm.get(stem, ("", ""))
                label = (title or stem) + (("（%s）" % ch) if ch and ch not in (title or "") else "")
                yield label + " ┄ " + os.path.relpath(f, ROOT), ps


def cmd_index():
    lines, nb = ["# 教材页码索引", "", "生成：`python3 kb_pages.py index`（pdftotext -layout 按页切）", ""], 0
    for name, ps in books():
        nb += 1
        hits = []
        for i, pg in enumerate(ps, 1):
            for ln in pg.split("\n"):
                if len(ln.strip()) >= 4 and HEAD.match(ln) and len(ln.strip()) < 60:
                    hits.append((i, ln.strip()))
        lines += ["## %s（%d 页，%d 个标题）" % (name, len(ps), len(hits)), ""]
        lines += ["| 页 | 标题 |", "|---|---|"] + ["| %d | %s |" % (p, t.replace("|", "/")) for p, t in hits[:200]]
        lines.append("")
    open(OUT_INDEX, "w", encoding="utf-8").write("\n".join(lines))
    print("页码索引：%d 本 -> %s" % (nb, OUT_INDEX))


def grams(s, n=4):
    """把题干+答案切成一堆 4 字窗，用来做近似命中（原样整串匹配太苛刻，3% 命中率）。"""
    s = re.sub(r"[^\u4e00-\u9fa5A-Za-z0-9]+", "", s or "")
    return {s[i:i + n] for i in range(max(0, len(s) - n + 1))}


def cmd_map(score_min=1.2, rare_min=1):
    """题干+答案 → 教材文件+页码。用 4 字窗 + 逆文档频率加权：泛词（「中国电信」）权重低，
    只在少数页出现的窗（「伸缩梯防滑垫」）权重高，避免把无关页判成命中。"""
    import math
    c = sqlite3.connect(QB)
    qs = c.execute("SELECT id,num,question,answerText,src,chapter FROM q ORDER BY src,num").fetchall()
    c.close()

    books_ = []
    for n, ps in books():
        books_.append({"name": n, "raw": ps, "flat": [flat(x) for x in ps]})

    G = set()
    qgrams = []
    for qid, num, q, at, src, ch in qs:
        ga = grams(at, 4)                       # 答案原文的窗口
        gq = grams(q, 4)                        # 题干窗口
        qgrams.append((gq | ga, ga))
        G |= (gq | ga)

    df = {}
    for b in books_:                                     # 每页只留「题目里出现过的 4 字窗」
        pgs = []
        for i, pg in enumerate(b["flat"]):
            s = {pg[j:j + 4] for j in range(max(0, len(pg) - 3))} & G
            pgs.append(s)
            for x in s:
                df[x] = df.get(x, 0) + 1
        b["pgs"] = pgs

    rows, hit50, n50 = [], 0, 0
    for (qid, num, q, at, src, ch), (g, ga) in zip(qs, qgrams):
        score, rare, mcount = {}, {}, {}
        for b in books_:
            for i, s in enumerate(b["pgs"]):             # 逐页比对：不能用 str.find，它总返回首次出现（偏向前面的目录页）
                m = s & g
                if not m:
                    continue
                k = (b["name"], i + 1)
                for x in m:
                    w = 2.0 if x in ga else 1.0           # 答案原句出现 = 更强的证据
                    score[k] = score.get(k, 0.0) + w / (1.0 + math.log(1 + df.get(x, 99)))
                    if df.get(x, 99) <= 3:
                        rare[k] = rare.get(k, 0) + 1
                mcount[k] = len(m)
        best = None
        if score:
            k = max(score, key=lambda kk: (rare.get(kk, 0), score[kk], mcount[kk]))
            if rare.get(k, 0) >= rare_min and score[k] >= score_min:
                nm, pg = k
                raw = [b for b in books_ if b["name"] == nm][0]["raw"][pg - 1]
                best = (nm, pg, re.sub(r"\s+", " ", raw)[:140], mcount[k], rare[k])
        if best:
            rows.append((qid, (q or "")[:26], (at or "")[:22], best[0], best[1], best[2][:70], "%d(稀有%d)" % (best[3], best[4])))
        else:
            rows.append((qid, (q or "")[:26], (at or "")[:22], "", "", "", ""))
        if src == "comm-safety-50.json":
            n50 += 1
            hit50 += 1 if best else 0
    ok = sum(1 for r in rows if r[3])
    lines = ["# 考点 → 教材页码对照", "",
             "生成：`python3 kb_pages.py map`　|　题库 %d 题，命中教材原文 **%d** 题（%.0f%%）"
             % (len(rows), ok, 100.0 * ok / max(1, len(rows))), "",
             "命中判据：题干+答案切 4 字窗；窗口按逆文档频率加权（泛词低权重），",
             "最高分页需至少含 1 个「全库不超过 3 页」的稀有窗，且加权分 ≥1.5。",
             "「命中词数(稀有)」= 该页命中的窗数，括号内是其中稀有窗数。",
             "",
             "注意：页码是「该题关键词最集中的那一页」，不等于条文所在页（判断题答案是「正确/错误」，只能靠题干定位）。",
             "要原文出处请用 `python3 kb.py search <关键词>` 看摘录，本表用来缩小翻书范围。", "",
             "| 题目 id | 题干 | 答案 | 教材 | 页 | 原文片段 | 命中词数(稀有) |", "|---|---|---|---|---|---|---|"]
    lines += ["| %s | %s | %s | %s | %s | %s | %s |" % r for r in rows]
    open(OUT_MAP, "w", encoding="utf-8").write("\n".join(lines) + "\n")
    with open(OUT_CSV, "w", encoding="utf-8") as fh:
        fh.write("id,stem,answer,book,page,snippet,grams\n")
        for r in rows:
            fh.write(",".join('"%s"' % str(x).replace('"', "'") for x in r) + "\n")
    print("考点对照：%d 题，命中教材原文 %d 题（%.0f%%）；comm-safety-50 命中 %d/%d"
          % (len(rows), ok, 100.0 * ok / max(1, len(rows)), hit50, n50))
    print("  -> %s\n  -> %s" % (OUT_MAP, OUT_CSV))


def selftest():
    bs = list(books())
    assert bs, "没有可读的 PDF"
    total = sum(len(ps) for _, ps in bs)
    assert total > 500, "总页数太少：%d" % total
    # 取一页里的长中文段反查：必须能定位回同一本书同一页
    name, ps = bs[0]
    for i, pg in enumerate(ps):
        m = re.findall(r"[\u4e00-\u9fa5]{12,}", pg)
        if m:
            probe = m[0][:12]
            found = any(flat(probe) in flat(x) for x in ps)
            assert found, "回环失败：%r" % probe
            print("selftest OK：%d 本 / %d 页；回环命中 %s 第 %d 页「%s」" % (len(bs), total, name, i + 1, probe))
            break
    else:
        raise AssertionError("第一本里没找到可回环的长段落")


if __name__ == "__main__":
    cmd = sys.argv[1] if len(sys.argv) > 1 else ""
    if cmd == "index":
        cmd_index()
    elif cmd == "map":
        cmd_map(*[float(x) for x in sys.argv[2:4]])
    elif cmd == "all":
        cmd_index()
        cmd_map()
    elif cmd == "selftest":
        selftest()
    else:
        print(__doc__)
