#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""深挖：找题库站结果 → 抓完整详情页 → 抽答案。用 4 道机器学习题试。"""
import re, subprocess, urllib.parse, json, time

UA = ("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 "
      "(KHTML, like Gecko) Chrome/124.0 Safari/537.36")

def curl(url, ref=None, timeout=15):
    cmd = ["curl", "-sS", "-L", "--max-time", str(timeout), "-H", "User-Agent: " + UA,
           "-H", "Accept-Language: zh-CN,zh;q=0.9"]
    if ref:
        cmd += ["-e", ref]
    cmd.append(url)
    return subprocess.run(cmd, capture_output=True, text=True, errors="ignore").stdout

def txt(h):
    h = re.sub(r"(?is)<(script|style).*?</\1>", " ", h)
    h = re.sub(r"(?s)<[^>]+>", " ", h)
    return re.sub(r"\s+", " ", h.replace("&nbsp;", " ")).strip()

TESTS = [
  "下面哪些方法无法缓解深度网络的梯度消失问题",
  "下列关于主成分分析法（PCA）说法正确的是",
  "有关机器学习分类算法的Precision和Recall 以下定义中正确的是",
]

for stem in TESTS:
    print("=" * 70)
    print("题:", stem)
    q = urllib.parse.quote(stem[:38])
    h = curl("https://www.so.com/s?q=%s" % q)
    # 抽结果链接
    links = []
    for m in re.finditer(r'href="(https?://[^"]+)"[^>]*>(.*?)</a>', h):
        u, t = m.group(1), txt(m.group(2))
        if any(k in u for k in ("so.com", "360.cn", "baidu.com/link", "sogou")) or len(t) < 8:
            continue
        if u not in [x[0] for x in links]:
            links.append((u, t))
    print("  找到外部链接 %d 个" % len(links))
    found = 0
    for u, t in links[:5]:
        body = txt(curl(u, ref="https://www.so.com/"))
        m = re.search(r"(?:参考答案|正确答案|答案)\s*[:：]?\s*([A-E]{1,4})\b", body)
        tag = m.group(1) if m else "无答案"
        if m:
            found += 1
        print("    [%s] %s | %d字 | %s" % (tag, t[:38] or u[:38], len(body), u[:58]))
        time.sleep(0.8)
    print("  -> 该题有答案的页面: %d" % found)
