#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""GitHub / Gitee 项目检索"""
import json, subprocess, urllib.parse, sys, time

def curl(url, hdrs=None):
    cmd = ["curl","-sSL","-m","30","-H","Accept: application/vnd.github+json",
           "-H","User-Agent: research"]
    for h in (hdrs or []): cmd += ["-H", h]
    cmd.append(url)
    try:
        r = subprocess.run(cmd, capture_output=True, text=True, timeout=40)
        return r.stdout
    except Exception as e:
        return ""

def gh(q, n=12):
    u = "https://api.github.com/search/repositories?q=%s&sort=stars&order=desc&per_page=%d" % (urllib.parse.quote(q), n)
    try:
        d = json.loads(curl(u))
    except Exception:
        return []
    out = []
    for it in d.get("items", [])[:n]:
        out.append((it["full_name"], it["stargazers_count"], (it.get("description") or "")[:110],
                    it["html_url"], it.get("pushed_at","")[:10], (it.get("language") or "")))
    return out

def gitee(q, n=10):
    u = "https://gitee.com/api/v5/search/repositories?q=%s&sort=stars_count&order=desc&per_page=%d" % (urllib.parse.quote(q), n)
    try:
        d = json.loads(curl(u))
    except Exception:
        return []
    out = []
    if isinstance(d, list):
        for it in d[:n]:
            out.append((it.get("full_name") or it.get("path",""), it.get("stargazers_count",0),
                        (it.get("description") or "")[:110], it.get("html_url",""), "", ""))
    return out

QUERIES = [
    ("GitHub", "题库 刷题", gh, "题库 刷题"),
    ("GitHub", "搜题", gh, "搜题"),
    ("GitHub", "自动答题", gh, "自动答题"),
    ("GitHub", "考试 答题 助手", gh, "考试 答题 助手"),
    ("GitHub", "quiz question bank llm", gh, "quiz question bank llm"),
    ("GitHub", "exam answer rag", gh, "exam answer rag"),
    ("GitHub", "超星 学习通 答题", gh, "超星 学习通 答题"),
    ("GitHub", "anki spaced repetition", gh, "anki spaced repetition"),
    ("Gitee", "题库", gitee, "题库"),
    ("Gitee", "刷题", gitee, "刷题"),
    ("Gitee", "搜题 答题", gitee, "搜题 答题"),
    ("Gitee", "考试系统", gitee, "考试系统"),
]

for src, label, fn, q in QUERIES:
    print("=" * 78)
    print(f"[{src}] 检索：{label}")
    print("=" * 78)
    res = fn(q)
    if not res:
        print("  （无结果 / 接口未通）\n")
        continue
    for name, stars, desc, url, pushed, lang in res:
        print(f"  ★{stars:<6} {name}  [{lang}] {pushed}")
        print(f"           {desc}")
        print(f"           {url}")
    print()
    time.sleep(1.2)
