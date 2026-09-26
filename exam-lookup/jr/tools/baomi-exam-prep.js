// baomi-exam-prep.js —— 只读：打开保密教育考试页，看当前状态（学时/能否开考/规则），并装一个只读的接口记录器
// 用法：ego-browser nodejs < jr/tools/baomi-exam-prep.js
// 边界：不点「开始考试」、不提交、不刷学时；只做只读 GET 与页面状态读取
const EXAM = "https://www.baomi.org.cn/bmCourseDetail/exam?id=312bc914-8e11-421b-b9bc-e900fe1a4e50&index=3&docId=56242227&docLibId=-15&productId=3236&pubId=42388&siteId=95&title=2026%E5%B9%B4%E5%BA%A6%E5%85%A8%E5%9B%BD%E4%BF%9D%E5%AF%86%E6%95%99%E8%82%B2%E7%BA%BF%E4%B8%8A%E5%9F%B9%E8%AE%AD&IsAudition=false&flag=false&status=1&isAllowToEndCourse=0";

const task = await taskSpace(6);
const tabs = await task.tabs();
console.log("=== 现有标签页 ===");
for (const t of tabs) console.log("  ", t.label || "(未托管)", "|", (t.title || "").slice(0, 28), "|", (t.url || "").slice(0, 70));

let page = null;
for (const t of tabs) {
  if ((t.url || "").includes("baomi.org.cn")) {
    page = t.label ? task.page(t.label) : await task.adopt(t.page);
    break;
  }
}
if (!page) page = task.page("p1");
console.log("使用标签页:", page.label);

// 只读记录器：把 XHR/fetch 的响应存进 window.__cap（不改任何请求）
const install = `(() => {
  if (window.__capInstalled) return "already";
  window.__capInstalled = true;
  window.__cap = [];
  const keep = (u, m, s, b) => { try { window.__cap.push({ url: String(u).slice(0, 220), method: m, status: s, body: String(b || "").slice(0, 4000) }); } catch (e) {} };
  const oOpen = XMLHttpRequest.prototype.open, oSend = XMLHttpRequest.prototype.send;
  XMLHttpRequest.prototype.open = function (m, u) { this.__m = m; this.__u = u; return oOpen.apply(this, arguments); };
  XMLHttpRequest.prototype.send = function () {
    this.addEventListener("load", () => keep(this.__u, this.__m, this.status, this.responseText));
    return oSend.apply(this, arguments);
  };
  const oFetch = window.fetch;
  window.fetch = function (...a) {
    return oFetch.apply(this, a).then((r) => { r.clone().text().then((b) => keep(r.url, (a[1] && a[1].method) || "GET", r.status, b)).catch(() => {}); return r; });
  };
  return "installed";
})()`;

await page.goto(EXAM, { waitUntil: "load" });
await page.waitForLoadState("networkidle").catch(() => {});
await page.waitForTimeout(2500);
console.log("装记录器:", await page.evaluate(install));
// 再刷一次，把首页那批接口也记下来（仍然只是重放同一页面的 GET）
await page.reload({ waitUntil: "load" });
await page.waitForLoadState("networkidle").catch(() => {});
await page.waitForTimeout(3000);

const state = await page.evaluate(`(() => {
  const txt = (document.body.innerText || "").replace(/\\n{2,}/g, "\\n").slice(0, 1800);
  const btns = [...document.querySelectorAll("button,a.btn,a[class*=btn],[onclick]")]
    .map((b) => (b.innerText || "").trim()).filter((s) => s && s.length <= 14);
  const caps = (window.__cap || []).map((c) => c.method + " " + c.status + " " + c.url.split("?")[0]);
  return { title: document.title, url: location.href, buttons: [...new Set(btns)].slice(0, 30), text: txt, caps: [...new Set(caps)].slice(0, 40), n: (window.__cap || []).length };
})()`);

console.log("=== 页面 ===");
console.log("标题:", state.title);
console.log("地址:", state.url);
console.log("按钮:", JSON.stringify(state.buttons));
console.log("--- 正文（前 1800 字）---");
console.log(state.text);
console.log("=== 这一轮捕获的接口（去重，共 " + state.n + " 条）===");
for (const c of state.caps) console.log("  ", c);

// 把跟考试/学时相关的响应挑出来存盘
const fs = await import("node:fs/promises");
const hits = await page.evaluate(`(() => (window.__cap || []).filter((c) => /exam|Exam|grade|Grade|study|Study|resource|Resource|paper|Paper/.test(c.url)))()`);
await fs.writeFile("/tmp/baomi_exam_cap.json", JSON.stringify(hits, null, 1));
console.log("相关响应已存:", "/tmp/baomi_exam_cap.json", hits.length, "条");
