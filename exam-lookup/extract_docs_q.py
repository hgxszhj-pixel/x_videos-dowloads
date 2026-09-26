#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""从 ~/Documents 三套无答案试卷抽取「题干 + 选项 + 题型 + 唯一ID」→ JSON

要点（都是踩过的坑）：
  1. OTN 卷分三节（单选/多选/判断），**每节题号都从 1 重排** → ID 必须带节号，否则撞车
  2. 判断题有两种写法：「正确，错误。」 和 「A.对 / B.错」→ 都要认出来
  3. 选项可能到 E（5 个），不能只认 A–D
  4. 该 txt 是 GBK，必须先 iconv
"""
import re, json, os, subprocess, collections

D = "/Users/hgxszhj/Documents"
OUT = "/Users/hgxszhj/x_videos-dowloads/exam-lookup/docs_questions.json"

SECT = re.compile(r"^\s*[一二三四五六]\s*[、.．]\s*(单选题|多选题|判断题|单选|多选|判断)[^（(]*")
QPAT = re.compile(r"^\s*(\d{1,3})\s*[、.．]\s*(.+)$")
OPAT = re.compile(r"^\s*([A-E])\s*[、.．]\s*(.*)$")
JPAT = re.compile(r"^\s*(正确|对)\s*[,，、]?\s*(错误|错)\s*[。.]?\s*$")
JUDGE_PAIR = {"对", "正确", "错", "错误"}


def tokens(text):
    text = text.replace("\r\n", "\n").replace("\u3000", " ")
    qs, cur, sec = [], None, (0, "single")

    def flush():
        if cur and cur["options"]:
            qs.append(cur)

    for ln in text.split("\n"):
        ms = SECT.match(ln)
        if ms:
            flush()
            cur = None
            nm = ms.group(1)
            sec = (sec[0] + 1, "multiple" if "多选" in nm else
                   "judge" if "判断" in nm else "single")
            continue
        mq, mo, mj = QPAT.match(ln), OPAT.match(ln), JPAT.match(ln)
        if mq and not mo:
            flush()
            cur = {"sec": sec[0], "num": int(mq.group(1)),
                   "stem": mq.group(2).strip(), "type": sec[1], "options": []}
            continue
        if cur is None:
            continue
        if mj:                                         # 「正确，错误。」
            cur["type"] = "judge"
            cur["options"] = [{"key": "A", "text": "正确"}, {"key": "B", "text": "错误"}]
            continue
        if mo:
            cur["options"].append({"key": mo.group(1), "text": mo.group(2).strip()})
            continue
        if ln.strip() and not cur["options"]:
            cur["stem"] += " " + ln.strip()            # 题干折行
    flush()
    return qs


def clean(qs, src, prefix):
    out, dropped = [], []
    for q in qs:
        s = re.sub(r"（[^）]*分[^）]*）|\([^)]*分[^)]*\)", "", q["stem"]).strip()
        s = re.sub(r"\s+", " ", s)
        opts = q["options"]
        # 「A.对 / B.错」→ 判断题
        if len(opts) == 2 and {o["text"] for o in opts} <= JUDGE_PAIR:
            q["type"] = "judge"
            opts = [{"key": "A", "text": "正确"}, {"key": "B", "text": "错误"}]
        # 注意：不要靠题干「哪些/包括」猜题型 —— 实测会误判
        # （「X属于第几类」「有多少个参数」「以下哪些方法不可以」都是单选）。
        # 题型权威来源是答案键个数，在 ingest_answers.py 里定。
        if len(s) < 8 or len(opts) < 2 or len(opts) > 5:
            dropped.append(q["num"])
            continue
        out.append({"id": "%s-S%d-%02d" % (prefix, q["sec"], q["num"]),
                    "num": q["num"], "sec": q["sec"], "stem": s,
                    "options": opts, "src": src, "type": q["type"]})
    return out, dropped


SETS = [
    ("bigdata-ai", "YB", os.path.join(D, "2023-年1-17日大数据和ai考试题目单选.pdf"), "pdf"),
    ("tianyi-security", "TY", os.path.join(D, "题目提取结果_2025-09-03T05-08-20.txt"), "utf8"),
    ("otn-transport", "OTN",
     os.path.join(D, "WeChat Files/yydxzhj/FileStorage/File/2022-10",
                  "2022年云网工程师-云网融合实操（传输）四级认证考试（10月场）.txt"), "gbk"),
]

result, totals = {}, collections.Counter()
for key, prefix, path, kind in SETS:
    if not os.path.exists(path):
        print("!! 缺文件:", path)
        continue
    if kind == "pdf":
        t = subprocess.run(["pdftotext", "-layout", path, "-"],
                           capture_output=True, text=True).stdout
    elif kind == "gbk":
        t = subprocess.run(["iconv", "-f", "GBK", "-t", "UTF-8", path],
                           capture_output=True, text=True).stdout
    else:
        t = open(path, encoding="utf-8", errors="ignore").read()
    got, dropped = clean(tokens(t), os.path.basename(path), prefix)
    result[key] = got
    c = collections.Counter(q["type"] for q in got)
    totals.update(c)
    print("%-16s %4d 题 | 单选%3d 多选%3d 判断%3d | 丢弃题号 %s" % (
        key, len(got), c["single"], c["multiple"], c["judge"], dropped or "无"))

json.dump(result, open(OUT, "w", encoding="utf-8"), ensure_ascii=False, indent=1)
print("\n合计 %d 题（单选%d / 多选%d / 判断%d）" % (
    sum(totals.values()), totals["single"], totals["multiple"], totals["judge"]))
print("-> %s" % OUT)
