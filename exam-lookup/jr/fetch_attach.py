#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""通用附件下载器：扫一个 structure.json 里所有 attachmentId，探 preview 元数据并下载原件。

为什么能裸下：preview 返回的 url 是 /api/v1/secure/<hash>，自带鉴权，curl 可直接取，
且实测下载字节数与接口报的 size 完全一致 —— 用字节数当校验，不做二次猜测。
"""
import json, os, subprocess, sys
sys.path.insert(0, "/Users/hgxszhj/x_videos-dowloads/exam-lookup/jr")
from jr import send

BASE = "https://kc.zhixueyun.com/api/v1"
STRUCT = sys.argv[1]
OUTD = sys.argv[2]
os.makedirs(OUTD, exist_ok=True)

d = json.load(open(STRUCT, encoding="utf-8"))
targets = []
for s in (d.get("sections") or []):
    for x in (s.get("inner") or []):
        if x.get("attachmentId") and x.get("attachmentId") != "—":
            targets.append({"attachmentId": x["attachmentId"], "name": (x.get("name") or "").strip(),
                            "sectionType": x.get("sectionType"), "chapter": s.get("chapter") or s.get("name")})
print("待探测 %d 个附件（%s）" % (len(targets), d.get("name")))

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
    } catch (e) { out.push(Object.assign({}, it, { err: String(e).slice(0,90) })); }
  }
  return JSON.stringify(out);
})()
""" % (json.dumps(BASE), json.dumps(targets, ensure_ascii=False))

r = send({"cmd": "eval", "js": JS}, wait=40)
v = (r.get("result") or {}).get("value")
if not v:
    print("元数据失败:", json.dumps(r, ensure_ascii=False)[:300]); sys.exit(1)
metas = json.loads(v)

manifest = []
for m in metas:
    mt = m.get("meta") or {}
    if m.get("status") != 200 or not mt.get("url"):
        print("  – %-46s  %s" % ((m["name"] or "")[:44], "无直链(视频类，走 m3u8)" if not mt else m.get("status")))
        continue
    fn = mt.get("filename") or (m["name"] + "." + (mt.get("extention") or "bin"))
    safe = "".join(c for c in fn if c not in '/\\:*?"<>|').strip()
    dst = os.path.join(OUTD, safe)
    url = "https://kc.zhixueyun.com" + mt["url"]
    # 直链是 CDN 相对路径（带 auth_key 自鉴权）。实测只有 CDN 主机能取，
    # /file-cloud 与主站的同路径都是 404 —— 所以按 hosts 顺序试，取第一个 200。
    hosts = ["https://dianxinsafecdn.zhixueyun.com",
             "https://dianxincdn.zhixueyun.com",
             "https://kc.zhixueyun.com/file-cloud",
             "https://kc.zhixueyun.com"]
    dst_size = mt.get("size")
    got = None
    for h in hosts:
        p = subprocess.run(["curl", "-sS", "-L", "--max-time", "600", "-o", dst, "-w",
                            "%{http_code} %{size_download}", h + mt["url"]],
                           capture_output=True, text=True)
        parts = p.stdout.strip().split()
        if parts and parts[0] == "200":
            got = parts[1]; break
    ok = got is not None and (got == str(dst_size))
    print("  %s %-50s %9s 字节  %s" % ("✓" if ok else "✗", safe[:48], dst_size,
          "字节校验一致" if ok else "！实得 %s (HTTP 非 200 或大小不符)" % (got or "失败")))
    if os.path.exists(dst):
        manifest.append({"file": safe, "size": os.path.getsize(dst), "type": mt.get("type"),
                         "chapter": m.get("chapter"), "section": m.get("name")})

json.dump(manifest, open(os.path.join(OUTD, "清单.json"), "w", encoding="utf-8"),
          ensure_ascii=False, indent=1)
print("\n下载 %d 个 -> %s" % (len(manifest), OUTD))
