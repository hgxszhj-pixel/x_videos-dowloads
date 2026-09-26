#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
make_collection.py —— 把若干套题库合成一份「长期备考」合集（刷题页直接用）
=====================================================================
  python3 make_collection.py                重建 banks/changqi-anquan.json
  python3 make_collection.py out.json a.json b.json   换名字/换来源

为什么要合并：刷题页是按「一个库 = 一个错题本」来记进度的（ask.html 的 localStorage 键
`examwrong:<文件名>`），安全类的两套题分着刷，错题本也是两本，复习时要来回切。
合成一套后：112 道题一个错题本、一个「只练错题」，长期刷才顺。

每题保留 srcBank 字段标明原出处，去重按「题干前 50 字归一化」。
"""
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
BANKS = os.path.join(HERE, "banks")
DEFAULT_SRC = ["dtzx-safety.json", "comm-safety-50.json"]        # 综维装维安全生产 + 通信安全生产
DEFAULT_OUT = "changqi-anquan.json"


def load(name):
    return json.load(open(os.path.join(BANKS, name), encoding="utf-8"))["questions"]


def merge(srcs):
    qs, seen, dup = [], set(), 0
    for name in srcs:
        for q in load(name):
            key = re.sub(r"\s+", "", q.get("question") or "")[:50]
            if key in seen:
                dup += 1
                continue
            seen.add(key)
            q = dict(q)
            q["srcBank"] = name[:-5]
            qs.append(q)
    return qs, dup


def main():
    args = sys.argv[1:]
    out = args[0] if args else DEFAULT_OUT
    srcs = args[1:] or DEFAULT_SRC
    n_in = sum(len(load(s)) for s in srcs)
    qs, dup = merge(srcs)
    title = "长期备考 · 综维装维安全生产（合集）" if srcs == DEFAULT_SRC else "合集 · " + " + ".join(s[:-5] for s in srcs)
    note = ("由 banks/%s 合并去重（%d 题 → %d 题，去重 %d）；每题 srcBank 标明出处。重跑：python3 make_collection.py"
            % ("+".join(srcs), n_in, len(qs), dup))
    assert all(q.get("answerKeys") and q.get("question") for q in qs), "合并后出现缺答案/缺题干的题"
    data = {"meta": {"title": title, "sourceNote": note, "mergeOf": srcs},
            "questions": qs}
    json.dump(data, open(os.path.join(BANKS, out), "w", encoding="utf-8"), ensure_ascii=False, indent=1)
    print("%s：%d 题（输入 %d，去掉重复 %d）" % (out, len(qs), n_in, dup))
    for s in srcs:
        print("   %-22s %3d 题" % (s, sum(1 for q in qs if q["srcBank"] == s[:-5])))
    return len(qs)


if __name__ == "__main__":
    main()
