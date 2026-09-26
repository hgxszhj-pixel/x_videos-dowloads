#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""导出复核清单：把所有需要人眼确认的题导成 CSV（可排序筛选）+ txt（可读）

需要复核的 = 低置信(low) ∪ 中置信(medium) ∪ 带 ⚠ 标记 ∪ 多选题
（高置信单选且无标记的，才算「模型判定可直接用」）
"""
import json, os, csv, collections

BASE = "/Users/hgxszhj/x_videos-dowloads/exam-lookup"
BANKS = os.path.join(BASE, "banks")

rows, total, stat, need = [], 0, collections.Counter(), 0
for f in sorted(os.listdir(BANKS)):
    if not f.endswith(".json"):
        continue
    b = json.load(open(os.path.join(BANKS, f), encoding="utf-8"))
    for q in b.get("questions", []):
        total += 1
        conf = q.get("confidence", "")
        ana = q.get("analysis", "")
        reasons = []
        if conf == "low":
            reasons.append("低置信")
        elif conf == "medium":
            reasons.append("中置信")
        if "⚠" in ana:
            reasons.append("可能漏选")
        stat[conf or "试卷自带/已核对"] += 1
        opts = {o["key"]: o["text"] for o in q["options"]}
        if reasons:
            need += 1
        rows.append({
            "题库": b["meta"].get("title", f)[:26],
            "题号": q.get("id", ""),
            "题型": q.get("typeLabel", q.get("type", "")),
            "题干": q["question"],
            "A": opts.get("A", ""), "B": opts.get("B", ""),
            "C": opts.get("C", ""), "D": opts.get("D", ""), "E": opts.get("E", ""),
            "判定答案": "/".join(q.get("answerKeys") or []),
            "答案文本": "；".join(q.get("answerText") or []),
            "置信度": conf or "（试卷自带）",
            "需复核原因": "、".join(reasons),
            "依据": ana.replace("⚠题干含「哪些/包括」但只给一个答案，可能漏选，建议复核 | ", "")
                       .replace("【答案来源】", ""),
        })

CSV = os.path.join(BASE, "复核清单.csv")
with open(CSV, "w", encoding="utf-8-sig", newline="") as fh:   # utf-8-sig 让 Excel 不乱码
    w = csv.DictWriter(fh, fieldnames=list(rows[0].keys()))
    w.writeheader()
    w.writerows(rows)

TXT = os.path.join(BASE, "复核清单.txt")
with open(TXT, "w", encoding="utf-8") as fh:
    fh.write("题库复核清单\n" + "=" * 72 + "\n")
    fh.write("题库总量 %d 题；需人工复核 %d 题（低置信/中置信/可能漏选）\n" % (total, need))
    fh.write("置信度分布: %s\n" % ", ".join("%s %d" % kv for kv in stat.most_common()))
    fh.write("=" * 72 + "\n\n")
    for r in [x for x in rows if x["需复核原因"]]:
        fh.write("【%s】%s  %s  答案=%s  (%s)\n" % (
            r["题号"], r["题型"], r["题干"][:70], r["判定答案"], r["置信度"]))
        for k in "ABCDE":
            if r[k]:
                mark = "✔" if k in r["判定答案"] else " "
                fh.write("    %s %s. %s\n" % (mark, k, r[k][:66]))
        fh.write("    复核原因: %s\n" % r["需复核原因"])
        fh.write("    依据: %s\n\n" % r["依据"][:150])

print("题库总量 %d 题" % total)
print("置信度分布: %s" % ", ".join("%s %d" % kv for kv in stat.most_common()))
print("需人工复核 %d 题（CSV 含全部 %d 题，可排序筛选）" % (need, len(rows)))
print("  按原因: %s" % ", ".join(
    "%s %d" % kv for kv in collections.Counter(
        x for r in rows for x in r["需复核原因"].split("、") if x).most_common()))
print("-> %s" % CSV)
print("-> %s" % TXT)
