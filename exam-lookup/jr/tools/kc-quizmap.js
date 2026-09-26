// 重新取 guide 块 id 并统计题量（之前 harvest 丢掉了 guide.id）
import fs from "node:fs/promises";

const DIR = "/Users/hgxszhj/x_videos-dowloads/exam-lookup/crawl/subjects";
const OUT = "/Users/hgxszhj/x_videos-dowloads/exam-lookup/crawl";
const files = (await fs.readdir(DIR)).filter((f) => f.endsWith(".json"));

const task = await taskSpace(6);
const tabs = await task.tabs();
const cur = tabs.find((t) => /kc\.zhixueyun\.com/.test(t.url || "")) || tabs.find((t) => t.active);
const page = cur && cur.label ? task.page(cur.label) : task.page("p1");
try { await page.goto("https://kc.zhixueyun.com/#/home-v?id=128558"); } catch (e) {}
await page.waitForTimeout(4000);

const summary = [];
for (const f of files) {
  const d = JSON.parse(await fs.readFile(`${DIR}/${f}`, "utf8"));
  const secs = [];
  const atts = [];
  for (const ch of d.chapters || []) for (const s of ch.sections || []) { if (s.courseId && s.attachmentId) secs.push({ courseId: s.courseId, attachmentId: s.attachmentId, name: s.name }); if (s.attachmentId) atts.push({ name: s.name, attachmentId: s.attachmentId, sectionType: s.sectionType, totalTime: s.totalTime, chapter: ch.chapter }); }

  const res = await page.evaluate(
    async ({ secs }) => {
      let tk = "";
      try { const t = JSON.parse(localStorage.getItem("token") || "{}"); tk = t.access_token || t.accessToken || ""; } catch (e) {}
      const H = { Authorization: "Bearer__" + tk, Version: "12.1.1", mucompanyidheader: "f32e65d0-fe3b-40d3-a025-4480a1808746", "X-Requested-With": "XMLHttpRequest", Accept: "application/json, text/plain, */*" };
      const per = [];
      const guideIds = [];
      for (const s of secs) {
        try {
          const r = await fetch("/api/v1/course-study/guide-study/get-guide-study-info?courseId=" + s.courseId + "&attachmentId=" + s.attachmentId, { credentials: "include", headers: H });
          const g = await r.json();
          if (Array.isArray(g)) { per.push({ section: s.name, blocks: g.length, ids: g.map((z) => z.id).filter(Boolean) }); guideIds.push(...g.map((z) => z.id).filter(Boolean)); }
          else per.push({ section: s.name, blocks: 0 });
        } catch (e) { per.push({ section: s.name, err: String(e).slice(0, 60) }); }
      }
      const uniq = [...new Set(guideIds)];
      const stats = [];
      for (let i = 0; i < uniq.length; i += 30) {
        try {
          const r = await fetch("/api/v1/exam/temp-question/answer-statistics?guideStudyIds=" + uniq.slice(i, i + 30).join(","), { credentials: "include", headers: H });
          const t = await r.text();
          try { stats.push(...JSON.parse(t)); } catch (e) {}
        } catch (e) {}
      }
      return { per, uniq, stats };
    },
    { secs },
  );

  const st = (res.stats || []).filter((x) => x && typeof x.allNum === "number");
  const totalQ = st.reduce((a, x) => a + x.allNum, 0);
  const withQ = st.filter((x) => x.allNum > 0).length;
  summary.push({ tag: d.tag, sections: secs.length, guideIds: (res.uniq || []).length, guideWithQuestions: withQ, totalQuestions: totalQ, attachments: atts.length });
  await fs.writeFile(`${OUT}/quizmap-${d.tag}.json`, JSON.stringify({ tag: d.tag, sections: res.per, stats: st, attachments: atts }, null, 1), "utf8");
  console.log(`${d.tag}: 小节 ${secs.length} / guide块 ${(res.uniq || []).length} / 有题块 ${withQ} / 题目 ${totalQ} / 附件 ${atts.length}`);
}
await fs.writeFile(`${OUT}/quiz-map.json`, JSON.stringify(summary, null, 1), "utf8");
console.log("合计题目:", summary.reduce((a, x) => a + x.totalQuestions, 0), "附件:", summary.reduce((a, x) => a + x.attachments, 0));
