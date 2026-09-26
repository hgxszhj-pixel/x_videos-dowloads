#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""摸 ctps.mylearning.cn 的公开面：哪些路径不需要登录、有哪些 API 路由、有没有文档/题库区"""
import subprocess, re, json, urllib.parse

UA = ("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 "
      "(KHTML, like Gecko) Chrome/124.0 Safari/537.36")

def probe(url, follow=False, timeout=12):
    cmd = ["curl", "-sS", "-o", "/tmp/probe_body", "-w", "%{http_code}|%{size_download}|%{redirect_url}",
           "--max-time", str(timeout), "-H", "User-Agent: " + UA,
           "-H", "Accept-Language: zh-CN,zh;q=0.9"]
    if follow:
        cmd.append("-L")
    cmd.append(url)
    r = subprocess.run(cmd, capture_output=True, text=True, errors="ignore")
    code, size, redir = (r.stdout.split("|") + ["", "", ""])[:3]
    return code, size, redir

PATHS = [
    "/", "/p1/", "/p1/index.html", "/p1/static/", "/p1/js/", "/p1/js/app.js",
    "/p1/manifest.json", "/p1/favicon.ico",
    "/api/", "/api/v1/", "/static/", "/public/", "/doc/", "/docs/",
    "/download/", "/file/", "/files/", "/upload/",
    "/tiku/", "/question/", "/exam/", "/paper/", "/course/", "/resource/",
    "/p1/api/", "/p1/index", "/p1/home", "/p1/main.js", "/p1/app.js",
    "/robots.txt", "/sitemap.xml",
]

print("=== ctps.mylearning.cn 公开路径探测 ===")
public, guarded = [], []
for p in PATHS:
    code, size, redir = probe("https://ctps.mylearning.cn" + p)
    tag = "公开" if code.startswith("2") and int(size or 0) > 200 and "sso" not in redir else \
          "跳登录" if "sso" in redir or code.startswith("3") else "其他"
    if tag == "公开":
        public.append((p, code, size))
    elif tag == "跳登录":
        guarded.append((p, code))
    print("  %-22s %-4s %8s  %s" % (p, code, size, tag))

print("\n=== 公开资源 ===")
for p, c, s in public:
    print("  %s  HTTP %s  %s 字节" % (p, c, s))
print("\n=== 全部跳登录（%d 条）——整站需要账号，符合预期" % len(guarded))
