#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
solve_pipeline.py —— 会自动长大的题库：本地命中 -> 联网兜底 -> 达标才入库 -> 否则排队
============================================================
零依赖（标准库 + curl）。目录结构由脚本自身位置决定，随包携带：

  <dirname>/banks/*.json     题库（已复核）
  <dirname>/learned.jsonl    自动/人工补录记录（只追加）
  <dirname>/pending.jsonl    待人工确认队列（含候选排序）
  <dirname>/qb.sqlite3       检索索引（可删，build 重建）

子命令
  build                      从 banks/*.json + learned.jsonl 重建索引
  ask "<题目>"               解一题：本地优先，未命中才联网；新题自动处理
  ask -f questions.txt       批量（一行一题）
  review                     查看待办队列（含候选排序）
  ingest "<题目>|<答案>"     人工确认后入库（入库的唯一闸门）
  merge                      把 learned.jsonl 合并成 banks/auto-learned.json
  stats                      规模统计

为什么要分四层 + 为什么要 ≥2 票：见 SKILL.md 的 Pitfalls 与
references/local-quiz-bank-prep.md「自动化」一节。**写错的答案比没有答案更糟。**
"""
import hashlib, json, os, re, sqlite3, subprocess, sys, html, time, random
import urllib.parse
from difflib import SequenceMatcher

ROOT = os.path.dirname(os.path.abspath(__file__))
BANKS = os.path.join(ROOT, "banks")
DB = os.path.join(ROOT, "qb.sqlite3")
LEARNED = os.path.join(ROOT, "learned.jsonl")
PENDING = os.path.join(ROOT, "pending.jsonl")
UA = ("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 "
      "(KHTML, like Gecko) Chrome/120.0 Safari/537.36")
COOKIE = "/tmp/qbank_cookies.txt"

_PUNC = re.compile(r"[\s，。、；：！？（）《》【】<>,.;:!?()\[\]\"'’‘“”\-—_/\\|]+")
def norm(s):
    """题干归一化：去标点空白。选项换序/标点差异都能命中，本地层秒回的关键。"""
    return _PUNC.sub("", (s or "")).strip()

def stem_of(s):
    """剥掉选项块，只留题干。用户粘进来的题常带「（ ）A.… B.…」，
    而库里存的是纯题干 —— 不剥就会走联网徒增延迟。"""
    t = re.split(r"[（(]\s*[)）]\s*", s or "")[0]
    if norm(t) == norm(s):
        t = re.split(r"\s*A[.、．]\s*", s or "")[0]
    return t.strip()

def sim(a, b):
    return SequenceMatcher(None, a, b).ratio()

COLS = ["id", "num", "type", "question", "nq", "nqstem", "answer",
        "answerText", "analysis", "src", "chapter"]

def db_init():
    c = sqlite3.connect(DB)
    have = [r[1] for r in c.execute("PRAGMA table_info(q)")]
    if have and have != COLS:            # 旧版本建的库：列对不上就重建
        c.execute("DROP TABLE q")
    c.execute("CREATE TABLE IF NOT EXISTS q (" + ", ".join(
        ["id TEXT PRIMARY KEY", "num INTEGER", "type TEXT", "question TEXT",
         "nq TEXT", "nqstem TEXT", "answer TEXT", "answerText TEXT",
         "analysis TEXT", "src TEXT", "chapter TEXT"]) + ")")
    c.execute("CREATE INDEX IF NOT EXISTS idx_nq ON q(nq)")
    return c

def build():
    os.makedirs(BANKS, exist_ok=True)
    c = db_init(); c.execute("DELETE FROM q"); n = 0
    for fn in sorted(os.listdir(BANKS)):
        if not fn.endswith(".json"):
            continue
        bank = json.load(open(os.path.join(BANKS, fn), encoding="utf-8"))
        meta = bank.get("meta") or {}
        if meta.get("generated") or meta.get("mergeOf") or meta.get("derived"):
            continue      # 生成题/合集只在刷题端用：生成题的「答案」是挖空值，混进查题结果会跟真题抢命中
        for q in bank.get("questions", []):
            at = q.get("answerText") or []
            at = at if isinstance(at, list) else [at]   # 防某解析器产出字符串时被逐字拆成「A、B、C」
            c.execute("INSERT OR REPLACE INTO q VALUES (?,?,?,?,?,?,?,?,?,?,?)", (
                q.get("id") or "%s-%s" % (fn, q.get("number")), q.get("number"),
                q.get("type"), q.get("question"), norm(q.get("question")),
                norm(stem_of(q.get("question"))),
                "/".join(q.get("answerKeys", [])),
                "、".join(at),
                q.get("analysis", ""), fn, q.get("topic", "")))
            n += 1
    if os.path.exists(LEARNED):
        for line in open(LEARNED, encoding="utf-8"):
            line = line.strip()
            if not line:
                continue
            try:
                r = json.loads(line)
            except Exception:
                continue
            c.execute("INSERT OR REPLACE INTO q VALUES (?,?,?,?,?,?,?,?,?,?,?)", (
                r["id"], None, r.get("type", "unknown"), r["question"], norm(r["question"]),
                norm(stem_of(r["question"])),
                r["answer"], r.get("answerText", ""),
                "[自动补录 %s 来源:%s 票数:%s]" % (r.get("ts", ""), r.get("src", ""), r.get("votes", "")),
                "learned.jsonl", "自动收录"))
            n += 1
    c.commit(); c.close()
    print("索引重建完成：%d 题 -> %s" % (n, DB))

def lookup(question, floor=0.72):
    """先全程精确/模糊，再拿「剥掉选项后的题干」比题库题干。"""
    c = sqlite3.connect(DB)
    full, stem = norm(question), norm(stem_of(question))
    for col, key in (("nq", full), ("nqstem", stem)):
        if not key:
            continue
        row = c.execute("SELECT * FROM q WHERE %s=?" % col, (key,)).fetchone()
        if row:
            c.close(); return row, 1.0
    best, score = None, 0.0
    for r in c.execute("SELECT * FROM q"):
        for col, key in ((r[4], full), (r[5], stem)):
            if not col or not key:
                continue
            s = sim(key, col)
            if s > score:
                best, score = r, s
    c.close()
    return (best, score) if score >= floor else (None, score)

def curl(url, tries=2):
    for _ in range(tries):
        try:
            r = subprocess.run(["curl", "-sSL", "-m", "25", "-A", UA,
                                "-H", "Accept-Language: zh-CN,zh;q=0.9",
                                "-b", COOKIE, "-c", COOKIE, url],
                               capture_output=True, text=True, timeout=35)
            if r.stdout and len(r.stdout) > 3000:
                return r.stdout
        except Exception:
            pass
        time.sleep(1.5)
    return ""

def query_of(question, maxlen=46):
    """检索词只留题干主干：长句带选项会被搜索引擎当噪声。"""
    s = re.sub(r"^[（(]?\s*[0-9〜~\-]{1,3}\s*[)）]?[、.．]?\s*", "", question.strip())
    s = re.split(r"[（(]\s*[)）]?", s)[0]
    s = re.split(r"\s*A[.、．]\s*", s)[0]
    s = re.sub(r"[，。,.]*$", "", s).strip()
    return (s[:maxlen] if s else question[:maxlen])

JUNK = re.compile(r"(\{[^}]{0,80}:[^}]{0,80}\}|\.g-|data-v-|@media|<style)")
LAST_URLS = []

def _good(t):
    return len(t) > 25 and not JUNK.search(t) and "反馈" not in t[:5]

def snippets(q):
    global LAST_URLS
    out, seen, urls = [], set(), []
    for name, tpl in (("360", "https://www.so.com/s?q={q}"),
                      ("sogou", "https://www.sogou.com/web?query={q}")):
        h = curl(tpl.format(q=urllib.parse.quote(q)))
        if not h or ("验证" in h and len(h) < 9000):
            continue
        for u in re.findall(r'<a[^>]+href="(https?://[^"]+)"', h):
            if not any(d in u for d in ("so.com", "sogou.com", "360.cn", "so.cn")):
                urls.append(html.unescape(u))
        for blk in re.split(r'<li class="res-list"', h)[1:] or re.split(r"<h3", h)[1:]:
            t = re.sub(r"\s+", " ", html.unescape(re.sub(r"<[^>]+>", " ", blk))).strip()[:400]
            if _good(t) and t not in seen:
                seen.add(t); out.append((name, t))
        time.sleep(random.uniform(1.2, 2.2))
    LAST_URLS = list(dict.fromkeys(urls))[:8]
    return out

ANS_PATS = [r"正确答案[是为：:]\s*([A-D]{1,4})", r"答案[是为：:]\s*([A-D]{1,4})",
            r"【答案】\s*([A-D]{1,4})", r"参考答案[是为：:]\s*([A-D]{1,4})"]
JUDGE_PATS = [r"答案[是为：:]\s*(正确|错误|对|错)", r"【答案】\s*(正确|错误|对|错)"]

def vote(sn, is_judge=False):
    """只认显式答案标记。摘要里的「A.xx B.xx C.xx」是题目自带选项，不是答案。"""
    from collections import Counter
    cnt, samples = Counter(), {}
    pats = JUDGE_PATS if is_judge else ANS_PATS
    for eng, t in sn:
        for p in pats:
            m = re.search(p, t)
            if m:
                a = re.sub(r"[、,，／/]", "", m.group(1))
                if is_judge:
                    a = a.replace("对", "正确").replace("错", "错误")
                cnt[a] += 1
                samples.setdefault(a, "[%s] %s" % (eng, t[:180]))
                break
    if not cnt:
        return None, 0, {}
    a, v = cnt.most_common(1)[0]
    return a, v, samples

def ingest(question, answer, answer_text="", src="", votes=0, qtype="unknown"):
    # id 必须能跨进程复现：Python 的 hash() 每进程随机加盐，同题两次录入会得到不同 id，
    # 于是 merge/build 的去重全失效（实测两次进程给出 284071136938 / 755604882297）。
    rec = {"id": "auto-" + hashlib.sha1(norm(question).encode("utf-8")).hexdigest()[:12],
           "question": question, "answer": answer, "answerText": answer_text,
           "type": qtype, "src": src, "votes": votes,
           "ts": time.strftime("%Y-%m-%d %H:%M:%S")}
    with open(LEARNED, "a", encoding="utf-8") as f:
        f.write(json.dumps(rec, ensure_ascii=False) + "\n")
    return rec

def candidates(question, sn):
    """选项按「选项用词 ∩ 证据用词」比例排序。只排序，不判定。"""
    opts = dict(re.findall(r"([A-D])[.、．]\s*([^A-D]{2,40}?)(?=\s*[A-D][.、．]|$)", question))
    ev = " ".join(t for _, t in sn)
    if not opts or not ev:
        return []
    toks = lambda s: set(re.findall(r"[\u4e00-\u9fa5]{2,}", s))
    evt = toks(ev); out = []
    for k, v in opts.items():
        ot = toks(v)
        if ot:
            out.append((round(len(ot & evt) / len(ot), 2), k, v.strip(), len(ot & evt)))
    return sorted(out, reverse=True)

def pending(question, q, sn=None, guess=None):
    cand = candidates(question, sn or [])
    rec = {"question": question, "query": q, "guess": guess,
           "candidates": [{"score": s, "key": k, "text": t, "overlap": o} for s, k, t, o in cand],
           "evidence": [t[:200] for _, t in (sn or [])][:4],
           "ts": time.strftime("%Y-%m-%d %H:%M:%S")}
    with open(PENDING, "a", encoding="utf-8") as f:
        f.write(json.dumps(rec, ensure_ascii=False) + "\n")
    return rec

def ask(question, do_web=True, auto_learn=True, verbose=True):
    is_judge = ("正确" in question and "错误" in question)
    row, score = lookup(question)
    if row:
        ans = row[6]
        src = "本地库精确" if score == 1.0 else "本地库模糊%.0f%%" % (score * 100)
        if verbose:
            print("  OK 命中 %s\n     答案: %s   (%s)" % (src, ans, row[7]))
            if row[8]:
                print("     解析: %s" % row[8][:120])
        return {"answer": ans, "source": src, "confidence": "high"}
    if not do_web:
        if verbose:
            print("  本地未命中（相似度 %.0f%%），未联网" % (score * 100))
        return {"answer": None, "source": "miss", "confidence": "none"}
    if verbose:
        print("  本地未命中（相似度 %.0f%%），联网检索…" % (score * 100))
    q = query_of(question)
    sn = snippets(q)
    if verbose:
        print("     检索词: %s   取回 %d 条" % (q, len(sn)))
    ans, v, samples = vote(sn, is_judge)
    if not ans:
        rec = pending(question, q, sn)
        if verbose:
            print("  摘要里没有显式答案标记（题库站把答案放在登录/付费钩子后）-> 已排队")
            for c in rec["candidates"][:4]:
                print("       %.2f  %s. %s" % (c["score"], c["key"], c["text"][:46]))
            print("     -> python3 solve_pipeline.py review")
        return {"answer": None, "source": "pending", "confidence": "none"}
    conf = "high" if v >= 3 else ("medium" if v == 2 else "low")
    if verbose:
        print("  检索答案: %s  票数 %d  置信度 %s" % (ans, v, conf))
        if samples.get(ans):
            print("     依据: %s" % samples[ans][:170])
    if auto_learn and v >= 2:
        rec = ingest(question, ans, src="+".join(sorted({s[0] for s in sn})), votes=v,
                     qtype="judge" if is_judge else "unknown")
        if verbose:
            print("  已自动回灌题库（%s），下次本地秒回" % rec["id"])
        return {"answer": ans, "source": "web+learned", "confidence": conf}
    pending(question, q, sn, guess=ans)
    if verbose:
        print("  只有 1 票，不足以自动入库 -> 进待办队列")
    return {"answer": ans, "source": "web-only", "confidence": conf}

def merge_banks():
    if not os.path.exists(LEARNED):
        print("还没有自动补录的题目"); return
    recs, seen = [], set()
    for line in open(LEARNED, encoding="utf-8"):
        line = line.strip()
        if not line:
            continue
        try:
            r = json.loads(line)
        except Exception:
            continue
        k = norm(r.get("question") or "") or ("id:" + str(r.get("id")))   # 按题干去重，不受旧版 hash id 影响
        if k in seen:
            continue
        seen.add(k); recs.append(r)
    qs = []
    for i, r in enumerate(recs, 1):
        qs.append({"id": r["id"], "volume": 1, "type": r["type"],
                   "typeLabel": {"single": "单选题", "multiple": "多选题",
                                 "judge": "判断题"}.get(r["type"], "自动收录"),
                   "number": i, "question": r["question"], "options": r.get("options") or [],
                   "answerKeys": list(r["answer"]), "answerText": [r.get("answerText", "")],
                   "topic": "自动补录", "category": "自动补录", "score": 1,
                   "analysis": "自动补录，来源 %s，票数 %s，时间 %s。未经人工复核。"
                               % (r.get("src"), r.get("votes"), r.get("ts"))})
    os.makedirs(BANKS, exist_ok=True)
    json.dump({"meta": {"title": "自动补录题库（未复核）", "questionCount": len(qs),
                        "volumes": 1, "sourceNote": "solve_pipeline.py 自动收录，未经人工复核。"},
               "questions": qs},
              open(os.path.join(BANKS, "auto-learned.json"), "w", encoding="utf-8"),
              ensure_ascii=False, indent=1)
    print("已合并 %d 道自动补录题 -> banks/auto-learned.json" % len(qs))

def stats():
    c = sqlite3.connect(DB)
    rows = list(c.execute("SELECT src, COUNT(*) FROM q GROUP BY src ORDER BY 2 DESC"))
    c.close()
    print("题库总量：%d 题" % sum(n for _, n in rows))
    for s, n in rows:
        print("  %5d  %s" % (n, s))

def main():
    if len(sys.argv) < 2:
        print(__doc__); return
    cmd = sys.argv[1]
    if cmd == "build":
        build()
    elif cmd == "stats":
        stats()
    elif cmd == "merge":
        merge_banks()
    elif cmd == "review":
        if not os.path.exists(PENDING):
            print("待办队列为空"); return
        n = 0
        for line in open(PENDING, encoding="utf-8"):
            line = line.strip()
            if not line:
                continue
            try:
                r = json.loads(line)
            except Exception:
                continue
            n += 1
            print("\n[%d] %s" % (n, r["question"][:78]))
            if r.get("guess"):
                print("    检索猜测: %s" % r["guess"])
            for c in r.get("candidates", [])[:4]:
                print("    %.2f  %s. %s" % (c["score"], c["key"], c["text"][:52]))
            if r.get("evidence"):
                print("    证据: %s" % r["evidence"][0][:100])
        if n:
            print('\n共 %d 题待确认，定下来后：python3 solve_pipeline.py ingest "<题目>|<答案>" && python3 solve_pipeline.py build' % n)
    elif cmd == "ingest":
        q, a = sys.argv[2].split("|", 1)
        print("已录入:", ingest(q.strip(), a.strip(), src="manual", votes=99)["id"])
    elif cmd == "ask":
        if len(sys.argv) > 3 and sys.argv[2] == "-f":
            for i, line in enumerate(open(sys.argv[3], encoding="utf-8"), 1):
                line = line.strip()
                if not line:
                    continue
                print("[%d] %s" % (i, line[:60]))
                ask(line); print()
        else:
            print("题目: %s" % sys.argv[2])
            ask(sys.argv[2])
    else:
        print(__doc__)

if __name__ == "__main__":
    main()
