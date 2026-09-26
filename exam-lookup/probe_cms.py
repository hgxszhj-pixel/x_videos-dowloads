#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""枚举 cms.mylearning.cn 的 /topic/ 内容树 —— 找题库/考试/课程类公开文档"""
import subprocess, re

UA = ("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 "
      "(KHTML, like Gecko) Chrome/124.0 Safari/537.36")

def get(url, timeout=10):
    r = subprocess.run(["curl", "-sS", "-o", "/tmp/cms_body", "-w",
                        "%{http_code}|%{size_download}", "--max-time", str(timeout),
                        "-H", "User-Agent: " + UA, url],
                       capture_output=True, text=True, errors="ignore")
    code, size = (r.stdout.split("|") + ["", ""])[:2]
    body = ""
    if code == "200":
        body = open("/tmp/cms_body", encoding="utf-8", errors="ignore").read()
    return code, size, body

def text(h):
    h = re.sub(r"(?is)<(script|style).*?</\1>", " ", h)
    t = re.sub(r"<[^>]+>", " ", h)
    return re.sub(r"\s+", " ", t).strip()

# 一级分类猜测（结合已知的 resource/account 结构）
TOPS = ["resource", "tiku", "exam", "question", "course", "study", "learn",
        "help", "guide", "manual", "notice", "news", "doc", "docs", "file",
        "download", "training", "safety", "security", "anquan", "kaoshi",
        "zhuangwei", "zongwei", "cert", "certification", "video", "live"]

YEARS = ["2026", "2025", "2024", "2023"]

print("=== 1. /topic/<分类>/ 一层 ===")
alive = []
for t in TOPS:
    code, size, body = get("https://cms.mylearning.cn/topic/%s/" % t)
    if code == "200":
        alive.append(t)
        print("  ✓ 200  %-14s %s 字节  %s" % (t, size, text(body)[:70]))
    elif code in ("301", "302"):
        pass

print("\n=== 2. 命中的分类 × 年份 × 常见栏目 ===")
SEC = ["index", "pc", "home", "list", "account", "tiku", "exam", "paper",
       "question", "doc", "file", "download", "guide", "help", "notice"]
for t in alive:
    for y in YEARS:
        for s in SEC:
            u = "https://cms.mylearning.cn/topic/%s/%s/%s.html" % (t, y, s)
            code, size, body = get(u, timeout=6)
            if code == "200" and int(size or 0) > 300:
                print("  ✓ %s  (%s 字节)  %s" % (u.replace("https://cms.mylearning.cn", ""),
                                                size, text(body)[:60]))

print("\n=== 3. 已知公开页的内容（找导航线索）===")
for u in ("https://cms.mylearning.cn/privacy.html",
          "https://cms.mylearning.cn/topic/resource/2025/account/pc.html"):
    code, size, body = get(u)
    if code != "200":
        continue
    print("\n--- %s (%s 字节) ---" % (u.replace("https://cms.mylearning.cn", ""), size))
    links = sorted({m for m in re.findall(r'(?:href|src)="([^"]{4,140})"', body)
                    if not m.startswith(("data:", "#", "javascript:"))})
    print("  链接 %d 条:" % len(links))
    for l in links[:25]:
        print("     ", l[:120])
    print("  正文:", text(body)[:300])
