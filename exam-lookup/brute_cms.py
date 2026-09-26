#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""穷举 cms.mylearning.cn 的 /topic/resource/<年>/<栏目>/<页>.html 空间"""
import subprocess, re, itertools, sys

UA = ("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 "
      "(KHTML, like Gecko) Chrome/124.0 Safari/537.36")
BASE = "https://cms.mylearning.cn"

def get(path, timeout=8):
    r = subprocess.run(["curl", "-sS", "-o", "/tmp/cm2", "-w", "%{http_code}|%{size_download}",
                        "--max-time", str(timeout), "-H", "User-Agent: " + UA, BASE + path],
                       capture_output=True, text=True, errors="ignore")
    code, size = (r.stdout.split("|") + ["", ""])[:2]
    body = open("/tmp/cm2", encoding="utf-8", errors="ignore").read() if code == "200" else ""
    return code, int(size or 0), body

def text(h):
    h = re.sub(r"(?is)<(script|style).*?</\1>", " ", h)
    return re.sub(r"\s+", " ", re.sub(r"<[^>]+>", " ", h)).strip()

YEARS = ["2023", "2024", "2025", "2026"]
SECS = ["account", "exam", "course", "study", "learn", "tiku", "question",
        "paper", "help", "guide", "notice", "news", "safety", "cert",
        "zhuangwei", "zongwei", "kaoshi", "training", "doc", "file", "app"]
PAGES = ["pc", "mobile", "index", "list", "home", "detail", "tiku", "exam",
         "question", "paper", "doc", "file", "download", "guide", "help"]

hits = []
tried = 0
for y, s, p in itertools.product(YEARS, SECS, PAGES):
    path = "/topic/resource/%s/%s/%s.html" % (y, s, p)
    code, size, body = get(path, timeout=5)
    tried += 1
    if code == "200" and size > 300:
        hits.append((path, size, text(body)[:90]))
        print("  ✓ %-52s %6d 字节  %s" % (path, size, text(body)[:70]))
    if tried % 120 == 0:
        print("  ...已试 %d 条，命中 %d" % (tried, len(hits)), file=sys.stderr)

print("\n试了 %d 条路径，命中 %d" % (tried, len(hits)))
