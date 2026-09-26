#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""把 banks/*.json 同步到 shiroha-quiz 网页端：拷文件 + 重建 banks-index.json"""
import json, os, shutil, collections

BASE = os.path.dirname(os.path.abspath(__file__))   # 别硬编码机器路径：换位置就会写错地方
BANKS = os.path.join(BASE, "banks")
WEB = os.path.join(BASE, "tools/shiroha-quiz-web-v0.8.7-alpha")
DATA = os.path.join(WEB, "data")

os.makedirs(DATA, exist_ok=True)
index, total = [], 0

# 先保留网页端原有的、不由 banks/ 管理的库（例如自带的驾照题库），
# 否则重建索引会把它们挤掉
IDX = os.path.join(DATA, "banks-index.json")
managed = {f[:-5] for f in os.listdir(BANKS) if f.endswith(".json")}
if os.path.exists(IDX):
    for e in json.load(open(IDX, encoding="utf-8")):
        if e.get("id") not in managed and os.path.exists(os.path.join(WEB, e.get("file", ""))):
            index.append(e)
            total += e.get("count", 0)
            print("  [保留原库] %-20s %4d 题" % (e.get("name", "")[:20], e.get("count", 0)))

for f in sorted(os.listdir(BANKS)):
    if not f.endswith(".json"):
        continue
    b = json.load(open(os.path.join(BANKS, f), encoding="utf-8"))
    qs = b.get("questions", [])
    if not qs:
        print("  跳过空库: %s" % f)
        continue
    meta = b.get("meta", {})
    shutil.copy2(os.path.join(BANKS, f), os.path.join(DATA, f))
    n = len(qs)
    total += n
    index.append({
        "id": f[:-5],
        "name": meta.get("title", f[:-5]),
        "count": n,
        "file": "data/%s" % f,
        "description": meta.get("sourceNote", ""),
    })
    print("  %-26s %4d 题  -> data/%s" % (meta.get("title", f[:-5])[:24], n, f))

tmp = os.path.join(DATA, "banks-index.json.tmp")
with open(tmp, "w", encoding="utf-8") as fh:
    json.dump(index, fh, ensure_ascii=False, indent=1)
os.replace(tmp, os.path.join(DATA, "banks-index.json"))   # 原子替换：写一半崩不会留半个索引
print("\n题库数: %d   合计 %d 题   已写入 data/banks-index.json" % (len(index), total))
