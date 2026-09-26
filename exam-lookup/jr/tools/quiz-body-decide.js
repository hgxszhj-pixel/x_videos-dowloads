// 决定性一测：已知有题的块（综维），题目本体到底能不能取到 —— 把每个候选接口的原始返回打出来
// 只读 GET。跑法：ego-browser nodejs < jr/tools/quiz-body-decide.js
import fs from "node:fs/promises";
const OUT = "/Users/hgxszhj/x_videos-dowloads/exam-lookup/crawl/quiz-body-decide.json";
const task = await taskSpace(6);
const page = await task.newPage();
await page.goto("https://kc.zhixueyun.com/#/home-v?id=128558");
await page.waitForTimeout(4000);

const r = await page.evaluate(`(async () => {
  const raw = localStorage.getItem("token") || ""; let tk = raw;
  try { const t = JSON.parse(raw); tk = t.access_token || t.accessToken || ""; } catch (e) {}
  const H = { Authorization: "Bearer__" + tk, Version: "12.1.1", mucompanyidheader: "f32e65d0-fe3b-40d3-a025-4480a1808746", Accept: "application/json, text/plain, */*" };
  const B = "/api/v1";
  const j = async (u) => { try { const x = await fetch(u, { credentials: "include", headers: H }); const t = await x.text(); return { s: x.status, t }; } catch (e) { return { err: String(e).slice(0, 90) }; } };
  const SID = "ef2f4e5a-8fcb-4b45-a857-715bc58e6ac5";
  const cp = JSON.parse((await j(B + "/course-study/subject/chapter-progress?courseId=" + SID + "&knowledgePaymentEnable=false")).t);
  const childs = []; for (const c of cp) for (const s of (c.courseChapterSections || [])) if (s.resourceId) childs.push({ name: s.name, id: s.resourceId });
  const pairs = [];
  for (const c of childs) {
    const one = JSON.parse((await j(B + "/course-study/course-front/info/" + c.id + "?type=6&sourceId=" + SID)).t);
    const arr = Array.isArray(one) ? one : [one];
    for (const c1 of arr) for (const sec of (c1.courseChapters || [])) for (const x of (sec.courseChapterSections || [])) if (x.attachmentId && x.courseId) pairs.push({ name: x.name, courseId: x.courseId, attachmentId: x.attachmentId });
  }
  const gids = [];
  for (const p of pairs) {
    const g = await j(B + "/course-study/guide-study/get-guide-study-info?courseId=" + p.courseId + "&attachmentId=" + p.attachmentId);
    try { const arr = JSON.parse(g.t); if (Array.isArray(arr)) gids.push(...arr.map(z => z.id).filter(Boolean)); } catch (e) {}
  }
  const uniq = [...new Set(gids)];
  const stats = [];
  for (let i = 0; i < uniq.length; i += 30) { try { stats.push(...JSON.parse((await j(B + "/exam/temp-question/answer-statistics?guideStudyIds=" + uniq.slice(i, i + 30).join(","))).t)); } catch (e) {} }
  const withQ = stats.filter(x => x && x.allNum > 0);
  const gid = withQ[0] && withQ[0].guideStudyId;
  const urls = gid ? [
    "/exam/temp-question/front/page?guideStudyId=" + gid + "&pageNum=1&pageSize=5",
    "/exam/temp-question/front/page?guideStudyIds=" + gid + "&pageNum=1&pageSize=5",
    "/exam/question/front/page?guideStudyId=" + gid + "&pageNum=1&pageSize=5",
    "/exam/question/front/page?guideStudyIds=" + gid,
    "/exam/temp-question/list?guideStudyId=" + gid,
    "/exam/temp-question/get?guideStudyId=" + gid,
    "/exam/temp-question/answer-statistics?guideStudyIds=" + gid,
    "/coach-practice/question/front/page?guideStudyId=" + gid + "&pageNum=1&pageSize=5",
  ] : [];
  const probe = [];
  for (const u of urls) { const x = await j(B + u); probe.push({ url: u.slice(0, 100), status: x.s, head: (x.t || x.err || "").slice(0, 260) }); }
  return { pairs: pairs.length, guideIds: uniq.length, blocksWithQ: withQ.length, totalQ: withQ.reduce((a, x) => a + x.allNum, 0),
    gid, statSample: JSON.stringify(withQ[0] || stats[0] || {}).slice(0, 220), probe };
})()`);

console.log("小节", r.pairs, "｜guide 块", r.guideIds, "｜有题块", r.blocksWithQ, "｜题量", r.totalQ);
console.log("第一个有题的块 id:", r.gid, "\n统计样本:", r.statSample);
for (const p of r.probe || []) console.log("\n" + (p.status || "?") + "  " + p.url + "\n    → " + (p.head || "").replace(/\s+/g, " "));
await fs.writeFile(OUT, JSON.stringify(r, null, 1), "utf8");
console.log("\n写入", OUT);
await page.close();
