#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""把专题里所有文档型小节（sectionType=1）的原始 PDF/PPT 拉下来。

链路：attachmentId --tools-center-v2/file-cloud/preview?id=--> {filename, url, size}
      url = /api/v1/secure/<hash>  ← 自鉴权，裸 curl 可下（实测 size 与下载字节数一致）

对每一节的每个 attachmentId 都探一次 preview，所以顺便能发现文档型之外的可下载件。
"""
import json, os, subprocess, sys, time
sys.path.insert(0, "/Users/hgxszhj/x_videos-dowloads/exam-lookup/jr")
from jr import send

BASE = "https://kc.zhixueyun.com/api/v1"
OUTD = "out/pdf"
os.makedirs(OUTD, exist_ok=True)

corpus = json.load(open("out/corpus/corpus.json", encoding="utf-8"))

# 收集所有小节的 attachmentId
targets = []
for x in corpus:
    for s in (x.get("sections") or []):
        if s.get("attachmentId"):
            targets.append({"attachmentId": s["attachmentId"], "name": s["name"],
                            "sectionType": s.get("sectionType"), "chapter": x["name"]})
print("待探测小节 %d 个" % len(targets))

# 一次性在浏览器里并发取 preview 元数据
JS = r"""
(async () => {
  const BASE = %s, list = %s;
  const tk = (() => { try { return JSON.parse(localStorage.getItem('token')||'{}').access_token||''; } catch(e){ return ''; } })();
  const H = { 'version': '12.1.1' }; if (tk) H.authorization = 'Bearer__' + tk;
  const out = [];
  for (const it of list) {
    try {
      const r = await fetch(BASE + '/tools-center-v2/file-cloud/preview?id=' + it.attachmentId,
                            { credentials: 'include', headers: H });
      const t = await r.text();
      let j = null; try { j = JSON.parse(t); } catch (e) {}
      out.push(Object.assign({}, it, { status: r.status, meta: j }));
    } catch (e) { out.push(Object.assign({}, it, { err: String(e).slice(0, 100) })); }
  }
  return JSON.stringify(out);
})()
""" % (json.dumps(BASE), json.dumps(targets, ensure_ascii=False))

r = send({"cmd": "eval", "js": JS}, wait=40)
v = (r.get("result") or {}).get("value")
if not v:
    print("元数据获取失败:", json.dumps(r, ensure_ascii=False)[:300]); sys.exit(1)
metas = json.loads(v)

# 落盘元数据 + 下载
manifest = []
for m in metas:
    mt = m.get("meta") or {}
    if m.get("status") != 200 or not mt.get("url"):
        continue
    fn = mt.get("filename") or (m["name"] + "." + (mt.get("extention") or "bin"))
    safe = "".join(c for c in fn if c not in '/\\:*?"<>|').strip()
    dst = os.path.join(OUTD, safe)
    url = "https://kc.zhixueyun.com" + mt["url"]
    if os.path.exists(dst) and os.path.getsize(dst) == mt.get("size"):
        print("  已存在 %s" % safe); 
    else:
        p = subprocess.run(["curl", "-sS", "-L", "--max-time", "180", "-o", dst, "-w",
                            "%{http_code} %{size_download}", url],
                           capture_output=True, text=True)
        got = p.stdout.strip().split()
        ok = got and got[0] == "200" and got[1] == str(mt.get("size"))
        print("  %s %-52s %8s 字节  %s" % ("✓" if ok else "✗", safe[:50],
              mt.get("size"), "校验一致" if ok else "！大小不符 " + p.stdout.strip()))
    manifest.append({"file": safe, "filename": fn, "size": mt.get("size"),
                     "type": mt.get("type"), "chapter": m.get("chapter"),
                     "section": m.get("name"), "attachmentId": m.get("attachmentId")})

json.dump(manifest, open("out/pdf/清单.json", "w", encoding="utf-8"), ensure_ascii=False, indent=1)
print("\n下载 %d 个文件，清单：out/pdf/清单.json" % len(manifest))
