// 只读探测：其它专区（安全学堂 / 网络和信息安全）的教材附件 + 题量，并实测「题目本体」能不能取到
// 全程 GET，不点开始考试、不创建练习记录。跑法：ego-browser nodejs < jr/tools/quiz-body-probe.js
import fs from "node:fs/promises";

const OUT = "/Users/hgxszhj/x_videos-dowloads/exam-lookup/crawl/quiz-body-probe.json";
const SUBJECTS = [
  { tag: "safety-school", id: "e61f9044-99ce-478e-987c-cfef46c574fd", name: "安全学堂某专题" },
  { tag: "wxaq", id: "5bb39fc3-5ce2-4896-bd4e-377ac49cbb86", name: "网络和信息安全某专题" },
  { tag: "control-xianchang", id: "ef2f4e5a-8fcb-4b45-a857-715bc58e6ac5", name: "综维（对照，已知 1644 题）" },
];

const task = await taskSpace(6);
const page = await task.newPage();
await page.goto("https://kc.zhixueyun.com/#/home-v?id=128558");
await page.waitForTimeout(4000);

const out = [];
for (const sub of SUBJECTS) {
  const r = await page.evaluate(`(async () => {
    let tk = ""; const raw = localStorage.getItem("token") || "";
    try { const t = JSON.parse(raw); tk = t.access_token || t.accessToken || ""; } catch (e) { tk = raw; }   // 本机 token 是裸字符串，JSON.parse 会抛
    if (!tk) tk = raw;
    const H = { Authorization: "Bearer__" + tk, Version: "12.1.1", mucompanyidheader: "f32e65d0-fe3b-40d3-a025-4480a1808746", "X-Requested-With": "XMLHttpRequest", Accept: "application/json, text/plain, */*" };
    const B = "/api/v1";
    // 注意：t 必须存全文 —— 早先这里 slice(0,600) 之后再 JSON.parse，直接抛错，结果「章 0 节 0」看着像没数据
    const j = async (u) => { try { const r = await fetch(u, { credentials: "include", headers: H }); const t = await r.text(); return { s: r.status, t, head: t.slice(0, 300) }; } catch (e) { return { err: String(e).slice(0, 100) }; } };
    const sub = ${JSON.stringify(sub)};
    const cp = await j(B + "/course-study/subject/chapter-progress?courseId=" + sub.id + "&knowledgePaymentEnable=false");
    let chapters = 0, childs = [];
    try { const arr = JSON.parse(cp.t); chapters = arr.length; for (const c of arr) for (const s of (c.courseChapterSections || [])) if (s.resourceId) childs.push({ name: s.name, id: s.resourceId }); } catch (e) {}
    const secs = [], guideIds = [], atts = [];
    for (const c of childs.slice(0, 6)) {
      const info = await j(B + "/course-study/course-front/info/" + c.id + "?type=6&sourceId=" + sub.id);
      try {
        const one = JSON.parse(info.t);
        const arr = Array.isArray(one) ? one : [one];        // 这个接口返回**对象**不是数组，直接 for...of 会抛（抛了就被 catch 吞成「小节 0」）
        for (const c1 of arr) for (const sec of (c1.courseChapters || [])) for (const x of (sec.courseChapterSections || [])) {
          secs.push({ name: x.name, courseId: x.courseId, attachmentId: x.attachmentId, exerciseCount: x.exerciseCount, sectionType: x.sectionType });
          if (x.attachmentId) atts.push(x.name);
        }
      } catch (e) {}
    }
    for (const s of secs) {
      if (!s.courseId || !s.attachmentId) continue;
      const g = await j(B + "/course-study/guide-study/get-guide-study-info?courseId=" + s.courseId + "&attachmentId=" + s.attachmentId);
      try { const arr = JSON.parse(g.t); if (Array.isArray(arr)) guideIds.push(...arr.map(z => z.id).filter(Boolean)); } catch (e) {}
    }
    const uniq = [...new Set(guideIds)];
    const stats = [];
    for (let i = 0; i < uniq.length; i += 30) {
      const st = await j(B + "/exam/temp-question/answer-statistics?guideStudyIds=" + uniq.slice(i, i + 30).join(","));
      try { stats.push(...JSON.parse(st.t)); } catch (e) {}
    }
    const withQ = stats.filter(x => x && x.allNum > 0);
    const total = withQ.reduce((a, x) => a + x.allNum, 0);
    // 题目本体：拿第一个有题的块，试所有已知/推测的取题接口，把原始返回记下来
    const probe = [];
    const gid = withQ[0] && withQ[0].guideStudyId;
    if (gid) {
      const urls = [
        "/exam/temp-question/front/page?guideStudyId=" + gid + "&pageNum=1&pageSize=5",
        "/exam/temp-question/front/page?guideStudyIds=" + gid + "&pageNum=1&pageSize=5",
        "/exam/question/front/page?guideStudyId=" + gid + "&pageNum=1&pageSize=5",
        "/exam/temp-question/list?guideStudyId=" + gid,
        "/exam/temp-question/get?guideStudyId=" + gid,
        "/exam/temp-question/answer-statistics?guideStudyIds=" + gid,
      ];
      for (const u of urls) { const x = await j(B + u); probe.push({ url: u.slice(0, 90), status: x.s, head: (x.t || x.err || "").slice(0, 300) }); }
    }
    return { tag: sub.tag, name: sub.name, tkLen: tk.length, cpRaw: (cp.head || cp.err || "").slice(0, 200), chapters, childs: childs.length, secs: secs.length, guides: uniq.length,
      atts: atts.slice(0, 12), statsBlocks: stats.length, withQuestions: withQ.length, totalQuestions: total, gid: gid || null, probe };
  })()`).catch((e) => ({ tag: sub.tag, err: String(e).slice(0, 160) }));
  out.push(r);
  console.log("== " + r.tag + " " + (r.name || "") + " ==");
  console.log("   token 长度", r.tkLen, "｜chapter-progress 原始返回:", (r.cpRaw || "").replace(/\s+/g, " ").slice(0, 120));
  console.log("   章", r.chapters, "｜子课程", r.childs, "｜小节", r.secs, "｜guide块", r.guides, "｜有题块", r.withQuestions, "｜题量", r.totalQuestions, r.err || "");
  if ((r.atts || []).length) console.log("   教材附件:", r.atts.slice(0, 4).join(" / ").slice(0, 150));
  for (const p of (r.probe || [])) console.log("   取题 " + (p.status || "?") + "｜" + p.url + "\n        → " + (p.head || "").replace(/\s+/g, " ").slice(0, 220));
}
await fs.writeFile(OUT, JSON.stringify(out, null, 1), "utf8");
console.log("\n写入", OUT);
await page.close();
