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

def _answer_str(q):
    """排序题按顺序拼成一串（CBDAFE）；填空题按空位顺序用；隔开；其他题型 A/B 隔开。"""
    if q.get("type") == "blank":
        at = q.get("answerText")
        items = at if isinstance(at, list) else ([at] if at else [])
        return "；".join(str(x).strip() for x in items if str(x or "").strip())
    keys = [str(k) for k in (q.get("answerKeys") or [])]
    ordered = bool(q.get("ordered")) or q.get("type") == "ordering"   # 排序 = 多选变体
    return "".join(keys) if ordered else "/".join(keys)


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
                _answer_str(q),
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
    real = c.execute("SELECT COUNT(*) FROM q").fetchone()[0]
    c.commit(); c.close()
    print("索引重建完成：%d 题（写入尝试 %d 次，去重后实际行数）-> %s" % (real, n, DB))

_CACHE = {"mtime": None, "rows": []}


def _grams3(s):
    return {s[i:i + 3] for i in range(len(s) - 2)} if len(s) >= 3 else ({s} if s else set())


def _rows():
    """题库行 + 预算好的 3-gram 集合，整个进程只建一次（qb.sqlite3 变了才重建）。

    为什么要缓存：以前每查一题都开一次库、把 1566 行全捞出来逐行 SequenceMatcher，
    实测一次未命中要 100–140 ms；导入 300 题就是 100 秒，浏览器直接超时报 Load failed。
    """
    m = os.path.getmtime(DB) if os.path.exists(DB) else None
    if m is None:
        return []
    if _CACHE["mtime"] != m:
        c = sqlite3.connect(DB)
        try:
            raw = c.execute("SELECT * FROM q").fetchall()
        finally:
            c.close()
        _CACHE["rows"] = [(r, _grams3(r[4] or ""), _grams3(r[5] or "")) for r in raw]
        _CACHE["mtime"] = m
    return _CACHE["rows"]


def lookup(question, floor=0.72, keep=60):
    """先精确、再用 3-gram 粗筛 + SequenceMatcher 精选。返回 (row, score)。"""
    rows = _rows()
    full, stem = norm(question), norm(stem_of(question))
    for r, _, _ in rows:                       # 精确命中：内存里比，不再走 SQL
        if full and r[4] == full:
            return r, 1.0
    for r, _, _ in rows:
        if stem and r[5] == stem:
            return r, 1.0
    if not rows:
        return None, 0.0
    kg, ks = _grams3(full), _grams3(stem)
    cand = []
    for r, tnq, tnqstem in rows:
        a = len(kg & tnq) / len(kg) if kg and tnq else 0.0
        b = len(ks & tnqstem) / len(ks) if ks and tnqstem else 0.0
        rough = max(a, b)
        if rough >= 0.30:                       # 粗筛门槛：低于这个连精算都不用做
            cand.append((rough, r))
    cand.sort(key=lambda x: -x[0])
    best, score = None, 0.0
    for _, r in cand[:keep]:
        for col, key in ((r[4], full), (r[5], stem)):
            if col and key:
                v = sim(key, col)
                if v > score:
                    best, score = r, v
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
LAST_STATS = {}

def _good(t):
    return len(t) > 25 and not JUNK.search(t) and "反馈" not in t[:5]

ENGINES = (
    ("baidu", "https://www.baidu.com/s?wd={q}"),
    ("360", "https://www.so.com/s?q={q}"),
    ("sogou", "https://www.sogou.com/web?query={q}"),
)
KSB_DOMAIN = "kaoshibao.com"
_FIXTURE = os.environ.get("EXAM_SEARCH_FIXTURE")   # 测试钩子：指向本地假结果页（file://），
                                                   # 引擎被限流时也能验证解析/优先/计票链路
_POOL = {"ex": None}
_HEALTH = {}          # engine -> {"fails": n, "until": 时间戳}
_COOLDOWN = 600       # 被限流/空手的引擎歇 10 分钟，别越撞越封
_SCACHE = {"q": {}, "ttl": 3600}   # 检索结果缓存（别叫 _CACHE：lookup 的题库缓存已经占了那名字）


def _blocks(engine, html_text):
    """按引擎把结果页切成「一条结果」的 HTML 片段。"""
    if engine == "baidu":
        parts = re.split(r'<div[^>]+class="result[^"]*"', html_text)[1:]
    elif engine == "360":
        parts = re.split(r'<li class="res-list"', html_text)[1:]
    else:
        parts = re.split(r"<h3", html_text)[1:]
    return parts[:12]


def _clean(fragment):
    t = re.sub(r"<script.*?</script>", " ", fragment, flags=re.S)
    t = re.sub(r"<[^>]+>", " ", t)
    t = html.unescape(t)
    t = re.sub(r"\s+", " ", t).strip()
    t = re.sub(r'^(?:[\w-]+="[^"]*"\s*)+>?\s*', "", t)      # 切段剩下的标签属性
    return t[:400]


def fetch_engine(task):
    """一个 (引擎, 检索式, 标签) 任务：取结果页 → 切条 → (标签, 引擎, 摘要, 是否考试宝命中, 是否正常)。

    跑在子进程里（多进程），所以必须是模块级函数、只能靠参数传数据。
    """
    engine, query, tag = task
    tpl = _FIXTURE or dict(ENGINES)[engine]
    time.sleep(random.uniform(0, 0.4))               # 轻微错峰，别让同一引擎瞬间挨两下
    if _FIXTURE:                                  # 测试钩子：直接读本地假页面，不走网络
        try:
            h = open(_FIXTURE.replace("file://", ""), encoding="utf-8").read()
        except OSError:
            h = ""
    else:
        h = curl(tpl.format(q=urllib.parse.quote(query)))
    low = h.lower()
    blocked = h and len(h) < 9000 and ("antispider" in low or "安全验证" in h or "请启用 JavaScript" in h
                                       or "访问验证" in h)
    if not h or blocked:
        return tag, engine, [], False, False        # ok=False：这条引擎这轮没拿到东西
    out, ksb = [], False
    for blk in _blocks(engine, h):
        t = _clean(blk)
        if len(t) > 25 and not JUNK.search(t) and "反馈" not in t[:5]:
            out.append(t)
        if KSB_DOMAIN in blk:
            ksb = True
    if tag == "ksb" and out:
        ksb = True
    return tag, engine, out, ksb, True


def _pool():
    """常驻进程池：macOS 是 spawn，每个 worker 起来要 200ms 左右，不能每次请求重建。"""
    if _POOL["ex"] is None:
        from concurrent.futures import ProcessPoolExecutor
        _POOL["ex"] = ProcessPoolExecutor(max_workers=3)
    return _POOL["ex"]


def snippets(q):
    """并行检索：百度/360/搜狗 同时发（多进程）。

    考试宝优先：额外发一条 site:kaoshibao.com 的检索式，且任何一条结果里出现
    kaoshibao.com 的链接，都算「考试宝命中」——这类摘要计 2 票（见 vote）。
    注意：这里查的是搜索引擎，不碰考试宝的签名接口（那个账号已停用、代码已冻结）。
    """
    global LAST_URLS, LAST_STATS
    hit = _SCACHE["q"].get(q)
    if hit and time.time() - hit[0] < _SCACHE["ttl"]:
        LAST_STATS = {"cached": True, "snippets": len(hit[1])}
        return list(hit[1])
    now = time.time()
    resting = {k for k, v in _HEALTH.items() if v.get("until", 0) > now}
    tasks = [("baidu", "site:%s %s" % (KSB_DOMAIN, q), "ksb"), ("baidu", q, "web")]
    tasks += [(eng, q, "web") for eng, _ in ENGINES if eng != "baidu"]
    tasks = [t for t in tasks if t[0] not in resting]
    if not tasks:                                  # 三个引擎都在冷却：这轮不打了，免得越撞越封
        LAST_STATS = {"all_resting": True, "snippets": 0, "resting": sorted(resting)}
        return []
    results, mode = [], "process"
    t0 = time.time()
    try:
        results = list(_pool().map(fetch_engine, tasks, timeout=45))
    except Exception:
        mode = "thread"
        try:                                          # 进程池起不来（少见，如 __main__ 是 stdin）：退回线程
            from concurrent.futures import ThreadPoolExecutor
            with ThreadPoolExecutor(max_workers=len(tasks)) as ex:
                results = list(ex.map(fetch_engine, tasks))
        except Exception:                             # 再不行就串行，宁慢不挂
            mode = "serial"
            results = [fetch_engine(t) for t in tasks]
    else:
        mode = "process"
    for tag, engine, _texts, _ksb, ok in results:          # 冷却表在父进程更新（子进程改了传不回来）
        if ok:
            _HEALTH[engine] = {"fails": 0, "until": 0}
        else:
            n = (_HEALTH.get(engine, {}).get("fails") or 0) + 1
            _HEALTH[engine] = {"fails": n, "until": now + (_COOLDOWN if n >= 2 else 120)}
    out, seen, urls, ksb_hits = [], set(), [], 0
    # 考试宝命中的排前面：前端和 vote 都按顺序优先用
    results.sort(key=lambda r: (not r[3], r[0] != "ksb"))
    for tag, engine, texts, ksb, _ok in results:
        if ksb:
            ksb_hits += 1
        for t in texts:
            if t in seen:
                continue
            seen.add(t)
            out.append(("ksb·%s" % engine if ksb else engine, t))
    LAST_URLS = urls[:8]
    LAST_STATS = {"mode": mode, "tasks": len(tasks), "snippets": len(out), "ksb_engines": ksb_hits,
                  "seconds": round(time.time() - t0, 2),
                  "engines_resting": [k for k, v in _HEALTH.items() if v.get("until", 0) > time.time()]}
    _SCACHE["q"][q] = (time.time(), out)
    return list(out)


ANS_PATS = [r"正确答案[是为：:]\s*([A-D]{1,4})", r"答案[是为：:]\s*([A-D]{1,4})",
            r"【答案】\s*([A-D]{1,4})", r"参考答案[是为：:]\s*([A-D]{1,4})"]
JUDGE_PATS = [r"答案[是为：:]\s*(正确|错误|对|错)", r"【答案】\s*(正确|错误|对|错)"]

JUDGE_OPTS = {"正确", "错误", "对", "错", "是", "否", "√", "×", "T", "F"}


def is_judge(question):
    """判断题判定：题干里同时出现「正确/错误」，或选项文字全是 正确/错误/对/错 这类。"""
    q = question or ""
    if "正确" in q and "错误" in q:
        return True
    opts = [o.strip("。，,、 ") for o in re.findall(r"[A-D][.、．]\s*([^\sA-D]{1,3})", q)]
    return bool(opts) and all(o in JUDGE_OPTS for o in opts)


def vote(sn, is_judge=False, require_engines=2):
    """多源投票：同一答案要**多个不同引擎**都出现才算数。

    - 同一段摘要被两个引擎都抓到 → 文本去重，只算 1 票
    - 考试宝来源（ksb·xxx）计 2 票，但它和 xxx 是同一个引擎，不凑「独立来源」数
    - require_engines=2：至少 2 个不同引擎给出同一答案才认（多源一致）
      只有 1 个来源时返回答案但标记单源，交给人工确认（宁可不给，也别给错）
    返回 (答案 or None, 加权票数, 样例 dict, 独立引擎 set)
    """
    from collections import Counter, defaultdict
    cnt, samples = Counter(), {}
    engines = defaultdict(set)
    pats = JUDGE_PATS if is_judge else ANS_PATS
    seen_text = set()
    for eng, t in sn:
        key = re.sub(r"\s+", "", t or "")[:120]
        if key in seen_text:
            continue
        seen_text.add(key)
        tag = str(eng)
        ksb = tag.startswith("ksb")
        base = tag.split("·")[-1]                      # ksb·baidu → baidu：同一引擎，别当两票
        weight = 2 if ksb else 1
        for p in pats:
            m = re.search(p, t)
            if m:
                a = re.sub(r"[、,，／/]", "", m.group(1))
                if is_judge:
                    a = a.replace("对", "正确").replace("错", "错误")
                cnt[a] += weight
                engines[a].add(base)
                samples.setdefault(a, "[%s] %s" % (tag, t[:180]))
                break
    if not cnt:
        return None, 0, {}, set()
    a, v = cnt.most_common(1)[0]
    if len(engines[a]) < max(1, require_engines):
        return None, v, samples, engines[a]            # 单源：不给答案，交给人工
    return a, v, samples, engines[a]


def ingest(question, answer, answer_text="", src="", votes=0, qtype="unknown"):
    # id 必须能跨进程复现：Python 的 hash() 每进程随机加盐，同题两次录入会得到不同 id，
    # 于是 merge/build 的去重全失效（实测两次进程给出 284071136938 / 755604882297）。
    rec = {"id": "auto-" + hashlib.sha1(norm(question).encode("utf-8")).hexdigest()[:12],
           "question": question, "answer": answer, "answerText": answer_text,
           "type": qtype, "src": src, "votes": votes,
           "ts": time.strftime("%Y-%m-%d %H:%M:%S")}
    if rec["id"] in _learned_ids():          # 同一个题面重复补录没意义，只留第一份
        rec["duplicate"] = True
        return rec
    with open(LEARNED, "a", encoding="utf-8") as f:
        f.write(json.dumps(rec, ensure_ascii=False) + "\n")
    return rec

def _grams(s):
    """字级 bigram：中文没有空格，整句选项用「词」取交集永远取不到（实测全是 0.00）。"""
    s = re.sub(r"[^\u4e00-\u9fa5A-Za-z0-9]", "", s or "")
    return {s[i:i + 2] for i in range(len(s) - 1)} if len(s) > 1 else ({s} if s else set())


def candidates(question, sn):
    """选项按「与检索摘要的字面重合度」排序。只排序，不判定。"""
    opts = dict(re.findall(r"([A-D])[.、．]\s*([^A-D]{2,40}?)(?=\s*[A-D][.、．]|$)", question))
    ev = " ".join(t for _, t in sn)
    if not opts or not ev:
        return []
    evg = _grams(ev)
    out = []
    for k, v in opts.items():
        og = _grams(v)
        if not og:
            continue
        hit = len(og & evg)
        # 字面重合只当弱信号（「30分钟/20分钟/10分钟」这种短选项会一起中），封顶 0.6；
        # 只有「选项原文整句出现在摘要里」才算强信号 1.0。
        score = min(round(hit / len(og), 2), 0.6)
        if v.strip() and v.strip() in ev:
            score = 1.0
        out.append((score, k, v.strip(), hit))
    return sorted(out, reverse=True)

_PENDING_SEEN = {"loaded": False, "keys": set()}


def pending(question, q, sn=None, guess=None):
    cand = candidates(question, sn or [])
    key = norm(question)[:200]
    if not _PENDING_SEEN["loaded"]:
        if os.path.exists(PENDING):
            for line in open(PENDING, encoding="utf-8"):
                try:
                    _PENDING_SEEN["keys"].add(norm(json.loads(line).get("question") or "")[:200])
                except Exception:
                    pass
        _PENDING_SEEN["loaded"] = True
    dup = bool(key) and key in _PENDING_SEEN["keys"]
    rec = {"question": question, "query": q, "guess": guess,
           "candidates": [{"score": s, "key": k, "text": t, "overlap": o} for s, k, t, o in cand],
           "evidence": [t[:200] for _, t in (sn or [])][:4],
           "ts": time.strftime("%Y-%m-%d %H:%M:%S")}
    if dup:
        rec["duplicate"] = True          # 已经排过一次队：不再追加（原来查十次就写十行）
        return rec
    _PENDING_SEEN["keys"].add(key)
    with open(PENDING, "a", encoding="utf-8") as f:
        f.write(json.dumps(rec, ensure_ascii=False) + "\n")
    return rec

def ask(question, do_web=True, auto_learn=False, verbose=True):
    """本地优先，未命中才联网。

    auto_learn 默认关：联网答案只当线索，写库必须走人工闸门（webui 的「收编」页
    或 solve.py ingest）。以前默认开会把 ≥2 票的搜索答案静默写进 learned.jsonl，
    再随 build 进查题索引 —— 等于让搜索引擎改题库。
    """
    judge = is_judge(question)
    row, score = lookup(question)
    blanks = (question or "").count("（）") + (question or "").count("( )") + (question or "").count("（ ）")
    if not row and ("排序题" in (question or "") or "填空题" in (question or "") or blanks >= 3):
        return {"answer": None, "source": "unsupported", "confidence": "none",
                "note": "这条是排序/填空题（%d 个空），题库里还没有这道题；联网检索只会判 A-D 单选多选，"
                        "答不了。把它按「多选+ordered」或 blank 题型存进题库（banks/*.json）后就能查了" % max(blanks, 1)}
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
    ans, v, samples, srcs = vote(sn, judge)
    single_src = False
    if not ans and not re.search(r"[A-D][.、．]", question or ""):
        # 没有任何选项标记 = 多半是判断题题干，再按「正确/错误」标记试一次
        a2, v2, s2, srcs2 = vote(sn, True)
        if a2:
            ans, v, samples, judge, srcs = a2, v2, s2, True, srcs2
    if not ans:
        rec = pending(question, q, sn)
        note = ("检索到 %d 条摘要，但没有一条带「答案：X」这类显式标记，不能据此定答案"
                % len(sn)) if sn else "搜索引擎没取回可用摘要（可能被限流或要验证码）"
        if verbose:
            print("  %s -> 已排队" % note)
            for c in rec["candidates"][:4]:
                print("       %.2f  %s. %s" % (c["score"], c["key"], c["text"][:46]))
            print("     -> python3 solve_pipeline.py review")
        best = next((c for c in rec["candidates"] if c["score"] >= 1.0), None)
        return {"answer": None, "source": "pending", "confidence": "none",
                "candidates": rec["candidates"][:4], "snippets": len(sn),
                "search": dict(LAST_STATS),
                "best_guess": ({"key": best["key"], "text": best["text"]} if best else None),
                "note": note, "evidence": rec["evidence"][:3]}
    conf = "high" if v >= 3 else ("medium" if v == 2 else "low")
    single = None
    if not ans:
        # 只有一个来源给了答案：不当答案用，但把「单源线索」带给前端/待审队列
        r_ans, r_v, _r_s, r_srcs = vote(sn, judge, require_engines=1)
        if r_ans:
            single_src = True
            single = {"answer": r_ans, "votes": r_v, "sources": sorted(r_srcs)}
    cand = candidates(question, sn)                 # 顺带给前端一个交叉校验用的候选榜
    bg = next((c for c in cand if c["score"] >= 1.0), None)
    extra = {"votes": v, "sources": sorted(srcs), "single_source": single_src, "single": single,
             "snippets": len(sn), "search": dict(LAST_STATS), "candidates": cand[:4],
             "best_guess": ({"key": bg[1], "text": bg[2]} if bg else None)}
    if verbose:
        print("  检索答案: %s  票数 %d  置信度 %s" % (ans, v, conf))
        if samples.get(ans):
            print("     依据: %s" % samples[ans][:170])
    if auto_learn and v >= 2:
        rec = ingest(question, ans, src="+".join(sorted({s[0] for s in sn})), votes=v,
                     qtype="judge" if judge else "unknown")
        if verbose:
            print("  已自动回灌题库（%s），下次本地秒回" % rec["id"])
        return dict(extra, answer=ans, source="web+learned", confidence=conf)
    pending(question, q, sn, guess=ans)
    if verbose:
        print("  只有 1 票，不足以自动入库 -> 进待办队列")
    return dict(extra, answer=ans, source="web-only", confidence=conf)

def selftest_search():
    """红绿：解析 / 考试宝优先标记 / 站内双票 / 冷却短路 —— 用本地假页面，不联网。"""
    import tempfile
    html_doc = (
        '<div class="result c-container"><h3><a href="https://www.kaoshibao.com/t/1.html">q1</a></h3>'
        '<div>有限空间作业前必须通风至少30分钟。本题答案：D 30分钟。检测指标包括氧浓度。</div></div>'
        '<div class="result c-container"><h3><a href="https://wenda.so.com/q/2">q2</a></h3>'
        '<div>最佳答案：30分钟。作业前必须进行强制通风，时间不得少于30分钟。</div></div>'
        '<div class="result c-container"><h3><a href="https://zhidao.baidu.com/q/3">q3</a></h3>'
        '<div>有限空间作业前应通风多久？答案：C 20分钟（干扰项，用来核对多源计票）。</div></div>')
    path = os.path.join(tempfile.gettempdir(), "solve_search_fixture.html")
    open(path, "w", encoding="utf-8").write(html_doc)
    global _FIXTURE
    old, _FIXTURE = _FIXTURE, "file://" + path
    old_env = os.environ.get("EXAM_SEARCH_FIXTURE")
    os.environ["EXAM_SEARCH_FIXTURE"] = _FIXTURE      # 子进程靠环境变量拿到同一份 fixture（spawn 不继承内存）
    _SCACHE["q"].clear(); _HEALTH.clear()
    try:
        sn = snippets("有限空间作业前应通风多长时间")
        assert len(sn) == 3, "应解析出 3 条摘要，实际 %d" % len(sn)
        assert all(tag.startswith("ksb·") for tag, _ in sn), "带 kaoshibao 链接的结果应标成 ksb：%r" % [t for t, _ in sn]
        # 三条摘要都来自 baidu（同一引擎）→ 不算多源一致，不该给答案
        ans, votes, _s, srcs = vote(sn)
        assert ans is None and srcs == {"baidu"}, "同一引擎的多条摘要不该凑成一致答案：%r/%r" % (ans, srcs)
        # 两个不同引擎给同一答案 → 采纳
        two = [("baidu", "本题答案：D 30分钟"), ("360", "正确答案：D。30分钟")]
        ans2, v2, _s2, srcs2 = vote(two)
        assert ans2 == "D" and len(srcs2) == 2, "两引擎一致应采纳 D，实际 %r/%r" % (ans2, srcs2)
        # 单源：返回空答案（交给人工），但仍能取到票数
        one = [("baidu", "本题答案：D 30分钟")]
        assert vote(one)[0] is None, "单源不该直接给答案"
        # 考试宝来源与同引擎全网结果不构成独立两源
        ksb_same = [("ksb·baidu", "本题答案：D"), ("baidu", "答案：D 30分钟")]
        assert vote(ksb_same)[0] is None, "ksb·baidu 与 baidu 是同一引擎，不能当两源"
        cand = candidates("有限空间作业前应通风多长时间？A. 5分钟 B. 10分钟 C. 20分钟 D. 30分钟", sn)
        assert cand and cand[0][1] == "D", "候选首位应为 D，实际 %r" % (cand[:2],)
        # 全引擎冷却时不再发请求
        _HEALTH.update({e: {"fails": 3, "until": time.time() + 600} for e, _ in ENGINES})
        _SCACHE["q"].clear()
        assert snippets("冷却短路测试") == [], "全冷却时应直接返回空"
        # 排序题/填空题：不联网、不抛异常，直接说答不了（以前会掉进 UnboundLocalError）
        r = ask("某战略要把握（）是核心、（）是动力、（）是根本的内涵要义。（排序题(10分)）"
                "A. 甲 B. 乙 C. 丙", do_web=True, verbose=False)
        assert r.get("answer") is None and r.get("source") == "unsupported", r
        assert LAST_STATS.get("all_resting"), "应标记 all_resting：%r" % LAST_STATS
    finally:
        _FIXTURE = old
        if old_env is None:
            os.environ.pop("EXAM_SEARCH_FIXTURE", None)
        else:
            os.environ["EXAM_SEARCH_FIXTURE"] = old_env
        _HEALTH.clear(); _SCACHE["q"].clear()
        try:
            os.remove(path)
        except OSError:
            pass
    print("search selftest 通过：解析 3 条 / 考试宝标记 / 站内双票(D=2) / 冷却短路")


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
    elif cmd == "search-selftest":
        selftest_search()
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
