#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""验证路线：搜狗取直达URL -> 抓正文 -> 抽答案"""
import subprocess, re, html, urllib.parse, time
UA = ("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 "
      "(KHTML, like Gecko) Chrome/120.0 Safari/537.36")

def get(u, t=20):
    try:
        r = subprocess.run(["curl", "-sSL", "-m", str(t), "-A", UA,
                            "-H", "Accept-Language: zh-CN,zh;q=0.9", u],
                           capture_output=True, text=True, timeout=t + 10)
        return r.stdout or ""
    except Exception:
        return ""

def text(h):
    h = re.sub(r"<(script|style)[^>]*>.*?</\1>", " ", h, flags=re.S | re.I)
    return re.sub(r"\s+", " ", html.unescape(re.sub(r"<[^>]+>", " ", h)))

def direct_urls(q):
    """搜狗结果里的真实 URL（非跳转链接）"""
    h = get("https://www.sogou.com/web?query=" + urllib.parse.quote(q))
    urls = re.findall(r'href="(https?://[^"]+)"', h)
    out = []
    for u in urls:
        u = html.unescape(u)
        if any(d in u for d in ("sogou.com", "sogoucdn", "soso.com")):
            continue
        if re.search(r"\.(css|js|png|jpg|gif|ico)$", u):
            continue
        out.append(u)
    return list(dict.fromkeys(out))

Q = "有限空间作业中断30分钟后重新进入需再次检测气体 答案"
urls = direct_urls(Q)
print(f"搜狗直达 URL {len(urls)} 条：")
for u in urls[:12]:
    print("  ", u[:110])
print()
hits = 0
for u in urls[:8]:
    h = get(u)
    n = len(re.findall(r"答案", h))
    t = text(h)
    print(f"--- {u[:80]}")
    print(f"    正文 {len(h)}B, '答案' {n} 次")
    for m in list(re.finditer(r".{55}答案[是为：:]\s*.{25}", t))[:2]:
        print(f"      ★ …{m.group(0)}…")
        hits += 1
    time.sleep(0.8)
print(f"\n合计抽到含'答案：X'的片段: {hits}")
