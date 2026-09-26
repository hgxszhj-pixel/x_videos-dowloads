#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
parse_baomi_records.py —— 把「中国保密在线」本人历史考试记录里的真题落进本地题库
==============================================================================
  python3 parse_baomi_records.py            生成 banks/baomi-2024.json + banks/baomi-我的错题.json
  python3 parse_baomi_records.py selftest   自检（FAIL 会 exit 1）

数据来源（合规）：`/portal/main-api/v2/activity/exam/examRecord.do?pageNum=1&pageSize=50`
—— 这是**用户本人**的考试记录接口，返回里每条记录的 `content` 字段就是那场的整卷：
题干 / 选项 / 官方正确答案 / 我当时的作答 / 对错标志。用的是自己考过的卷子，
不碰「开始考试」、不消耗次数、不改任何计数（和 mylearning 那边的「考后存档」同一个道理）。

抓取命令（在已登录的 ego 标签页里）：
  ego-browser nodejs < jr/tools/baomi-pull-records.js
产物：jr/out/baomi/examRecord.json

题型映射：单选题→single、多选题→multiple、判断题→judge（判断题的选项就是 正确/错误）。
"""
import hashlib
import html
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
RAW = os.path.join(HERE, "jr/out/baomi/examRecord.json")
def out_of(year):
    """按考试年份分库：banks/baomi-2024.json（2024 那两场）、banks/baomi-2026.json（2026 这场）"""
    return os.path.join(HERE, "banks/baomi-%s.json" % year)


OUT_ALL = os.path.join(HERE, "banks/baomi-2024.json")      # 兼容旧引用（自检里用）
OUT_WRONG = os.path.join(HERE, "banks/baomi-我的错题.json")
TYPE_MAP = {"单选题": "single", "多选题": "multiple", "判断题": "judge"}
LABEL = {"single": "单选（保密）", "multiple": "多选（保密）", "judge": "判断（保密）"}
TAG = re.compile(r"<[^>]+>")


def clean(s):
    """去掉 platform 塞在题干/选项外面的 XHTML 壳子，并还原实体"""
    if not s:
        return ""
    t = TAG.sub("", str(s))
    t = html.unescape(html.unescape(t))
    return re.sub(r"\s+", " ", t).strip()


def qid(text, pid=None):
    # 题干做指纹：两场重复的题会撞成同一个 id。pid 保留只为兼容旧调用 ——
    # 原来 `"BM-%s" % h if not pid else "BM-%s" % h` 两支完全相同（死分支），已去掉。
    return "BM-%s" % hashlib.sha1(text.encode("utf-8")).hexdigest()[:8]


def load_raw():
    if not os.path.exists(RAW):
        sys.exit("没找到原始记录 %s —— 先跑 jr/tools/baomi-pull-records.js" % RAW)
    d = json.load(open(RAW, encoding="utf-8"))
    recs = (d.get("data") or {}).get("recordList") or []
    if not recs:
        sys.exit("记录里没有 recordList（原始返回可能不是考试记录接口）")
    return recs


def harvest():
    """→ (题目列表, 统计) —— 按题干去重，保留「我第一次答错」的标记"""
    seen, rows, stat = {}, [], {"records": 0, "raw": 0, "types": {}, "wrong_ids": set()}
    for r in load_raw():
        stat["records"] += 1
        try:
            c = json.loads(r["content"])
        except Exception:
            continue
        score, when = r.get("score"), r.get("create_date") or r.get("correct_date")
        name = r.get("exam_name") or "保密教育线上培训考试"
        ym = re.search(r"(\d{4})\s*年", name)
        year = ym.group(1) if ym else "未知"
        for tl in c.get("typeList") or []:
            for q in tl.get("questionList") or []:
                stat["raw"] += 1
                typ = TYPE_MAP.get(q.get("QST_TYPE") or q.get("type"))
                stem = clean(q.get("content"))
                if not typ or len(stem) < 6:
                    continue
                opts = [{"key": k, "text": clean(v)} for k, v in sorted((q.get("options") or {}).items())]
                ans = str(q.get("answer") or "").strip()
                keys = [ch for ch in re.sub(r"[^A-Za-z]", "", ans).upper()]
                if not keys or not opts:
                    continue
                if not set(keys) <= {o["key"] for o in opts}:
                    continue                                             # 答案字母对不上选项 → 这条不要
                mine = str(q.get("userAnswer") or "").strip()
                wrong = str(q.get("resultflag")) == "0"
                k = qid(stem, q.get("id"))
                if k in seen:                                            # 两场里同题：留第一次的作答记录
                    if wrong:
                        stat["wrong_ids"].add(k)
                    continue
                stat["types"][typ] = stat["types"].get(typ, 0) + 1
                if wrong:
                    stat["wrong_ids"].add(k)
                row = {
                    "id": k, "number": 0, "year": year, "type": typ, "typeLabel": LABEL[typ],
                    "question": stem, "options": opts, "answerKeys": keys,
                    "answerText": [o["text"] for o in opts if o["key"] in keys],
                    "analysis": clean(q.get("analysis")) or
                                "来源：%s（本人历史考试记录，含官方正确答案）｜这场得分 %s，本题%s" %
                                (name, score, "我答错了" if wrong else "我答对了"),
                    "topic": "保密教育线上培训", "srcBank": "保密教育线上培训考试（%s）" % name,
                    "myAnswer": mine, "myWrong": wrong, "examWhen": when,
                }
                seen[k] = row
                rows.append(row)
    for i, r in enumerate(rows, 1):
        r["number"] = i
    return rows, stat


def count_law(path):
    """数一数这个题库里有几道题配了法条（cite_law.py 写的 law 字段）"""
    try:
        j = json.load(open(path, encoding="utf-8"))
    except (OSError, ValueError):
        return 0
    return sum(1 for q in (j.get("questions") or []) if q.get("law"))


def carry_over(rows, path):
    """把上一版配好的法条按 id 带过来。

    没有这一步，重跑一次解析就会把 47 题的条文依据全抹掉（实测踩过：
    `parse_baomi_records.py selftest` 顺手把 law 抹了，刷题页里「依据：《保守国家秘密法》第X条」就没了）。
    这样两个脚本的执行顺序就不再要紧：parse 完可以再 cite，cite 完再 parse 也不会丢。
    """
    if not os.path.exists(path):
        return 0
    try:
        old = json.load(open(path, encoding="utf-8"))
    except (OSError, ValueError):
        return 0
    keep = {q.get("id"): q for q in (old.get("questions") or []) if q.get("law")}
    n = 0
    for r in rows:
        o = keep.get(r.get("id"))
        if not o:
            continue
        r["law"] = o["law"]
        if "｜依据：" in (o.get("analysis") or ""):        # 解析里那段依据也一起带过来
            base = re.sub(r"｜依据：.*$", "", r.get("analysis") or "")
            r["analysis"] = base + o["analysis"][o["analysis"].index("｜依据："):]
        n += 1
    print("  带上上一版配好的法条：%d 题" % n)
    return n


def build():
    rows, stat = harvest()
    years = {}
    for r in rows:
        years.setdefault(r["year"], []).append(r)
    written = []
    for year, group in sorted(years.items()):
        path = out_of(year)
        for i, r in enumerate(group, 1):
            r["number"] = i
        carry_over(group, path)
        meta = {"title": "保密教育线上培训考试（%s 卷 · %d 题）" % (year, len(group)),
                "external": True, "year": year,
                "sourceNote": "来自你本人 %s 年「保密教育线上培训考试」的历史考试记录"
                              "（examRecord.do 返回的整卷：题干/选项/官方正确答案/我当时的作答）。"
                              "共 %d 道题（单选 %d / 多选 %d / 判断 %d），我当时答错 %d 道。"
                              "这份是另一门考试（保密教育），所以标了 external：不计入电信依据库覆盖率、"
                              "也不进「模拟考」池子。"
                              % (year, len(group),
                                 sum(1 for r in group if r["type"] == "single"),
                                 sum(1 for r in group if r["type"] == "multiple"),
                                 sum(1 for r in group if r["type"] == "judge"),
                                 sum(1 for r in group if r["myWrong"]))}
        json.dump({"meta": meta, "questions": group}, open(path, "w", encoding="utf-8"),
                  ensure_ascii=False, indent=1)
        written.append((year, path, len(group), sum(1 for r in group if r["myWrong"])))
        print("已生成：%s —— %d 题（单选 %d / 多选 %d / 判断 %d，我当时答错 %d）"
              % (path, len(group), sum(1 for r in group if r["type"] == "single"),
                 sum(1 for r in group if r["type"] == "multiple"),
                 sum(1 for r in group if r["type"] == "judge"),
                 sum(1 for r in group if r["myWrong"])))

    wrong = [dict(r, number=i + 1) for i, r in enumerate(r for r in rows if r["myWrong"])]
    carry_over(wrong, OUT_WRONG)
    json.dump({"meta": {"title": "保密考试 · 我当时答错的题（%d 题）" % len(wrong),
                        "external": True, "mergeOf": [os.path.basename(p) for _, p, _, _ in written],
                        "sourceNote": "从各年保密真题库里筛出 resultflag=0（我答错）的题，"
                                      "每题带官方正确答案和我当时的作答，用来专攻薄弱点。"},
               "questions": wrong}, open(OUT_WRONG, "w", encoding="utf-8"),
              ensure_ascii=False, indent=1)
    print("已生成：%s —— %d 题（我当时答错的；收集自 %d 个年份卷）"
          % (OUT_WRONG, len(wrong), len(written)))
    print("  原始题数 %d，按题干去重掉 %d 道（跨年重复的题归到新那一年）"
          % (stat["raw"], stat["raw"] - len(rows)))
    for r in rows[:3]:
        print("  样例 [%s/%s] %s" % (r["year"], r["typeLabel"], r["question"][:52]))
        print("        %s → 官方答案 %s（我当时选 %s%s）"
              % (" / ".join(o["text"][:14] for o in r["options"]), "".join(r["answerKeys"]),
                 r["myAnswer"] or "—", "，错" if r["myWrong"] else "，对"))
    return rows, wrong


def selftest():
    before = {y: count_law(out_of(y)) for y in ("2024", "2026")}
    rows, wrong = build()
    after = {y: count_law(out_of(y)) for y in ("2024", "2026")}
    for y in ("2024", "2026"):
        assert after[y] >= before[y], "重跑解析把 %s 卷的 %d 条法条依据抹掉了（carry_over 没生效）" % (y, before[y] - after[y])
    assert count_law(OUT_WRONG) >= 1 or sum(before.values()) == 0, "错题库里的法条依据没带过来"
    years = {}
    for r in rows:
        years.setdefault(r["year"], []).append(r)
    assert len(rows) >= 60, "真题太少（%d）—— 原始记录没读全？" % len(rows)
    assert len(years.get("2024", [])) >= 40, "2024 卷只有 %d 题" % len(years.get("2024", []))
    if "2026" in years:
        assert len(years["2026"]) >= 15, "2026 卷只有 %d 题" % len(years["2026"])
    assert len(wrong) >= 5, "我答错的题只有 %d 道，不对（2024 两场一共错了 20 道）" % len(wrong)
    assert len({r["id"] for r in rows}) == len(rows), "id 有重复，去重没生效"
    for r in rows:
        need = 2 if r["type"] == "judge" else 4
        assert len(r["options"]) >= need, "%s 选项只有 %d 个：%s" % (r["id"], len(r["options"]), r["question"][:30])
        assert set(r["answerKeys"]) <= {o["key"] for o in r["options"]}, "%s 答案不在选项里" % r["id"]
        if r["type"] != "multiple":
            assert len(r["answerKeys"]) == 1, "%s 单选/判断题却有多个答案 %s" % (r["id"], r["answerKeys"])
        assert len(r["answerText"]) == len(r["answerKeys"]), "%s 答案文本对不上" % r["id"]
        assert not TAG.search(r["question"]) and "<XHTML" not in r["question"], "%s 题干还带着 XHTML 壳" % r["id"]
        assert "XHTML" not in " ".join(r["answerText"]), "%s 选项没清干净" % r["id"]
        assert r["type"] in ("single", "multiple", "judge") and r["number"] >= 1
    assert all(r["myWrong"] for r in wrong), "错题库里混进了答对的题"
    assert {r["id"] for r in wrong} <= {r["id"] for r in rows}, "错题库有题不在总库里"
    print("selftest OK：%d 道真题（%s）、XHTML 清干净、答案都在选项内、id 唯一、错题库 %d 道全是我答错的、"
          "重跑不丢法条（2024 %d→%d、2026 %d→%d）"
          % (len(rows), " / ".join("%s 卷 %d 题" % (y, len(g)) for y, g in sorted(years.items())),
             len(wrong), before["2024"], after["2024"], before["2026"], after["2026"]))


if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1] == "selftest":
        selftest()
    else:
        build()
