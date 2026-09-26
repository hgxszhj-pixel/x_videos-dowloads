#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""搜 topic / subject 列表类接口，用来一次列出「技能认证教材」下所有学习班。"""
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
    const m = t.match(/["'`](\.\.\/|\/api\/v\d\/)[A-Za-z0-9_\-\/]*(topic|subject|business-home|home-config|search)[A-Za-z0-9_\-\/]*["'`]/gi) || [];
    for (const p of m) found.add(p.slice(1, -1));
  }
  return [...found].sort().join('\n');
})()
"""
r = send({"cmd": "eval", "js": JS}, wait=30)
v = (r.get("result") or {}).get("value") or ""
for x in [y for y in v.split("\n") if y.strip()]:
    print("  " + x)
