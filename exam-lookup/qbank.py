#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
360/搜狗 检索 + 摘要提取
题库站的结果摘要里通常直接含"答案：X"，比点进网页更快。
用法: python3 qbank.py "查询词" [num]
"""
import subprocess, re, html, sys, urllib.parse, random, time

UA = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36"

def curl(url):
    for i in range(3):
        try:
            r = subprocess.run(["curl","-sSL","-m","30","-A",UA,
                                "-H","Accept-Language: zh-CN,zh;q=0.9",
                                "-b","/tmp/exam_lookup_cookies.txt",
                                "-c","/tmp/exam_lookup_cookies.txt", url],
                               capture_output=True, text=True, timeout=45)
            if r.stdout and len(r.stdout) > 3000:
                return r.stdout
        except Exception:
            pass
        time.sleep(2)
    return ""

def clean(s):
    return re.sub(r'\s+',' ', html.unescape(re.sub(r'<[^>]+>','', s))).strip()

def parse_360(h):
    out = []
    # 360 结果块： <li class="res-list"> ... <h3><a href=..>title</a></h3> ... <p class="res-desc">snippet</p>
    for blk in re.split(r'<li class="res-list"', h)[1:]:
        m = re.search(r'<h3[^>]*>\s*<a[^>]*href="([^"]+)"[^>]*>(.*?)</a>', blk, re.S)
        if not m: continue
        title = clean(m.group(2)); url = m.group(1)
        d = re.search(r'(?:class="res-desc"[^>]*>|class="res-comm-con"[^>]*>)(.*?)</p>', blk, re.S)
        snip = clean(d.group(1)) if d else ""
        out.append({"title": title, "snippet": snip, "url": url})
    if not out:
        for m in re.finditer(r'<h3[^>]*>\s*<a[^>]*href="([^"]+)"[^>]*>(.*?)</a>(.*?)(?=<h3|$)', h, re.S):
            out.append({"title": clean(m.group(2)), "snippet": clean(m.group(3))[:200], "url": m.group(1)})
    return out

def main(q, n=8):
    for eng, url in [("360","https://www.so.com/s?q={q}"),
                     ("搜狗","https://www.sogou.com/web?query={q}")]:
        h = curl(url.format(q=urllib.parse.quote(q)))
        if not h or ("验证" in h and len(h) < 9000):
            print(f"[{eng}] 被限流/无结果"); continue
        res = parse_360(h)
        print(f"===== [{eng}] {q} =====")
        for r in res[:n]:
            print("·", r["title"][:80])
            if r["snippet"]:
                print("   ", r["snippet"][:220])
        print()
        time.sleep(random.uniform(2.5,4))

if __name__ == "__main__":
    q = sys.argv[1]; n = int(sys.argv[2]) if len(sys.argv)>2 else 8
    main(q, n)
