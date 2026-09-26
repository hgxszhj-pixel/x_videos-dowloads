#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""解析「通信安全生产考试试卷（50题）」→ banks/comm-safety-50.json

试卷格式（用户提供的打印版）：
  一、单项选择题（共20题，每题2分，合计40分）
  1. 题干（）
  A、选项
  答案：A
  二、判断题（共30题，每题2分，合计60分）
  21. 题干（）
  答案：√        # √→正确(A) ×→错误(B)

答案由试卷自带，不做模型判定；跑完断言题数/题型/答案完整性，对不上就 exit 1。
"""
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
SRC = os.path.join(HERE, "通信安全生产考试试卷50题.txt")
OUT = os.path.join(HERE, "banks", "comm-safety-50.json")
PREFIX, TOPIC = "TXAQ", "通信安全生产"
TITLE = "通信安全生产考试试卷（50题）"

SECTION = re.compile(r"^[一二三四五六七八九十]+、(.+?)（\s*共\s*(\d+)\s*题[，,]\s*每题\s*([\d.]+)\s*分")
QHEAD = re.compile(r"^(\d+)\s*[.、]\s*(.+)$")
OPT = re.compile(r"^([A-F])\s*[、.．]\s*(.+)$")
ANS = re.compile(r"^答案\s*[：:]\s*(.+)$")
TAIL_BLANK = re.compile(r"[（(]\s*[)）]\s*$")      # 题干末尾的填空括号，页面上由选项代替


def parse(path):
    qs, kind, score, cur = [], None, 1.0, None
    for raw in open(path, encoding="utf-8"):
        line = raw.strip()
        if not line:
            continue
        m = SECTION.match(line)
        if m:
            # 注意：不能判断 "单选" in "单项选择题" —— 该字符串是 单+项+选+择，中间隔着「项」，
            # 子串匹配必然为假（踩过：整个单选段被跳过，只剩 30 道判断题）
            kind = "judge" if "判断" in m.group(1) else "single" if "选择" in m.group(1) else None
            score = float(m.group(3))
            continue
        m = QHEAD.match(line)
        if m and kind:
            if cur:
                qs.append(cur)
            cur = {"number": int(m.group(1)), "type": kind, "score": score,
                   "question": TAIL_BLANK.sub("", m.group(2)).strip(), "options": [], "raw": ""}
            continue
        if cur is None:
            continue
        m = OPT.match(line)
        if m:
            cur["options"].append({"key": m.group(1), "text": m.group(2).strip()})
            continue
        m = ANS.match(line)
        if m:
            cur["raw"] = m.group(1).strip()
    if cur:
        qs.append(cur)
    return qs


def to_bank(qs):
    out = []
    for q in qs:
        raw = q["raw"]
        if q["type"] == "judge":
            keys = ["A"] if raw in ("√", "对", "正确", "T", "是") else ["B"]
            opts = [{"key": "A", "text": "正确"}, {"key": "B", "text": "错误"}]
        else:
            keys = sorted(set(re.sub(r"[^A-F]", "", raw)))
            opts = q["options"]
        om = {o["key"]: o["text"] for o in opts}
        if not keys or any(k not in om for k in keys):
            raise SystemExit("第 %d 题答案无法对应选项：raw=%r keys=%s" % (q["number"], raw, keys))
        out.append({
            "id": "%s-%s%04d" % (PREFIX, q["type"][:1], q["number"]),
            "volume": 1, "type": q["type"],
            "typeLabel": {"single": "单选题", "judge": "判断题"}[q["type"]],
            "number": q["number"], "question": q["question"], "options": opts,
            "answerKeys": keys, "answerText": [om[k] for k in keys],
            "topic": TOPIC, "category": TOPIC, "score": q["score"],
            "analysis": "答案来源：试卷自带（打印版，未逐题核对官方原文）",
        })
    return out


def main():
    bank = to_bank(parse(SRC))
    nums = [q["number"] for q in bank]
    assert len(bank) == 50, "题数应为 50，实得 %d" % len(bank)
    assert nums == list(range(1, 51)), "题号应 1-50 连续，实得 %s" % nums
    assert sum(1 for q in bank if q["type"] == "single") == 20, "单选应 20 题"
    assert sum(1 for q in bank if q["type"] == "judge") == 30, "判断应 30 题"
    assert all(q["answerKeys"] and q["answerText"] for q in bank), "有题缺答案"
    assert all(len(q["options"]) == 3 for q in bank if q["type"] == "single"), "单选题选项数不是 3"
    meta = {"title": TITLE, "questionCount": len(bank), "volumes": 1,
            "sourceNote": "%s（单选20题×2分 + 判断30题×2分，答案试卷自带）。原稿：通信安全生产考试试卷50题.txt"
                          % TITLE}
    os.makedirs(os.path.dirname(OUT), exist_ok=True)
    json.dump({"meta": meta, "questions": bank}, open(OUT, "w", encoding="utf-8"),
              ensure_ascii=False, indent=1)
    print("解析成功：%d 题（单选 %d / 判断 %d）"
          % (len(bank), sum(1 for q in bank if q["type"] == "single"),
             sum(1 for q in bank if q["type"] == "judge")))
    for q in (bank[0], bank[19], bank[20], bank[-1]):
        print("  [%s] 第%d题 %s  -> %s" % (q["id"], q["number"], q["question"][:36],
                                           "".join(q["answerKeys"])))
    print("-> %s" % OUT)
    return 0


if __name__ == "__main__":
    sys.exit(main())
