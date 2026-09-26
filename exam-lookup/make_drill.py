#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
make_drill.py —— 把「数字型考点」变成可刷的自测题（生成的，不是真题）
=====================================================================
  python3 make_drill.py            生成 banks/自测-安全数字.json
  python3 make_drill.py selftest   自检（FAIL 会 exit 1）

为什么要生成：平台上那 2,005 道真题拿不到（机构开关为 0），手上的安全类真题只有 112 道，
背两轮就记混了。而《管理办法》《规范》和各分册教材里有 400 多条带数字的硬规定 ——
把这些句子挖空、用同一类别的**真实数值**当干扰项，就得到一份「答案有原文背书」的自测题。

三条自我约束（防止生成题冒充真题）：
  1. 库名 / meta / 每题都带 generated 标记，刷题页题头会显示「生成题」；
  2. 干扰项只从语料里同类别的真实数值里取，不编数字；
  3. 句子里同类别的数字多于一个就丢弃（挖空会歧义），宁可少出题。
"""
import csv
import hashlib
import json
import os
import random
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
SRC_CSV = os.path.join(HERE, "crawl/数字考点速查.csv")
OUT = os.path.join(HERE, "banks/自测-安全数字.json")
import num_points                                            # 同目录，复用安全主题关键词表

CATS = ("重量", "长度", "时间", "周期", "频次", "电气")        # 有量纲的才适合挖空（比例类多半是达标率，句子不成句）
EXTRA_CATS = ("个数", "人")            # 「2 个灭火器」「不少于 2 人」也是硬规定，但这一类里
                                       # 混着「一个光方向」这种非数量的「个」→ 只收有规范措辞的（normative=1）
JUNK = re.compile(r"（\s*）|（　）|见本章|见下表|参见|如下表|Ø|^[a-z0-9\)．.、·]|\|")
JUNK_STEM = re.compile(r"见本章|见下表|参见|如下表|Ø|^[a-z0-9\)]|\|")   # 题干里我们自己会插空框，检查时排除空框本身
TOKEN_RX = re.compile(r"^(?:\d+(?:\.\d+)?|[一二三四五六七八九十百两半]+)(.*)$")


def load_rows():
    if not os.path.exists(SRC_CSV):
        sys.exit("缺 %s —— 先跑：python3 num_points.py" % SRC_CSV)
    rows = []
    with open(SRC_CSV, encoding="utf-8") as fh:
        for r in csv.DictReader(fh):
            if r["category"] not in CATS + EXTRA_CATS or not r["sentence"] or not r["num"]:
                continue
            if r["category"] in EXTRA_CATS and r.get("normative") not in ("是", "1", "y", "yes"):
                continue                     # 没规范措辞的「个数/人」多半不是考点数量（「同一人字梯」里的「一人」），丢掉
            topic = num_points.topic_of(r["sentence"])
            if JUNK.search(r["sentence"]):
                continue                     # 不成句/带表格残渣的挖空没意义（如「业务资源与现场一致性（ ）」）
            if topic == "其他" and not re.search(r"管理办法|规范|解读|YD", r["book"]):
                continue                     # 「其他」主题只留命题主依据那几本，其余（达标率之类）不要
            rows.append(r)
    return rows


ANY_NUM = re.compile(r"(?:\d+(?:\.\d+)?|[一二三四五六七八九十百两半]+)"
                     r"(?:kg|公斤|千克|斤|吨|毫米|cm|厘米|米|m|km|千米|公里|伏|kV|V|安|毫安|"
                     r"分钟|秒|小时|天|日|周|月|年|次|人|个|台|件|处|%|‰|度|级)")


def numbers_in(sent):
    """句子里所有「数字+单位」（带边界检查，P2MP 里的 2M 不算）"""
    out = []
    for m in ANY_NUM.finditer(sent):
        a = sent[m.start() - 1] if m.start() else ""
        b = sent[m.end()] if m.end() < len(sent) else ""
        if a.isascii() and a.isalnum():
            continue
        if b.isascii() and b.isalnum():
            continue
        out.append(m.group(0))
    return out


def flex(token):
    """把 token 变成「字符之间允许空白」的正则：pdftotext 会把 0.5m 排成 "0. 5m" """
    return r"\s*".join(re.escape(c) for c in token)


def unit_of(token):
    m = TOKEN_RX.match(token)
    return m.group(1) if m else ""


NORM_WORD = re.compile(r"不得|不应|至少|不少于|不超过|必须|严禁|禁止|要求|应|每|检查|测试|进行|使用|保持|设置|作业|确认|以上|以下|之内|以内")


def tidy(stem, tail_ok=False):
    """掐掉从原文行首/行尾带进来的半截话。
    PDF 按行切句会把「…考核制度，安全带每…」切出「度，…」这种头，也会留下「…帽壳强」这种断尾。
    tail_ok=True 表示原文那句是完整句（以。；结束），此时最后一个小句可以保留。"""
    parts = stem.split("，")
    if len(parts) > 1 and len(parts[0].strip()) <= 4:
        parts = parts[1:]
    if len(parts) > 1:
        last = parts[-1].strip()
        if (len(last) <= 5 or (not tail_ok and not NORM_WORD.search(last))) and not last.endswith(("。", "；", "）", "”")):
            parts = parts[:-1]
    return "，".join(parts).strip("，、 　")


_FLAT = None


def _label_map():
    """平台附件（《管理办法》正文与附件1《作业规范》）里的句子 → 「《管理办法（试行）·附件1》小节」这种出处。
    复用 cite_law.py 的小节切分与 flat()，免得同一个出处两套写法（也比过：标点不归一就永远匹配不上）。"""
    global _FLAT
    try:
        import cite_law as CL
        units = CL.sent_units(CL.CORPORA["电信"], "电信")
    except Exception:
        return []
    _FLAT = CL.flat
    return [(CL.flat(sent), a, l) for a, l, sent in units]


def source_of(sent, book, page, labels):
    """出处的写法：平台附件优先换成人能翻到的小节，其它保持「书名 第N页」"""
    if book.startswith("平台附件") and labels and _FLAT:
        fs = _FLAT(sent)
        best = None
        for unit, alias, label in labels:
            if fs and fs in unit and (best is None or len(unit) < len(best[0])):
                best = (unit, alias, label)
        if best:
            return "《%s》%s 第%s页" % (best[1], best[2], page)
    return "%s 第%s页" % (book, page)


def build():
    rows = load_rows()
    labels = _label_map()
    pool = {}
    for r in rows:
        pool.setdefault((r["category"], unit_of(r["num"])), set()).add(r["num"])
    out, seen = [], set()
    for r in sorted(rows, key=lambda x: (x["category"], x["num"], x["sentence"])):
        sent, token, cat = re.sub(r"\s+", " ", r["sentence"]), r["num"], r["category"]
        if not (8 <= len(sent) <= 140):
            continue
        m = re.search(flex(token), sent)
        if not m:
            continue                         # 原文里找不到这个数值（排版差异）→ 放弃这条
        nxt = sent[m.end():m.end() + 1]
        if token.endswith("个") and nxt in ("月", "年", "天", "小时"):
            token = token + nxt              # 「每 12 个月」被抽成「12个」→ 补回真正的单位，
                                             # 不然挖空后成「每（　）月」，选项却是「12个」
        if cat in EXTRA_CATS and re.sub(r"\s+", "", sent).endswith(re.sub(r"\s+", "", token)):
            continue                         # 「…每 12 个」这种被 PDF 断行截断的，量词后面真正的单位在下一行
        others_in_sent = [n for n in numbers_in(sent) if n != token]
        if any(unit_of(n) == unit_of(token) for n in others_in_sent):
            continue                       # 同单位还有别的数字 → 挖空会歧义
        if any(n == token for n in others_in_sent):
            continue                       # 同值出现在别处 → 等于泄题
        # 注意：空框里的 U+3000 也是空白，绝不能再对它做 \s+ 归一化（否则空框变「( )」，判空就挂）
        tail_ok = bool(re.search(r"[。；”.]$", r["sentence"].strip()))
        stem = tidy(sent[:m.start()] + "（　）" + sent[m.end():], tail_ok)
        if len(stem) < 10:
            continue
        if stem in seen:
            continue
        others = sorted((v for v in pool.get((cat, unit_of(token)), ()) if v != token),
                        key=lambda v: abs(num_points.to_value(v) - num_points.to_value(token)))  # 同一单位 + 数值挨着
        if len(others) < 3:
            continue                       # 同单位的真实数值不够 3 个 → 不出这道题（不编数字）
        rnd = random.Random(hashlib.md5((sent + token).encode()).hexdigest())   # 可复现的洗牌
        distract = others[:6]                                  # 从最接近的 6 个里随机取 3，避免每次都是同一组
        distract = rnd.sample(distract, 3)
        opts = [token] + distract
        rnd.shuffle(opts)
        keys = "ABCD"
        seen.add(stem)
        out.append({
            "id": "GEN-N%04d" % (len(out) + 1), "number": len(out) + 1, "type": "single",
            "typeLabel": "单选（生成）", "question": stem, "score": 1,
            "options": [{"key": keys[i], "text": o} for i, o in enumerate(opts)],
            "answerKeys": [keys[opts.index(token)]], "answerText": [token],
            # 解析里放**整句原文**（不 tidy）：tidy 会把含数字的末句掐掉，答案就查不到了，
            # 而且「依据」本来就该是完整原句
            "analysis": "原文：%s（%s）" % (re.sub(r"\s+", " ", sent),
                                          source_of(sent, r["book"], r["page"], labels)),
            "topic": "综维安全数字（生成题）", "generated": True,
            "srcBank": "自测-安全数字", "evidence": {"book": r["book"], "page": r["page"], "sentence": sent},
        })
        # 同一句再出 1 道判断题：一半用原文值（对），一半把数值换成同单位邻近值（错）
        n_jud = sum(1 for q in out if q["type"] == "judge")
        make_false = n_jud % 2 == 0                     # 一「对」一「错」交替，否则全是一个答案等于送分
        text = tidy(sent[:m.start()] + (distract[0] if make_false else token) + sent[m.end():])
        if len(text) >= 10 and text not in seen:
            seen.add(text)
            out.append({
                "id": "GEN-J%04d" % (n_jud + 1), "number": len(out) + 1,
                "type": "judge", "typeLabel": "判断（生成）", "question": text, "score": 1,
                "options": [{"key": "A", "text": "对"}, {"key": "B", "text": "错"}],
                "answerKeys": ["B" if make_false else "A"],
                "answerText": ["错（原文是 %s）" % token] if make_false else ["对（与原文一致）"],
                "analysis": "%s：原文为「%s」（%s）" % ("错" if make_false else "对",
                            re.sub(r"\s+", " ", sent),
                            source_of(sent, r["book"], r["page"], labels)),
                "topic": "综维安全数字（生成题）", "generated": True, "srcBank": "自测-安全数字",
                "evidence": {"book": r["book"], "page": r["page"], "sentence": sent},
            })
    return out


def main():
    qs = build()
    data = {"meta": {"title": "自测 · 综维安全数字（生成题，非真题）", "generated": True,
                     "sourceNote": "由 crawl/数字考点速查.csv 生成：把带数字的规范句挖空，干扰项取同类别真实数值。"
                                   "共 %d 题。每题 analysis 里有原文与页码，可逐条回查。" % len(qs)},
            "questions": qs}
    json.dump(data, open(OUT, "w", encoding="utf-8"), ensure_ascii=False, indent=1)
    print("已生成：%s —— %d 题（来源：数字考点速查.csv）" % (OUT, len(qs)))
    from collections import Counter
    print("  题型:", Counter(q["type"] for q in qs))
    for q in qs[:3]:
        print("   题%s %s" % (q["number"], q["question"][:56]))
        print("        选项: %s ｜ 正确答案 %s（%s）" % (" / ".join(o["text"] for o in q["options"]),
                                                    q["answerKeys"][0], q["answerText"][0]))
    return qs


def selftest():
    qs = build()
    sel = [q for q in qs if q["type"] == "single"]
    jud = [q for q in qs if q["type"] == "judge"]
    assert len(sel) >= 25 and len(jud) >= 20, "生成题太少：单选 %d / 判断 %d" % (len(sel), len(jud))
    from collections import Counter as _C
    dist = _C(q["answerKeys"][0] for q in jud)
    assert set(dist) == {"A", "B"}, "判断题答案只有一个方向（等于送分）：%s" % dict(dist)
    ids = [q["id"] for q in qs]
    assert len(set(ids)) == len(ids), "id 有重复"
    for q in qs:
        assert q["generated"] and q["srcBank"], "生成题没打标记，会跟真题混"
        assert q["question"] == q["question"].strip() and not q["question"].startswith(("，", "、")), \
            "题干首尾没清干净：%s" % q["question"][:40]
        need = 2 if q["type"] == "judge" else 4
        assert len(q["options"]) == need and len({o["text"] for o in q["options"]}) == need, \
            "%s 的选项数不对/有重复：%s" % (q["type"], [o["text"] for o in q["options"]])
        assert q["answerKeys"][0] in [o["key"] for o in q["options"]], "答案不在选项里"
        if q["type"] == "single":
            assert "（　）" in q["question"], "题干没挖空：%s" % q["question"][:40]
        assert not JUNK_STEM.search(q["question"]), "题干里混进了表头/交叉引用：%s" % q["question"][:40]
        assert re.search(r"第\d+页", q["analysis"]), "解析缺页码"
        # 正确答案必须真的出现在原文里
        assert q["answerKeys"][0] in ("A", "B", "C", "D"), "答案键异常"
        if q["type"] == "single":
            # 原文里数字和单位之间可能有空格（"2 年"），比较时把空白去掉
            assert q["answerText"][0] in q["analysis"].replace(" ", ""), \
                "答案与原文对不上：%s" % q["analysis"][:60]
    # 抽一条已知的核对：梯子高出吊线 0.5m
    # 干扰项必须与答案同单位、且值各不相同
    for q in qs:
        if q["type"] != "single":
            continue
        unit = unit_of(q["answerText"][0])
        assert all(unit_of(o["text"]) == unit for o in q["options"]), "有跨单位干扰项：%s" % q["options"]
        assert len({o["text"] for o in q["options"]}) == 4
    hit = [q for q in qs if "顶端至少应高出吊线" in q["question"]]
    assert hit, "缺「梯子高出吊线」那条已知题"
    assert hit[0]["answerText"][0] == "0.5m", "那条的答案应为 0.5m，实得 %s" % hit[0]["answerText"][0]
    assert hit[0]["answerKeys"][0] in "ABCD"
    print("selftest OK：%d 道生成题（单选 %d / 判断 %d），id 唯一、选项不重复、答案在选项内、页码齐备；"
          "「梯子高出吊线」那条答案 = 0.5m（%s）" % (len(qs), len(sel), len(jud), hit[0]["answerKeys"][0]))


if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1] == "selftest":
        selftest()
    else:
        main()
