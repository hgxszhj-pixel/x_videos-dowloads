#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""抓取单个结果页并抽出与"答案/选项"相关的文本"""
import subprocess, re, html, sys, random

UA = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36"

def fetch(url, tries=3):
    for i in range(tries):
        try:
            r = subprocess.run(["curl","-sSL","-m","30","-A",UA,
                                "-H","Accept-Language: zh-CN,zh;q=0.9",
                                "-b","/tmp/exam_lookup_cookies.txt",
                                "-c","/tmp/exam_lookup_cookies.txt", url],
                               capture_output=True, text=True, timeout=45)
            if r.stdout and len(r.stdout) > 500:
                return r.stdout
        except Exception:
            pass
    return ""

def text_of(h):
    h = re.sub(r'<script.*?</script>', ' ', h, flags=re.S)
    h = re.sub(r'<style.*?</style>', ' ', h, flags=re.S)
    t = re.sub(r'<[^>]+>', ' ', h)
    t = html.unescape(t)
    return re.sub(r'\s+', ' ', t).strip()

def snippet(t, keys, win=260):
    out = []
    for k in keys:
        for m in re.finditer(re.escape(k), t):
            s = max(0, m.start()-win//2); e = min(len(t), m.end()+win)
            out.append(t[s:e])
    return out[:4]

if __name__ == "__main__":
    url = sys.argv[1]
    keys = sys.argv[2].split(",") if len(sys.argv) > 2 else ["答案","正确答案","参考答案"]
    h = fetch(url)
    if not h:
        print("FETCH_FAILED"); sys.exit(1)
    t = text_of(h)
    print("URL:", url)
    print("LEN:", len(t))
    print("---- 命中片段 ----")
    for s in snippet(t, keys):
        print(">>>", s)
    print("---- 正文前 1200 字 ----")
    print(t[:1200])
