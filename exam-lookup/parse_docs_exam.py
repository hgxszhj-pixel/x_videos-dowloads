#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""解析 ~/Documents/考试.txt.txt（云改数转智惠战略）→ 题库 JSON

格式：
  1、 题干（单选题(1.5分)）
   A. 选项
   B. 选项
  【答案】A
"""
import json, os, re, sys

SRC = "/Users/hgxszhj/Documents/考试.txt.txt"
OUT = "/Users/hgxszhj/x_videos-dowloads/exam-lookup/banks/dtzx-yungai.json"

txt = open(SRC, encoding="utf-8", errors="ignore").read().replace("\r\n", "\n")
# 按题号切块
blocks = re.split(r"\n(?=\s*\d+、)", "\n" + txt)
qs, skipped, ordering = [], 0, []
for b in blocks:
    b = b.strip()
    if not b:
        continue
    m = re.match(r"(\d+)、\s*(.+?)\s*\n(.*)", b, re.S)
    if not m:
        skipped += 1
        continue
    num = int(m.group(1))
    stem = m.group(2).strip()
    body = m.group(3)
    opts = re.findall(r"^\s*([A-F])[.、．]\s*(.+?)\s*$", body, re.M)
    # 答案可能是「A」、「A、B、C、D」、「CBDAFE」（排序题）——必须吃下分隔符，
    # 否则 [A-D]{1,4} 遇到「、」就断，只取到第一个字母（曾经把这 20 题全截断成单答案）
    am = re.search(r"【答案】\s*([A-F](?:\s*[、,，/]\s*[A-F])*|正确|错误|对|错)", body)
    a = re.sub(r"[^A-F]", "", am.group(1)) if am else ""
    raw_ans = am.group(1).strip() if am else ""
    # 判断题：无选项，答案为 正确/错误
    if not opts and raw_ans in ("正确", "错误", "对", "错"):
        qs.append({
            "id": "YG-%04d" % num, "volume": 1, "type": "judge",
            "typeLabel": "判断题", "number": num,
            "question": re.sub(r"（[^）]*）\s*$", "", stem).strip(),
            "options": [{"key": "A", "text": "正确"}, {"key": "B", "text": "错误"}],
            "answerKeys": ["A"] if raw_ans in ("正确", "对") else ["B"],
            "answerText": ["正确" if raw_ans in ("正确", "对") else "错误"],
            "topic": "云改数转智惠战略", "category": "云改数转智惠战略", "score": 1,
            "analysis": "来源：~/Documents/考试.txt.txt（原始答案由试卷自带【答案】标记）",
        })
        continue
    if not opts:
        skipped += 1
        continue
    # 排序题（答案是有序序列如 CBDAFE）无法用「选项集合」表达，跳过不硬塞
    if "排序" in stem or (len(a) == len(opts) and len(a) > 2 and "、" not in raw_ans):
        skipped += 1
        ordering.append((num, raw_ans))
        continue
    keys = sorted(set(a)) if a else []        # 多选答案排序，避免 CBDA 这种乱序
    t = "multiple" if len(keys) > 1 else "single"   # 以答案键个数为准，见上
    om = dict(opts)
    qs.append({
        "id": "YG-%04d" % num, "volume": 1, "type": t,
        "typeLabel": "多选题" if t == "multiple" else "单选题",
        "number": num,
        "question": re.sub(r"（[^）]*）\s*$", "", stem).strip(),
        "options": [{"key": k, "text": v} for k, v in opts],
        "answerKeys": keys,
        "answerText": [om[k] for k in keys if k in om],
        "topic": "云改数转智惠战略", "category": "云改数转智惠战略", "score": 1.5,
        "analysis": "来源：~/Documents/考试.txt.txt（原始答案由试卷自带【答案】标记）",
    })

bank = {"meta": {"title": "中国电信「云改数转智惠」战略考试（自带答案）",
                 "questionCount": len(qs), "volumes": 1,
                 "sourceNote": "解析自 ~/Documents/考试.txt.txt，答案为试卷自带【答案】标记，未经二次核对。"},
        "questions": qs}
os.makedirs(os.path.dirname(OUT), exist_ok=True)
json.dump(bank, open(OUT, "w", encoding="utf-8"), ensure_ascii=False, indent=1)
print("题目解析成功: %d 题 (跳过 %d 块)" % (len(qs), skipped))
print("有答案: %d / 无答案: %d" % (sum(1 for q in qs if q["answerKeys"]),
                                  sum(1 for q in qs if not q["answerKeys"])))
print("题型分布: 单选 %d / 多选 %d / 判断 %d" % (
    sum(1 for q in qs if q["type"] == "single"),
    sum(1 for q in qs if q["type"] == "multiple"),
    sum(1 for q in qs if q["type"] == "judge")))
if ordering:
    print("跳过排序题 %d 道（答案是有序序列，不适合选项集合模型）: %s"
          % (len(ordering), ", ".join("第%d题=%s" % o for o in ordering)))
print("-> %s" % OUT)
if qs:
    print("\n抽样:")
    for q in (qs[0], qs[len(qs) // 2], qs[-1]):
        print("  [%s] %s" % (q["id"], q["question"][:52]))
        print("       %s 答案=%s %s" % (q["typeLabel"], "/".join(q["answerKeys"]) or "（缺）",
                                        "、".join(q["answerText"])[:40]))
