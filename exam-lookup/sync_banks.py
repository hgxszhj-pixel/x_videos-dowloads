#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""把 banks/*.json 同步到 shiroha-quiz 网页端：拷文件 + 重建 banks-index.json。

    python3 sync_banks.py           # 同步（幂等，可反复跑）
    python3 sync_banks.py --check   # 只校验：网页端 data/ 与 banks/ 是否一致；正常则无输出
"""
import collections
import hashlib
import json
import os
import shutil
import sys

BASE = os.path.dirname(os.path.abspath(__file__))   # 别硬编码机器路径：换位置就会写错地方
BANKS = os.path.join(BASE, "banks")
WEB = os.path.join(BASE, "tools/shiroha-quiz-web-v0.8.7-alpha")
DATA = os.path.join(WEB, "data")
IDX = os.path.join(DATA, "banks-index.json")


def managed_banks():
    """banks/ 下真正要同步的库：文件名 → 题目列表（空库跳过，和同步逻辑保持一致）。"""
    out = {}
    for f in sorted(os.listdir(BANKS)):
        if not f.endswith(".json"):
            continue
        b = json.load(open(os.path.join(BANKS, f), encoding="utf-8"))
        qs = b.get("questions") or []
        if qs:
            out[f] = (b.get("meta", {}), qs)
    return out


def sync():
    os.makedirs(DATA, exist_ok=True)
    banks = managed_banks()
    managed = {f[:-5] for f in os.listdir(BANKS) if f.endswith(".json")}

    index, total = [], 0

    # 先保留网页端原有的、不由 banks/ 管理的库（例如自带的驾照题库），
    # 否则重建索引会把它们挤掉
    if os.path.exists(IDX):
        for e in json.load(open(IDX, encoding="utf-8")):
            if e.get("id") not in managed and os.path.exists(os.path.join(WEB, e.get("file", ""))):
                index.append(e)
                total += e.get("count", 0)
                print("  [保留原库] %-20s %4d 题" % (e.get("name", "")[:20], e.get("count", 0)))

    for f, (meta, qs) in banks.items():
        shutil.copy2(os.path.join(BANKS, f), os.path.join(DATA, f))
        n = len(qs)
        total += n
        index.append({"id": f[:-5], "name": meta.get("title", f[:-5]), "count": n,
                      "file": "data/%s" % f, "description": meta.get("sourceNote", "")})
        print("  %-26s %4d 题  -> data/%s" % (meta.get("title", f[:-5])[:24], n, f))

    tmp = IDX + ".tmp"
    with open(tmp, "w", encoding="utf-8") as fh:
        json.dump(index, fh, ensure_ascii=False, indent=1)
    os.replace(tmp, IDX)   # 原子替换：写一半崩不会留半个索引
    print("\n题库数: %d   合计 %d 题   已写入 data/banks-index.json" % (len(index), total))


def sha1(path):
    h = hashlib.sha1()
    with open(path, "rb") as f:
        for blk in iter(lambda: f.read(1 << 16), b""):
            h.update(blk)
    return h.hexdigest()


def check():
    """正常时一个字都不输出（给定时任务用：空输出=不打扰），有问题才打印并返回 1。"""
    bad = []
    if not os.path.exists(IDX):
        print("banks-index.json 不存在：先跑一次 python3 sync_banks.py")
        return 1
    index = json.load(open(IDX, encoding="utf-8"))
    by_id = {e.get("id"): e for e in index}
    banks = managed_banks()

    for f, (meta, qs) in banks.items():
        src, dst = os.path.join(BANKS, f), os.path.join(DATA, f)
        bid = f[:-5]
        if not os.path.exists(dst):
            bad.append("%s 没拷到 data/" % f)
        elif sha1(src) != sha1(dst):
            bad.append("%s 两边内容不一致（banks 改过但没同步）" % f)
        e = by_id.get(bid)
        if not e:
            bad.append("%s 不在 banks-index.json 里" % f)
        elif e.get("count") != len(qs):
            bad.append("%s 索引题数 %s ≠ 实际 %d" % (f, e.get("count"), len(qs)))

    # 反向：索引里指向 data/ 的文件必须存在（防止删了库却留条目）
    for e in index:
        p = os.path.join(WEB, e.get("file", ""))
        if e.get("file", "").startswith("data/") and not os.path.exists(p):
            bad.append("索引条目 %s 指向的文件不存在：%s" % (e.get("id"), e.get("file")))

    dup = [k for k, v in collections.Counter(e.get("id") for e in index).items() if v > 1]
    if dup:
        bad.append("索引里重复的 id：%s" % dup)

    for b in bad:
        print("✗ " + b)
    return 1 if bad else 0


def main():
    if "--check" in sys.argv[1:]:
        return check()
    sync()
    return 0


if __name__ == "__main__":
    sys.exit(main())
