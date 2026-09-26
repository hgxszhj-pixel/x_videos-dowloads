#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""按 crawl/attach-manifest.json 下载平台附件（文档类），逐字节校验 size。

直链是 /api/v1/secure/<hash>，自带鉴权，无需 cookie。
文件名形如 <hash>_t.txt（平台 AI 提取的教材文字）时，改用 sectionName 命名。
"""
import json, os, sys, urllib.request

ROOT = "/Users/hgxszhj/x_videos-dowloads/exam-lookup/crawl"
OUTD = os.path.join(ROOT, "attach")
os.makedirs(OUTD, exist_ok=True)

# 清单路径可选：默认老的 attach-manifest.json，也可以传新解析出来的（如 attach-manifest-new.json）
MAN = sys.argv[1] if len(sys.argv) > 1 else os.path.join(ROOT, "attach-manifest.json")
man = json.load(open(MAN, encoding="utf-8"))
# 只认「文件类型」白名单，别信 sectionType：实测一批录像的 sectionType 不是 2，
# 标记 isVideo 失效 → 会去拉 7.5GB 的 mp4。按扩展名切最稳（平台 preview 报的 extention）。
DOC_EXT = {"pdf", "doc", "docx", "ppt", "pptx", "xls", "xlsx", "txt", "html", "htm"}
todo = [a for a in man if a.get("url") and (a.get("extention") or "").lower() in DOC_EXT]
print("待下载文档 %d 个；跳过 %d 个（非白名单类型，多为视频/音频）" % (len(todo), len(man) - len(todo)))
print("合计待下 %.1f MB" % (sum(a.get("size") or 0 for a in todo) / 1048576))

ok = skip = fail = 0
report = []
for a in todo:
    sub = os.path.join(OUTD, a["subject"])
    os.makedirs(sub, exist_ok=True)
    nm = a["name"]
    if nm.endswith("_t.txt") or ".txt" in nm or nm.endswith("_t.html"):
        base = (a.get("sectionName") or a["subject"]).strip() or a["subject"]
        clean = "".join(c for c in base if c not in '/\\:*?"<>|').strip()
        nm = clean + ("_t.txt" if nm.endswith("_t.txt") else os.path.splitext(nm)[1] or ".txt")
    # 清单里的 name 多是小节名（没有扩展名），落盘时补上平台报的真实扩展名
    if not os.path.splitext(nm)[1] and a.get("extention"):
        nm = nm + "." + str(a["extention"])
    dst = os.path.join(sub, nm)
    want = a.get("size")
    if os.path.exists(dst) and (not want or os.path.getsize(dst) == want):
        skip += 1
        report.append((a["subject"], nm, "skip", os.path.getsize(dst)))
        continue
    try:
        req = urllib.request.Request(a["url"], headers={"User-Agent": "Mozilla/5.0"})
        with urllib.request.urlopen(req, timeout=120) as r, open(dst, "wb") as f:
            while True:
                chunk = r.read(1 << 16)
                if not chunk:
                    break
                f.write(chunk)
        got = os.path.getsize(dst)
        if want and got != want:
            fail += 1
            report.append((a["subject"], nm, "SIZE_MISMATCH %s!=%s" % (got, want), got))
        else:
            ok += 1
            report.append((a["subject"], nm, "ok", got))
    except Exception as e:
        fail += 1
        report.append((a["subject"], nm, "ERR " + str(e)[:60], 0))

for s, n, st, sz in report:
    print("  %-22s %-52s %-10s %s" % (s, n[:50], st, sz))
print("\n完成：新下 %d，已存在 %d，失败 %d；目录 %s" % (ok, skip, fail, OUTD))
sys.exit(0 if fail == 0 else 1)
