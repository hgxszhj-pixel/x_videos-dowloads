// baomi-hours-check.js —— 只读：查 2026 保密教育课的「已学学时 / 门槛 4 学时」与考试页按钮状态
// 用法：ego-browser nodejs < jr/tools/baomi-hours-check.js
// 边界：只做同源只读 GET；不点「开始考试」、不刷学时、不提交任何东西
const CID = "312bc914-8e11-421b-b9bc-e900fe1a4e50";
const OUT = "/Users/hgxszhj/x_videos-dowloads/exam-lookup/jr/out/baomi";

const task = await taskSpace(6);
const tabs = await task.tabs();
const t = tabs.find((x) => /baomi\.org\.cn/.test(x.url || ""));
if (!t) { console.log("没找到 baomi 标签页"); process.exit(1); }
const page = t.label ? task.page(t.label) : await task.adopt(t.page);

const res = await page.evaluate(`(async () => {
  const tok = localStorage.getItem("token") || sessionStorage.getItem("token") || "";
  const get = async (u) => { try { const r = await page_fetch(u); return await r.text(); } catch (e) { return "ERR " + e; } };
  const page_fetch = (u) => fetch(u, { credentials: "include", headers: tok ? { token: tok } : {} });
  const api = "/portal/main-api/v2/coursePacket/";
  const out = {};
  out.tokenLen = tok.length;
  out.grade = await get(api + "getStudyCourseGradeList?coursePacketId=${CID}&token=" + tok + "&pageNum=1&pageSize=1000");
  out.stat = await get(api + "getCourseUserStatistic?coursePacketId=${CID}&token=" + tok);
  const btn = [...document.querySelectorAll("*")].filter((e) => (e.innerText || "").trim() === "开始考试").slice(0, 2)
    .map((e) => (e.outerHTML || "").slice(0, 300));
  out.examBtn = btn;
  out.gate = (document.body.innerText || "").split("\\n").filter((l) => /学时|合格|优秀|试题数/.test(l)).slice(0, 6);
  return out;
})()`);

const fs = await import("node:fs/promises");
await fs.writeFile(OUT + "/grade.json", res.grade || "");
await fs.writeFile(OUT + "/stat.json", res.stat || "");
console.log("token 长度:", res.tokenLen, "（只读，不打印）");
console.log("--- 考试页门槛文案 ---");
for (const l of res.gate || []) console.log("  ", l.trim());
console.log("--- 「开始考试」按钮 DOM（看是不是 disabled）---");
for (const b of res.examBtn || []) console.log("  ", b.replace(/\s+/g, " ").slice(0, 240));
console.log("--- 学时接口 ---");
try {
  const stat = JSON.parse(res.stat);
  const d = stat.data || {};
  console.log("  接口统计:", JSON.stringify({ gradeSum: d.gradeSum, resourceSum: d.resourceSum, studyResourceNum: d.studyResourceNum, totalGrade: d.totalGrade }).slice(0, 200));
} catch (e) { console.log("  stat 解析失败:", String(res.stat).slice(0, 120)); }
try {
  const g = JSON.parse(res.grade);
  const list = (g.data && (g.data.recordList || g.data.list)) || [];
  console.log("  学习记录条数:", list.length);
} catch (e) { console.log("  grade 解析失败:", String(res.grade).slice(0, 120)); }
console.log("原始返回已存:", OUT + "/grade.json , stat.json");
