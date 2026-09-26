#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""在 bundle 里给指定关键字抓上下文 —— 看接口怎么调的（参数从哪来）。"""
import json, sys
sys.path.insert(0, "/Users/hgxszhj/x_videos-dowloads/exam-lookup/jr")
from jr import send

KEY = sys.argv[1] if len(sys.argv) > 1 else "index.m3u8"

JS = r"""
(async () => {
  const KEY = %s;
  const srcs = [...document.querySelectorAll('script[src]')].map(s => s.src)
      .filter(u => /bundle|chunk|app\.|main-|knowledge|study/i.test(u));
  const out = [];
  for (const u of srcs) {
    let t = '';
    try { t = await (await fetch(u)).text(); } catch (e) { continue; }
    let i = -1, n = 0;
    while ((i = t.indexOf(KEY, i + 1)) >= 0 && n < 4) {
      n++;
      out.push('【' + u.split('/').pop() + '】\n' + t.slice(Math.max(0, i - 320), i + 200));
    }
  }
  return out.join('\n\n========\n\n');
})()
""" % json.dumps(KEY)

r = send({"cmd": "eval", "js": JS}, wait=25)
v = (r.get("result") or {}).get("value")
print(v or ("没抓到：" + json.dumps(r, ensure_ascii=False)[:400]))
