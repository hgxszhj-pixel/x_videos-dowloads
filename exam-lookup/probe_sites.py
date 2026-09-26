#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""测试：可达题库站的页面里是否直接暴露答案"""
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

Q = "有限空间作业中断30分钟后重新进入需再次检测气体"

# 各站站内搜索入口（猜测常见形态）
PROBES = [
    ("题百科", "https://m.tibaike.com/search?keyword={q}"),
    ("上学吧", "https://www.shangxueba.com/search.aspx?q={q}"),
    ("上学吧2", "https://www.shangxueba.com/so/{q}.html"),
    ("考题宝", "https://www.educity.cn/search?q={q}"),
    ("皮皮学", "https://www.pipixue.com/search?q={q}"),
    ("刷刷题", "https://www.shuashuati.com/search?q={q}"),
]
for name, tpl in PROBES:
    u = tpl.format(q=urllib.parse.quote(Q))
    h = get(u)
    n_ans = len(re.findall(r"答案", h))
    print(f"{name:8s} {u[:64]}")
    print(f"         HTTP内容 {len(h)}B, '答案'出现 {n_ans} 次")
    if n_ans and len(h) < 400000:
        t = text(h)
        for m in list(re.finditer(r".{60}答案.{60}", t))[:2]:
            print(f"         …{m.group(0)}…")
    print()
    time.sleep(1)
