#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""扫 exam 模块 bundle，列出全部 /api/v1/exam/ 接口 —— 找题库/答题接口。"""
import json, sys
sys.path.insert(0, "/Users/hgxszhj/x_videos-dowloads/exam-lookup/jr")
from jr import send

JS = r"""
(async () => {
  const srcs = [...document.querySelectorAll('script[src]')].map(s => s.src);
  const found = new Set();
  for (const u of srcs) {
    let t = '';
    try { t = await (await fetch(u)).text(); } catch (e) { continue; }
    const m = t.match(/["'`](\.\.\/|\/api\/v\d\/)[A-Za-z0-9_\-\/]*(exam|question|paper|answer)[A-Za-z0-9_\-\/]*["'`]/gi) || [];
    for (const p of m) found.add(p.slice(1, -1));
  }
  return [...found].sort().join('\n');
})()
"""

r = send({"cmd": "eval", "js": JS}, wait=30)
v = (r.get("result") or {}).get("value") or ""
lines = [x for x in v.split("\n") if x.strip()]
print("命中 %d 条 exam/question/paper/answer 接口：" % len(lines))
for x in lines:
    print("  " + x)
