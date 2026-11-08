#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""srs —— 错题复习调度（FSRS-6 算法移植，插件形式挂载）

算法来源：open-spaced-repetition/py-fsrs（MIT，519★）的 Scheduler 公式，参数用其
FSRS-6 默认值 DEFAULT_PARAMETERS。为什么移植而不是 pip 装：项目是零依赖路线，
而且这里只需要「给定评分 → 下次到期日」这一段，不需要它的 optimizer（要 torch）。
正确性由 plugins/srs_golden.json 里 7 组基准向量保证 —— 那些数字是用参考实现
（learning_steps=() / 无抖动）实跑出来的，本文件必须逐项对上。

数据落在 jr/out/srs.json（独立文件，不进 docs.sqlite）：删掉这个文件=清空复习历史，
插件整体下线也不影响查题/刷题。

评分口径（前端或 CLI 传到 rating）：
    again 不会/答错   hard 勉强   good 会   easy 秒答
"""
import json
import math
import os
import sqlite3
import time
from datetime import date, datetime, timedelta

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
STATE = os.path.join(ROOT, "jr/out/srs.json")
GOLDEN = os.path.join(HERE, "srs_golden.json")
DB = os.path.join(ROOT, "jr/out/docs.sqlite")

# ---- FSRS-6 常量（照抄 py-fsrs，顺序不能动）----
P = (0.212, 1.2931, 2.3065, 8.2956, 6.4133, 0.8334, 3.0194, 0.001, 1.8722, 0.1666,
     0.796, 1.4835, 0.0614, 0.2629, 1.6483, 0.6014, 1.8729, 0.5425, 0.0912, 0.0658, 0.1542)
DECAY = -P[20]
FACTOR = 0.9 ** (1 / DECAY) - 1
RETENTION = 0.9
MAX_INTERVAL = 36500
MIN_D, MAX_D = 1.0, 10.0
S_MIN = 0.001
RATINGS = {"again": 1, "hard": 2, "good": 3, "easy": 4}


def _clamp_s(s):
    return max(s, S_MIN)


def _clamp_d(d):
    return min(max(d, MIN_D), MAX_D)


def init_stability(rating):
    return _clamp_s(P[RATINGS[rating] - 1])


def init_difficulty(rating, clamp=True):
    d = P[4] - math.e ** (P[5] * (RATINGS[rating] - 1)) + 1
    return _clamp_d(d) if clamp else d


def retrievability(elapsed_days, stability):
    return (1 + FACTOR * elapsed_days / stability) ** DECAY


def next_interval(stability):
    ivl = (stability / FACTOR) * ((RETENTION ** (1 / DECAY)) - 1)
    return min(max(int(round(ivl)), 1), MAX_INTERVAL)


def short_term_stability(stability, rating):
    inc = math.e ** (P[17] * (RATINGS[rating] - 3 + P[18])) * (stability ** -P[19])
    if RATINGS[rating] >= 2:
        inc = max(inc, 1.0)
    return _clamp_s(stability * inc)


def next_difficulty(difficulty, rating):
    arg1 = init_difficulty("easy", clamp=False)
    delta = -(P[6] * (RATINGS[rating] - 3))
    arg2 = difficulty + (10.0 - difficulty) * delta / 9.0
    return _clamp_d(P[7] * arg1 + (1 - P[7]) * arg2)


def next_forget_stability(difficulty, stability, retr):
    long_term = (P[11] * (difficulty ** -P[12]) * (((stability + 1) ** P[13]) - 1)
                 * math.e ** ((1 - retr) * P[14]))
    short_term = stability / math.e ** (P[17] * P[18])
    return min(long_term, short_term)


def next_recall_stability(difficulty, stability, retr, rating):
    hard = P[15] if rating == "hard" else 1
    easy = P[16] if rating == "easy" else 1
    return stability * (1 + math.e ** P[8] * (11 - difficulty) * (stability ** -P[9])
                        * ((math.e ** ((1 - retr) * P[10])) - 1) * hard * easy)


def rate_card(card, rating, when=None):
    """核心：给这张卡打一次分，返回更新后的卡（不落盘）。card=None 表示新卡。"""
    if rating not in RATINGS:
        raise ValueError("rating 只能是 %s，收到 %r" % ("/".join(RATINGS), rating))
    when = when or date.today()
    card = dict(card or {})
    s, d = card.get("s"), card.get("d")
    last = card.get("last")
    if s is None or d is None:
        s, d = init_stability(rating), init_difficulty(rating)
    else:
        elapsed = max((when - date.fromisoformat(last)).days, 0) if last else 0
        # ponytail: 不做分钟级 learning steps（考前复习按天排），同一天连点按短期公式，
        # 天花板=不能表达「10 分钟后再来一次」；要那个粒度就照 py-fsrs 的 step 逻辑补。
        if elapsed < 1:
            s = short_term_stability(s, rating)
        else:
            s = (next_forget_stability(d, s, retrievability(elapsed, s)) if rating == "again"
                 else next_recall_stability(d, s, retrievability(elapsed, s), rating))
            s = _clamp_s(s)
        d = next_difficulty(d, rating)
    ivl = next_interval(s)
    card.update({
        "s": round(s, 6), "d": round(d, 6), "reps": int(card.get("reps") or 0) + 1,
        "lapses": int(card.get("lapses") or 0) + (1 if rating == "again" else 0),
        "last": when.isoformat(), "due": (when + timedelta(days=ivl)).isoformat(),
        "interval": ivl, "rating": rating,
    })
    return card


# ---------- 状态文件（原子写，坏文件不炸） ----------
def load_state():
    if not os.path.exists(STATE):
        return {"schema_version": 1, "cards": {}}
    try:
        d = json.load(open(STATE, encoding="utf-8"))
        if not isinstance(d, dict):
            raise ValueError("顶层不是对象")
        d.setdefault("schema_version", 1)
        d.setdefault("cards", {})
        if not isinstance(d["cards"], dict):
            raise ValueError("cards 不是对象")
        return d
    except Exception:                        # 坏文件不该让插件整体不可用：读空、下次写回就好
        return {"schema_version": 1, "cards": {}, "recovered_from_bad_file": True}


def save_state(state):
    state["updated_at"] = time.strftime("%Y-%m-%d %H:%M:%S")
    os.makedirs(os.path.dirname(STATE), exist_ok=True)
    tmp = STATE + ".tmp"
    with open(tmp, "w", encoding="utf-8") as f:
        json.dump(state, f, ensure_ascii=False, indent=1)
    os.replace(tmp, STATE)                   # 原子替换：写一半崩不会留半个 JSON


def _key(bank, qid):
    return "%s|%s" % (bank, qid)


def review(bank, qid, rating, when=None):
    st = load_state()
    k = _key(bank, qid)
    card = rate_card(st["cards"].get(k), rating, when)
    st["cards"][k] = card
    save_state(st)
    return card


def due(limit=20, bank=None, today=None):
    """到期（含逾期）的卡，带题干/选项/答案；库打不开就只给排期，不抛异常。"""
    today = today or date.today()
    st = load_state()
    rows = []
    for k, c in st["cards"].items():
        if c.get("due") and c["due"] <= today.isoformat():
            b, _, q = k.partition("|")
            if bank and b != bank:
                continue
            rows.append((c["due"], b, q, c))
    rows.sort(key=lambda r: (r[0], r[1], r[2]))
    rows = rows[:max(1, int(limit or 20))]
    info = _questions([(b, q) for _, b, q, _ in rows])
    out = []
    for due_day, b, q, c in rows:
        d = dict(info.get(_key(b, q)) or {})
        d.update({"bank": b, "qid": q, "due": due_day, "interval": c.get("interval"),
                  "reps": c.get("reps"), "lapses": c.get("lapses")})
        if not (d.get("question") or "").strip():
            # 排期卡还在，但题库里查不到这道题（题库重建后 qid 变了 / 库被挪走）
            d["missing"] = True
        out.append(d)
    return out


def _questions(pairs):
    """从 questions 表补题面（只读；任何异常都退化成空，不影响排期本身）。"""
    if not pairs:
        return {}
    try:
        db = sqlite3.connect("file:%s?mode=ro" % DB, uri=True, timeout=5)
        out = {}
        for b, q in pairs:
            r = db.execute("SELECT question, options, answer_keys, answer_text FROM questions"
                           " WHERE bank=? AND qid=?", (b, q)).fetchone()
            if r:
                out[_key(b, q)] = {"question": r[0], "options": json.loads(r[1] or "[]"),
                                   "answerKeys": json.loads(r[2] or "[]"), "answerText": r[3]}
        db.close()
        return out
    except Exception:
        return {}


def stats(today=None):
    today = today or date.today()
    st = load_state()
    cards = st["cards"].values()
    n = len(st["cards"])
    d = sum(1 for c in cards if c.get("due") and c["due"] <= today.isoformat())
    ivls = sorted(c.get("interval") or 0 for c in cards)
    # 每个库各有多少张卡、多少张今天到期 —— 复习下拉要照着标，不然用户以为别的库「加载不出来」
    by_bank = {}
    for k, c in st["cards"].items():
        b = k.partition("|")[0]
        slot = by_bank.setdefault(b, {"cards": 0, "due": 0})
        slot["cards"] += 1
        if c.get("due") and c["due"] <= today.isoformat():
            slot["due"] += 1
    orphans = len(orphan_keys())
    return {"total": n, "due": d, "orphans": orphans, "lapsed": sum(1 for c in cards if c.get("lapses")),
            "interval_median": ivls[n // 2] if ivls else 0,
            "interval_max": ivls[-1] if ivls else 0,
            "by_bank": by_bank, "state_file": STATE,
            "recovered": bool(st.get("recovered_from_bad_file"))}


def orphan_keys():
    """排期里有、题库里查不到的卡（key 形如 "bank|qid"）。"""
    st = load_state()
    keys = list(st["cards"])
    pairs = [(k.partition("|")[0], k.partition("|")[2]) for k in keys]
    info = _questions(pairs)
    return [k for k in keys if k not in info]


def prune(today=None):
    """把失效卡从排期里删掉；只删查不到原题的，正常卡一张不动。"""
    st = load_state()
    gone = orphan_keys()
    for k in gone:
        st["cards"].pop(k, None)
    if gone:
        save_state(st)
    return {"removed": len(gone), "keys": gone[:20], "stats": stats(today)}


def enroll(limit=200, only_wrong=True, bank=None):
    """把题装进复习队列：默认只装「考场答错过」或「库存与推导冲突」的题。"""
    try:
        db = sqlite3.connect("file:%s?mode=ro" % DB, uri=True, timeout=5)
    except Exception as e:
        return {"error": "打不开题库：%s" % e}
    sql = ("SELECT q.bank, q.qid, q.extra FROM questions q "
           "LEFT JOIN answers a ON a.bank=q.bank AND a.qid=q.qid WHERE 1=1")
    args = []
    if bank:
        sql += " AND q.bank=?"
        args.append(bank)
    if only_wrong:
        sql += " AND (a.status='conflict' OR q.extra LIKE '%\"myWrong\": true%')"
    sql += " LIMIT ?"
    args.append(max(1, int(limit)))
    st = load_state()
    added = 0
    today = date.today().isoformat()
    for b, q, _ in db.execute(sql, args):
        k = _key(b, q)
        if k not in st["cards"]:
            st["cards"][k] = {"due": today, "reps": 0, "lapses": 0}
            added += 1
    db.close()
    save_state(st)
    return {"added": added, "total": len(st["cards"]), "only_wrong": only_wrong}


# ---------- 给 webui / CLI 用的两个入口 ----------
def _r_due(q):
    return {"due": due(limit=q.get("limit") or 20, bank=q.get("bank")),
            "stats": stats()}


def _r_review(q):
    if not (q.get("bank") and q.get("qid")):
        return {"error": "要 bank 和 qid"}
    card = review(q["bank"], q["qid"], q.get("rating") or "good")
    return {"card": card, "stats": stats()}


def _r_stats(q):
    return stats()


def _r_prune(q):
    return prune()


def _r_enroll(q):
    return enroll(limit=q.get("limit") or 50, only_wrong=(q.get("all") != "1"),
                  bank=q.get("bank"))


PLUGIN = {
    "name": "srs",
    "desc": "错题复习调度（FSRS-6 移植；数据在 jr/out/srs.json）",
    "routes": {"/api/srs/due": _r_due, "/api/srs/review": _r_review,
               "/api/srs/stats": _r_stats, "/api/srs/enroll": _r_enroll,
               "/api/srs/prune": _r_prune},
    "selftest": None,          # 下面赋值，便于函数定义在前
}


def selftest():
    """红/绿：基准向量逐一对照 + 评分单调 + 落盘/读回 + 坏文件不炸 + 到期查询。"""
    vecs = json.load(open(GOLDEN, encoding="utf-8"))
    checked = 0
    for v in vecs:
        card = {}
        day = date(2026, 1, 1)
        for step in v["seq"]:
            day = day + timedelta(days=step[1])
            card = rate_card(card, step[0], day)
        exp = v["expect"][-1]
        assert abs(card["s"] - exp["stability"]) < 1e-4, \
            "稳定性对不上参考实现：%r vs %r（序列 %r）" % (card["s"], exp["stability"], v["seq"])
        assert abs(card["d"] - exp["difficulty"]) < 1e-4, \
            "难度对不上：%r vs %r（序列 %r）" % (card["d"], exp["difficulty"], v["seq"])
        assert card["interval"] == exp["interval"], \
            "间隔对不上：%r vs %r（序列 %r）" % (card["interval"], exp["interval"], v["seq"])
        checked += 1

    # 同一张卡，评分越高下次越晚
    base = {"s": 5.0, "d": 5.0, "last": "2026-01-01", "due": "2026-01-06"}
    ivs = {r: rate_card(base, r, date(2026, 1, 11))["interval"] for r in RATINGS}
    assert ivs["again"] < ivs["hard"] <= ivs["good"] < ivs["easy"], "评分单调性被破坏：%r" % ivs

    # 落盘 → 读回 → 到期清单；用临时文件，不碰真实复习记录
    global STATE
    keep = STATE
    STATE = os.path.join(ROOT, "jr/out", "_selftest_srs.json")
    try:
        if os.path.exists(STATE):
            os.remove(STATE)
        assert stats()["total"] == 0, "空状态应为 0 张卡"
        review("自测库.json", "T-1", "good")
        review("自测库.json", "T-2", "again")
        assert stats()["total"] == 2, "应落盘 2 张卡"
        assert load_state()["cards"]["自测库.json|T-1"]["reps"] == 1
        got = due(limit=10, today=date(2026, 12, 31))       # 远期视角：两张都该到期
        assert len(got) == 2, "远期视角应有 2 张到期卡，实际 %d" % len(got)
        assert all("bank" in g and "qid" in g for g in got)
        # 失效卡：题库里查不到原题的要标 missing、能被 prune 清掉；真卡一张不许误删
        db = sqlite3.connect("file:%s?mode=ro" % DB, uri=True, timeout=5)
        real = db.execute("SELECT bank, qid FROM questions WHERE options NOT IN ('', '[]') LIMIT 1").fetchone()
        db.close()
        assert real, "题库空，没法验「不误删」"
        review(real[0], real[1], "good")                     # 一张真卡
        st = load_state()
        st["cards"]["不存在的库.json|X-9"] = {"due": "2020-01-01", "reps": 0, "lapses": 0}
        save_state(st)
        got2 = due(limit=20, today=date(2026, 12, 31))
        ghost = [g for g in got2 if g["qid"] == "X-9"]
        assert ghost and ghost[0].get("missing") is True, "查不到原题的卡应标 missing"
        assert any(g.get("question") and not g.get("missing") for g in got2), "真卡不该被标 missing"
        bb = stats().get("by_bank") or {}
        assert bb.get(real[0], {}).get("cards", 0) >= 1, "by_bank 没统计到真卡：%r" % bb
        assert sum(v["cards"] for v in bb.values()) == stats()["total"], "by_bank 卡片数之和 != 总数"
        before = stats()["orphans"]
        assert before >= 1, "应报失效卡，实际 %r" % before
        r = prune()
        assert r["removed"] == before and stats()["orphans"] == 0, "prune 应清光失效卡"
        left = [k for k in load_state()["cards"]]
        assert "%s|%s" % (real[0], real[1]) in left, "prune 误删了真卡"

        # 状态文件写坏 → 读空而不是抛
        open(STATE, "w", encoding="utf-8").write("{ 这不是 JSON")
        assert stats()["total"] == 0 and stats()["recovered"], "坏状态文件应被识别并降级"
    finally:
        if os.path.exists(STATE):
            os.remove(STATE)
        STATE = keep
    print("  srs 自检：%d 组基准向量 + 评分单调 + 落盘读回 + 坏文件降级 全过" % checked)


PLUGIN["selftest"] = selftest
