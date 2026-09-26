// baomi-pull-course.js —— 中国保密在线「2026年度全国保密教育线上培训」学时类只读抓取
// ================================================================================================
// 用法：ego-browser nodejs < jr/tools/baomi-pull-course.js
// 产出（全部是接口原始返回，喂给 python3 baomi_study_plan.py 算学时缺口）：
//   jr/out/baomi/resources.json  各子目录的 getCourseResourceList 原始返回（41 个资源的权威来源）
//   jr/out/baomi/course.json     课程页三个目录的正文（只用来贴目录标签 + 失败时当兜底）
//   jr/out/baomi/grade.json      我的学习记录（必须带 pageSize=1000，默认一页会漏最新的）
//   jr/out/baomi/stat.json       课程统计（resourceSum / gradeSum / totalGrade / studyResourceNum）
//
// 只读：全部是 GET，不开考、不提交、不刷学时、不碰密码。
// 子目录 id 从页面自己发的请求里取（顶层 getCourseDirectoryList 只回 3 个顶层目录，子目录它不回），
// 所以这里先点一遍三个顶层目录名把它们的请求勾出来，再按 id 拉资源。
// 已知 7 个子目录 id 写死在 FALLBACK 里，页面结构变了也能兜住。
const fs = (await import("node:fs/promises"));
const CID = "312bc914-8e11-421b-b9bc-e900fe1a4e50";
const TOPS = ["保密优良传统教育", "保密知识技能教育", "保密纪律教育"];
const FALLBACK = [
  "8b6a1343-3212-4934-b609-94c12e6b1a0d", // 保密优良传统教育 · 守望 红色保密往事（6）
  "a2276fd7-ac04-40ed-b06f-bd2172f6e294", // 保密知识技能教育 · 守护：警惕身边的泄密风险（5）
  "b785749b-b640-4d00-a9f3-4fc988a18c34", // 保密知识技能教育 · 家国密事之反窃密（5）
  "05186de9-b830-45fd-ac0e-e90cae213d43", // 保密知识技能教育 · 新修订保密法实施条例解读（4）
  "8e2dfce8-81a6-41b7-9fc1-0b0202e86bd5", // 保密知识技能教育 · 2020-2026年公益宣传片（7）
  "65081f9d-83c1-494c-b013-66a6b8082e3f", // 保密纪律教育 · 红线不能触碰 底线不能逾越（12）
  "7f9951c6-2d4b-4185-aa6a-7d59da2a7bd5", // 保密纪律教育 · 事以密成：公职人员保密警示教育片（2）
];
const OUTDIR = "/Users/hgxszhj/x_videos-dowloads/exam-lookup/jr/out/baomi";

const task = await taskSpace(6);
const tabs = await task.tabs();
const tab = tabs.find((x) => /baomi\.org\.cn/.test(x.url || "")) || tabs[tabs.length - 1];
const page = task.page(tab.label);
console.log("标签页:", tab.label, "|", String(tab.url).slice(0, 70));

// 1) 先装嗅探器（必须在页面发请求之前装，否则漏首个请求）
await page.evaluate(() => {
  window.__sn = { hits: [] };
  const push = (u, s, b) => {
    const x = String(b || "");
    if (/^\s*[[{]/.test(x) && x.length > 60 && window.__sn.hits.length < 300) window.__sn.hits.push({ u: String(u), s, b: x.slice(0, 300000) });
  };
  const of = window.fetch;
  window.fetch = async function (...a) {
    const r = await of.apply(this, a);
    try { const u = (a[0] && (a[0].url || a[0])) || ""; r.clone().text().then((x) => push(u, r.status, x)).catch(() => {}); } catch (e) {}
    return r;
  };
  const oo = XMLHttpRequest.prototype.open, os = XMLHttpRequest.prototype.send;
  XMLHttpRequest.prototype.open = function (m, u, ...rest) { this.__u = u; return oo.call(this, m, u, ...rest); };
  XMLHttpRequest.prototype.send = function (...a) { this.addEventListener("load", () => { try { push(this.__u, this.status, this.responseText); } catch (e) {} }); return os.apply(this, a); };
});

// 2) 挨个点顶层目录名，把它展开时发的 getCourseResourceList 勾出来
const dirs = {};
const dirText = {};
for (const nm of TOPS) {
  const ok = await page.evaluate((name) => {
    const el = [...document.querySelectorAll("*")].filter((e) => (e.innerText || "").trim() === name && e.children.length <= 2).pop();
    if (!el) return false;
    const r = el.getBoundingClientRect();
    for (const ty of ["pointerdown", "mousedown", "pointerup", "mouseup", "click"]) el.dispatchEvent(new MouseEvent(ty, { bubbles: true, clientX: r.x + 4, clientY: r.y + 4 }));
    return true;
  }, nm);
  await page.waitForTimeout(3500);
  const txt = await page.evaluate((name) => {
    const i = (document.body.innerText || "").replace(/\s+/g, " ").indexOf(name);
    return i < 0 ? "" : document.body.innerText.replace(/\s+/g, " ").slice(i, i + 2200);
  }, nm);
  dirText[nm] = txt;
  console.log(ok ? `点了「${nm}」（正文 ${txt.length} 字）` : `没找到「${nm}」——用上次的正文兜底`);
}

const sniffed = await page.evaluate(() => (window.__sn.hits || []).map((h) => h.u));
const ids = [...new Set(sniffed
  .filter((u) => /getCourseResourceList/.test(u))
  .map((u) => { try { return new URL(u, "https://www.baomi.org.cn").searchParams.get("directoryId"); } catch (e) { return null; } })
  .filter(Boolean))];
const dirIds = ids.length >= 7 ? ids : [...new Set([...ids, ...FALLBACK])];
console.log("子目录 id:", dirIds.length, "个（嗅到", ids.length, "个）");

// 3) 按 id 拉资源 + 拉统计/学习记录
const data = await page.evaluate(async (args) => {
  const { CID, dirIds } = args;
  const tok = localStorage.getItem("token") || "";
  const get = async (u) => { try { const r = await fetch(u, { credentials: "include", headers: { token: tok } }); return await r.text(); } catch (e) { return "ERR " + e; } };
  const res = {};
  for (const id of dirIds) res[id] = await get(`/portal/main-api/v2/coursePacket/getCourseResourceList?coursePacketId=${CID}&directoryId=${id}&pageNum=1&pageSize=200`);
  return {
    dirs: res,
    grade: await get(`/portal/main-api/v2/coursePacket/getStudyCourseGradeList?coursePacketId=${CID}&token=${tok}&pageNum=1&pageSize=1000`),
    stat: await get(`/portal/main-api/v2/coursePacket/getCourseUserStatistic?coursePacketId=${CID}&token=${tok}`),
  };
}, { CID, dirIds });

const keep = (o) => Object.fromEntries(Object.entries(o).filter(([, v]) => String(v).length > 200));
const stamp = new Date().toISOString().slice(0, 16).replace("T", " ");
await fs.writeFile(`${OUTDIR}/resources.json`, JSON.stringify({ coursePacketId: CID, course: "2026年度全国保密教育线上培训", capturedAt: stamp, note: "各子目录 getCourseResourceList 原始返回（baomi-pull-course.js）", dirs: keep(data.dirs) }, null, 1), "utf8");
await fs.writeFile(`${OUTDIR}/course.json`, JSON.stringify({ coursePacketId: CID, course: "2026年度全国保密教育线上培训", capturedAt: stamp, dirs: dirText }, null, 1), "utf8");
await fs.writeFile(`${OUTDIR}/grade.json`, data.grade, "utf8");
await fs.writeFile(`${OUTDIR}/stat.json`, data.stat, "utf8");

let n = 0;
for (const v of Object.values(keep(data.dirs))) { try { n += (JSON.parse(v).data.listdata || []).length; } catch (e) {} }
const st = JSON.parse(data.stat).data || {};
console.log(`资源 ${n} 个（接口 resourceSum=${st.resourceSum}）| 已学 ${st.studyResourceNum} 个 / ${st.totalGrade} 学时 | 门槛 4 学时`);
console.log("下一步：cd exam-lookup && python3 baomi_study_plan.py selftest");
