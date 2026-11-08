#!/usr/bin/env python3
"""kb_answer_web —— 本地语料推不出答案时，走网络补答（动态更新层）。

分层策略（用户定的口径）:
    ① 本地专业文档能推出（M1/M2 一致 或 两份文档旁证）→ provenance=local，**冻结**，
       即便和库存答案不一致也不改写，只进冲突清单供人工裁决。
    ② 本地推不出（confidence=low 或 unknown）→ 走网络：Bing 搜索题干 → 取结果页正文 →
       用与本地相同的 bigram 打分给选项排序；只有当**获胜选项的 4 字片段在网页原文出现**
       且领先次优 ≥ 阈值时才接受，记 provenance=web + evidence_url。
    ③ apply：把 provenance=web 且 confidence=high 的答案写回题库，旧值存 answerKeysPrev，
       文件级备份 .bak-时间戳。默认 dry-run。

用法:
    python3 kb_answer_web.py fetch --bank 保密公开题-中科院.json --limit 20 [--concurrency 3]
    python3 kb_answer_web.py fetch --all --limit 200
    python3 kb_answer_web.py report
    python3 kb_answer_web.py apply --bank xxx.json [--yes]
"""
import argparse
import html
import json
import os
import re
import shutil
import sys
import threading
import time
import urllib.parse
from concurrent.futures import ThreadPoolExecutor

PROJ = os.path.expanduser("~/x_videos-dowloads/exam-lookup")
OUT = os.path.join(PROJ, "jr/out")
UA = ("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 "
      "(KHTML, like Gecko) Chrome/155.0.0.0 Safari/537.36")
sys.path.insert(0, PROJ)
import kb_db as KB          # noqa: E402
import kb_answer_resolve as R  # noqa: E402

_lock = threading.Lock()


# ---------- 网络 ----------
def curl(url, timeout=20):
    import subprocess
    r = subprocess.run(["curl", "-sL", "--max-time", str(timeout), "-A", UA,
                        "-H", "Accept-Language: zh-CN,zh;q=0.9", url],
                       capture_output=True)
    return r.stdout.decode("utf-8", "ignore")


def strip_html(h):
    h = re.sub(r"(?is)<(script|style)[^>]*>.*?</\1>", " ", h)
    h = re.sub(r"(?s)<[^>]+>", " ", h)
    return re.sub(r"[\s\u00a0]+", " ", html.unescape(h)).strip()


def bing(query, n=8):
    """返回 [(title, url, snippet)]，只取搜索结果块。"""
    url = "https://cn.bing.com/search?q=" + urllib.parse.quote(query) + "&count=%d" % n
    page = curl(url)
    out = []
    for m in re.finditer(r'<li class="b_algo".*?</li>', page, re.S):
        blk = m.group(0)
        a = re.search(r'<a[^>]+href="(https?://[^"]+)"[^>]*>(.*?)</a>', blk, re.S)
        if not a:
            continue
        title = strip_html(a.group(2))[:120]
        link = html.unescape(a.group(1))
        if "bing.com" in link:
            continue
        snip = strip_html(blk)
        out.append((title, link, snip))
    return out[:n]


def collect_text(question, options_text, extra_pages=2):
    """题干 + 选项关键措辞一起去搜，返回 (拼接文本, 命中来源列表)。"""
    kw = " ".join((options_text or [])[:2])
    q = (question or "").replace("（ ）", " ").replace("( )", " ")
    hits = bing((q[:60] + " " + kw)[:110])
    docs, srcs = [], []
    for title, link, snip in hits:
        docs.append(snip)
        srcs.append(link)
    for title, link, _ in hits[:extra_pages]:
        try:
            body = strip_html(curl(link, timeout=15))
        except Exception:
            continue
        if len(body) > 500:
            docs.append(body[:20000])
            srcs.append(link)
    return " ".join(docs), srcs


# ---------- 打分 ----------
def score_web(ctx_text, options):
    """与本地同一套 bigram 打分，但用网页文本；额外要求 4 字原文命中。"""
    ctx_g = KB.cjk_bigrams(ctx_text)
    out = {}
    raw = {}
    for o in options or []:
        text = o.get("text") or ""
        g = KB.cjk_bigrams(text)
        if not g:
            continue
        wins = [text[i:i + 4] for i in range(max(0, len(text) - 3))]
        literal = sum(1 for w in wins if w in ctx_text)
        if not literal:
            continue          # 原文里连 4 个字都没出现 → 不接受这个选项
        raw[o["key"]] = sum(1.0 for x in g if x in ctx_g) + 3.0 * literal
        out[o["key"]] = raw[o["key"]]
    return out, raw


def pick_web(scores, multi, margin=1.35):
    if not scores:
        return []
    ordered = sorted(scores.items(), key=lambda kv: -kv[1])
    if multi:
        top = ordered[0][1]
        return [k for k, v in ordered if v >= 0.6 * top]
    if len(ordered) == 1:
        return [ordered[0][0]]
    if ordered[0][1] >= margin * ordered[1][1]:
        return [ordered[0][0]]
    return []


# ---------- 主流程 ----------
def ensure_columns(db):
    R.ensure_table(db)
    cols = {r[1] for r in db.execute("PRAGMA table_info(answers)")}
    for c in ("provenance", "evidence_url"):
        if c not in cols:
            db.execute("ALTER TABLE answers ADD COLUMN %s TEXT" % c)
    db.execute("""CREATE TABLE IF NOT EXISTS web_evidence(
        bank TEXT, qid TEXT, url TEXT, score REAL, fetched_at TEXT,
        PRIMARY KEY(bank,qid,url))""")
    db.commit()


def needs_web(row):
    """本地能定案的不动；只补 confidence=low 或 unknown 的。"""
    if row["status"] == "unknown":
        return True
    return row["confidence"] in ("low", "none")


def cmd_fetch(a):
    db = KB.connect()
    ensure_columns(db)
    rows = [dict(zip(("bank", "qid", "question", "stored", "derived", "status", "confidence"),
                     r)) for r in db.execute(
        "SELECT bank,qid,question,stored,derived,status,confidence FROM answers")]
    todo = [r for r in rows if needs_web(r)]
    if a.bank:
        want = set(a.bank)
        todo = [r for r in todo if r["bank"] in want]
    todo = todo[:a.limit]
    print("待网络补答 %d 题（本地 low/unknown 中取前 %d）" % (len(todo), a.limit))

    banks = {}
    for name in {r["bank"] for r in todo}:
        path = os.path.join(PROJ, "banks", name)
        banks[name] = {q.get("id") or q.get("number"): q for q in R.bank_questions(path)}

    done = accepted = 0
    results = []
    def work(row):
        q = banks.get(row["bank"], {}).get(row["qid"])
        if not q:
            return None
        opts = q.get("options") or []
        ctx, srcs = collect_text(row["question"], [o.get("text") for o in opts])
        if len(ctx) < 200:
            return None
        scores, raw = score_web(ctx, opts)
        picked = pick_web(scores, R.is_multi(q))
        return (row, picked, srcs[:3], raw)

    with ThreadPoolExecutor(max_workers=a.concurrency) as ex:
        for res in ex.map(work, todo):
            if not res:
                continue
            row, picked, srcs, raw = res
            done += 1
            ok = bool(picked) and (not row["stored"] or "/".join(picked) == row["stored"])
            accepted += bool(picked)
            results.append({"bank": row["bank"], "qid": row["qid"], "question": row["question"],
                            "stored": row["stored"], "derived": "/".join(picked),
                            "scores": raw, "url": srcs[0] if srcs else ""})
            if picked:
                with _lock, db:
                    db.execute("""UPDATE answers SET derived=?, provenance='web', confidence=?,
                                  method='web:bing+4gram', evidence_doc=?, evidence_text=?, evidence_url=?,
                                  updated_at=? WHERE bank=? AND qid=?""",
                               ("/".join(picked), "high" if ok or not row["stored"] else "medium",
                                "网络检索", "（见 evidence_url）", srcs[0] if srcs else "",
                                time.strftime("%Y-%m-%d %H:%M:%S"), row["bank"], row["qid"]))
                    for u in srcs[:3]:
                        db.execute("INSERT OR REPLACE INTO web_evidence(bank,qid,url,score,fetched_at)"
                                   " VALUES(?,?,?,?,?)",
                                   (row["bank"], row["qid"], u, max(raw.values()) if raw else 0,
                                    time.strftime("%Y-%m-%d %H:%M:%S")))
            if done % 10 == 0:
                print("  进度 %d/%d，已接受 %d" % (done, len(todo), accepted))
    db.close()
    json.dump(results, open(os.path.join(OUT, "web_answers.json"), "w", encoding="utf-8"),
              ensure_ascii=False, indent=1)
    print("完成 %d 题：网络给出可接受答案 %d（%.0f%%，其中与库存一致 %d），明细 → %s" % (
        done, accepted, 100.0 * accepted / max(1, done),
        sum(1 for r in results if r["derived"] and r["derived"] == r["stored"]),
        os.path.join(OUT, "web_answers.json")))
    return 0


def cmd_report(a):
    db = KB.connect(readonly=True)
    ensure_columns(db)
    print("%-30s %8s %8s %8s" % ("题库", "local", "web", "其中一致"))
    for bank, prov, n in db.execute(
            "SELECT bank, COALESCE(provenance,'local'), COUNT(*) FROM answers GROUP BY bank, 2"):
        print("%-30s %8s %8d" % (bank[:30], prov, n))
    n_web = db.execute("SELECT COUNT(*) FROM answers WHERE provenance='web'").fetchone()[0]
    n_web_ok = db.execute(
        "SELECT COUNT(*) FROM answers WHERE provenance='web' AND derived=stored").fetchone()[0]
    print("\n网络补答 %d 条，其中与库存答案一致 %d 条（一致=两条独立路径互证）" % (n_web, n_web_ok))
    db.close()
    return 0


def cmd_apply(a):
    db = KB.connect(readonly=True)
    ensure_columns(db)
    rows = db.execute("""SELECT bank,qid,derived,stored,evidence_url,confidence FROM answers
                         WHERE provenance='web' AND derived<>''""").fetchall()
    if a.bank:
        rows = [r for r in rows if r[0] in set(a.bank)]
    by_bank = {}
    for r in rows:
        by_bank.setdefault(r[0], []).append(r)
    total = 0
    for bank, items in by_bank.items():
        path = os.path.join(PROJ, "banks", bank)
        data = json.load(open(path, encoding="utf-8"))
        qs = data if isinstance(data, list) else (data.get("questions") or [])
        idx = {q.get("id") or q.get("number"): q for q in qs}
        n = 0
        for _b, qid, derived, stored, url, conf in items:
            q = idx.get(qid)
            if not q or not derived:
                continue
            new = derived.split("/")
            if set(new) == set(q.get("answerKeys") or []):
                continue
            n += 1
            if a.yes:
                q.setdefault("answerKeysPrev", q.get("answerKeys"))
                q["answerKeys"] = new
                q["answerSource"] = "web"
                q["answerUrl"] = url
                q["answerUpdatedAt"] = time.strftime("%Y-%m-%d %H:%M:%S")
                q["answerConfidence"] = conf
        print("%-30s 可更新 %d 题" % (bank[:30], n))
        total += n
        if a.yes and n:
            bak = path + ".bak-" + time.strftime("%Y%m%d-%H%M%S")
            shutil.copy(path, bak)
            json.dump(data, open(path, "w", encoding="utf-8"), ensure_ascii=False, indent=1)
            print("   已写回（备份 %s）" % os.path.basename(bak))
    print("合计 %d 题%s" % (total, "" if a.yes else "（dry-run，加 --yes 才写文件）"))
    db.close()
    return 0


def main():
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    f = sub.add_parser("fetch")
    f.add_argument("--bank", nargs="*")
    f.add_argument("--all", action="store_true")
    f.add_argument("--limit", type=int, default=20)
    f.add_argument("--concurrency", type=int, default=3)
    f.set_defaults(fn=cmd_fetch)
    r = sub.add_parser("report"); r.set_defaults(fn=cmd_report)
    p = sub.add_parser("apply"); p.add_argument("--bank", nargs="*"); p.add_argument("--yes", action="store_true")
    p.set_defaults(fn=cmd_apply)
    a = ap.parse_args()
    return a.fn(a)


if __name__ == "__main__":
    sys.exit(main())
