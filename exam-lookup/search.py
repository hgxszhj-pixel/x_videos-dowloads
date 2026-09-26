#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
多引擎检索脚本（稳健版）
- 直连 curl，带 cookie jar、随机 UA、延时与重试，尽量规避限流
- 自动从多个中文搜索引擎取结果（搜狗 / Bing CN / 360 / 神马 / 头条）
- 用法: python3 search.py "查询词" [引擎,...]
"""
import subprocess, re, html, urllib.parse, sys, time, random, os, json

UA_POOL = [
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36",
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0 Safari/537.36",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15",
]

ENGINES = {
    "sogou":  "https://www.sogou.com/web?query={q}",
    "bing":   "https://cn.bing.com/search?q={q}&setlang=zh-CN",
    "360":    "https://www.so.com/s?q={q}",
    "sm":     "https://m.sm.cn/s?q={q}",
    "toutiao":"https://so.toutiao.com/search?keyword={q}",
}

COOKIE = "/tmp/exam_lookup_cookies.txt"

def fetch(url, tries=3):
    last = ""
    for i in range(tries):
        ua = random.choice(UA_POOL)
        cmd = ["curl", "-sSL", "-m", "25", "-A", ua,
               "-c", COOKIE, "-b", COOKIE,
               "-H", "Accept-Language: zh-CN,zh;q=0.9",
               url]
        try:
            r = subprocess.run(cmd, capture_output=True, text=True, timeout=40)
            last = r.stdout
            if last and len(last) > 3000:
                return last
        except Exception:
            pass
        time.sleep(1.5 * (i + 1))
    return last

def blocked(h):
    return ("验证" in h and len(h) < 8000) or "安全验证" in h

def parse(h):
    out = []
    # 通用：h3 > a
    for m in re.finditer(r'<h3[^>]*>\s*<a[^>]*href="([^"]+)"[^>]*>(.*?)</a>', h, re.S):
        t = html.unescape(re.sub(r'<[^>]+>', '', m.group(2))).strip()
        u = m.group(1)
        if t:
            out.append({"title": t, "url": u})
    # bing 备用
    if not out:
        for m in re.finditer(r'<a[^>]*href="(https?://[^"]+)"[^>]*>(.*?)</a>', h, re.S):
            t = html.unescape(re.sub(r'<[^>]+>', '', m.group(2))).strip()
            if 8 < len(t) < 90:
                out.append({"title": t, "url": m.group(1)})
    # 去重
    seen, res = set(), []
    for r in out:
        if r["title"] not in seen:
            seen.add(r["title"]); res.append(r)
    return res

def search(q, engines=None):
    engines = engines or list(ENGINES)
    allres = {}
    for name in engines:
        url = ENGINES[name].format(q=urllib.parse.quote(q))
        h = fetch(url)
        if not h:
            allres[name] = []
            continue
        if blocked(h):
            allres[name] = ["__BLOCKED__"]
        else:
            allres[name] = parse(h)[:8]
        time.sleep(random.uniform(2.5, 4.5))   # 关键：限流规避
    return allres

if __name__ == "__main__":
    q = sys.argv[1]
    eng = sys.argv[2].split(",") if len(sys.argv) > 2 else None
    res = search(q, eng)
    print(json.dumps(res, ensure_ascii=False, indent=2))
