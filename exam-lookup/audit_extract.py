#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""题目抽取质量体检：找出抽取噪声，避免脏题入库"""
import json, re, collections

Q = json.load(open("/Users/hgxszhj/x_videos-dowloads/exam-lookup/docs_questions.json",
                   encoding="utf-8"))
issues = collections.Counter()
detail = collections.defaultdict(list)
ids = collections.Counter()

for setname, qs in Q.items():
    for q in qs:
        ids[q["id"]] += 1
        st, op = q["stem"], q["options"]
        keys = [o["key"] for o in op]
        texts = [o["text"] for o in op]
        if len(st) < 12:
            issues["题干过短"] += 1; detail["题干过短"].append((q["id"], st))
        if any(len(t) < 1 for t in texts):
            issues["选项为空"] += 1; detail["选项为空"].append((q["id"], texts))
        if len(op) == 2 and q["type"] != "judge":
            issues["只有2选项但非判断题"] += 1; detail["只有2选项但非判断题"].append(
                (q["id"], st[:40] + " || " + " / ".join(texts)))
        if q["type"] == "judge" and len(op) != 2:
            issues["判断题选项数异常"] += 1; detail["判断题选项数异常"].append((q["id"], texts))
        if keys != sorted(keys):
            issues["选项顺序乱"] += 1; detail["选项顺序乱"].append((q["id"], "".join(keys)))
        if re.search(r"[（(]\s*[)）]\s*$", st):
            issues["题干尾部留空括号"] += 1
        if re.search(r"^\s*[A-E]\s*[、.．]", st):
            issues["题干里混进选项"] += 1; detail["题干里混进选项"].append((q["id"], st[:50]))

print("=== 抽取质量体检（共 %d 题）===" % sum(len(v) for v in Q.values()))
if not issues:
    print("干净，无问题 ✓")
for k, v in issues.most_common():
    print("  %-22s %4d" % (k, v))

print("\n=== 需要人眼看的样例 ===")
for k, lst in detail.items():
    if k in ("题干过短", "选项为空", "判断题选项数异常", "题干里混进选项"):
        print("-- %s:" % k)
        for i, s in lst[:8]:
            print("     %s  %s" % (i, str(s)[:96]))

print("\n=== 重复 ID ===")
dup = {k: v for k, v in ids.items() if v > 1}
print("  无" if not dup else "  %s" % dup)

print("\n=== 每套题型分布 ===")
for setname, qs in Q.items():
    c = collections.Counter(q["type"] for q in qs)
    oc = collections.Counter(len(q["options"]) for q in qs)
    print("  %-16s %3d题  单选%3d 多选%3d 判断%3d  选项数分布%s"
          % (setname, len(qs), c["single"], c["multiple"], c["judge"],
             dict(sorted(oc.items()))))
