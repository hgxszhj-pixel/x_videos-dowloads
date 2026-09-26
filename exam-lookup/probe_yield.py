#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""检索命中率实测：抽 12 题，走多个搜索路径，看能不能真拿到答案"""
import json, re, subprocess, random, sys, time, urllib.parse

Q = json.load(open("/Users/hgxszhj/x_videos-dowloads/exam-lookup/docs_questions.json", encoding="utf-8"))

def curl(url, ref=None, timeout=12):
    cmd = ["curl", "-sS", "-L", "--max-time", str(timeout),
           "-H", "User-Agent: Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) "
                 "AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36",
           "-H", "Accept-Language: zh-CN,zh;q=0.9"]
    if ref:
        cmd += ["-e", ref]
    cmd.append(url)
    r = subprocess.run(cmd, capture_output=True, text=True, errors="ignore")
    return r.stdout

def strip_html(h):
    h = re.sub(r"(?is)<(script|style).*?</\1>", " ", h)
    h = re.sub(r"(?s)<[^>]+>", " ", h)
    h = h.replace("&nbsp;", " ").replace("&amp;", "&")
    return re.sub(r"\s+", " ", h).strip()

def query_of(stem):
    """题干 → 检索词：去掉括号/问号/空格，保留主干"""
    q = re.sub(r"[（(]\s*[)）]", " ", stem)
    q = re.sub(r"[（）()？?。，,、；;]", " ", q)
    q = re.sub(r"\s+", " ", q).strip()
    return q[:40]

def has_answer(text, opts):
    """在正文里找『答案: X』或『正确答案 X』"""
    for m in re.finditer(r"(?:参考答案|正确答案|答案)\s*[:：]?\s*([A-E]{1,4})", text):
        a = m.group(1)
        if all(k in [o["key"] for o in opts] for k in set(a)):
            return a
    return None

# 抽 12 题：每套各 4 题
sample = []
for k, v in Q.items():
    random.seed(7)
    sample += random.sample(v, min(4, len(v)))

print("样本 %d 题\n" % len(sample))
stat = {"360": 0, "sogou": 0, "answer_found": 0, "blocked": 0}
for i, q in enumerate(sample, 1):
    qq = urllib.parse.quote(query_of(q["stem"]))
    got = ""
    src = "-"
    # 路径1: 360 搜索
    h = curl("https://www.so.com/s?q=%s" % qq)
    t = strip_html(h)
    if len(t) > 800 and "验证" not in t[:300]:
        got, src = t, "360"; stat["360"] += 1
    else:
        # 路径2: 搜狗
        h = curl("https://www.sogou.com/web?query=%s" % qq)
        t = strip_html(h)
        if len(t) > 800 and "验证码" not in t[:400]:
            got, src = t, "sogou"; stat["sogou"] += 1
        else:
            stat["blocked"] += 1
    a = has_answer(got, q["options"]) if got else None
    if a:
        stat["answer_found"] += 1
    print("%2d [%s] %s" % (i, q["id"], q["stem"][:46]))
    print("     路径=%s 正文=%d字 答案=%s" % (src, len(got), a or "未找到"))
    time.sleep(1.2)

print("\n=== 小结 ===")
print("360 成功 %d / 搜狗成功 %d / 被拦 %d" % (stat["360"], stat["sogou"], stat["blocked"]))
print("其中『正文里直接给出答案』的: %d / %d" % (stat["answer_found"], len(sample)))
