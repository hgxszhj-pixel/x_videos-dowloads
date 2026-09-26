#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
webui.py —— 本地查题页：粘题目 → 题库答案 + 官方原文依据（浏览器里用，不敲命令）
============================================================
  python3 webui.py              起服务并自动打开 http://127.0.0.1:8765
  python3 webui.py --port 9000  换端口
  python3 webui.py selftest     自检（不起浏览器，FAIL 会 exit 1）

后端只有两个接口，逻辑全部复用现成模块，不重写：
  GET /            -> ask.html
  GET /api/ask?q=  -> {answers:[…], docs:[…]}     solve.lookup 出答案、kb.search 出原文
  POST /api/import -> 上传「有题没答案」的文件（txt/md/csv/docx/pdf）→ 逐题给题库答案或原文依据
  POST /api/import_model -> 把没命中的那批交给模型兜底答一遍（上限 20 题，会写 pending.jsonl）
"""
import base64
import glob
import html
import io
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import urllib.parse
import webbrowser
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

import kb
import solve                                    # 复用：题库查询与待办队列都在里面

ROOT = os.path.dirname(os.path.abspath(__file__))
PAGE = os.path.join(ROOT, "ask.html")
BANKS = os.path.join(ROOT, "banks")
CJK = re.compile(r"[\u4e00-\u9fa5]{4,}")
MOCK = {"pick": 50, "seconds": 3600, "perScore": 2,
        "note": "真题随机抽题 · 60 分钟 · 每题 2 分（对齐你那张 50 题/100 分的卷子）；"
                "你上次实考 81 分 = 不及格，本平台及格线高于 81，具体值没取到，所以只看正确率别看绝对分"}


def bank_list():
    """题库清单：和刷题端同源的 banks/*.json，别处不再维护第二份。"""
    out = []
    for f in sorted(glob.glob(os.path.join(BANKS, "*.json"))):
        try:
            d = json.load(open(f, encoding="utf-8"))
        except Exception:
            continue
        qs = d.get("questions") or []
        if not qs:
            continue
        meta = d.get("meta") or {}
        out.append({"file": os.path.basename(f), "title": meta.get("title") or os.path.basename(f),
                    "count": len(qs), "note": meta.get("sourceNote", ""),
                    "types": {t: sum(1 for q in qs if q.get("type") == t)
                              for t in ("single", "multiple", "judge")}})
    return out


def quiz(bank, typ=""):
    """整套题库一次给前端，前端自己洗牌/翻页（题量小，省来回）。

    bank="__all__" 是模拟考：把**真题**库混起来抽（合集/生成题/教材派生题都不掺，
    那些不是考卷）；每题带上 bankFile，模拟考的错题才能落回各自库的错题本。
    """
    if bank == "__all__":
        qs = []
        for f in sorted(glob.glob(os.path.join(BANKS, "*.json"))):
            try:
                d = json.load(open(f, encoding="utf-8"))
            except Exception:
                continue
            meta = d.get("meta") or {}
            if meta.get("mergeOf") or meta.get("generated") or meta.get("derived") or meta.get("external"):
                continue      # external = 另一门考试（如保密教育）的题，不进电信模拟卷
            title = meta.get("title") or os.path.basename(f)
            for q in d.get("questions") or []:
                if typ and q.get("type") != typ:
                    continue
                qs.append(dict(q, bankFile=os.path.basename(f), srcBank=q.get("srcBank") or title))
        return {"bank": "__all__", "total": len(qs), "questions": qs,
                "mock": dict(MOCK, pool=len(qs))}
    path = os.path.join(BANKS, os.path.basename(bank or ""))   # 只认文件名，挡路径穿越
    if not os.path.exists(path):
        return {"error": "没有这个题库：%s" % bank}
    d = json.load(open(path, encoding="utf-8"))
    qs = [dict(q, bankFile=os.path.basename(path))
          for q in (d.get("questions") or []) if not typ or q.get("type") == typ]
    return {"bank": os.path.basename(path), "total": len(qs), "questions": qs}


def evidence(q, n=6):
    """给题目找官方原文依据：整句短语命中不了就退到题干主干的长片段，逐级缩短。

    只认 ≥6 字的长探针：试过的 4-5 字短探针能把覆盖率从 14/50 抬到 36/50，
    但换来的是「夜间施工」撞到河底挖沟、『从业人员』撞到课程介绍这类假依据 ——
    宁可显示「没找到对应条文」，也不给看着像其实不相干的出处。
    """
    stem = solve.stem_of(q)
    probes = [solve.query_of(stem, 40)] + [m[:k] for m in CJK.findall(stem) for k in (12, 10, 8, 6)]
    seen, out = set(), []
    for p in probes:
        p = p.strip()
        if len(p) < 4 or p in seen:
            continue
        seen.add(p)
        out = kb.search(p, n)
        if out:
            return p, out
    return "", []


# ── 背题卡：问答体材料（如《应知应会手册》343 条官方问答）───────────────────
# 单列一个来源，不塞进 banks/*.json：那些是选择题，要判分、要进查题索引、要算覆盖率；
# 问答题没有选项，硬塞进刷题页只会得到一堆点不动的题（这个判断在 README 里写过）。
# 真源仍是 crawl/*问答*.json（parse_handbook_qa.py 的产物），这里只做只读转发，不复制一份。
CARD_SRC = "crawl"


def card_books():
    out = []
    for f in sorted(glob.glob(os.path.join(ROOT, CARD_SRC, "*问答*.json"))):
        try:
            d = json.load(open(f, encoding="utf-8"))
        except Exception:
            continue
        qs = d.get("questions") or []
        if not qs:
            continue
        chs = []
        for q in qs:
            if q.get("chapter") and q["chapter"] not in chs:
                chs.append(q["chapter"])
        meta = d.get("meta") or {}
        out.append({"file": os.path.basename(f), "title": meta.get("title") or os.path.basename(f),
                    "count": len(qs), "chapters": chs,
                    "note": meta.get("sourceNote", "")})
    return out


def cards(book, chapter=""):
    b = next((x for x in card_books() if x["file"] == os.path.basename(book or "")), None)
    if not b:                                   # 只认列出来的文件名，挡路径穿越
        return {"error": "没有这本背题卡：%s" % book}
    d = json.load(open(os.path.join(ROOT, CARD_SRC, b["file"]), encoding="utf-8"))
    items = [q for q in (d.get("questions") or []) if not chapter or q.get("chapter") == chapter]
    return {"book": b["file"], "title": b["title"], "chapter": chapter, "total": len(items),
            "chapters": b["chapters"], "note": b["note"],
            "items": [{"id": "%s#%s-%s" % (b["file"], q.get("chapter"), q.get("num")),
                       "chapter": q.get("chapter"), "num": q.get("num"), "page": q.get("page"),
                       "q": q.get("q"), "a": q.get("a")} for q in items]}


# ==================== 导入题目文件（题干有、答案没有） ====================
# 支持 txt / md / csv / docx / pdf：全是零依赖路子 —— 纯文本直接解码（带 GBK 兜底，
# 微信、Word 里导出来的中文 txt 常常不是 UTF-8），docx 是 zip + XML 自己扒，
# pdf 借系统 pdftotext（kb.py 早就依赖它，不算新依赖）。xls/doc 这类老二进制格式不装库读不了，
# 直接告诉用户另存为 txt / docx。
IMPORT_CAP = 300                                 # 单次导入的题量上限，防一份 5000 行文件把服务拖死
MODEL_CAP = 20                                   # 交给模型兜底的题量上限（每题一次联网检索，慢）
TEXT_EXT = (".txt", ".md", ".csv", ".tsv")
# 题号行：顿号/点/括号后面可以直接跟正文（「3、高处作业…」中间常没有空格），
# 但分隔符后面紧跟数字的不算（「1.5m 以内」不是第 1 题）
QHEAD = re.compile(r"(?m)^\s*(?:第\s*)?(\d{1,3})\s*(?:[、.．)）:：](?!\d)|\s)\s*")


def extract_text(name, blob):
    """上传的文件 → 纯文本。返回 (文本, 错误)。"""
    ext = os.path.splitext(name or "")[1].lower()
    if ext in TEXT_EXT:
        for enc in ("utf-8", "utf-8-sig", "gb18030", "big5"):
            try:
                return blob.decode(enc), ""
            except UnicodeDecodeError:
                continue
        return blob.decode("utf-8", "ignore"), ""
    if ext == ".docx":
        import zipfile
        try:
            with zipfile.ZipFile(io.BytesIO(blob)) as z:
                xml = z.read("word/document.xml").decode("utf-8", "ignore")
        except Exception as e:
            return "", "docx 读不动（%s）—— 试试另存为 txt" % e
        xml = xml.replace("</w:p>", "\n").replace("<w:br/>", "\n").replace("<w:tab/>", "\t")
        return html.unescape(re.sub(r"<[^>]+>", "", xml)), ""
    if ext == ".pdf":
        if not shutil.which("pdftotext"):
            return "", "本机没有 pdftotext，PDF 读不了 —— 另存为 txt 再传"
        tag = "exam-import-%d" % os.getpid()
        src = os.path.join(tempfile.gettempdir(), tag + ".pdf")
        dst = os.path.join(tempfile.gettempdir(), tag + ".txt")
        try:
            with open(src, "wb") as fh:
                fh.write(blob)
            subprocess.run(["pdftotext", "-layout", src, dst], check=True, timeout=120)
            return open(dst, encoding="utf-8", errors="ignore").read(), ""
        except Exception as e:
            return "", "PDF 转文本失败（%s）" % e
        finally:
            for f in (src, dst):
                if os.path.exists(f):
                    os.unlink(f)
    if ext in (".doc", ".xls", ".wps"):
        return "", "%s 是老二进制格式，零依赖读不了 —— 另存为 txt / docx 再传" % ext
    return "", "不认这个格式（%s）：支持 txt / md / csv / docx / pdf" % (ext or "无扩展名")


def split_questions(text):
    """把整份文件切成逐题。有题号就按题号切，没有就按空行切块。"""
    text = (text or "").replace("\r\n", "\n").replace("\r", "\n")
    text = re.sub(r"\n{3,}", "\n\n", text)
    marks = list(QHEAD.finditer(text))
    nums = [int(m.group(1)) for m in marks]
    # 信不信题号法：至少两处、且编号严格递增（「1. 2. 3.」或从中间截的一段「21. 22.」都算；
    # 正文里孤零零一个数字、或编号乱跳的不算）。只按「≥3 个」卡会漏掉两题的小文件，
    # 那样的文件会被空行法当成一整块。
    numbered = len(marks) >= 2 and all(b > a for a, b in zip(nums, nums[1:]))
    items = []
    if numbered:
        for i, m in enumerate(marks):
            end = marks[i + 1].start() if i + 1 < len(marks) else len(text)
            items.append((m.group(1), text[m.end():end]))
    else:                                        # 空行法：一块一题
        for blk in re.split(r"\n\s*\n", text):
            items.append(("", blk))
    out, seen = [], set()
    for num, body in items:
        body = re.sub(r"[ \t]+", " ", (body or "")).strip()
        if len(re.sub(r"\s", "", body)) < 6:
            continue
        key = re.sub(r"\s+", "", body)[:100]
        if key in seen:
            continue
        seen.add(key)
        out.append({"num": num, "q": body})
    return out


def import_lookup(name, blob):
    """逐题：先查题库（命中就给答案），没命中的再走依据库（给原文条文线索）。"""
    text, err = extract_text(name, blob)
    if err:
        return {"name": name, "error": err}
    items = split_questions(text)
    if not items:
        return {"name": name, "error": "这份文件里没切出题来（题号行或空行分段都行，但要有正文）"}
    truncated = 0
    if len(items) > IMPORT_CAP:
        truncated = len(items) - IMPORT_CAP
        items = items[:IMPORT_CAP]
    out = []
    for it in items:
        row, score = solve.lookup(it["q"])
        rec = {"num": it["num"], "q": it["q"], "status": "none", "score": round(score, 3),
               "answer": "", "answerText": "", "src": "", "ev": None}
        if row:
            rec.update(status="bank", answer=row[6], answerText=row[7] or "", src=row[9],
                       qtype=row[2], analysis=(row[8] or "")[:400])
        else:
            probe, docs = evidence(it["q"])       # 复用查题页那套探针
            if docs:
                k, t, sec, src, sn, _, pg = docs[0]
                rec.update(status="doc", ev={"kind": k, "title": t, "section": sec,
                                             "src": src, "snippet": sn, "page": pg})
        out.append(rec)
    hit = sum(1 for r in out if r["status"] == "bank")
    doc = sum(1 for r in out if r["status"] == "doc")
    return {"name": name, "total": len(out), "hit": hit, "doc": doc,
            "miss": len(out) - hit - doc, "truncated": truncated, "items": out}


def import_model(questions):
    """没命中的题交给模型兜底（solve.ask 的联网检索链路），结果顺手进 pending.jsonl 待审。"""
    out = []
    for q in (questions or [])[:MODEL_CAP]:
        q = (q or "").strip()
        if not q:
            continue
        try:
            a = solve.ask(q, do_web=True, auto_learn=False, verbose=False)
        except Exception as e:
            a = {"error": str(e)}
        out.append({"q": q, "result": a})
    return {"asked": len(out), "items": out}


def ask(q, web=False):
    q = (q or "").strip()
    if not q:
        return {"q": q, "answers": [], "docs": []}
    res = {"q": q, "answers": [], "docs": [], "probe": "", "web": None}
    row, score = solve.lookup(q)
    if row:
        res["answers"].append({
            "num": row[1], "type": {"single": "单选", "multiple": "多选", "judge": "判断"}.get(row[2], row[2] or ""),
            "question": row[3], "answer": row[6], "answerText": row[7] or "",
            "analysis": (row[8] or ""), "src": row[9], "topic": row[10], "score": round(score, 3)})
    elif web:                                   # 可选兜底：走 solve 的联网检索（会写 pending.jsonl）
        res["web"] = solve.ask(q, do_web=True, auto_learn=False, verbose=False)
    probe, docs = evidence(q)
    res["probe"] = probe
    res["docs"] = [{"kind": k, "title": t, "section": s, "src": src, "snippet": sn, "page": pg}
                   for k, t, s, src, sn, _, pg in docs]
    return res


class Handler(BaseHTTPRequestHandler):
    server_version = "exam-lookup-webui"

    def _send(self, code, body, ctype):
        self.send_response(code)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(body)

    def do_POST(self):
        try:
            self._route_post()
        except Exception as e:                  # 兜底：任何异常都回 JSON，别让连接断在半路
            self._send(500, json.dumps({"error": "%s: %s" % (type(e).__name__, e)},
                                       ensure_ascii=False).encode(),
                       "application/json; charset=utf-8")

    def _route_post(self):
        u = urllib.parse.urlparse(self.path)
        try:
            n = int(self.headers.get("Content-Length") or 0)
            payload = json.loads(self.rfile.read(n).decode("utf-8") or "{}")
        except Exception:
            return self._send(400, b'{"error":"bad json"}', "application/json; charset=utf-8")
        if u.path == "/api/import":
            try:
                blob = base64.b64decode(payload.get("data") or "")
            except Exception:
                return self._send(400, b'{"error":"bad base64"}', "application/json; charset=utf-8")
            res = import_lookup(payload.get("name") or "上传文件", blob)
            return self._send(200, json.dumps(res, ensure_ascii=False).encode(),
                              "application/json; charset=utf-8")
        if u.path == "/api/import_model":
            res = import_model(payload.get("questions") or [])
            return self._send(200, json.dumps(res, ensure_ascii=False).encode(),
                              "application/json; charset=utf-8")
        return self._send(404, b"not found", "text/plain; charset=utf-8")

    def do_GET(self):
        try:
            self._route_get()
        except Exception as e:                  # 兜底：索引缺失/查询异常也回 JSON，而不是断连
            self._send(500, json.dumps({"error": "%s: %s" % (type(e).__name__, e)},
                                       ensure_ascii=False).encode(),
                       "application/json; charset=utf-8")

    def _route_get(self):
        u = urllib.parse.urlparse(self.path)
        if u.path in ("/", "/index.html"):
            if not os.path.exists(PAGE):
                return self._send(500, b"ask.html missing", "text/plain; charset=utf-8")
            return self._send(200, open(PAGE, "rb").read(), "text/html; charset=utf-8")
        if u.path == "/api/ask":
            qs = urllib.parse.parse_qs(u.query)
            q = (qs.get("q") or [""])[0]
            web = (qs.get("web") or ["0"])[0] == "1"
            body = json.dumps(ask(q, web), ensure_ascii=False).encode()
            return self._send(200, body, "application/json; charset=utf-8")
        if u.path == "/api/banks":
            return self._send(200, json.dumps({"banks": bank_list()}, ensure_ascii=False).encode(),
                              "application/json; charset=utf-8")
        if u.path == "/api/cards":
            qs = urllib.parse.parse_qs(u.query)
            body = json.dumps(cards((qs.get("book") or [""])[0], (qs.get("chapter") or [""])[0]),
                              ensure_ascii=False).encode()
            return self._send(200, body, "application/json; charset=utf-8")
        if u.path == "/api/cardbooks":
            return self._send(200, json.dumps({"books": card_books()}, ensure_ascii=False).encode(),
                              "application/json; charset=utf-8")
        if u.path == "/api/quiz":
            qs = urllib.parse.parse_qs(u.query)
            body = json.dumps(quiz((qs.get("bank") or [""])[0], (qs.get("type") or [""])[0]),
                              ensure_ascii=False).encode()
            return self._send(200, body, "application/json; charset=utf-8")
        return self._send(404, b"not found", "text/plain; charset=utf-8")

    def log_message(self, *a):
        pass                                    # 别把查询日志刷到终端上


def selftest():
    """断言：查题命中+依据非空 / 空查询不炸 / 题库清单可用 / 取题有答案且题型过滤生效。"""
    r = ask("有限空间作业中应每几分钟复测气体")
    assert r["answers"], "题库没命中已知题目"
    a = r["answers"][0]
    assert a["answer"] == "C", "第35题答案应为 C(30分钟)，实得 %r" % a["answer"]
    assert r["docs"], "没给出官方原文依据"
    assert any("30" in d["snippet"] or "复测" in d["snippet"] for d in r["docs"]), "依据里没提到复测间隔"
    r2 = ask("有限空间作业中应每几分钟复测气体", web=False)
    assert ask("")["answers"] == [] and ask("   ")["docs"] == []

    bs = bank_list()
    assert bs and all(b["count"] > 0 for b in bs), "题库清单为空"
    # 页面上「全库合计 N 套 / M 题 ｜ 单选 a · 多选 b · 判断 c」是前端对这张表求和：
    # 某个库的题型分布跟题数对不上，那一行就会当场错，所以在这儿卡住
    bad = [b["file"] for b in bs if sum((b.get("types") or {}).values()) != b["count"]]
    assert not bad, "这些库的题型分布与题数对不上，页面「全库合计」会算错：%s" % bad
    q = quiz(bs[0]["file"])
    assert q["total"] == bs[0]["count"] and q["questions"], "取题数量对不上"
    assert all(x.get("answerKeys") and x.get("question") for x in q["questions"]), "有题缺答案或题干"
    j = quiz("dtzx-safety.json", "judge")
    assert j["questions"] and all(x["type"] == "judge" for x in j["questions"]), "题型过滤没生效"
    assert quiz("../../etc/passwd")["error"], "路径穿越没挡住"

    m = quiz("__all__")                                   # 模拟考
    assert m["total"] >= 300, "模拟考池子太小：%d" % m["total"]
    assert m["mock"]["seconds"] == 3600 and m["mock"]["pick"] == 50, "模拟考参数不对"
    assert all(x.get("bankFile") for x in m["questions"]), "模拟考的题没带 bankFile，错题本会落错库"
    assert all(x.get("srcBank") and x.get("answerKeys") for x in m["questions"]), "模拟考有题缺出处或答案"
    mix = {x["bankFile"] for x in m["questions"]}
    assert not mix & {"changqi-anquan.json", "自测-安全数字.json", "教材原题-线路安全.json"}, \
        "模拟考里混进了合集/生成题/教材题：%s" % mix

    cb = card_books()                            # 背题卡
    assert cb and all(b["count"] > 0 and b["chapters"] for b in cb), "背题卡清单为空"
    c0 = cards(cb[0]["file"])
    assert c0["total"] == cb[0]["count"], "背题卡数量对不上：%d vs %d" % (c0["total"], cb[0]["count"])
    assert all((x["q"] or "").strip() and (x["a"] or "").strip() for x in c0["items"]), "背题卡有空的题干或答案"
    assert all(x["chapter"] for x in c0["items"]), "背题卡缺章节"
    # 页码不是每份材料都有：数字卡来自规章原文（一定有页码），公开卷卡来自题库（本来就没有页码）
    for b in cb:
        if "数字考点" in b["file"]:
            assert all(x["page"] for x in cards(b["file"])["items"]), "数字考点卡缺页码"
    assert cards(cb[0]["file"], c0["chapters"][0])["total"] < c0["total"], "按章筛选没生效"
    assert cards("../../etc/passwd")["error"], "背题卡的路径穿越没挡住"

    # ── 导入题目文件：切题 / 命中 / 没命中 / 格式错误 四条路都要过 ──
    txt = ("一、综合题\n\n1. 有限空间作业中应每几分钟复测气体？（ ）\nA 10分钟 B 20分钟 C 30分钟\n\n"
           "2. 登杆作业前必须检查哪些工器具的完好情况，说说你的做法。\n\n"
           "3、高处作业指坠落高度基准面多少米及以上作业？\n").encode("utf-8")
    r = import_lookup("模拟卷.txt", txt)
    assert r["total"] == 3, "切题数不对：%d（题号法该切出 3 题）" % r["total"]
    assert r["hit"] >= 1 and any(x["status"] == "bank" and x["answer"] for x in r["items"]), \
        "导入的已知题没在题库里命中"
    assert any(x["num"] == "2" for x in r["items"]), "题号没解析出来"
    gbk = import_lookup("gbk卷.txt", "1. 有限空间作业复测气体间隔是多少分钟？\n2. 安全带多久做一次承重测试？".encode("gb18030"))
    assert gbk["total"] == 2, "GBK 编码的文件没读出来：%r" % gbk.get("error")
    blank = import_lookup("空行卷.txt", ("高处作业指坠落高度基准面多少米及以上作业？\n\n"
                                        "人孔作业前应先做什么检测？\n").encode("utf-8"))
    assert blank["total"] == 2 and all(x["num"] == "" for x in blank["items"]), "空行切块没生效"
    assert import_lookup("x.doc", b"abc")["error"], "老二进制格式该报错"
    assert import_lookup("有题没答案.docx", b"not a zip")["error"], "坏 docx 该报错"
    assert split_questions("") == [] and import_lookup("空.txt", b"   " )["error"], "空文件该报错"
    capped = import_lookup("超长.txt", ("\n\n".join("%d. 第%d道题：这是用来撑题量的句子，够长了吧" % (i, i)
                                                    for i in range(1, IMPORT_CAP + 21))).encode("utf-8"))
    assert capped["total"] == IMPORT_CAP and capped["truncated"] == 20, \
        "题量上限没生效：%d/%d" % (capped["total"], capped["truncated"])
    assert import_model([])["asked"] == 0 and len(import_model(["a"] * 99)["items"]) == MODEL_CAP, \
        "模型兜底的上限没生效"

    print("selftest OK：查题命中（35题=C）+ 依据 %d 条（探针 %r）｜ 题库 %d 套（首套 %s %d 题）"
          "｜ 题型过滤/答案完整性/路径穿越 都过 ｜ 模拟考池 %d 题（%d 库混抽，参数 60 分钟/50 题/每题 2 分）"
          % (len(r2["docs"]), r2["probe"], len(bs), bs[0]["file"], bs[0]["count"],
             m["total"], len({x["bankFile"] for x in m["questions"]})))
    print("  导入题目文件：切题 %d/3（题号法）、GBK/空行切块/格式报错/坏 docx/空文件/题量上限(<=%d)/"
          "模型兜底上限(%d) 都过；模拟卷 3 题里题库命中 %d、依据命中 %d"
          % (r["total"], IMPORT_CAP, MODEL_CAP, r["hit"], r["doc"]))
    print("  依据示例：%s ｜ %s" % (r2["docs"][0]["kind"], re.sub(r"\s+", " ", r2["docs"][0]["snippet"])[:110]))


def main():
    if len(sys.argv) > 1 and sys.argv[1] == "selftest":
        return selftest()
    port = int(sys.argv[sys.argv.index("--port") + 1]) if "--port" in sys.argv else 8765
    for p in range(port, port + 5):                 # 端口被占就顺延，别让双击的人看到报错
        try:
            srv = ThreadingHTTPServer(("127.0.0.1", p), Handler)
            break
        except OSError:
            continue
    else:
        sys.exit("端口 %d~%d 都被占用了，换一个：python3 webui.py --port 9000" % (port, port + 4))
    url = "http://127.0.0.1:%d/" % p
    print("查题页已启动：%s   （Ctrl+C 停）" % url, flush=True)
    webbrowser.open(url)
    srv.serve_forever()


if __name__ == "__main__":
    main()
