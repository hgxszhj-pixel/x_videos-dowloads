#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""harvest —— 把「联网兜底找到、人工确认过」的题收编进题库（动态扩容的入口）

原则（写死的边界）：
  1. 只有人工在页面上点过「确认入库」的题才会进来，联网检索自己永远不写库
  2. 进的是独立题库 banks/web-learned.json（带 schema_version，走 kb_db.validate_bank 校验）
  3. 一次写库要三处同时生效：docs.sqlite 的 questions 表、查题索引 qb.sqlite3、刷题端网页
  4. 题面里没选项的（判断题/简答）不收 —— warnings 里会说明原因，不进半成品

接口：
  GET /api/harvest/list?limit=20          待收编清单（题干 / 选项 / 候选 / 依据片段）
  GET /api/harvest/commit?h=<hash>&key=C  确认入库（h 来自 /list）
  GET /api/harvest/ignore?h=<hash>        不收，标记为忽略
"""
import hashlib
import json
import os
import re
import time

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
BANK = os.path.join(ROOT, "banks/web-learned.json")
PENDING = os.path.join(ROOT, "pending.jsonl")
STATE = os.path.join(ROOT, "jr/out/harvest_state.json")
TITLE = "联网收编 · 人工确认"

_PUNC = re.compile(r"[\s，。、；：！？（）《》【】<>,.;:!?()\[\]\"'’‘“”\-—_/\\|]+")


def _norm(s):
    return _PUNC.sub("", s or "").strip()


def qhash(question):
    return hashlib.sha1(_norm(question).encode("utf-8")).hexdigest()[:12]


def parse_options(text):
    """从题面里切出题干与选项。返回 (stem, [{key,text}])。

    做法：先找所有「A. / B、」这类标记，再只保留 A→B→C→D 连续递增的那一串
    （避免正文里的杂字母被当成选项），最后按标记位置切片。
    """
    text = (text or "").strip()
    marks = [(m.start(), m.end(), m.group(1))
             for m in re.finditer(r"(?<![A-Za-z0-9])([A-F])[.、．]\s*", text)]
    if len(marks) < 2:
        return "", []
    order, seq = "ABCDEF", [marks[0]]
    for m in marks[1:]:
        if order.index(m[2]) == order.index(seq[-1][2]) + 1:
            seq.append(m)
    if len(seq) < 2:
        return "", []
    stem = text[:seq[0][0]].strip(" \n，,。;；")
    opts = []
    for i, (s0, e0, k) in enumerate(seq):
        end = seq[i + 1][0] if i + 1 < len(seq) else len(text)
        val = text[e0:end].strip(" \n，,。;；")
        if val:
            opts.append({"key": k, "text": val})
    return stem, opts


def _load_json(path, default):
    try:
        with open(path, encoding="utf-8") as f:
            return json.load(f)
    except Exception:
        return default


def _state():
    st = _load_json(STATE, {"done": {}, "ignored": {}})
    st.setdefault("done", {})
    st.setdefault("ignored", {})
    return st


def _save_state(st):
    os.makedirs(os.path.dirname(STATE), exist_ok=True)
    tmp = STATE + ".tmp"
    with open(tmp, "w", encoding="utf-8") as f:
        json.dump(st, f, ensure_ascii=False, indent=1)
    os.replace(tmp, STATE)


def _pending_raw():
    """pending.jsonl → {hash: 合并后的一条}（同题多行取最后一次，保留候选/证据）。"""
    out = {}
    if not os.path.exists(PENDING):
        return out
    for line in open(PENDING, encoding="utf-8"):
        line = line.strip()
        if not line:
            continue
        try:
            rec = json.loads(line)
        except Exception:
            continue
        q = rec.get("question") or ""
        h = qhash(q)
        cur = out.get(h) or {"h": h, "question": q, "ts": "", "candidates": [], "evidence": []}
        cur["ts"] = rec.get("ts") or cur["ts"]
        if rec.get("candidates"):
            cur["candidates"] = rec["candidates"]
        if rec.get("evidence"):
            cur["evidence"] = rec["evidence"][:3]
        out[h] = cur
    return out


def list_items(limit=20, include_done=False):
    """待收编清单：先排序切片，再只给**可见的那几条**做昂贵查询。

    踩过：原来边遍历边查（201 条全查一遍才切片），一页 20 条要 10+ 秒；
    现在只对返回的条目做题库查询/原文依据，其余保持轻量。
    """
    st = _state()
    light = []
    for h, rec in _pending_raw().items():
        if h in st["done"] and not include_done:
            continue
        if h in st["ignored"]:
            continue
        stem, opts = parse_options(rec["question"])
        light.append({"h": h, "question": rec["question"], "ts": rec.get("ts") or "",
                      "stem": stem or rec["question"], "options": opts,
                      "candidates": rec.get("candidates") or [],
                      "evidence": rec.get("evidence") or [],
                      "usable": len(opts) >= 2})
    light.sort(key=lambda r: (not r["usable"], -(len(r["candidates"])), r["ts"]))
    page = light[:max(1, int(limit or 20))]
    for rec in page:
        if rec["usable"]:
            rec["local"] = []
            continue
        rec.update(_bank_lookup(rec["stem"]))
        rec["local"] = [] if rec.get("in_bank") else _local_evidence(rec["stem"])
    return page


def _bank_lookup(stem, floor=0.86):
    """这题是不是已经在题库里了？有就给答案与出处，省掉人工补题面。

    实测：192 条「只有题干」的老记录里 175 条题库本来就有答案——
    它们缺的不是题面，是「已存在」这个判断。
    """
    try:
        import solve
        row, score = solve.lookup(stem or "")
        if row and score >= floor:
            return {"in_bank": True, "bank_id": row[0], "bank_answer": row[6],
                    "bank_answer_text": row[7] or "", "bank_src": row[9], "bank_score": round(score, 3)}
    except Exception:
        pass
    return {"in_bank": False}


def exists_all(limit=1000):
    """一次把「题库已有」的全部划掉（175 条一条条点没人干得动）。"""
    st = _state()
    n = 0
    for it in list_items(limit=limit):
        if it.get("in_bank") and it["h"] not in st["done"]:
            st["done"][it["h"]] = "已在题库（%s）" % it.get("bank_id", "")
            n += 1
    _save_state(st)
    return {"ok": True, "done": n, "stats": stats()}


_LOCAL_CACHE = {}


def _local_evidence(question, n=2):
    key = (question or "")[:120]
    if key in _LOCAL_CACHE:
        return _LOCAL_CACHE[key]
    val = _local_evidence_uncached(question, n)
    if len(_LOCAL_CACHE) > 600:
        _LOCAL_CACHE.clear()
    _LOCAL_CACHE[key] = val
    return val


def _local_evidence_uncached(question, n=2):
    """本地原文依据：收编前先看教材/法规怎么写的，别只凭搜索摘要拍答案。

    踩过：杆上作业随身工具总重量，摘要里看不见数字，凭印象点了 25kg；
    教材原文写的是「不准超过20Kg」——错了 1 个选项。所以把原文摆在卡片上。
    """
    try:
        import kb
        import re as _re
        # 长题干直接搜命中率低（bigram 被稀释）：先用题干主干，再退到最长的几个命中段
        probes = []
        stem = _re.sub(r"[（(][^）)]*[）)]\s*$", "", question or "").strip()
        probes.append(stem[:28])
        probes += [x[:12] for x in _re.findall(r"[\u4e00-\u9fa5]{4,}", stem)][:2]
        out, seen = [], set()
        rows = []
        for pr in [x for x in probes if len(x) >= 4]:
            rows = kb.search(pr, n) or []
            if rows:
                break
        for row in rows:
            r = list(row) + [None] * 7
            txt = (r[4] or "").replace("\n", " ")
            if txt:
                out.append({"title": r[1] or "", "section": r[2] or "", "page": r[6] or "",
                            "text": txt[:220]})
        # 再补一记「原件直证」：分段检索常把关键那句话切到别的块里，
        # 直接在 docs_new_text 原件里按 3-gram 命中找片段，能捞到「不准超过20Kg」这种原句
        try:
            import ask_ctx
            for f in ask_ctx.file_hits(stem or question, 2):
                out.append({"title": f["file"], "section": "原件命中", "page": "",
                            "text": f["snippet"][:320]})
        except Exception:
            pass
        return out[:n + 2]
    except Exception:
        return []


def _bank():
    data = _load_json(BANK, None)
    if not isinstance(data, dict):
        data = {"schema_version": 1,
                "meta": {"title": TITLE,
                         "sourceNote": "来自联网兜底检索 + 人工确认；analysis 里记了命中来源与确认时间",
                         "public": False},
                "questions": []}
    data.setdefault("questions", [])
    return data


def _qtype(opts, keys):
    texts = {o["text"].strip() for o in opts}
    if len(keys) > 1:
        return "multiple"
    if texts and texts <= {"正确", "错误", "对", "错"}:
        return "judge"
    return "single"


def build_entry(question, key, sources=None, when=None):
    """纯函数：把「题干原文 + 选定答案」变成一条符合 schema 的题目。不合格就抛 ValueError。"""
    stem, opts = parse_options(question)
    if len(opts) < 2:
        raise ValueError("题面里切不出 2 个以上的选项（判断题/简答不收，避免半成品进库）")
    keys = [k.strip().upper() for k in re.split(r"[,，/、\s]+", key or "") if k.strip()]
    valid = {o["key"] for o in opts}
    bad = [k for k in keys if k not in valid]
    if not keys or bad:
        raise ValueError("答案 %r 不在选项 %s 里" % (key, sorted(valid)))
    at = next(o["text"] for o in opts if o["key"] == keys[0])
    return {
        "id": "WEB-" + qhash(question),
        "type": _qtype(opts, keys),
        "question": stem,
        "options": opts,
        "answerKeys": keys,
        "answerText": "、".join(o["text"] for o in opts if o["key"] in keys) if len(keys) > 1 else at,
        "analysis": "来源：联网兜底 + 人工确认%s（%s）" % (
            ("：" + "；".join(sources)) if sources else "", when or time.strftime("%Y-%m-%d")),
        "topic": "联网收编",
    }


def commit(h, key, when=None, question=None):
    """确认入库：写 banks/web-learned.json → questions 表 → qb 索引 → 刷题端。每步单独报错。

    question 非空时用它代替 pending 里那条（补题面场景：老记录只存了题干，没有选项）。
    """
    rec = _pending_raw().get(h)
    if not rec:
        raise ValueError("找不到这条待收编记录（pending.jsonl 里没有 h=%s）" % h)
    sources = [e[:80] for e in (rec.get("evidence") or [])[:2]]
    q = build_entry(question or rec["question"], key, sources, when)
    data = _bank()
    if any(x.get("id") == q["id"] for x in data["questions"]):
        return {"ok": True, "duplicate": True, "id": q["id"], "steps": {}}
    q["number"] = len(data["questions"]) + 1
    data["questions"].append(q)
    os.makedirs(os.path.dirname(BANK), exist_ok=True)
    tmp = BANK + ".tmp"
    with open(tmp, "w", encoding="utf-8") as f:
        json.dump(data, f, ensure_ascii=False, indent=1)
    os.replace(tmp, BANK)

    steps = {}
    try:                                          # 1) 格式契约
        import kb_db
        bad = kb_db.validate_bank(BANK)
        if bad:
            raise ValueError("；".join(bad[:3]))
        steps["validate"] = "ok"
    except Exception as e:
        data["questions"] = [x for x in data["questions"] if x.get("id") != q["id"]]
        with open(BANK, "w", encoding="utf-8") as f:
            json.dump(data, f, ensure_ascii=False, indent=1)
        raise ValueError("题库校验没过，已回滚这次写入：%s" % e)

    try:                                          # 2) questions 表
        import kb_db
        db = kb_db.connect()
        ins, upd, same, stale = kb_db.import_bank(db, BANK)
        db.close()
        steps["questions"] = "新 %d 改 %d 未变 %d" % (ins, upd, same)
    except Exception as e:
        steps["questions"] = "失败：%s" % e

    try:                                          # 3) 查题索引
        import solve
        solve.build()
        steps["qb"] = "已重建"
    except Exception as e:
        steps["qb"] = "失败：%s" % e

    try:                                          # 4) 刷题端网页
        import sync_banks
        sync_banks.sync()
        steps["web"] = "已同步"
    except Exception as e:
        steps["web"] = "失败：%s" % e

    st = _state()
    st["done"][h] = {"id": q["id"], "when": time.strftime("%Y-%m-%d %H:%M:%S"),
                     "qid": q["id"], "answer": "".join(q["answerKeys"])}
    _save_state(st)
    return {"ok": True, "id": q["id"], "total": len(data["questions"]), "steps": steps,
            "question": q["question"], "answer": "".join(q["answerKeys"])}


def complete(h, question, answer, when=None):
    """补题面再收编：pending 里只有题干、没有选项的老记录，人工把完整题面粘进来再说。

    为什么要这一步：早期待审记录只存了题干（后来才去解析选项），直接点选项无从点起；
    50 多字节的题干补成带 A/B/C/D 的完整题面后，才能走 build_entry 那套校验。
    """
    question = (question or "").strip()
    if not question:
        raise ValueError("题面是空的：把带选项的完整题目粘进来（形如「题干……\nA. …\nB. …」）")
    stem, opts = parse_options(question)
    if len(opts) < 2:
        raise ValueError("题面里还是切不出 2 个以上选项（要 A. / B. 这种写法，判断题按 A.正确 B.错误 填）")
    if not (answer or "").strip():
        raise ValueError("答案没填：单选填 D，多选填 ABCD 或 A、B")
    return commit(h, answer, when, question=question)


def exists(h, note="已在题库"):
    """题库已有这题：直接从待收编里划掉（不写库、不改题，只记状态）。"""
    st = _state()
    if h not in st["done"]:
        st["done"][h] = note
        _save_state(st)
    return {"ok": True, "h": h, "done": note, "stats": stats()}


def ignore(h):
    rec = _pending_raw().get(h)
    st = _state()
    st["ignored"][h] = {"when": time.strftime("%Y-%m-%d %H:%M:%S")}
    _save_state(st)
    return {"ok": True, "ignored": h, "question": (rec or {}).get("question", "")[:60]}


def stats():
    st = _state()
    bank = _bank()
    raw = _pending_raw()
    left = sum(1 for h in raw if h not in st["done"] and h not in st["ignored"])
    return {"pending_unique": len(raw), "left": left,          # left = 还要你处理的
            "done": len(st["done"]), "ignored": len(st["ignored"]),
            "bank_count": len(bank["questions"]), "bank_file": BANK}


# ---------- HTTP 路由 ----------
def _r_list(q):
    return {"items": list_items(q.get("limit") or 20), "stats": stats()}


def _r_commit(q):
    if not q.get("h"):
        return {"error": "缺 h（这是 /api/harvest/list 给的）"}
    return commit(q["h"], q.get("key") or "")


def _r_complete(q):
    if not q.get("h"):
        return {"error": "缺 h（这是 /api/harvest/list 给的）"}
    try:
        return complete(q["h"], q.get("question") or "", q.get("answer") or "")
    except ValueError as e:
        return {"error": str(e)}


def _r_exists_all(q):
    return exists_all(int(q.get("limit") or 1000))


def _r_exists(q):
    if not q.get("h"):
        return {"error": "缺 h"}
    return exists(q["h"], q.get("note") or "已在题库")


def _r_ignore(q):
    if not q.get("h"):
        return {"error": "缺 h"}
    return ignore(q["h"])


PLUGIN = {"name": "harvest", "desc": "联网收编：人工确认后把题写进 banks/web-learned.json",
          "routes": {"/api/harvest/list": _r_list, "/api/harvest/commit": _r_commit,
                     "/api/harvest/complete": _r_complete, "/api/harvest/exists": _r_exists, "/api/harvest/exists_all": _r_exists_all,
                     "/api/harvest/ignore": _r_ignore}}


def selftest():
    """红绿：切选项 / 拒收无选项题 / 拒收答案不在选项里 / 条目符合 schema / 幂等。"""
    q = "2、杆上作业，随身携带工具的总重量不得超过多少 KG？\nA. 10kg\nB. 15kg\nC. 20kg\nD. 25kg"
    stem, opts = parse_options(q)
    assert stem.startswith("2、杆上作业") and len(opts) == 4, (stem, opts)
    assert [o["key"] for o in opts] == ["A", "B", "C", "D"], opts
    assert opts[0]["text"].startswith("10kg"), opts

    e = build_entry(q, "B")
    assert e["answerKeys"] == ["B"] and e["type"] == "single", e
    assert e["answerText"] == "15kg", e
    assert e["id"].startswith("WEB-"), e
    assert e["options"] and len(e["options"]) == 4

    for bad, why in ((q, "Z"), (q, ""), ("判断题：有限空间必须设专人监护。", "A")):
        try:
            build_entry(bad, why)
            raise AssertionError("应拒绝：%r / %r" % (bad[:12], why))
        except ValueError:
            pass

    # 补题面：没选项 / 空题面 / 空答案 都必须拒绝（不写库）
    for bad_args, why in (((("", "A")), "空题面"), (("判断题没选项", "A"), "没选项"),
                          (("题干\nA. 甲\nB. 乙", ""), "空答案")):
        try:
            complete("deadbeef", bad_args[0], bad_args[1])
            raise AssertionError("补题面应拒绝：%s" % why)
        except ValueError:
            pass
    j = build_entry("有限空间作业必须设专人监护。A. 正确 B. 错误", "A")
    assert j["type"] == "judge", j

    # 幂等：同题同答案 → 同 id（重复收编不会造出两条）
    assert build_entry(q, "B")["id"] == e["id"]
    print("  harvest 自检：切选项 / 拒无选项 / 拒错答案 / 判断题型 / 幂等 id 全过")


PLUGIN["selftest"] = selftest
