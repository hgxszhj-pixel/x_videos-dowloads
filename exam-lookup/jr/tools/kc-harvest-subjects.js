// 采集多个专题：章节树 → 子课程小节 → 教材文字 + 内嵌问答（题）
// 可重复运行（断点续采：已采过的 childCourseId 跳过）
import fs from "node:fs/promises";

const DIR = "/Users/hgxszhj/x_videos-dowloads/exam-lookup/crawl/subjects";
await fs.mkdir(DIR, { recursive: true });

// 专题清单：原先写死在这里，2026-09-22 起改成「读 crawl/cms_subjects.json」——
// cms.mylearning.cn 各专区的入口 id 会变，写死在脚本里每次都要重编辑一遍。
// 文件由 crawl 侧生成：kc 专题/课程 id（38 个，安全学堂/网信安/研发/技能认证…）
const SUBJECT_FILE = "/Users/hgxszhj/x_videos-dowloads/exam-lookup/crawl/cms_subjects.json";
const SUBJECTS = [
  { tag: "xianchang-zonghe-weihu", id: "ef2f4e5a-8fcb-4b45-a857-715bc58e6ac5", name: "2026现场综合化维护技能认证教材" },
  { tag: "zhuangwei-jiaofu-1", id: "c7d7f4da-b169-4f0c-a4aa-847be4b299ca", name: "装维交付(1)" },
  { tag: "zhuangwei-jiaofu-2", id: "9f981a29-1fb4-4ce9-b16f-51a180cfaa10", name: "装维交付(2)" },
  { tag: "jieru-wang", id: "555f03e3-66a5-43ac-8a73-5fbce1690c72", name: "接入网" },
  { tag: "donghuan", id: "73d4d1df-b7df-4f44-be7b-597e5a29e6fd", name: "动环" },
  { tag: "safety-school", id: "e61f9044-99ce-478e-987c-cfef46c574fd", name: "安全学堂某专题" },
  { tag: "wxaq", id: "5bb39fc3-5ce2-4896-bd4e-377ac49cbb86", name: "网络和信息安全某专题" },
];
try {
  const extra = JSON.parse(await fs.readFile(SUBJECT_FILE, "utf8")).subjects || [];
  const have = new Set(SUBJECTS.map((x) => x.id));
  for (const x of extra) if (x.id && !have.has(x.id)) SUBJECTS.push({ tag: x.tag, id: x.id, name: x.name || x.tag });
} catch (e) { console.log("（没读到 " + SUBJECT_FILE + "，只采脚本里写死的这几个）"); }


const task = await taskSpace(6);
const tabs = await task.tabs();
const cur = tabs.find((t) => /kc\.zhixueyun\.com/.test(t.url || "")) || tabs.find((t) => t.active);
const page = cur && cur.label ? task.page(cur.label) : task.page("p1");
try { await page.goto("https://kc.zhixueyun.com/#/home-v?id=128558"); } catch (e) {}
await page.waitForTimeout(4000);

const BUDGET_MS = 200000; // 每次运行的工作预算
for (const sub of SUBJECTS) {
  const file = `${DIR}/${sub.tag}.json`;
  let store = { tag: sub.tag, id: sub.id, name: sub.name, chapters: [] };
  try { store = JSON.parse(await fs.readFile(file, "utf8")); } catch (e) {}
  const done = new Set((store.chapters || []).map((c) => c.childCourseId).filter(Boolean));

  const out = await page.evaluate(
    async ({ subjectId, budget, doneIds }) => {
      let tk = "";
      try { const t = JSON.parse(localStorage.getItem("token") || "{}"); tk = t.access_token || t.accessToken || ""; } catch (e) {}
      const H = { Authorization: "Bearer__" + tk, Version: "12.1.1", mucompanyidheader: "f32e65d0-fe3b-40d3-a025-4480a1808746", "X-Requested-With": "XMLHttpRequest", Accept: "application/json, text/plain, */*" };
      const B = "/api/v1";
      const j = async (u) => {
        try { const r = await fetch(u, { credentials: "include", headers: H }); const t = await r.text(); try { return JSON.parse(t); } catch (e) { return { __raw: t.slice(0, 300) }; } }
        catch (e) { return { __err: String(e).slice(0, 120) }; }
      };
      const cp = await j(B + "/course-study/subject/chapter-progress?courseId=" + subjectId + "&knowledgePaymentEnable=false");
      if (!Array.isArray(cp) || !cp.length) return { err: "chapter-progress:" + JSON.stringify(cp).slice(0, 300), chapters: [] };

      const outer = [];
      for (const ch of cp) for (const s of (ch.courseChapterSections || []))
        if (s.resourceId) outer.push({ chapter: ch.name || ch.title || "", name: s.name || "", childCourseId: s.resourceId, sectionType: s.sectionType });

      const t0 = Date.now();
      const res = [];
      const doneSet = new Set(doneIds);
      for (const s of outer) {
        if (doneSet.has(s.childCourseId)) continue;
        if (Date.now() - t0 > budget) { res.push({ __stopped: true }); break; }
        const item = Object.assign({}, s, { sections: [], questions: [], err: null });
        const info = await j(B + "/course-study/course-front/info/" + s.childCourseId + "?type=6&sourceId=" + subjectId);
        const arr = Array.isArray(info) ? info : (info && info.courseChapters ? [info] : null);
        if (!arr) { item.err = "info:" + JSON.stringify(info).slice(0, 200); res.push(item); continue; }
        const inner = [];
        for (const c of arr) for (const sec of (c.courseChapters || [])) for (const x of (sec.courseChapterSections || [])) inner.push(x);
        for (const x of inner) {
          const rec = { name: x.name, sectionType: x.sectionType, totalTime: x.totalTime, attachmentId: x.attachmentId, courseId: x.courseId, exerciseCount: x.exerciseCount };
          if (x.attachmentId && x.courseId) {
            const g = await j(B + "/course-study/guide-study/get-guide-study-info?courseId=" + x.courseId + "&attachmentId=" + x.attachmentId);
            if (Array.isArray(g)) rec.guide = g.map((z) => ({ b: z.beginTime, e: z.endTime, n: z.name || "", c: z.content || "" })).filter((z) => z.c);
            const q = await j(B + "/course-study/guide-study/query-barrage-question?courseId=" + x.courseId + "&attachmentId=" + x.attachmentId + "&beginTime=0&endTime=" + (x.totalTime || 99999));
            if (q && (Array.isArray(q) ? q.length : (q.__raw === undefined && q.__err === undefined))) item.questions.push({ section: x.name, data: q });
            else item.qErr = JSON.stringify(q).slice(0, 200);
          }
          item.sections.push(rec);
        }
        res.push(item);
      }
      return { chapters: res.filter((x) => !x.__stopped), stopped: res.some((x) => x.__stopped), total: outer.length, name: (cp[0] && cp[0].name) || "" };
    },
    { subjectId: sub.id, budget: BUDGET_MS, doneIds: [...done] },
  );

  if (out.err) { console.log(`[${sub.tag}] 跳过：${out.err.slice(0, 120)}`); continue; }
  store.chapters.push(...(out.chapters || []));
  await fs.writeFile(file, JSON.stringify(store, null, 1), "utf8");
  const nq = store.chapters.reduce((a, c) => a + (c.questions || []).length, 0);
  const nsec = store.chapters.reduce((a, c) => a + (c.sections || []).length, 0);
  console.log(`[${sub.tag}] 本次 +${(out.chapters || []).length} 章 / 累计 ${store.chapters.length} 章（共 ${out.total}），小节 ${nsec}，含问答小节 ${nq}${out.stopped ? "（预算到，需再跑）" : ""}`);
}
