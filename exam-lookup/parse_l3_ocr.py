#!/usr/bin/env python3
"""把 Vision OCR 的原始输出解析成结构化题目。

输入：ocr_zh 的输出（每个文件一段 "===== 路径"，后面是识别行）
输出：jr/out/l3-questions.json —— [{no, type, stem, options:{A..}, images:[...]}]

只做结构切分，不改字；识别错字留给人工校对（另出一份 needs_review 清单）。
"""
import argparse
import json
import os
import re
import sys

PROJ = os.path.expanduser("~/x_videos-dowloads/exam-lookup")
OUT = os.path.join(PROJ, "jr/out")

# 选项行：A、xxx / A. xxx / A xxx（判断题是「A 正确」「B、错误」）
OPT = re.compile(r"^([A-F])\s*[、.．]?\s*(.*)$")
NUM = re.compile(r"^(\d{1,3})\s*[、.．]\s*(单选题|多选题|判断题|单选|多选|判断)")
JUNK = re.compile(r"^(计分方式|拖动题干|必|null|\d+\s*分)")


def clean(s):
    s = s.replace("\u00a0", " ").strip()
    s = re.sub(r"\s{2,}", " ", s)
    return s


def parse(text):
    blocks, cur = [], None
    for line in text.splitlines():
        if line.startswith("===== "):
            cur = {"image": line[6:].strip(), "lines": []}
            blocks.append(cur)
        elif cur is not None:
            cur["lines"].append(line)

    items = []
    for b in blocks:
        lines = [clean(x) for x in b["lines"]]
        lines = [x for x in lines if x and not JUNK.match(x)]
        if not lines:
            continue
        head = None
        m = NUM.match(lines[0])
        if not m:                                   # 有的截图没截到题号，退一步找含"单选/多选/判断"的行
            for i, x in enumerate(lines[:3]):
                m2 = re.search(r"(单选题|多选题|判断题)\s*\d?", x)
                if m2:
                    m, head = re.search(r"(\d{1,3})", x) or None, i
                    break
            if not m:
                continue
            no = int(m.group(1)) if m else None
            typ = re.search(r"(单选题|多选题|判断题)", lines[head]).group(1)
        else:
            no, typ = int(m.group(1)), m.group(2)
            head = 0
            typ = typ if typ.endswith("题") else typ + "题"
        body = lines[head + 1:]
        # 题干 = 到第一个选项行之前
        opts, stem_parts, seen_opt = {}, [], False
        for x in body:
            mo = OPT.match(x)
            if mo and (mo.group(1) not in opts):
                seen_opt = True
                opts[mo.group(1)] = mo.group(2).strip()
            elif mo and seen_opt and mo.group(1) in opts:
                # 同字母重复出现（跨题连拍），停
                break
            elif not seen_opt:
                stem_parts.append(x)
            else:
                # 选项内的续行
                k = sorted(opts)[-1]
                opts[k] = (opts[k] + x).strip()
        stem = " ".join(stem_parts).strip()
        if not stem or not opts:
            continue
        items.append({"no": no, "type": typ, "stem": stem,
                      "options": dict(sorted(opts.items())), "image": b["image"]})
    return items


def dedupe(items):
    out, seen = [], {}
    for it in items:
        key = re.sub(r"[\s，。、；：？！（）()\"'“”‘’]", "", it["stem"])
        if key in seen:
            out[seen[key]]["images"].append(it["image"])
            continue
        it["images"] = [it["image"]]
        seen[key] = len(out)
        out.append(it)
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("ocr_txt", nargs="?", default=os.path.join(OUT, "l3-ocr-vision.txt"))
    ap.add_argument("--out", default=os.path.join(OUT, "l3-questions.json"))
    a = ap.parse_args()
    text = open(os.path.expanduser(a.ocr_txt), encoding="utf-8").read()
    raw = parse(text)
    items = dedupe(raw)
    nums = [i["no"] for i in items if i["no"]]
    json.dump({"count": len(items), "questions": items},
              open(a.out, "w", encoding="utf-8"), ensure_ascii=False, indent=1)
    print("截图 %d 张 → 解析 %d 条 → 去重后 %d 道" % (text.count("===== "), len(raw), len(items)))
    print("题号范围 %s..%s；缺号 %s" % (min(nums or [0]), max(nums or [0]),
                                      sorted(set(range(min(nums or [1]), max(nums or [1]) + 1)) - set(nums))))
    miss_opt = [i["no"] for i in items if len(i["options"]) < 2]
    if miss_opt:
        print("选项少于2个（需人工看）：", miss_opt)
    print("→", a.out)
    return 0


if __name__ == "__main__":
    sys.exit(main())
