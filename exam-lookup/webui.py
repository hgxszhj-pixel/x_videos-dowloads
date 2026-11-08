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

try:                                            # 插件系统是可选件：坏了也不许拖垮查题页
    import plugins as _plugins
except Exception as _e:                         # noqa: BLE001
    _plugins = None
    print("[webui] 插件系统不可用（查题不受影响）：%s" % _e, file=sys.stderr)


def _plugin_routes():
    try:
        return _plugins.routes() if _plugins else {}
    except Exception as e:                       # noqa: BLE001
        print("[webui] 插件路由读取失败：%s" % e, file=sys.stderr)
        return {}


def _plugin_status():
    try:
        return _plugins.status() if _plugins else {"plugins": [], "errors": {"loader": "插件系统未加载"}}
    except Exception as e:                       # noqa: BLE001
        return {"plugins": [], "errors": {"loader": "%s: %s" % (type(e).__name__, e)}}

ROOT = os.path.dirname(os.path.abspath(__file__))
def peer_allowed(ip):
    """只放行本机与私有网段（RFC1918 + 链路本地 + IPv6 本机）。

    为什么要有这一条：--lan 一开就是监听全网卡，路由器一旦做了端口映射，
    这套带答案的题库就等于挂在公网上。判据写死在代码里，不靠用户记得关。
    """
    ip = (ip or "").split("%")[0]
    if ip in ("127.0.0.1", "::1", "localhost"):
        return True
    if ip.startswith("::ffff:"):
        ip = ip[7:]
    parts = ip.split(".")
    if len(parts) == 4 and all(p.isdigit() for p in parts):
        a, b = int(parts[0]), int(parts[1])
        return a == 10 or (a == 192 and b == 168) or (a == 172 and 16 <= b <= 31) \
            or (a == 169 and b == 254) or a == 127
    # IPv6 里本机/链路本地（fe80::/10）放行，其余内网地址一律拒
    return ip.lower().startswith(("fe80:", "::1"))


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
                    # 排序是多选变体：只算进「排序」一档，别在「多选」里再算一次（否则合计会多）
                    "types": {t: sum(1 for q in qs if (
                                  (t == "ordering" and (q.get("type") == "ordering" or q.get("ordered")))
                                  or (t == "multiple" and q.get("type") == "multiple" and not q.get("ordered"))
                                  or (t not in ("ordering", "multiple") and q.get("type") == t)))
                              for t in ("single", "multiple", "judge", "ordering", "blank")}})
    return out


def _match_type(q, typ):
    """题型筛选：排序是多选变体，必须和「多选」分开筛，否则排序题会被当多选判分。"""
    if not typ:
        return True
    if typ == "ordering":
        return q.get("type") == "ordering" or bool(q.get("ordered"))
    if typ == "multiple":
        return q.get("type") == "multiple" and not q.get("ordered")
    return q.get("type") == typ


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
                if not _match_type(q, typ):
                    continue
                qs.append(dict(q, bankFile=os.path.basename(f), srcBank=q.get("srcBank") or title))
        return {"bank": "__all__", "total": len(qs), "questions": qs,
                "mock": dict(MOCK, pool=len(qs))}
    path = os.path.join(BANKS, os.path.basename(bank or ""))   # 只认文件名，挡路径穿越
    if not os.path.exists(path):
        return {"error": "没有这个题库：%s" % bank}
    d = json.load(open(path, encoding="utf-8"))
    qs = [dict(q, bankFile=os.path.basename(path))
          for q in (d.get("questions") or []) if _match_type(q, typ)]
    return {"bank": os.path.basename(path), "total": len(qs), "questions": qs}


def evidence(q, n=6):
    """给题目找官方原文依据：整句短语命中不了就退到题干主干的长片段，逐级缩短。

    只认 ≥6 字的长探针：试过的 4-5 字短探针能把覆盖率从 14/50 抬到 36/50，
    但换来的是「夜间施工」撞到河底挖沟、『从业人员』撞到课程介绍这类假依据 ——
    宁可显示「没找到对应条文」，也不给看着像其实不相干的出处。
    """
    stem = solve.stem_of(q)
    # 探针封顶 6 个：原来按 CJK 片段数 × 4 档长度会展开十几条，每条一次 kb.search，
    # 未命中的题光这一步就 150–200 ms，批量导入 300 题直接超时。
    probes = ([solve.query_of(stem, 40)] + [m[:k] for m in CJK.findall(stem) for k in (12, 8, 6)])[:6]
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
    # 题库也能当背题材料：考前把答案连着选项一起过一遍，比刷题快
    for f in sorted(glob.glob(os.path.join(ROOT, "banks", "*.json"))):
        name = os.path.basename(f)
        try:
            d = json.load(open(f, encoding="utf-8"))
        except Exception:
            continue
        qs = d.get("questions") or []
        if not qs:
            continue
        meta = d.get("meta") or {}
        out.append({"file": "bank:" + name,
                    "title": (meta.get("title") or name.replace(".json", "")) + "（题库 · 看题看答案）",
                    "count": len(qs), "chapters": [],
                    "note": "来自刷题题库：显示题干、全部选项与正确答案，不判分（要判分去「刷题」）"})
    return out


def cards(book, chapter=""):
    key = book or ""
    b = next((x for x in card_books() if x["file"] in (key, os.path.basename(key))), None)
    if not b:                                   # 只认列出来的书名，挡路径穿越
        return {"error": "没有这本背题卡：%s" % book}
    if b["file"].startswith("bank:"):            # 题库当背题材料
        name = b["file"][5:]
        d = json.load(open(os.path.join(ROOT, "banks", name), encoding="utf-8"))
        items = []
        for q in (d.get("questions") or []):
            opts = q.get("options") or []
            stem = q.get("question") or ""
            if opts:
                stem += "\n" + "\n".join("%s. %s" % (o.get("key", ""), o.get("text", "")) for o in opts)
            keys = [str(k) for k in (q.get("answerKeys") or [])]
            picked = [o.get("text", "") for o in opts if str(o.get("key")) in keys]
            ans = "答案 %s" % ("".join(keys) or "?")
            at = q.get("answerText")
            if isinstance(at, list):
                at = "；".join(str(x) for x in at)
            at = (at or "").strip().strip("[]").replace("'", "").replace('"', "")
            label = "；".join(picked) if picked else at
            if label and label not in ("", "?"):
                ans += " · %s" % label
            items.append({"id": "%s#%s" % (name, q.get("id") or q.get("number")),
                          "chapter": q.get("typeLabel") or q.get("type") or "", "num": q.get("number"),
                          "page": q.get("page"), "q": stem, "a": ans})
        return {"book": b["file"], "title": b["title"], "chapter": "", "total": len(items),
                "chapters": [], "note": b["note"], "items": items}
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
STEM_CAP = 1200                                  # 单题题干上限：超过就是切题把正文当题干了，截断再查
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


# 答案标记：字母型【答案】A / 判断型【答案】正确 / 排序型【答案】CBDAFE。
# 抓出来单独存（自带答案），并把「标记及其后面的解析」从题干里切掉——否则题干会带着答案和箭头解析。
ANSWER_MARK = re.compile(r"[【\[（(]\s*(?:参考)?答案\s*[】\]）)]\s*[:：]?\s*"
                         r"(正确|错误|对|错|√|×|✓|✗|(?:[A-FＡ-Ｆa-f][、,，/／\s]*){1,8})")


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
        raw = body or ""
        m = ANSWER_MARK.search(raw)
        ans = m.group(1) if m else ""
        if m:                                      # 【答案】X 之后是解析，不属于题干
            body = raw[:m.start()]
        body = re.sub(r"[ \t]+", " ", body).strip()
        if len(re.sub(r"\s", "", body)) < 6:
            continue
        key = re.sub(r"\s+", "", body)[:100]
        if key in seen:
            continue
        seen.add(key)
        out.append({"num": num, "q": body, "ans": ans})
    return out


def _norm_ans(x):
    """答案归一：字母/字母串→大写去分隔；判断类→A/B。"""
    x = str(x or "").strip()
    if not x:
        return ""
    if x in ("正确", "对", "√", "✓", "是"):
        return "A"
    if x in ("错误", "错", "×", "✗", "否"):
        return "B"
    return re.sub(r"[^A-F]", "", x.upper())


def _same_ans(a, b):
    a, b = _norm_ans(a), _norm_ans(b)
    return bool(a) and a == b


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
        q = it["q"]
        trimmed = max(0, len(q) - STEM_CAP)
        if trimmed:
            q = q[:STEM_CAP]          # 超长题干多半是切题失败（把整段正文当成一道题），截断再查
        row, score = solve.lookup(q)
        self_ans = _norm_ans(it.get("ans") or "")
        rec = {"num": it["num"], "q": q, "status": "none", "score": round(score, 3),
               "trimmed": trimmed, "self_answer": self_ans,
               "verdict": "", "bank_answer": (row[6] if row else ""),
               "answer": "", "answerText": "", "src": "", "ev": None}
        if row:
            rec.update(status="bank", answer=row[6], answerText=row[7] or "", src=row[9],
                       qtype=row[2], analysis=(row[8] or "")[:400])
            if self_ans:
                rec["verdict"] = "一致" if _same_ans(self_ans, row[6]) else "不一致"
        else:
            if self_ans:
                rec["verdict"] = "题库无此题"      # 自带答案 + 题库没有 = 可收编
            probe, docs = evidence(q)             # 复用查题页那套探针
            if docs:
                k, t, sec, src, sn, _, pg = docs[0]
                rec.update(status="doc", ev={"kind": k, "title": t, "section": sec,
                                             "src": src, "snippet": sn, "page": pg})
        out.append(rec)
    hit = sum(1 for r in out if r["status"] == "bank")
    doc = sum(1 for r in out if r["status"] == "doc")
    n_self = sum(1 for r in out if r["self_answer"])
    n_new = sum(1 for r in out if r["verdict"] in ("题库无此题", "不一致"))
    return {"name": name, "total": len(out), "hit": hit, "doc": doc,
            "miss": len(out) - hit - doc, "truncated": truncated,
            "self_answer": n_self, "addable": n_new, "items": out}


LEARNED_BANK = os.path.join(BANKS, "web-learned.json")


def _entry_from_import(rec, source):
    """把一条导入结果变成题库条目（自带答案 → 选项/答案/解析），做不出来就返回 None。"""
    from plugins.harvest import parse_options
    stem, opts = parse_options(ANSWER_MARK.sub(" ", rec.get("q") or ""))   # 双保险：题干里残留的标记也摘掉
    ans = _norm_ans(rec.get("self_answer"))
    if not stem or not ans:
        return None
    if ans in ("A", "B") and len(opts) < 2:                 # 判断题：只有「正确/错误」两个选项
        opts = [{"key": "A", "text": "正确"}, {"key": "B", "text": "错误"}]
        qtype, answer_text = "judge", ["正确" if ans == "A" else "错误"]
    else:
        keys = list(ans)
        if not all(k in [o["key"] for o in opts] for k in keys):
            return None                                     # 自带答案的字母对不上选项，别硬收
        answer_text = [next(o["text"] for o in opts if o["key"] == k) for k in keys]
        if len(keys) == 1:
            qtype = "single"
        else:
            qtype = "multiple"
    e = {"id": "WEB-" + __import__("hashlib").sha1(
             re.sub(r"[\s\W]+", "", stem).encode()).hexdigest()[:12],
         "type": qtype, "typeLabel": {"single": "单选题", "multiple": "多选题", "judge": "判断题"}[qtype],
         "question": stem, "options": opts, "answerKeys": list(ans) if qtype != "judge" else [ans],
         "answerText": answer_text, "topic": "", "category": "",
         "analysis": "自带答案（%s）；来源：%s" % (ans, source),
         "warn": "答案来自导入文件自带的【答案】标记，未经二次核对"}
    if qtype == "multiple" and len(ans) == len(opts) >= 3:
        e["type"], e["ordered"] = "multiple", True         # 选项全选且顺序有意义 = 排序变体
        e["typeLabel"] = "排序题（多选变体）"
    return e


def bank_append(items, source=""):
    """把「自带答案」的题写进 banks/web-learned.json，然后 validate → import → build → sync。

    唯一入口是人在导入结果里点「加入题库」；任一步失败整批回滚，不留半个文件。
    """
    import hashlib                                        # noqa: F401  (给 _entry_from_import 用)
    import kb_db
    import solve
    entries, skipped = [], 0
    for rec in items or []:
        e = _entry_from_import(rec, source or "导入文件")
        if not e:
            skipped += 1
            continue
        entries.append(e)
    if not entries:
        return {"added": 0, "skipped": skipped, "steps": {}, "note": "没有可入库的题"}
    try:
        data = json.load(open(LEARNED_BANK, encoding="utf-8"))
    except Exception:
        data = {"schema_version": 1, "meta": {"title": "联网/导入收编题（人工确认）",
                                              "questionCount": 0, "volumes": 1,
                                              "sourceNote": "只收人工点过「加入/收编」的题"}, "questions": []}
    have = {x.get("id") for x in data.get("questions", [])}
    fresh = [e for e in entries if e["id"] not in have]
    skipped += len(entries) - len(fresh)
    if not fresh:
        return {"added": 0, "skipped": skipped, "steps": {}, "note": "这些题已经在题库里了"}
    backup = json.dumps(data, ensure_ascii=False)
    for i, e in enumerate(fresh, 1):
        e["number"] = len(data["questions"]) + i
        data["questions"].append(e)
    data["meta"]["questionCount"] = len(data["questions"])
    os.makedirs(os.path.dirname(LEARNED_BANK), exist_ok=True)
    tmp = LEARNED_BANK + ".tmp"
    with open(tmp, "w", encoding="utf-8") as f:
        json.dump(data, f, ensure_ascii=False, indent=1)
    os.replace(tmp, LEARNED_BANK)

    steps = {}
    try:
        bad = kb_db.validate_bank(LEARNED_BANK)           # 1) 格式契约
        if bad:
            raise ValueError("；".join(bad[:3]))
        steps["validate"] = "ok"
        db = kb_db.connect()
        try:
            ins, upd, same, stale = kb_db.import_bank(db, LEARNED_BANK)   # 2) 落 questions 表
        finally:
            db.close()
        steps["import"] = "新 %d 改 %d 未变 %d" % (ins, upd, same)
        solve.build()                                     # 3) 重建查题索引
        steps["build"] = "ok"
    except Exception as e:                                # noqa: BLE001
        with open(LEARNED_BANK, "w", encoding="utf-8") as f:
            f.write(backup)                               # 回滚文件
        raise ValueError("入库失败，已回滚这次写入：%s" % e)
    try:                                                  # 4) 刷题端同步（失败不算入库失败）
        import sync_banks
        sync_banks.sync()
        steps["sync"] = "ok"
    except Exception as e:                                # noqa: BLE001
        steps["sync"] = "失败：%s" % e
    return {"added": len(fresh), "skipped": skipped, "steps": steps,
            "ids": [e["id"] for e in fresh]}


def import_model(questions):
    """没命中的题走兜底链（原文直证 → 多引擎；批量不调 Claude，太慢太贵）。结果顺手进 pending.jsonl 待审。"""
    import answer_chain
    out = []
    for q in (questions or [])[:MODEL_CAP]:
        q = (q or "").strip()
        if not q:
            continue
        try:
            c = answer_chain.chain(q, web=True, llm=False)
            a = {"answer": c["answer"], "source": c["source"] or ("%s 未答出" % c["tier"]),
                 "confidence": c["confidence"], "note": c.get("note", ""),
                 "tier": c["tier"], "tiers": c["tiers"]}
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
            "num": row[1], "type": {"single": "单选", "multiple": "多选", "judge": "判断", "ordering": "排序", "blank": "填空"}.get(row[2], row[2] or ""),
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
    protocol_version = "HTTP/1.1"      # Safari/WebKit 对 1.0 的 POST 会偶发 "Load failed"
    BODY_CAP = 30 * 1048576            # 30MB：再大就是别的用途了，直接回话不硬读

    def _send(self, code, body, ctype):
        self.send_response(code)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(body)

    def do_POST(self):
        if self._deny_if_foreign():
            return
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
        except Exception:
            return self._send(400, b'{"error":"bad content-length"}', "application/json; charset=utf-8")
        if n > self.BODY_CAP:
            # 先把 413 回出去，再把已到的包按 64KB 读掉：不读的话浏览器还在写就被断开，
            # 前端只会看到 "Load failed"，白丢一条本来能看懂的提示。
            self._send(413, json.dumps(
                {"error": "文件太大（%.1f MB）；上限 %d MB。扫描版 PDF 请先转成 txt"
                          % (n / 1048576, self.BODY_CAP // 1048576)}, ensure_ascii=False).encode(),
                "application/json; charset=utf-8")
            left = min(n, self.BODY_CAP * 2)
            try:
                while left > 0:
                    chunk = self.rfile.read(min(65536, left))
                    if not chunk:
                        break
                    left -= len(chunk)
            except Exception:
                pass
            self.close_connection = True
            return
        try:
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
        if u.path == "/api/bank/append":
            try:
                res = bank_append(payload.get("items") or [], payload.get("name") or "导入文件")
            except Exception as e:                   # noqa: BLE001
                return self._send(500, json.dumps({"error": str(e)}, ensure_ascii=False).encode(),
                                  "application/json; charset=utf-8")
            return self._send(200, json.dumps(res, ensure_ascii=False).encode(),
                              "application/json; charset=utf-8")
        if u.path == "/api/import_model":
            res = import_model(payload.get("questions") or [])
            return self._send(200, json.dumps(res, ensure_ascii=False).encode(),
                              "application/json; charset=utf-8")
        return self._send(404, b"not found", "text/plain; charset=utf-8")

    def _deny_if_foreign(self):
        ip = self.client_address[0]
        if peer_allowed(ip):
            return False
        self._send(403, b"forbidden: only LAN clients", "text/plain; charset=utf-8")
        return True

    def do_GET(self):
        if self._deny_if_foreign():
            return
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
            deep = (qs.get("deep") or ["0"])[0] == "1"      # 四级兜底链（含原文直证 + claude）
            if deep:
                try:
                    import answer_chain
                    r = answer_chain.chain(q, web=True, llm=(qs.get("llm") or ["0"])[0] == "1")
                except Exception as e:                       # noqa: BLE001 链路崩了也要能答出前两级
                    r = {"answer": None, "tier": "error", "tiers": [],
                         "note": "%s: %s" % (type(e).__name__, e)}
                body = json.dumps({"chain": r, "q": q}, ensure_ascii=False).encode()
                return self._send(200, body, "application/json; charset=utf-8")
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
        if u.path in ("/ffd.html", "/ffd"):
            # 免费资源清单：cron 每周把 ffd.py 的静态页写到 ~/free-for-dev.html，这里只负责送出去
            f = os.path.expanduser("~/free-for-dev.html")
            try:
                body = open(f, "rb").read()
                return self._send(200, body, "text/html; charset=utf-8")
            except OSError:
                tip = ("<meta charset=utf-8><body style='font:16px -apple-system;padding:2rem'>"
                       "还没生成免费资源页。跑一次：<code>python3 ~/.hermes/tools/ffd/ffd.py --html</code></body>")
                return self._send(200, tip.encode(), "text/html; charset=utf-8")
        if u.path == "/api/plugins":
            return self._send(200, json.dumps(_plugin_status(), ensure_ascii=False).encode(),
                              "application/json; charset=utf-8")
        # ---- 插件路由：每次请求才取（热加载，改插件不必重启服务）；异常只毁这一次请求 ----
        fn = _plugin_routes().get(u.path)
        if fn is not None:
            qs = {k: v[0] for k, v in urllib.parse.parse_qs(u.query).items()}
            try:
                body = json.dumps(fn(qs), ensure_ascii=False).encode()
                return self._send(200, body, "application/json; charset=utf-8")
            except Exception as e:               # noqa: BLE001
                return self._send(500, json.dumps({"error": "%s: %s" % (type(e).__name__, e)},
                                                  ensure_ascii=False).encode(),
                                  "application/json; charset=utf-8")
        return self._send(404, b"not found", "text/plain; charset=utf-8")

    def log_message(self, *a):
        pass                                    # 别把查询日志刷到终端上


def selftest():
    """断言：查题命中+依据非空 / 空查询不炸 / 题库清单可用 / 取题有答案且题型过滤生效。"""
    # 内网准入：本机与私有网段放行，公网地址一律拒（--lan 时这道闸就是最后一道）
    for ip in ("127.0.0.1", "::1", "192.168.1.20", "10.2.3.4", "172.16.0.9", "172.31.255.1", "169.254.1.1"):
        assert peer_allowed(ip), "该放行却拒了：%s" % ip
    for ip in ("8.8.8.8", "1.1.1.1", "172.32.0.1", "203.0.113.7", "2001:db8::1"):
        assert not peer_allowed(ip), "该拒却放行了：%s" % ip
    # 前端判据守卫：isOrd 曾写成 `q.ordered === true || isOrd(q)` 自我递归，
    # 一点「开始」就 Maximum call stack size exceeded，题目永远出不来（且服务端全绿）。
    page_js = open(PAGE, encoding="utf-8").read()
    m = re.search(r"function isOrd\(q\)\{(.*?)\}", page_js, re.S)
    assert m, "ask.html 里找不到 isOrd"
    body = m.group(1)
    assert "ordering" in body, "isOrd 认不出老 type=ordering：%s" % body.strip()
    assert not re.search(r"isOrd\s*\(", body), "isOrd 自我调用了（会无限递归）：%s" % body.strip()
    # 纯函数语义也量一遍：multiple+ordered 与老 ordering 都算排序，普通多选题不算
    ns = {}
    exec("def isOrd(q): return bool(q) and (q.get('ordered') is True or q.get('type') == 'ordering')", ns)
    assert ns["isOrd"]({"type": "multiple", "ordered": True}) and ns["isOrd"]({"type": "ordering"})
    assert not ns["isOrd"]({"type": "multiple"}) and not ns["isOrd"](None)
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

    cb = card_books()                            # 背题卡：问答材料 + 题库（bank: 前缀）
    assert cb and all(b["count"] > 0 for b in cb), "背题卡清单为空"
    qa = [b for b in cb if not b["file"].startswith("bank:")]
    bk = [b for b in cb if b["file"].startswith("bank:")]
    assert qa and all(b["chapters"] for b in qa), "问答材料应带章节：%r" % [b["file"] for b in qa]
    assert bk, "题库没进背题下拉（bank: 前缀）"
    c0 = cards(qa[0]["file"])
    assert c0["total"] == qa[0]["count"], "背题卡数量对不上：%d vs %d" % (c0["total"], qa[0]["count"])
    assert all((x["q"] or "").strip() and (x["a"] or "").strip() for x in c0["items"]), "背题卡有空的题干或答案"
    assert all(x["chapter"] for x in c0["items"]), "背题卡缺章节"
    # 题库当背题材料：题面必须带选项、答案必须是选项原文（不能漏成「答案 B · ['…']」这种 Python 列表串）
    cb0 = cards(bk[0]["file"])
    assert cb0["total"] == bk[0]["count"] == len(cb0["items"]), "题库背书条目数对不上"
    assert all("\n" in x["q"] for x in cb0["items"][:5]), "题库背书的题面没带选项行"
    assert all(x["a"].startswith("答案 ") and "[" not in x["a"] for x in cb0["items"][:5]), \
        "答案行没拼成选项原文：%r" % cb0["items"][0]["a"]
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
    _sp = split_questions("1、 云改数转智惠战略是在哪个战略基础上发展而来？（单选题）\n"
                          "  A. 云改数转战略\n  B. 数字中国战略\n【答案】A\n\n"
                          "2、 深刻理解该战略，必须准确把握什么要求？（单选题）\n"
                          "  A. 经济性要求\n  B. 政治性要求\n【答案】B\n")
    assert len(_sp) == 2, "两题应切成两条：%r" % _sp
    assert all("【答案】" not in x["q"] for x in _sp), "题干里还留着【答案】标记：%r" % [_ / 0 for _ in []] if False else _sp
    assert _sp[0]["q"].rstrip().endswith("数字中国战略"), _sp[0]["q"][-40:]
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

    # 插件：路由挂得上、状态可读、插件内部抛异常只毁这一次请求（不冒到服务层）
    rs = _plugin_routes()
    assert "/api/srs/due" in rs, "srs 插件路由没挂上：%r" % sorted(rs)
    st = _plugin_status()
    assert st.get("plugins"), "插件状态为空：%r" % st
    assert rs["/api/srs/due"]({"limit": 2}).get("stats"), "srs /api/srs/due 应返回 stats"

    print("selftest OK：查题命中（35题=C）+ 依据 %d 条（探针 %r）｜ 题库 %d 套（首套 %s %d 题）"
          "｜ 题型过滤/答案完整性/路径穿越 都过 ｜ 模拟考池 %d 题（%d 库混抽，参数 60 分钟/50 题/每题 2 分）"
          "｜ 插件 %d 个（%s）"
          % (len(r2["docs"]), r2["probe"], len(bs), bs[0]["file"], bs[0]["count"],
             m["total"], len({x["bankFile"] for x in m["questions"]}),
             len(st["plugins"]), ",".join(sorted(rs))))
    print("  导入题目文件：切题 %d/3（题号法）、GBK/空行切块/格式报错/坏 docx/空文件/题量上限(<=%d)/"
          "模型兜底上限(%d) 都过；模拟卷 3 题里题库命中 %d、依据命中 %d"
          % (r["total"], IMPORT_CAP, MODEL_CAP, r["hit"], r["doc"]))
    print("  依据示例：%s ｜ %s" % (r2["docs"][0]["kind"], re.sub(r"\s+", " ", r2["docs"][0]["snippet"])[:110]))


def _lan_ips():
    """列出本机的局域网地址（不含回环），给同事照着输。"""
    out = []
    try:
        import socket
        for info in socket.getaddrinfo(socket.gethostname(), None, socket.AF_INET):
            ip = info[4][0]
            if peer_allowed(ip) and not ip.startswith("127.") and ip not in out:
                out.append(ip)
    except OSError:
        pass
    if not out:                                  # 主机名解析不出来时走 UDP 探一下默认路由地址
        try:
            import socket
            sk = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
            sk.connect(("192.0.2.1", 9))
            ip = sk.getsockname()[0]
            sk.close()
            if ip and not ip.startswith("127."):
                out.append(ip)
        except OSError:
            pass
    return out


def _warm_cache():
    """后台把查题目录读进内存：solve.lookup 首次调用要建 CJK bigram 索引（实测 ~10s），
    不收编页首次打分会卡住页面。启动时先热一遍，用户就不必等这一步。"""
    import threading

    def run():
        try:
            import time
            t0 = time.time()
            solve.lookup("预热")
            print("[webui] 查题缓存预热完成 %.1fs" % (time.time() - t0), file=sys.stderr)
        except Exception as e:                       # noqa: BLE001
            print("[webui] 预热失败（不影响使用）：%s" % e, file=sys.stderr)

    threading.Thread(target=run, daemon=True, name="warm-cache").start()


def main():
    if len(sys.argv) > 1 and sys.argv[1] == "selftest":
        return selftest()
    port = int(sys.argv[sys.argv.index("--port") + 1]) if "--port" in sys.argv else 8765
    lan = "--lan" in sys.argv
    host = "0.0.0.0" if lan else "127.0.0.1"
    for p in range(port, port + 5):                 # 端口被占就顺延，别让双击的人看到报错
        try:
            srv = ThreadingHTTPServer((host, p), Handler)
            break
        except OSError:
            continue
    else:
        sys.exit("端口 %d~%d 都被占用了，换一个：python3 webui.py --port 9000" % (port, port + 4))
    url = "http://%s:%d/" % ("127.0.0.1" if lan else "127.0.0.1", p)
    print("查题页已启动：%s   （Ctrl+C 停）" % url, flush=True)
    if lan:
        for ip in _lan_ips():
            print("  内网同事用这个：http://%s:%d/   （只放行 10./172.16-31./192.168. 网段）" % (ip, p), flush=True)
    _warm_cache()
    webbrowser.open(url)
    srv.serve_forever()


if __name__ == "__main__":
    main()
