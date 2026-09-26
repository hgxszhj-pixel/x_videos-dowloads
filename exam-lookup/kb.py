#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
kb.py —— 本地知识库：把「官方原件 / 教材 / 专题附件」原文灌成可检索的 sqlite
============================================================
零依赖（标准库；PDF 走系统 pdftotext）。目录由脚本自身位置决定。

  python3 kb.py build                  重建索引（幂等，可反复跑）
  python3 kb.py search 有限空间 [-n 8] [--kind 官方原件|教材|专题附件|教材章节]
  python3 kb.py stats                  规模统计
  python3 kb.py selftest               自检（FAIL 会 exit 1）

索引：kb.sqlite3
  docs(id, src, kind, title, section, seq, text, chars, sha1)
  docs_fts(fts5, trigram, content=docs)   —— 中文 >=3 字走 bm25 排序 + 高亮摘录
  <3 字的词走 LIKE（trigram 最短 3 字符）

为什么要它：题库(qb.sqlite3)里只有题目和答案，原文（管理办法/YD5201/教材）原本
只散在 jr/out/ 下当文件躺着，搜不到。灌进来后核题可以直接按关键词回原文出处。
"""
import glob
import hashlib
import json
import os
import random
import re
import sqlite3
import subprocess
import sys

ROOT = os.path.dirname(os.path.abspath(__file__))
DB = os.path.join(ROOT, "kb.sqlite3")
CORPUS = os.path.join(ROOT, "jr/out/corpus/corpus.json")
PDFCACHE = os.path.join(ROOT, "jr/out/.pdftext")

# 来源清单：加一类来源 = 加一行。顺序决定去重时谁被保留（先到先留）
SOURCES = [
    # 保密教育（中国保密在线）考试用，和电信综维那套不是同一门：单独一个 kind，
    # 免得它落进电信覆盖率的分母/命中里（实测会把 docs-tianyi-security 虚抬 1 题）
    ("保密法规", "jr/out/baomi/法规/*.txt"),
    ("官方原件", "jr/out/官方原件/*.txt"),
    ("法规", "jr/out/法规/*.txt"),          # 公开法规原文（安全生产法等），命题的公共依据
    ("专题附件", "jr/out/sub-*/files/*.txt"),
    ("教材", "jr/out/pdf/*.txt"),
    ("教材汇编", "jr/out/综维教材文字汇编.txt"),
]
JSON_GUIDES = ["jr/out/sub-*/structure.json",        # 视频课 AI 提取文字（嵌套 guide[].c）
               "crawl/subjects/*.json"]              # kc 平台专题教材文字（同样嵌套 guide[].c）
PDF_GLOBS = ["jr/out/**/*.pdf", "crawl/attach/**/*.pdf"]
PPTX_GLOBS = ["crawl/attach/**/*.pptx"]              # 装维交付课件，platform 只给 pptx
CHUNK = 800          # 每块目标字数：够装上一条完整条文，又便于摘录
FTS_COLS = "title, section, flat"
FLAT = re.compile(r"\s+")   # PDF 抽取的原文里数字和词之间夹空格（「每 30 分钟」）


def flat(t):
    """检索用正文：去掉所有空白。原文「每 30 分钟」不这样的话搜「每30分钟」搜不到。"""
    return FLAT.sub("", t)


def norm(t):
    """去 PDF 换页符/零宽字符，压掉 3 行以上空行。"""
    t = re.sub(r"[\f\u200b\u00a0]", lambda m: "\n" if m.group(0) == "\f" else " ", t)
    return re.sub(r"\n{3,}", "\n\n", t).strip()


def page_starts(raw, txt):
    """在归一化后的整篇正文 txt 里定位每页起点 -> [(偏移, 页码)]。
    正文仍旧走 norm(raw) 整篇归一化：别改成「逐页归一化再拼」，那样块的切分点会变，
    跨来源的同文块就不再字节相同，去重直接失效 —— 实测块数会从 966 虚涨到 1325。
    页码用「每页前 40 字」做锚点顺序查找，找不到就退到前 12 字。"""
    starts, off = [], 0
    for i, pg in enumerate(raw.split("\f"), 1):
        t = norm(pg)
        if len(t) < 12:
            continue
        j = txt.find(t[:40], off)
        if j < 0:
            j = txt.find(t[:12], off)
        if j < 0:
            continue
        starts.append((j, i))
        off = j + 12
    return starts


def page_at(starts, off):
    """偏移 -> 页码；没有页码信息（txt/json 来源）返回 None。"""
    p = None
    for start, no in starts:
        if start <= off:
            p = no
        else:
            break
    return p


def names():
    """从各专题的 清单.json/metas.json 建 hash文件名 -> 标题 映射，让标题可读。"""
    m = {}
    for f in glob.glob(os.path.join(ROOT, "jr/out/**/清单.json"), recursive=True) + \
             glob.glob(os.path.join(ROOT, "jr/out/**/metas.json"), recursive=True):
        try:
            d = json.load(open(f, encoding="utf-8"))
        except Exception:
            continue
        items = d if isinstance(d, list) else d.get("data") or []

        def take(x, meta=None):
            meta = meta or (x.get("meta") or {})
            fn = x.get("file") or meta.get("filename") or ""
            title = x.get("section") or x.get("name") or meta.get("name") or ""
            ch = x.get("chapter") or ""
            if fn and title:
                m[os.path.splitext(os.path.basename(fn))[0]] = (ch, title)
        for x in items:
            if isinstance(x, dict):
                take(x)
    return m


def chunks(text, size=CHUNK):
    """按空行/换行切段再攒块，不切断段落。"""
    out, buf = [], ""
    for para in re.split(r"\n\s*\n", text):
        para = para.strip()
        if not para:
            continue
        if buf and len(buf) + len(para) + 2 > size:
            out.append(buf)
            buf = ""
        buf = (buf + "\n\n" + para) if buf else para
    if buf:
        out.append(buf)
    return out


def pdf_text(p):
    """PDF -> 文本：优先用旁边的同名 .txt，否则 pdftotext -layout 落到 .pdftext/。"""
    sib = os.path.splitext(p)[0] + ".txt"
    if os.path.exists(sib) and os.path.getsize(sib) > 2000:
        return sib                                 # 旁边的 txt 就是现成文本源，直接用它
        # 早先这里 return None（想「走 txt 那条线」），但 crawl/attach/** 下没有任何 txt 通配在收，
        # 于是「PDF + 同名 txt」这对文件会被整对静默丢掉（2026-09-22 灌网信安手册时实测踩到）
    os.makedirs(PDFCACHE, exist_ok=True)
    out = os.path.join(PDFCACHE, hashlib.sha1(p.encode()).hexdigest()[:12] + ".txt")
    if not os.path.exists(out) or os.path.getsize(out) < 2000:
        if not subprocess.run(["which", "pdftotext"], capture_output=True).stdout:
            return None
        subprocess.run(["pdftotext", "-layout", p, out], capture_output=True)
    return out if os.path.exists(out) and os.path.getsize(out) > 2000 else None


def pptx_text(p):
    """PPTX -> 文本：pptx 就是个 zip，直接抠 ppt/slides/slide*.xml 里的 <a:t> 文字。
    ponytail: 不解析版式/表格/备注，只取正文 runs；够用来做关键词回原文。"""
    import zipfile
    os.makedirs(PDFCACHE, exist_ok=True)
    out = os.path.join(PDFCACHE, hashlib.sha1(p.encode()).hexdigest()[:12] + ".txt")
    if not os.path.exists(out) or os.path.getsize(out) < 200:
        try:
            z = zipfile.ZipFile(p)
            pages = sorted((n for n in z.namelist()
                            if re.match(r"ppt/slides/slide\d+\.xml$", n)),
                           key=lambda n: int(re.findall(r"\d+", n)[0]))
            buf = []
            for n in pages:
                xml = z.read(n).decode("utf-8", "ignore")
                txt = "".join(re.findall(r"<a:t>(.*?)</a:t>", xml, re.S))
                txt = re.sub(r"<[^>]+>", "", txt).strip()
                if txt:
                    buf.append("[第 %s 页]\n%s" % (re.findall(r"\d+", n)[0], txt))
            z.close()
            open(out, "w", encoding="utf-8").write("\n\n".join(buf))
        except Exception:
            return None
    return out if os.path.exists(out) and os.path.getsize(out) > 200 else None


def doc_text(p):
    """PDF/PPTX -> 文本：按文件魔数分派，不信扩展名。
    平台下发的 .pptx 实际是转好的 PDF（file 一看就知道），照扩展名走会全落空。"""
    head = open(p, "rb").read(4)
    if head == b"%PDF":
        return pdf_text(p)
    if head[:2] == b"PK":
        return pptx_text(p)
    return None


def guides(o, out, section=""):
    """从嵌套 json 里捞出 (最近的小标题, 正文)：认 'c' 为正文、'n'/'name' 为标题。"""
    if isinstance(o, dict):
        nm = o.get("n") if isinstance(o.get("n"), str) else (o.get("name") if isinstance(o.get("name"), str) else section)
        if isinstance(o.get("c"), str) and len(o["c"]) > 60:
            out.append((nm or "", o["c"]))
        for v in o.values():
            guides(v, out, nm or section)
    elif isinstance(o, list):
        for v in o:
            guides(v, out, section)


def rows():
    """产出 (kind, src, title, section, text, page_starts) 源记录，去重交调用方。
    page_starts 是 [(该页起始偏移, 页码)]，只有 PDF 有；txt/json 来源给 None。"""
    nm = names()
    for kind, pat in SOURCES:
        for f in sorted(glob.glob(os.path.join(ROOT, pat))):
            if os.path.basename(f).startswith("_"):
                continue
            stem = os.path.splitext(os.path.basename(f))[0]
            ch, title = nm.get(stem, ("", stem))
            txt = norm(open(f, encoding="utf-8", errors="ignore").read())
            if len(txt) > 200:
                yield kind, os.path.relpath(f, ROOT), title or stem, ch, txt, None
    for pat in PDF_GLOBS:
        for f in sorted(glob.glob(os.path.join(ROOT, pat), recursive=True)):
            p = doc_text(f)
            if not p:
                continue
            stem = os.path.splitext(os.path.basename(f))[0]
            ch, title = nm.get(stem, ("", stem))
            raw = open(p, encoding="utf-8", errors="ignore").read()
            txt = norm(raw)
            if len(txt) > 200:
                yield "教材", os.path.relpath(f, ROOT), title or stem, ch, txt, page_starts(raw, txt)
    for pat in PPTX_GLOBS:
        for f in sorted(glob.glob(os.path.join(ROOT, pat), recursive=True)):
            p = doc_text(f)
            if not p:
                continue
            stem = os.path.splitext(os.path.basename(f))[0]
            ch, title = nm.get(stem, ("", stem))
            raw = open(p, encoding="utf-8", errors="ignore").read()
            txt = norm(raw)
            if len(txt) > 200:
                yield "课件", os.path.relpath(f, ROOT), title or stem, ch, txt, page_starts(raw, txt)
    if os.path.exists(CORPUS):
        for ch in json.load(open(CORPUS, encoding="utf-8")):
            for s in ch.get("sections") or []:                       # guide 挂在章节的 sections 下
                for g in s.get("guide") or []:
                    body = norm(g.get("c") or "")
                    if len(body) > 60:
                        yield "教材章节", "jr/out/corpus/corpus.json", \
                            "%s / %s" % (ch.get("chapter") or "", ch.get("name") or ""), \
                            g.get("n") or s.get("name") or "", body, None
    for pat in JSON_GUIDES:                     # 视频课提取文字：嵌套结构统一递归捞 guide[].c
        for f in sorted(glob.glob(os.path.join(ROOT, pat))):
            got = []
            guides(json.load(open(f, encoding="utf-8")), got)
            for section, body in got:
                yield "视频课件", os.path.relpath(f, ROOT), os.path.basename(f), section, norm(body), None


def build(fresh=True):
    c = sqlite3.connect(DB)
    cols = [r[1] for r in c.execute("PRAGMA table_info(docs)")]
    if cols and ("flat" not in cols or "page" not in cols):   # 旧版索引：列不齐就重建（索引可弃）
        c.execute("DROP TABLE docs")
    c.execute("CREATE TABLE IF NOT EXISTS docs(id INTEGER PRIMARY KEY, src TEXT, kind TEXT,"
              " title TEXT, section TEXT, seq INTEGER, page INTEGER, text TEXT, flat TEXT, chars INTEGER, sha1 TEXT)")
    c.execute("CREATE INDEX IF NOT EXISTS idx_docs_sha ON docs(sha1)")
    try:
        c.execute("DROP TABLE IF EXISTS docs_fts")     # 索引可弃：每次重建，避免 external-content 的 rebuild 不同步
        c.execute("CREATE VIRTUAL TABLE docs_fts USING fts5(%s, tokenize='trigram')" % FTS_COLS)
        fts = 1
    except sqlite3.OperationalError:                   # 老 sqlite 没 trigram -> 退回 LIKE
        fts = 0
    if fresh:
        c.execute("DELETE FROM docs")
    seen, n, dup = set(), 0, 0
    for kind, src, title, section, txt, starts in rows():
        off = 0
        for i, ch in enumerate(chunks(txt), 1):
            j = txt.find(ch, off)                      # 块在整篇里的偏移 -> 页码
            if j < 0:
                j = off
            off = j + len(ch)
            page = page_at(starts, j) if starts else None
            h = hashlib.sha1(ch.encode()).hexdigest()
            if h in seen:                              # 同文出现两次（官方原件 + 专题附件）
                dup += 1
                continue
            seen.add(h)
            n += 1
            c.execute("INSERT INTO docs(src,kind,title,section,seq,page,text,flat,chars,sha1)"
                      " VALUES (?,?,?,?,?,?,?,?,?,?)",
                      (src, kind, title, section, i, page, ch, flat(ch), len(ch), h))
            if fts:
                c.execute("INSERT INTO docs_fts(rowid,title,section,flat) VALUES (?,?,?,?)",
                          (c.execute("SELECT last_insert_rowid()").fetchone()[0], title, section, flat(ch)))
    c.execute("CREATE TABLE IF NOT EXISTS meta(k TEXT PRIMARY KEY, v TEXT)")
    c.execute("INSERT OR REPLACE INTO meta VALUES ('fts',?)", ("trigram" if fts else "like",))
    c.commit()
    tot = c.execute("SELECT COUNT(*), SUM(chars) FROM docs").fetchone()
    withpage = c.execute("SELECT COUNT(*) FROM docs WHERE page IS NOT NULL").fetchone()[0]
    c.close()
    print("索引重建完成：%d 块 / %s 字（去重跳过 %d 块；其中 %d 块带页码）-> %s\n检索：fts5-%s"
          % (tot[0], tot[1], dup, withpage, DB, "trigram" if fts else "LIKE(fallback)"))


def search(q, n=8, kind=None):
    q = flat(norm(q))
    if not q:
        return []
    c = sqlite3.connect(DB)
    kind_sql, kind_arg = ("AND kind=?", (kind,)) if kind else ("", ())
    sql = ("SELECT d.kind,d.title,d.section,d.src,%s, %s, d.page FROM docs_fts "
           "JOIN docs d ON d.id=docs_fts.rowid WHERE docs_fts MATCH ? %s ORDER BY 6 LIMIT ?")
    if len(q) >= 3:
        m = '"%s"' % q.replace('"', "")
        try:
            hit = c.execute(sql % ("snippet(docs_fts,2,'[',']',' … ',200)", "bm25(docs_fts,6.0,3.0,1.0)",
                                   kind_sql), (m,) + kind_arg + (n,)).fetchall()
        except sqlite3.OperationalError:
            hit = []
    else:
        hit = []
    if not hit:                                        # <3 字 / fts 命中空 -> 子串扫
        # LIKE 里的 % / _ / \ 是通配符，用户查询里带着就得转义，否则会把匹配放大
        like = "%%%s%%" % q.replace("\\", "\\\\").replace("%", "\\%").replace("_", "\\_")
        hit = c.execute(
            "SELECT kind,title,section,src, substr(flat, max(1,instr(flat,?)-70), 240), 0.0, page"
            " FROM docs WHERE (flat LIKE ? ESCAPE '\\' OR title LIKE ? ESCAPE '\\') %s LIMIT ?" % kind_sql,
            (q, like, like) + kind_arg + (n,)).fetchall()
    c.close()
    return hit


def stats():
    c = sqlite3.connect(DB)
    print("db: %s (%.1f MB)" % (DB, os.path.getsize(DB) / 1e6))
    for k, n, ch in c.execute("SELECT kind, COUNT(*), SUM(chars) FROM docs GROUP BY kind ORDER BY 2 DESC"):
        print("  %-6s %5d 块  %8d 字" % (k, n, ch))
    print("  合计   %5d 块" % c.execute("SELECT COUNT(*) FROM docs").fetchone()[0])
    c.close()


def selftest():
    """四条断言：够量 / 无重复 / 回环查得回该块且不误报 / <3 字不炸。"""
    c = sqlite3.connect(DB)
    n = c.execute("SELECT COUNT(*) FROM docs").fetchone()[0]
    assert n > 200, "文档块太少：%d" % n
    d = c.execute("SELECT COUNT(*) - COUNT(DISTINCT sha1) FROM docs").fetchone()[0]
    assert d == 0, "有 %d 块重复未去重" % d
    rows = [(i, f) for i, f in c.execute("SELECT id,flat FROM docs")
            if re.search(r"[\u4e00-\u9fa5]{10,}", f)]
    random.Random(42).shuffle(rows)
    for rid, text in rows[:5]:                              # 回环：拿原文片段反查该块
        probe = max(re.findall(r"[\u4e00-\u9fa5]{10,}", text), key=len)[:12]
        ids = [r[0] for r in c.execute(
            "SELECT rowid FROM docs_fts WHERE docs_fts MATCH ?", ('"%s"' % probe,))]
        assert rid in ids, "回环查不到该块：%r" % probe
        junk = [i for i in ids if probe not in
                (c.execute("SELECT flat FROM docs WHERE id=?", (i,)).fetchone() or [""])[0]]
        assert not junk, "短语查询误报（命中了不含该片段的块）：%s" % junk
    hit = search("每30分钟复测气体", 5)                      # 端到端：原文「每 30 分钟」也要搜得到
    assert hit and any("管理办法" in h[3] for h in hit), "数字夹空格的原文搜不到"
    assert search("有限") is not None                        # 2 字走 LIKE，不许抛错
    c.close()
    print("selftest OK：%d 块，去重干净，5/5 回环命中且零误报，空格式差异可查，2 字短词可查" % n)


def main():
    if len(sys.argv) < 2:
        print(__doc__)
        return
    cmd = sys.argv[1]
    if cmd == "build":
        build()
    elif cmd == "stats":
        stats()
    elif cmd == "selftest":
        selftest()
    elif cmd == "search":
        q = sys.argv[2]
        n = int(sys.argv[sys.argv.index("-n") + 1]) if "-n" in sys.argv else 8
        kind = sys.argv[sys.argv.index("--kind") + 1] if "--kind" in sys.argv else None
        hit = search(q, n, kind)
        for i, (k, t, s, src, snip, score, page) in enumerate(hit, 1):
            print("\n[%d] %s ｜ %s" % (i, k, t))
            if s:
                print("    节: %s" % s)
            print("    源: %s%s" % (src, ("　第 %s 页" % page) if page else ""))
            print("    %s" % re.sub(r"\s+", " ", snip))
        if not hit:
            print("（无命中）")
    else:
        print(__doc__)


if __name__ == "__main__":
    main()
