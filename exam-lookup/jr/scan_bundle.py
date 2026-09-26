#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""在前端 JS bundle 里搜接口路径 —— 找「资源/附件」这类没被点出来的接口。

为什么不靠点击：SPA 的章节项是 div + JS 事件委托，合成 click 触不发；
直接读 bundle 里的路径字面量更快更全。
"""
import json, sys, re
sys.path.insert(0, "/Users/hgxszhj/x_videos-dowloads/exam-lookup/jr")
from jr import send

JS = r"""
(async () => {
  const srcs = [...document.querySelectorAll('script[src]')].map(s => s.src)
      .filter(u => /bundle|chunk|app\.|main-|knowledge|study/i.test(u));
  const found = new Set();
  for (const u of srcs) {
    let t = '';
    try { t = await (await fetch(u)).text(); } catch (e) { continue; }
    // 抓所有形如 /api/v1/xxx 的路径字面量（允许 {{}} 占位）
    const paths = t.match(/["'`](\/api\/v\d\/[A-Za-z0-9_\-\/{}.\$:]+)["'`]/g) || [];
    for (const p of paths) {
      const s = p.slice(1, -1);
      if (/resource|attachment|section|file|document|material|download|course-info|section-info|chapter/i.test(s))
        found.add(s);
    }
  }
  return [...found].sort().join('\n');
})()
"""

r = send({"cmd": "eval", "js": JS}, wait=25)
v = (r.get("result") or {}).get("value")
if not v:
    print("没拿到结果:", json.dumps(r, ensure_ascii=False)[:500]); sys.exit(1)
lines = [x for x in v.split("\n") if x.strip()]
print("命中 %d 条接口路径：" % len(lines))
for x in lines:
    print("  " + x)
