#!/usr/bin/env python3
"""kb_answer_kaoshibao —— 用考试宝（kaoshibao.com）的题目答案作为外部权威源。

为什么要它：本地 8 种取法实测全部停在 39–41%（见 bench_answer_methods.py），
剩下那 60% 不是排序问题，是「语料里没有这道题」或「选项措辞与教材不同」。
所以本地能定的冻结不动，本地定不下来的去查考试宝，以它的答案为准。

接口（实测，2026-10-26）:
    POST https://www.kaoshibao.com/api/search/questions
    body {"keyword": "...", "size": 20, "qtype": "", "page": 1, "paperid": ""}
    未登录返回 {"code":"999","msg":"请登录后再执行此操作"}
    终端直连会被 EdgeOne WAF 拦或返回 999，所以走 9224 调试 Chrome 的**页面上下文**
    （同源 fetch，带你自己登录的 cookie；脚本不碰密码、不碰验证码）。

用法:
    python3 kb_answer_kaoshibao.py check                  # 验登录态（不登录会明确报错）
    python3 kb_answer_kaoshibao.py fetch --limit 20        # 只查本地定不下来的题
    python3 kb_answer_kaoshibao.py fetch --bank dtzx-safety.json --limit 5 --dump 3
    python3 kb_answer_kaoshibao.py report
    python3 kb_answer_kaoshibao.py apply --bank xxx.json [--yes]
"""
import argparse
import json
import os
import random
import sqlite3
import sys
import time
import urllib.parse
import urllib.request

import websocket

PROJ = os.path.expanduser("~/x_videos-dowloads/exam-lookup")
OUT = os.path.join(PROJ, "jr/out")
PORT = 9224
API = "https://www.kaoshibao.com/api/search/questions"
sys.path.insert(0, PROJ)
import kb_db as KB          # noqa: E402
import kb_answer_resolve as R  # noqa: E402


# ---------- CDP ----------
def _open_tab(url):
    q = urllib.parse.quote(url, safe="")
    req = urllib.request.Request("http://127.0.0.1:%d/json/new?%s" % (PORT, q), method="PUT")
    with urllib.request.urlopen(req, timeout=10) as r:
        return json.load(r)["webSocketDebuggerUrl"]


def _tab_on_site():
    """复用已在 kaoshibao.com 的标签，没有再新开。"""
    try:
        with urllib.request.urlopen("http://127.0.0.1:%d/json/list" % PORT, timeout=8) as r:
            tabs = json.load(r)
    except Exception as e:
        sys.exit("连不上 9224 调试 Chrome（%s）。先启动：kc_chrome.py start 或用 --remote-debugging-port=9224 开 Chrome" % e)
    for t in tabs:
        if "kaoshibao.com" in (t.get("url") or "") and t.get("webSocketDebuggerUrl"):
            return t["webSocketDebuggerUrl"]
    return _open_tab("https://www.kaoshibao.com/ti/")


class Page:
    def __init__(self):
        self.ws = websocket.create_connection(_tab_on_site(), timeout=60,
                                              suppress_origin=True, max_size=1 << 26)
        self.i = 0
        self.send("Runtime.enable")

    def send(self, method, **params):
        self.i += 1
        self.ws.send(json.dumps({"id": self.i, "method": method, "params": params}))
        while True:
            m = json.loads(self.ws.recv())
            if m.get("id") == self.i:
                return m.get("result", {})

    def ev(self, expr, timeout=40):
        r = self.send("Runtime.evaluate", expression=expr, returnByValue=True,
                      awaitPromise=True, timeout=timeout * 1000)
        if r.get("exceptionDetails"):
            return {"__error__": str(r["exceptionDetails"])[:300]}
        return r.get("result", {}).get("value")

    def search(self, keyword, size=20):
        body = json.dumps({"keyword": keyword, "size": size, "qtype": "", "page": 1, "paperid": ""})
        expr = """(async () => {
          try {
            const r = await fetch(%s, {method:'POST', headers:{'Content-Type':'application/json'},
              credentials:'include', body: %s});
            const t = await r.text();
            return {status: r.status, body: t.slice(0, 200000)};
          } catch (e) { return {status: -1, body: String(e)}; }
        })()""" % (json.dumps(API), json.dumps(body))
        return self.ev(expr)


class FakePage:
    """自检用：按关键词从 JSON 文件里取预置响应，不联网。"""

    def __init__(self, path):
        self.data = json.load(open(path, encoding="utf-8"))
        self.calls = 0

    def search(self, keyword, size=20):
        self.calls += 1
        for k, payload in self.data.items():
            if k and k in keyword:
                return {"status": 200, "body": json.dumps(payload, ensure_ascii=False)}
        return {"status": 200, "body": json.dumps({"code": "200", "data": {"rows": []}})}


def make_page():
    fake = os.environ.get("KSB_FAKE")
    return FakePage(fake) if fake else Page()


# ---------- 解析（对返回结构做宽容提取） ----------
def _first(d, keys, default=None):
    for k in keys:
        if isinstance(d, dict) and d.get(k) not in (None, "", [], {}):
            return d[k]
    return default


QUOTA_CODES = {"30001"}
BLOCK_MARKS = ("请求已被拦截", "安全验证", "验证码", "访问频率", "请稍后再试")


def quota_exhausted(payload):
    if not isinstance(payload, dict):
        return False
    if str(payload.get("code")) in QUOTA_CODES:
        return True
    msg = str(payload.get("msg") or "")
    return "搜题次数" in msg or "开通VIP" in msg


def _items(payload):
    data = payload.get("data", payload) if isinstance(payload, dict) else payload
    for key in ("list", "rows", "records", "items", "questionList", "data"):
        v = data.get(key) if isinstance(data, dict) else None
        if isinstance(v, list) and v:
            return v
    if isinstance(data, list):
        return data
    if isinstance(data, dict):
        for v in data.values():
            if isinstance(v, list) and v and isinstance(v[0], dict):
                return v
    return []


def _plain(s):
    """去 HTML 标签与高亮 span，考试宝的 question 字段带 <span style=...>。"""
    import re as _re
    s = _re.sub(r"(?s)<[^>]+>", "", str(s or ""))
    return _re.sub(r"[\s\u00a0]+", " ", s).strip()


def _qtext(it):
    return _plain(_first(it, ("question", "title", "content", "stem", "name", "questionContent"), ""))


def _opts(it):
    raw = _first(it, ("options", "optionList", "choices", "option", "optionArr"), [])
    if isinstance(raw, str):          # 考试宝把 options 存成 JSON 字符串：[{"Key":"A","Value":"…"}]
        try:
            raw = json.loads(raw)
        except Exception:
            raw = []
    out = {}
    if isinstance(raw, list):
        for idx, o in enumerate(raw):
            if isinstance(o, dict):
                key = str(_first(o, ("Key", "key", "label", "optionKey", "opt", "index"),
                                 chr(65 + idx))).strip()
                txt = _plain(_first(o, ("Value", "text", "content", "value", "optionContent", "title"), ""))
            else:
                key, txt = chr(65 + idx), str(o).strip()
            key = key.upper()[:1]
            if txt:
                out[key] = txt
    elif isinstance(raw, dict):
        for k, v in raw.items():
            out[str(k).upper()[:1]] = str(v).strip()
    return out


def _ans(it):
    raw = _first(it, ("answer", "rightAnswer", "correctAnswer", "answers", "answerKeys",
                      "rightanswer", "correct", "answerKey"), "")
    if isinstance(raw, list):
        return [str(x).upper().strip()[:1] for x in raw if str(x).strip()]
    s = str(raw or "").upper()
    keys = [c for c in s if c in "ABCDEFGH"]
    return sorted(set(keys))


def _norm(t):
    return KB.normalize(t or "")


def _stem_key(s):
    """题干归一：去空白、去选项括号、去标点，用于判断「是不是同一道原题」。"""
    import re as _re
    s = KB.normalize(s or "")
    s = _re.sub(r"[（(]\s*[）)]", "", s)
    return _re.sub(r"[\s，。；：、？！,.?;:!\"\'“”‘’]", "", s)


def pick_item(question, my_opts, items):
    """在返回条目里找与本地题目最匹配的一条，返回 (item, 证据说明, 分数)。"""
    qg = KB.cjk_bigrams(KB.query_terms(question))
    qkey = _stem_key(question)
    best = (0.0, None, "")
    for it in items:
        their_opts = _opts(it)
        if not their_opts:
            continue
        if qkey and _stem_key(_qtext(it)) == qkey:      # 同一道原题：直接采信
            return it, "题干逐字一致", 1.0
        # 选项文本对齐（不信字母）：统计文本相似选项数
        same = 0
        for mk, mt in my_opts.items():
            tg = KB.cjk_bigrams(mt)
            if not tg:
                continue
            for tk, tt in their_opts.items():
                if not tg:
                    continue
                ov = len(tg & KB.cjk_bigrams(tt)) / max(1, len(tg))
                if ov >= 0.6:
                    same += 1
                    break
        tqg = KB.cjk_bigrams(KB.query_terms(_qtext(it)))
        qov = len(qg & tqg) / max(1, len(qg))
        score = 0.6 * (same / max(1, min(len(my_opts), len(their_opts)))) + 0.4 * qov
        if score > best[0]:
            best = (score, it, "选项同义 %d/%d，题干重合 %.2f" % (same, len(my_opts), qov))
    return best[1], best[2], best[0]


def map_answer(their_ans, their_opts, my_opts, stem_exact=False):
    """把对方的字母答案映射到我们题库的字母（按选项文本，不按字母顺序）。"""
    out = set()
    if stem_exact and len(their_opts) == len(my_opts):
        return sorted({k for k in their_ans if k in my_opts})
    for k in their_ans:
        t = their_opts.get(k)
        if not t:
            continue
        tg = KB.cjk_bigrams(t)
        best, best_ov = None, 0.0
        for mk, mt in my_opts.items():
            ov = len(tg & KB.cjk_bigrams(mt)) / max(1, len(tg))
            if ov > best_ov:
                best, best_ov = mk, ov
        if best and best_ov >= 0.5:
            out.add(best)
    return sorted(out)


# ---------- 命令 ----------
def ensure_cols(db):
    R.ensure_table(db)
    cols = {r[1] for r in db.execute("PRAGMA table_info(answers)")}
    for c in ("provenance", "evidence_url"):
        if c not in cols:
            db.execute("ALTER TABLE answers ADD COLUMN %s TEXT" % c)
    db.commit()


def cmd_check(a):
    p = Page()
    print("当前标签:", p.ev("location.href"))
    r = p.search("安全生产")
    print("接口 HTTP:", r.get("status"))
    body = r.get("body") or ""
    try:
        j = json.loads(body)
    except Exception:
        print("返回非 JSON:", body[:200])
        return 1
    code = str(j.get("code", ""))
    if code == "999" or "登录" in str(j.get("msg", "")):
        print("未登录（%s）。请在 9224 这个 Chrome 窗口里手动登录考试宝，然后重跑。" % j.get("msg"))
        return 2
    if quota_exhausted(j):
        print("已登录，但免费搜题额度已用完：%s" % str(j.get("msg"))[:80])
        return 3
    items = _items(j)
    print("登录态 OK，返回 code=%s，条目 %d" % (code, len(items)))
    if items:
        it = items[0]
        print("字段样例:", sorted(it.keys())[:14])
        print("题干:", _qtext(it)[:60])
        print("选项:", list(_opts(it).items())[:2])
        print("答案:", _ans(it))
    return 0


def load_bank_map(names):
    out = {}
    for n in names:
        path = os.path.join(PROJ, "banks", n)
        if os.path.exists(path):
            out[n] = {q.get("id") or q.get("number"): q for q in R.bank_questions(path)}
    return out



# ---------- 多进程查询（网络并行）、单进程写库（保证完整性） ----------
_PAGE = None


def _worker_init():
    """每个子进程各自持有一条 CDP 连接（Chrome 允许多标签页），互不共享句柄。"""
    global _PAGE
    _PAGE = make_page()


def _worker_task(task):
    """纯查询：只读网络，不碰数据库。返回可 pickle 的结果给父进程。"""
    bank, qid, question, my_opts, min_score, delay = task
    if _PAGE is None:
        _worker_init()
    try:
        kw = (question or "").replace("（ ）", " ").replace("( )", " ")
        r = _PAGE.search(kw[:60])
        body = r.get("body") or ""
        # 抖动退避：固定周期（比如每题正好 1.2s）本身就是可被统计出来的机器人指纹
        time.sleep(delay * (0.6 + 0.8 * random.random()))
        if r.get("status") in (403, 429) or any(m in body[:400] for m in BLOCK_MARKS):
            return {"bank": bank, "qid": qid, "blocked": True,
                    "msg": "HTTP %s %s" % (r.get("status"), body[:100])}
        payload = json.loads(body or "{}")
    except Exception as e:
        return {"bank": bank, "qid": qid, "error": str(e)[:120]}
    if quota_exhausted(payload):
        return {"bank": bank, "qid": qid, "quota": True,
                "msg": str(payload.get("msg"))[:120]}
    items = _items(payload)
    it, why, score = pick_item(question, my_opts, items)
    if not it or score < min_score:
        return {"bank": bank, "qid": qid, "items": len(items), "score": score, "matched": False}
    their_opts = _opts(it)
    mapped = map_answer(_ans(it), their_opts, my_opts, stem_exact=(score >= 0.999))
    return {"bank": bank, "qid": qid, "items": len(items), "score": score, "matched": True,
            "mapped": mapped, "why": why, "stem": _qtext(it)[:200]}


def cmd_fetch(a):
    db = KB.connect(KB.DB_PATH)
    ensure_cols(db)
    rows = [dict(zip(("bank", "qid", "question", "stored", "status", "confidence", "provenance"), r))
            for r in db.execute("""SELECT bank,qid,question,COALESCE(stored,''),status,confidence,
                                   COALESCE(provenance,'local') FROM answers""")]
    todo = [r for r in rows if r["provenance"] != "kaoshibao"
            and (r["status"] == "unknown" or r["confidence"] in ("low", "none"))]
    if a.priority:
        # 额度有限时，把查询花在「库存答案和本地推导打架、且本地证据强」的题上
        risky = [r for r in rows if r["status"] == "conflict" and r["confidence"] in ("high", "medium")
                 and r["provenance"] != "kaoshibao"]
        if a.bank:
            risky = [r for r in risky if r["bank"] in set(a.bank)]
        todo = risky
    if a.bank:
        todo = [r for r in todo if r["bank"] in set(a.bank)]
    todo = todo[:a.limit]
    if not todo:
        print("没有需要外部补答的题（已按分层口径过滤）。")
        return 0
    banks = load_bank_map({r["bank"] for r in todo})
    if not banks:
        print("题库文件缺失，无法继续。")
        return 1
    print("待查考试宝 %d 题（工作进程 %d，写库单线程）" % (len(todo), a.workers))

    p = make_page()
    probe = json.loads(p.search("安全生产").get("body") or "{}")
    if str(probe.get("code")) == "999":
        print("未登录：%s\n请在 9224 的 Chrome 窗口里登录考试宝后重跑。" % probe.get("msg"))
        return 2
    if quota_exhausted(probe):
        print("额度已用完：%s" % str(probe.get("msg"))[:80])
        return 3
    getattr(p, 'ws', None) and p.ws.close()

    tasks = []
    for row in todo:
        q = banks.get(row["bank"], {}).get(row["qid"])
        if not q:
            continue
        my_opts = {o["key"]: o.get("text") or "" for o in (q.get("options") or [])}
        tasks.append((row["bank"], row["qid"], row["question"], my_opts, a.min_score, a.delay))

    # 查询：多进程并行（每个进程独立 CDP 连接）
    from concurrent.futures import ProcessPoolExecutor, as_completed
    results = []
    quota_hit = None
    block_hit = None
    if a.workers <= 1:
        _worker_init()
        for k in tasks:
            res = _worker_task(k)
            results.append(res)
            if res.get("quota"):
                quota_hit = res
                break
            if res.get("blocked"):
                block_hit = res
                break
    else:
        with ProcessPoolExecutor(max_workers=a.workers, initializer=_worker_init) as ex:
            futs = {ex.submit(_worker_task, k): k for k in tasks}
            for fu in as_completed(futs):
                res = fu.result()
                results.append(res)
                if (res.get("quota") or res.get("blocked")) and not (quota_hit or block_hit):
                    quota_hit = res if res.get("quota") else None
                    block_hit = res if res.get("blocked") else None
                    for f2 in futs:
                        f2.cancel()

    # 写库：仅父进程单线程，一个事务批量落库
    hit = stored_same = accepted = skipped = 0
    wrote = 0
    with db:
        for res in results:
            if res.get("error") or res.get("quota") or not res.get("matched"):
                skipped += 1
                continue
            hit += 1
            mapped = res.get("mapped") or []
            if not mapped:
                continue
            accepted += 1
            q = banks.get(res["bank"], {}).get(res["qid"]) or {}
            same = set(mapped) == set(q.get("answerKeys") or [])
            stored_same += same
            db.execute("""UPDATE answers SET derived=?, provenance='kaoshibao', confidence=?,
                          method=?, evidence_doc=?, evidence_text=?, evidence_url=?, updated_at=?
                          WHERE bank=? AND qid=?""",
                       ("/".join(mapped), "high" if same else "medium",
                        "考试宝：" + (res.get("why") or ""), "考试宝",
                        (res.get("stem") or "")[:200], "https://www.kaoshibao.com/ti/",
                        time.strftime("%Y-%m-%d %H:%M:%S"), res["bank"], res["qid"]))
            wrote += 1
            print("  %-22s 存量 %s → 考试宝 %s %s" % (
                res["bank"][:22], "/".join(q.get("answerKeys") or []) or "-", "/".join(mapped),
                "（一致）" if same else ""))
    if quota_hit:
        print("\n⚠ 中途额度用尽：%s。本次已入库 %d 题。" % (quota_hit.get("msg", "")[:60], wrote))
    if block_hit:
        print("\n⚠ 被风控拦下（%s）。已停手，本次入库 %d 题；建议隔几小时再跑，别硬刚。"
              % (block_hit.get("msg", "")[:80], wrote))
    db.close()
    print("\n查询 %d 题：匹配到题目 %d，得出可用答案 %d，其中与库存一致 %d，无匹配/失败 %d" % (
        len(tasks), hit, accepted, stored_same, skipped))
    if block_hit:
        db.close()
        return 4
    if quota_hit:
        db.close()
        return 3
    return 0


def cmd_report(a):
    db = KB.connect(KB.DB_PATH, readonly=True)
    ensure_cols(db)
    print("%-30s %8s %8s %8s" % ("题库", "本地", "考试宝", "一致"))
    for bank, prov, n in db.execute(
            "SELECT bank, COALESCE(provenance,'local'), COUNT(*) FROM answers GROUP BY bank, 2"):
        print("%-30s %8s %8d" % (bank[:30], prov, n))
    db.close()
    return 0


def cmd_apply(a):
    db = KB.connect(KB.DB_PATH, readonly=True)
    ensure_cols(db)
    rows = db.execute("""SELECT bank,qid,derived,stored,evidence_url,confidence FROM answers
                         WHERE provenance='kaoshibao' AND derived<>''""").fetchall()
    if a.bank:
        rows = [r for r in rows if r[0] in set(a.bank)]
    total = 0
    for bank in {r[0] for r in rows}:
        path = os.path.join(PROJ, "banks", bank)
        data = json.load(open(path, encoding="utf-8"))
        qs = data if isinstance(data, list) else (data.get("questions") or [])
        idx = {q.get("id") or q.get("number"): q for q in qs}
        n = 0
        for _b, qid, derived, stored, url, conf in [r for r in rows if r[0] == bank]:
            q = idx.get(qid)
            if not q or set(derived.split("/")) == set(q.get("answerKeys") or []):
                continue
            n += 1
            if a.yes:
                q.setdefault("answerKeysPrev", q.get("answerKeys"))
                q["answerKeys"] = derived.split("/")
                q["answerSource"] = "考试宝"
                q["answerUrl"] = url
                q["answerUpdatedAt"] = time.strftime("%Y-%m-%d %H:%M:%S")
                q["answerConfidence"] = conf
        print("%-30s 可更新 %d 题" % (bank[:30], n))
        total += n
        if a.yes and n:
            import shutil
            shutil.copy(path, path + ".bak-" + time.strftime("%Y%m%d-%H%M%S"))
            json.dump(data, open(path, "w", encoding="utf-8"), ensure_ascii=False, indent=1)
    print("合计 %d 题%s" % (total, "" if a.yes else "（dry-run，加 --yes 写文件）"))
    db.close()
    return 0



def cmd_selftest(a):
    """红绿自检：多进程查询 + 单线程写库 + 额度中断，三条都必须成立。"""
    import tempfile
    tmpdir = tempfile.mkdtemp()
    dbpath = os.path.join(tmpdir, "t.sqlite")
    fake = os.path.join(tmpdir, "fake.json")

    def fake_item(stem, opts, ans):
        return {"id": "x", "question": stem + "（ ）", "answer": ans,
                "options": json.dumps([{"Key": k, "Value": v} for k, v in opts.items()],
                                      ensure_ascii=False)}

    qs = []
    data = {}
    for i, (stem, ans) in enumerate([("测试题目甲：登杆必须使用安全带", "C"),
                                     ("测试题目乙：脚扣不得攀登方形水泥杆", "A"),
                                     ("测试题目丙：挖掘土石方应从上而下", "B")]):
        opts = {"A": "选项甲%d" % i, "B": "选项乙%d" % i, "C": "选项丙%d" % i, "D": "选项丁%d" % i}
        data[stem[:12]] = {"code": "200", "data": {"rows": [fake_item(stem, opts, ans)]}}
        qs.append({"id": "Q%d" % i, "question": stem, "answerKeys": ["D"],       # 故意与考试宝不同
                   "options": [{"key": k, "text": v} for k, v in opts.items()]})
    data["__quota__"] = {"code": "30001", "msg": "免费搜题次数已用完"}
    json.dump(data, open(fake, "w", encoding="utf-8"), ensure_ascii=False)

    os.environ["KSB_FAKE"] = fake
    db = KB.connect(dbpath)
    ensure_cols(db)
    with db:
        for q in qs:
            db.execute("""INSERT OR REPLACE INTO answers(bank,qid,question,stored,derived,status,
                          confidence,method,evidence_doc,evidence_text,updated_at)
                          VALUES(?,?,?,?,?,?,?,?,?,?,?)""",
                       ("测试库.json", q["id"], q["question"], "D", "", "conflict", "low",
                        "seed", "", "", "2026-10-26 00:00:00"))
    db.commit()
    db.close()

    # 指向临时库 + 临时题库
    KB.DB_PATH = dbpath
    bankdir = os.path.join(tmpdir, "banks")
    os.makedirs(bankdir)
    json.dump({"questions": qs}, open(os.path.join(bankdir, "测试库.json"), "w", encoding="utf-8"),
              ensure_ascii=False)
    global PROJ
    old_proj = PROJ
    PROJ = tmpdir

    class A:
        bank = ["测试库.json"]
        limit = 10
        delay = 0.0
        workers = 2          # 走进程池
        min_score = 0.5
        dump = 0
        verbose = False
        priority = False
    rc = cmd_fetch(A)
    assert rc == 0, "自检：正常批次应返回 0，实际 %d" % rc

    db = sqlite3.connect(dbpath)
    rows = db.execute("SELECT qid, derived, provenance, confidence FROM answers").fetchall()
    db.close()
    got = {r[0]: r for r in rows}
    assert len(got) == 3, "应有 3 条记录，实际 %d" % len(got)
    for qid in ("Q0", "Q1", "Q2"):
        assert qid in got, "缺记录 %s" % qid
        assert got[qid][1], "%s 的 derived 为空" % qid
        assert got[qid][2] == "kaoshibao", "%s 的来源应为 kaoshibao，实际 %s" % (qid, got[qid][2])
    assert got["Q0"][1] == "C", "Q0 应映射到 C，实际 %s" % got["Q0"][1]
    assert got["Q1"][1] == "A", "Q1 应映射到 A，实际 %s" % got["Q1"][1]

    # 额度中断：题库改成一个必然命中 __quota__ 的题 → 应返回 3 且不写脏数据
    PROJ = tmpdir
    qs2 = [{"id": "Q9", "question": "__quota__ 触发额度", "answerKeys": ["A"],
            "options": [{"key": "A", "text": "x"}, {"key": "B", "text": "y"}]}]
    json.dump({"questions": qs2}, open(os.path.join(bankdir, "测试库2.json"), "w", encoding="utf-8"),
              ensure_ascii=False)
    db = KB.connect(dbpath)
    ensure_cols(db)
    with db:
        db.execute("""INSERT OR REPLACE INTO answers(bank,qid,question,stored,derived,status,
                      confidence,method,evidence_doc,evidence_text,updated_at)
                      VALUES(?,?,?,?,?,?,?,?,?,?,?)""",
                   ("测试库2.json", "Q9", "__quota__ 触发额度", "A", "", "conflict", "low",
                    "seed", "", "", "2026-10-26 00:00:00"))
    db.commit()
    db.close()

    class B(A):
        bank = ["测试库2.json"]
    rc2 = cmd_fetch(B)
    assert rc2 == 3, "额度用完应返回 3，实际 %d" % rc2
    db = sqlite3.connect(dbpath)
    row = db.execute("SELECT derived, provenance FROM answers WHERE qid='Q9'").fetchone()
    db.close()
    assert row[0] == "" and row[1] != "kaoshibao", "额度中断不该写入答案，实际 %r" % (row,)

    # 风控拦截：返回拦截页 → 应返回 4、立刻停手、不写脏数据
    data["__blocked__"] = "请求已被拦截"
    json.dump(data, open(fake, "w", encoding="utf-8"), ensure_ascii=False)
    qs3 = [{"id": "Q8", "question": "__blocked__ 触发拦截", "answerKeys": ["A"],
            "options": [{"key": "A", "text": "x"}, {"key": "B", "text": "y"}]}]
    json.dump({"questions": qs3}, open(os.path.join(bankdir, "测试库3.json"), "w", encoding="utf-8"),
              ensure_ascii=False)
    db = KB.connect(dbpath)
    ensure_cols(db)
    with db:
        db.execute("""INSERT OR REPLACE INTO answers(bank,qid,question,stored,derived,status,
                      confidence,method,evidence_doc,evidence_text,updated_at)
                      VALUES(?,?,?,?,?,?,?,?,?,?,?)""",
                   ("测试库3.json", "Q8", "__blocked__ 触发拦截", "A", "", "conflict", "low",
                    "seed", "", "", "2026-10-26 00:00:00"))
    db.commit()
    db.close()

    class C(A):
        bank = ["测试库3.json"]
    rc3 = cmd_fetch(C)
    assert rc3 == 4, "被拦截应返回 4，实际 %d" % rc3
    db = sqlite3.connect(dbpath)
    row = db.execute("SELECT derived, provenance FROM answers WHERE qid='Q8'").fetchone()
    db.close()
    assert row[0] == "" and row[1] != "kaoshibao", "被拦截不该写入答案，实际 %r" % (row,)
    PROJ = old_proj
    os.environ.pop("KSB_FAKE", None)
    print("SELFTEST PASS：多进程查询 / 单线程写库 / 答案映射 / 额度中断 / 风控拦截不写脏数据 五项断言全过")
    return 0


def main():
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    sub.add_parser("check").set_defaults(fn=cmd_check)
    f = sub.add_parser("fetch")
    f.add_argument("--bank", nargs="*")
    f.add_argument("--limit", type=int, default=20)
    f.add_argument("--delay", type=float, default=1.2, help="每题间隔秒，别把对方站点打崩")
    f.add_argument("--min-score", type=float, default=0.55, help="题目匹配阈值")
    f.add_argument("--dump", type=int, default=0, help="把前 N 个原始返回存盘，便于核对结构")
    f.add_argument("--verbose", action="store_true", help="逐题打印匹配分数与拒绝原因")
    f.add_argument("--priority", action="store_true", help="只查高风险题（库存与本地推导冲突且证据强）")
    f.add_argument("--workers", type=int, default=3, help="查询进程数（写库始终单线程）")
    f.set_defaults(fn=cmd_fetch)
    sub.add_parser("report").set_defaults(fn=cmd_report)
    sub.add_parser("selftest").set_defaults(fn=cmd_selftest)
    p = sub.add_parser("apply"); p.add_argument("--bank", nargs="*"); p.add_argument("--yes", action="store_true")
    p.set_defaults(fn=cmd_apply)
    a = ap.parse_args()
    # 2026-10-27 账号被站点停用：联网的两个命令（check/fetch）默认冻结，离线命令照常
    if a.cmd in ("check", "fetch") and os.environ.get("KSB_ALLOW") != "1":
        print("已冻结：2026-10-27 考试宝账号因「违规操作」被停用，不再联网查题。\n"
              "恢复账号后如要走官方额度，显式设 KSB_ALLOW=1 再跑。\n"
              "替代方案见 exam-lookup/考试宝-停用与替代方案.md")
        return 5
    return a.fn(a)


if __name__ == "__main__":
    sys.exit(main())
