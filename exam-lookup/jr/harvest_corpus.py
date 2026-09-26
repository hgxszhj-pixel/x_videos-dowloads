#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""采集【现场综合化维护】专题全部章节的教材文字 + 视频内嵌问答题。

链路（实测）：
  subject(ef2f4e5a) --chapter-progress--> 46 个外层章节(每个有 attachmentId = 子课程 id)
  子课程 --course-front/info/<子课程id>?type=6&sourceId=<专题id>--> 内层小节(视频/文档)
  小节 --guide-study/get-guide-study-info?courseId=&attachmentId=--> 教材文字(带时间轴)
  小节 --guide-study/query-barrage-question?courseId=&attachmentId=&beginTime=&endTime=--> 内嵌问答

为什么要批量：逐个走 jr.py 每次 1-3 秒，46 节要几分钟；在页面里 Promise.all 并发快得多。
"""
import json, os, sys, time
sys.path.insert(0, "/Users/hgxszhj/x_videos-dowloads/exam-lookup/jr")
from jr import send

BASE = "https://kc.zhixueyun.com/api/v1"
SUBJECT = "ef2f4e5a-8fcb-4b45-a857-715bc58e6ac5"
OUT = "out/corpus"

os.makedirs(OUT, exist_ok=True)

# ---------- 1. 取专题章节树 ----------
cp = json.load(open("out/api/course-study-subject-chapter-progress.json", encoding="utf-8"))
outer = []
for ch in cp:
    for s in (ch.get("courseChapterSections") or []):
        if not s.get("resourceId"):
            continue
        outer.append({
            "chapter": ch.get("name") or ch.get("title") or "",
            "name": s.get("name") or "",
            "childCourseId": s.get("resourceId"),
            "sectionId": s.get("id"),
            "sectionType": s.get("sectionType"),
        })
print("外层章节 %d 个，开始采集" % len(outer))

JS_TMPL = r"""
(async () => {
  const BASE = %s;
  const SUBJECT = %s;
  const list = %s;
  const tk = (() => { try { return JSON.parse(localStorage.getItem('token')||'{}').access_token||''; } catch(e){ return ''; } })();
  const H = { 'version': '12.1.1' };
  if (tk) H.authorization = 'Bearer__' + tk;
  async function j(u) {
    try { const r = await fetch(u, { credentials: 'include', headers: H });
          const t = await r.text();
          try { return JSON.parse(t); } catch (e) { return { __raw: t.slice(0,400) }; } }
    catch (e) { return { __err: String(e).slice(0,120) }; }
  }
  const out = [];
  for (const s of list) {
    const item = Object.assign({}, s, { sections: [], err: null });
    // 子课程内容（实测返回单个对象，不是数组）
    const info = await j(BASE + '/course-study/course-front/info/' + s.childCourseId
        + '?type=6&sourceId=' + SUBJECT);
    const arr = Array.isArray(info) ? info : (info && info.courseChapters ? [info] : null);
    if (!arr) { item.err = 'info:' + JSON.stringify(info).slice(0,160); out.push(item); continue; }
    const inner = [];
    for (const c of arr) for (const sec of (c.courseChapters || []))
      for (const x of (sec.courseChapterSections || [])) inner.push(x);
    for (const x of inner) {
      const rec = { name: x.name, sectionType: x.sectionType, totalTime: x.totalTime,
                    attachmentId: x.attachmentId, courseId: x.courseId };
      if (x.attachmentId && x.courseId) {
        const g = await j(BASE + '/course-study/guide-study/get-guide-study-info?courseId='
            + x.courseId + '&attachmentId=' + x.attachmentId);
        if (Array.isArray(g)) rec.guide = g.map(z => ({ b: z.beginTime, e: z.endTime,
            n: z.name || '', c: z.content || '' })).filter(z => z.c);
      }
      item.sections.push(rec);
    }
    out.push(item);
  }
  return JSON.stringify(out);
})()
"""

CHUNK = 6
corpus = []
if os.path.exists(os.path.join(OUT, "corpus.json")):
    corpus = json.load(open(os.path.join(OUT, "corpus.json"), encoding="utf-8"))
done = {x["childCourseId"] for x in corpus}
todo = [s for s in outer if s["childCourseId"] not in done]
print("已完成 %d，待采集 %d" % (len(done), len(todo)))

for i in range(0, len(todo), CHUNK):
    batch = todo[i:i + CHUNK]
    js = JS_TMPL % (json.dumps(BASE), json.dumps(SUBJECT), json.dumps(batch, ensure_ascii=False))
    r = send({"cmd": "eval", "js": js}, wait=30)
    v = (r.get("result") or {}).get("value")
    if not v:
        print("  批次 %d 失败: %s" % (i // CHUNK + 1, json.dumps(r, ensure_ascii=False)[:200]))
        continue
    try:
        got = json.loads(v)
    except Exception as e:
        print("  批次 %d 解析失败: %s" % (i // CHUNK + 1, e)); continue
    corpus.extend(got)
    json.dump(corpus, open(os.path.join(OUT, "corpus.json"), "w", encoding="utf-8"),
              ensure_ascii=False, indent=1)
    nsec = sum(len(x.get("sections") or []) for x in got)
    ntxt = sum(len(z.get("c") or "") for x in got for s in (x.get("sections") or [])
               for z in (s.get("guide") or []))
    print("  批次 %d/%d: %d 章, %d 小节, %d 字" %
          (i // CHUNK + 1, (len(todo) + CHUNK - 1) // CHUNK, len(got), nsec, ntxt))

print("\n采集完成，共 %d 章" % len(corpus))
