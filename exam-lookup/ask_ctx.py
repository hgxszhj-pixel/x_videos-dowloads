#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""ask_ctx —— 给「模型来答题」准备证据包（给 Claude Code / 任何 LLM 用）

为什么要它：题目常常不考「原文那一句」，而是把教材里的六个要素打散成排序题、
把法规条款改写成选择题。字面检索（bigram）答不了这类题，但模型读得懂。

它不判定答案，只把「能支撑判断的材料」按可核对的方式摆出来：

    python3 ask_ctx.py "题目原文（含选项）"            # 人读 / 直接塞给模型
    python3 ask_ctx.py "..." --json                    # 机器读（含结构化字段）
    python3 ask_ctx.py -f 题目.txt                     # 一次多题（空行或题号分行）

输出四块：
    ① 题库命中        本地已核验的答案（有就直接用，别猜）
    ② 教材/课件段落    kb.sqlite3 里的原文块，带书名/章节/页码
    ③ 平台文档/法规    docs.sqlite 里的条文，带文件与字符位置
    ④ 原文文件命中     docs_new_text 等纯文本里命中的片段（原卷/宣贯要点常在这里）
外加「选项 × 证据」的对照，方便模型判断哪个选项有原文支撑。
"""
import argparse
import json
import os
import re
import sys
import time

ROOT = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, ROOT)

import kb                                                       # noqa: E402
import kb_db                                                   # noqa: E402
import solve                                                   # noqa: E402

TEXT_DIRS = [os.path.join(ROOT, "jr/out/docs_new_text")]
STEM_LIMIT = 400


def _stem(q):
    s = solve.stem_of(q) or q
    return s.strip()[:STEM_LIMIT]


def _grams(s):
    s = re.sub(r"[^\u4e00-\u9fa5A-Za-z0-9]", "", s or "")
    return {s[i:i + 3] for i in range(len(s) - 2)}


def bank_hit(q):
    row, score = solve.lookup(q)
    if not row:
        return None
    return {"qid": row[0], "type": row[2], "question": row[3], "answer": row[6],
            "answerText": row[7], "analysis": (row[8] or "")[:300], "src": row[9],
            "score": round(score, 3)}


def blank_labels(q):
    """从「（）是核心、（）是动力」里抽出空位标签，排序题/填空题全靠它。"""
    raw = re.findall(r"[（(]\s*[)）]\s*(?:是|为)?\s*([\u4e00-\u9fa5]{2,6})", q or "")
    out = []
    for x in raw:
        x = re.split(r"[的之与和、，。；]", x)[0]
        if 2 <= len(x) <= 4 and x not in out:
            out.append(x)
    return out[:8]


def kb_passages(q, n=6):
    """教材/课件（kb.sqlite3）：按题干主干取段落；排序/填空题再按「选项 × 空位标签」补搜。"""
    out, seen = [], set()
    _, opts = _parse_opts(q)
    labels = blank_labels(q)
    # 先补「选项 × 空位」：排序题要的正是「X 是核心」这种映射，光搜题干主干搜不到
    for o in opts[:6]:
        for lab in labels[:6]:
            probe = ("%s %s" % (o["text"], lab)).strip()
            try:
                rows = kb.search(probe, 2)
            except Exception:
                rows = []
            for row in rows:
                kind, title, section, src, text, _x, page = (list(row) + [None] * 7)[:7]
                t = re.sub(r"\s+", " ", text or "")[:400]
                if t and not any(x["text"][:60] == t[:60] for x in out):
                    out.append({"kind": kind, "title": title, "section": section, "src": src,
                                "page": page, "text": t, "via": probe})
    out = out[:n]
    for probe in ([solve.query_of(_stem(q), 40)] + [m[:k] for m in re.findall(r"[\u4e00-\u9fa5]{4,}", _stem(q)) for k in (12, 8, 6)])[:5]:
        probe = (probe or "").strip()
        if len(probe) < 4 or probe in seen:
            continue
        seen.add(probe)
        for row in kb.search(probe, n):
            kind, title, section, src, text, _extra, page = (list(row) + [None] * 7)[:7]
            item = {"kind": kind, "title": title, "section": section, "src": src,
                    "page": page, "text": re.sub(r"\s+", " ", text or "")[:500]}
            if not any(x["text"][:80] == item["text"][:80] for x in out):
                out.append(item)
        if len(out) >= n:
            break
    return out[:n]


def docs_passages(q, n=4):
    """平台文档/法规（docs.sqlite）。"""
    db = kb_db.connect(readonly=True)
    try:
        hits = kb_db.search_with_cite(db, _stem(q), topk=n)
    finally:
        db.close()
    return [{"doc": h["doc"], "course": h["course"], "domain": h["domain"],
             "text": re.sub(r"\s+", " ", h["text"] or "")[:500]} for h in hits]


_FILE_CACHE = {}                     # path -> (mtime, size, txt, grams)


def _file_txt_grams(path):
    """按 (mtime,size) 缓存每个原件的文本与 3-gram 集合。

    踩过：原来每个问题都把全部原件读一遍再算 gram，收编页一页 17 条要 7.6s；
    原件不变时 grams 完全可复用，现在只有第一次读盘。
    """
    try:
        st = os.stat(path)
    except OSError:
        return None, set()
    key = (st.st_mtime, st.st_size)
    hit = _FILE_CACHE.get(path)
    if hit and hit[0] == key:
        return hit[1], hit[2]
    try:
        txt = open(path, encoding="utf-8", errors="ignore").read()
    except OSError:
        return None, set()
    val = (key, txt, _grams(txt))
    if len(_FILE_CACHE) > 2000:
        _FILE_CACHE.clear()
    _FILE_CACHE[path] = val
    return txt, val[2]


def file_hits(q, limit=3):
    """纯文本原件（原卷 txt / 宣贯要点）：按 3-gram 命中率排序，给片段与文件名。"""
    g = _grams(_stem(q))
    if not g:
        return []
    scored = []
    for d in TEXT_DIRS:
        if not os.path.isdir(d):
            continue
        for fn in sorted(os.listdir(d)):
            if not fn.endswith(".txt"):
                continue
            txt, grams = _file_txt_grams(os.path.join(d, fn))
            if txt is None:
                continue
            score = len(g & grams) / len(g)
            if score >= 0.5:
                scored.append((score, fn, txt))
    scored.sort(reverse=True, key=lambda x: x[0])
    out = []
    for score, fn, txt in scored[:limit]:
        i = txt.find(_stem(q)[:12])
        if i < 0:
            anchor = next((x for x in g if x in txt), "")
            i = txt.find(anchor) if anchor else 0
        snippet = re.sub(r"\s+", " ", txt[max(0, i - 120): i + 420])
        out.append({"file": fn, "score": round(score, 2), "snippet": snippet})
    return out


_FLAT = {"t": 0.0, "files": []}


def _flat_corpus(ttl=120):
    """把原件文本去空白后整库缓存（120s）。批量 20 题时省掉 20 次全盘扫描。"""
    if _FLAT["files"] and time.time() - _FLAT["t"] < ttl:
        return _FLAT["files"]
    files = []
    for d in TEXT_DIRS:
        if not os.path.isdir(d):
            continue
        for fn in sorted(os.listdir(d)):
            if fn.endswith(".txt"):
                try:
                    files.append((fn, re.sub(r"\s+", "", open(os.path.join(d, fn),
                                  encoding="utf-8", errors="ignore").read())))
                except OSError:
                    pass
    _FLAT.update(t=time.time(), files=files)
    return files


def mapping_hits(q, limit=12):
    """排序/填空题的决定性证据：原文里「X 是 核心」这类映射句。

    做法：把原件文本去掉所有空白后，直接找「选项原文 + 是/为 + 空位标签」。
    比模糊重合可靠——命中就能指着原句说「核心=科技创新」。
    """
    _, opts = _parse_opts(q)
    labels = blank_labels(q)
    if not opts or not labels:
        return []
    files = _flat_corpus()
    out, seen = [], set()
    for o in opts:
        for lab in labels:
            pat = re.compile(re.escape(o["text"]) + r"(?:" + re.escape(lab) + r"|是" + re.escape(lab) + r")")
            for fn, flat in files:
                m = pat.search(flat)
                if not m:
                    continue
                key = (o["key"], lab)
                if key in seen:
                    continue
                seen.add(key)
                out.append({"key": o["key"], "option": o["text"], "label": lab,
                            "file": fn, "quote": flat[max(0, m.start() - 40): m.end() + 40]})
                break
        if len(out) >= limit:
            break
    return out


def verbatim_answer(q):
    """排序/填空题：六个空位都被原文映射覆盖时，直接拼出答案串。

    返回 (答案 或 None, 命中明细)。拼不出（有空位没证据）就返回 (None, []),
    交给后面的联网/模型兜底——宁可不答，也不半答。
    """
    labels = blank_labels(q)
    if len(labels) < 2:
        return None, []
    maps = mapping_hits(q)
    by_label = {}
    for m in maps:
        by_label.setdefault(m["label"], m)
    if any(lab not in by_label for lab in labels):
        return None, []
    return "".join(by_label[lab]["key"] for lab in labels), [by_label[lab] for lab in labels]


def option_matrix(q, evidences):
    """选项 × 证据：每个选项的文字在证据里出现没有 / 重合多少（只给事实，不判定）。"""
    _, opts = _parse_opts(q)
    if not opts:
        return []
    blob = " ".join(e.get("text") or e.get("snippet") or "" for e in evidences)
    bg = _grams(blob)
    rows = []
    for o in opts:
        og = _grams(o["text"])
        hit = len(og & bg) / len(og) if og else 0.0
        rows.append({"key": o["key"], "text": o["text"], "overlap": round(hit, 2),
                     "verbatim": o["text"] in blob})
    return rows


def _parse_opts(q):
    return _parse_options(q)


def _parse_options(text):
    text = (text or "").strip()
    marks = [(m.start(), m.end(), m.group(1))
             for m in re.finditer(r"(?<![A-Za-z0-9A-Z])([A-F])[.、．]\s*", text)]
    if len(marks) < 2:
        return text, []
    order, seq = "ABCDEF", [marks[0]]
    for m in marks[1:]:
        if order.index(m[2]) == order.index(seq[-1][2]) + 1:
            seq.append(m)
    if len(seq) < 2:
        return text, []
    stem = text[:seq[0][0]].strip(" \n，,。;；")
    opts = []
    for i, (s0, e0, k) in enumerate(seq):
        end = seq[i + 1][0] if i + 1 < len(seq) else len(text)
        val = text[e0:end].strip(" \n，,。;；")
        if val:
            opts.append({"key": k, "text": val})
    return stem, opts


def pack(q):
    hit = bank_hit(q)
    kb_p = kb_passages(q)
    docs_p = docs_passages(q)
    files = file_hits(q)
    maps = mapping_hits(q)
    evidences = [{"text": p["text"]} for p in kb_p] + \
                [{"text": p["text"]} for p in docs_p] + \
                [{"snippet": f["snippet"]} for f in files]
    return {"question": q, "stem": _stem(q), "bank": hit,
            "kb": kb_p, "docs": docs_p, "files": files, "maps": maps,
            "options": option_matrix(q, evidences)}


def render(p):
    """给模型/人看的纯文本。控制长度（模型上下文友好）。"""
    L = []
    L.append("【问题】" + re.sub(r"\s+", " ", p["stem"])[:300])
    L.append("")
    b = p["bank"]
    L.append("【① 本地题库】" + ("命中：" + b["answer"] + " " + (b["answerText"] or "") +
                              "（来源 " + (b["src"] or "") + "，相似度 %.2f）" % b["score"]
                              if b else "未命中（题库里没有这道题）"))
    if b and b.get("analysis"):
        L.append("    解析：" + re.sub(r"\s+", " ", b["analysis"])[:200])
    L.append("")
    L.append("【② 教材/课件段落】" + ("共 %d 段" % len(p["kb"]) if p["kb"] else "无命中"))
    for i, it in enumerate(p["kb"], 1):
        L.append("  [%d] %s / %s%s" % (i, it["title"], it["section"],
                                       ("（第 %s 页）" % it["page"]) if it.get("page") else ""))
        L.append("      " + it["text"][:300])
        if it.get("via"):
            L.append("      （检索词：%s）" % it["via"])
    L.append("")
    L.append("【③ 平台文档/法规】" + ("共 %d 段" % len(p["docs"]) if p["docs"] else "无命中"))
    for i, it in enumerate(p["docs"], 1):
        L.append("  [%d] %s（%s）" % (i, it["doc"], it["domain"]))
        L.append("      " + it["text"][:300])
    L.append("")
    L.append("【④ 原件文本命中】" + ("共 %d 个文件" % len(p["files"]) if p["files"] else "无命中"))
    for i, f in enumerate(p["files"], 1):
        L.append("  [%d] %s（3-gram 命中 %.0f%%）" % (i, f["file"], f["score"] * 100))
        L.append("      " + f["snippet"][:400])
    L.append("")
    if p.get("maps"):
        L.append("【⑤ 空位 × 选项 原文直接命中】（排序/填空题就看这块）")
        for m in p["maps"]:
            L.append("  %s. %s  →  %s" % (m["key"], m["option"], m["label"]))
            L.append("      原文：…%s…（%s）" % (m["quote"], m["file"][:60]))
        L.append("")
    if p["options"]:
        L.append("【⑥ 选项 × 证据】")
        for o in p["options"]:
            L.append("  %s. %-28s 原文出现：%s｜字面重合：%.0f%%"
                     % (o["key"], o["text"][:28], "是" if o["verbatim"] else "否", o["overlap"] * 100))
    L.append("")
    L.append("【给模型的指令】只根据上面材料作答：给出答案字母与原文依据（引用到文件/章节/页码）；")
    L.append("材料不足以判断时，直接说「证据不足」，不要凭常识硬猜。")
    return "\n".join(L)


def selftest():
    q = ("云改数转智惠战略，是党建统领的战略，我们要以客户为中心，把握（）是核心、（）是动力、"
         "（）是根本、（）是基础、（）是底色、（）是枢纽的内涵要义。（排序题(10分)）"
         "A. 云网融合 B. 改革开放 C. 科技创新 D. 干部人才 E. 数智化平台 F. 绿色安全")
    p = pack(q)
    got = {}
    for m in p["maps"]:
        got.setdefault(m["label"], m["key"])
    want = {"核心": "C", "动力": "B", "根本": "D", "基础": "A", "底色": "F", "枢纽": "E"}
    assert got == want, "映射不符：%s != %s" % (got, want)
    order = "".join(want[lab] for lab in ["核心", "动力", "根本", "基础", "底色", "枢纽"])
    print("ask_ctx selftest 通过：空位映射 %s → 正确顺序 %s（原文直证 6/6）" % (got, order))
    return 0


def main():
    ap = argparse.ArgumentParser(description="为模型准备查题证据包")
    ap.add_argument("question", nargs="?", help="题目原文（含选项）")
    ap.add_argument("-f", "--file", help="题目文件（空行或题号分行）")
    ap.add_argument("--json", action="store_true", help="输出 JSON")
    ap.add_argument("--selftest", action="store_true", help="自检：排序题空位映射")
    a = ap.parse_args()
    if a.selftest:
        return selftest()
    if a.file:
        raw = open(os.path.expanduser(a.file), encoding="utf-8", errors="ignore").read()
        items = [x.strip() for x in re.split(r"\n\s*\n", raw) if x.strip()]
        for i, q in enumerate(items, 1):
            print("\n" + "=" * 20 + " 第 %d 题 " % i + "=" * 20)
            print(render(pack(q)))
        return 0
    q = a.question or (sys.stdin.read().strip() if not sys.stdin.isatty() else "")
    if not q:
        ap.print_help()
        return 2
    p = pack(q)
    print(json.dumps(p, ensure_ascii=False, indent=1) if a.json else render(p))
    return 0


if __name__ == "__main__":
    sys.exit(main())
