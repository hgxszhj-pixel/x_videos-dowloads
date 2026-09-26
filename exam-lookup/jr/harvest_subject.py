#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""通用专题采集器：给一个专题 id，抓章节树 + 每节教材文字 + 每节附件元数据。

用于「综维作业安全学习-专题」「装维作业安全学习-专题」这两个直接对应考试的专题。
"""
import json, os, sys
sys.path.insert(0, "/Users/hgxszhj/x_videos-dowloads/exam-lookup/jr")
from jr import send

BASE = "https://kc.zhixueyun.com/api/v1"
SUBJECT = sys.argv[1]
TAG = sys.argv[2] if len(sys.argv) > 2 else "s"
OUT = "out/sub-%s" % TAG
os.makedirs(OUT, exist_ok=True)

JS = r"""
(async () => {
  const BASE = %s, SUBJECT = %s;
  const tk = (() => { try { return JSON.parse(localStorage.getItem('token')||'{}').access_token||''; } catch(e){ return ''; } })();
  const H = { 'version': '12.1.1' }; if (tk) H.authorization = 'Bearer__' + tk;
  async function j(u) {
    try { const r = await fetch(u, {credentials:'include', headers:H});
          const t = await r.text();
          try { return JSON.parse(t); } catch(e) { return {__raw:t.slice(0,300)}; } }
    catch (e) { return {__err:String(e).slice(0,120)}; }
  }
  const out = { subjectId: SUBJECT, chapters: [], sections: [] };

  // 1) 专题基本信息
  const si = await j(BASE + '/course-study/course-front/subject-info/' + SUBJECT);
  if (si && si.name) out.name = si.name;
  if (si && si.description) out.description = si.description;

  // 2) 章节树
  const cp = await j(BASE + '/course-study/subject/chapter-progress?courseId=' + SUBJECT
                     + '&knowledgePaymentEnable=false');
  const chs = Array.isArray(cp) ? cp : [];
  if (!chs.length) { out.err = 'chapter-progress:' + JSON.stringify(cp).slice(0,200); return JSON.stringify(out); }

  for (const ch of chs) {
    out.chapters.push({ name: ch.name || ch.title, count: (ch.courseChapterSections||[]).length });
    for (const s of (ch.courseChapterSections || [])) {
      const rec = { chapter: ch.name || ch.title, name: s.name, sectionType: s.sectionType,
                    attachmentId: s.attachmentId, resourceId: s.resourceId };
      // 子课程 → 内层小节
      if (s.resourceId) {
        const info = await j(BASE + '/course-study/course-front/info/' + s.resourceId
                             + '?type=6&sourceId=' + SUBJECT);
        const arr = Array.isArray(info) ? info : (info && info.courseChapters ? [info] : null);
        rec.inner = [];
        if (arr) {
          for (const c of arr) for (const cc of (c.courseChapters||[]))
            for (const x of (cc.courseChapterSections||[])) rec.inner.push(x);
        }
        // 直接可播放的（内嵌在专题章节里的视频/文档）
        if (!rec.inner.length && s.attachmentId) rec.inner.push(s);
      } else if (s.attachmentId) {
        rec.inner = [s];
      }
      // 3) 每节教材文字
      for (const x of (rec.inner || [])) {
        if (!x.courseId || !x.attachmentId) { x.guide = []; continue; }
        const g = await j(BASE + '/course-study/guide-study/get-guide-study-info?courseId='
                          + x.courseId + '&attachmentId=' + x.attachmentId);
        x.guide = Array.isArray(g) ? g.map(z => ({b:z.beginTime,e:z.endTime,n:z.name||'',c:z.content||''}))
                                     .filter(z => z.c) : [];
      }
      out.sections.push(rec);
    }
  }
  return JSON.stringify(out);
})()
""" % (json.dumps(BASE), json.dumps(SUBJECT))

r = send({"cmd": "eval", "js": JS}, wait=60)
v = (r.get("result") or {}).get("value")
if not v:
    print("采集失败:", json.dumps(r, ensure_ascii=False)[:400]); sys.exit(1)
d = json.loads(v)
json.dump(d, open(os.path.join(OUT, "structure.json"), "w", encoding="utf-8"),
          ensure_ascii=False, indent=1)

print("专题：%s" % d.get("name"))
print("说明：%s" % d.get("description"))
if d.get("err"): print("错误：%s" % d["err"])
print("章节 %d 个：" % len(d.get("chapters") or []))
for c in (d.get("chapters") or []):
    print("   · %-46s (%d 节)" % ((c.get("name") or "")[:44], c.get("count") or 0))
print()
ns = 0; ntxt = 0
for s in (d.get("sections") or []):
    print("■ %s   [%s]" % ((s.get("name") or "")[:56], (s.get("chapter") or "")[:22]))
    for x in (s.get("inner") or []):
        ns += 1
        chars = sum(len(z.get("c") or "") for z in (x.get("guide") or []))
        ntxt += chars
        print("    %-52s type=%-3s att=%s  %s" % ((x.get("name") or "")[:50], x.get("sectionType"),
              (x.get("attachmentId") or "—")[:8], ("%d字"%chars) if chars else "无文字"))
print("\n合计 %d 小节，%d 字教材文字" % (ns, ntxt))
print("落地：%s/structure.json" % OUT)
