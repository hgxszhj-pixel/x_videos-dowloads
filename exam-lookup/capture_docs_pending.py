#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""把 ~/Documents 里「有题无答案」的几套抓进 pending.jsonl（待补答案队列）"""
import re, json, os, time, subprocess

D = "/Users/hgxszhj/Documents"
OUT = "/Users/hgxszhj/x_videos-dowloads/exam-lookup/pending.jsonl"
ts = time.strftime("%Y-%m-%d %H:%M:%S")
rows = []

def add(q, src, note=""):
    q = re.sub(r"\s+", " ", q).strip()
    if len(q) > 12:
        rows.append({"question": q, "query": "", "guess": None, "candidates": [],
                     "evidence": ["来源: %s %s" % (src, note)], "ts": ts})

# ---- 1. 题目提取结果（50 题，天翼安全/资产监测）----
p1 = os.path.join(D, "题目提取结果_2025-09-03T05-08-20.txt")
if os.path.exists(p1):
    t = open(p1, encoding="utf-8", errors="ignore").read()
    for m in re.finditer(r"(?m)^\s*\d+\.\s*(.+?)$", t):
        add(m.group(1), "题目提取结果.txt", "(原文无答案)")

# ---- 2. 2022 云网工程师四级认证（GBK，单/多选）----
p2 = os.path.join(D, "WeChat Files/yydxzhj/FileStorage/File/2022-10",
                  "2022年云网工程师-云网融合实操（传输）四级认证考试（10月场）.txt")
if os.path.exists(p2):
    t = subprocess.run(["iconv", "-f", "GBK", "-t", "UTF-8", p2],
                       capture_output=True, text=True).stdout
    for m in re.finditer(r"(?m)^\s*\d+\.(.+?)（\d+分）\s*$", t):
        add(m.group(1), "2022云网工程师四级认证.txt", "(原文无答案)")

# ---- 3. 大数据和AI考试题目单选（PDF）----
p3 = os.path.join(D, "2023-年1-17日大数据和ai考试题目单选.pdf")
if os.path.exists(p3):
    t = subprocess.run(["pdftotext", p3, "-"], capture_output=True, text=True).stdout
    for m in re.finditer(r"(?m)^\s*\d+[、.]\s*(.+?)$", t):
        add(m.group(1), "2023大数据和AI考试题目单选.pdf", "(原文无答案)")

with open(OUT, "a", encoding="utf-8") as f:
    for r in rows:
        f.write(json.dumps(r, ensure_ascii=False) + "\n")

print("已抓入待办队列: %d 题" % len(rows))
from collections import Counter
c = Counter(r["evidence"][0].split()[1] for r in rows)
for k, v in c.most_common():
    print("   %3d  %s" % (v, k))
print("-> %s" % OUT)
