// 考后存档：把「成绩详情 / 答卷」页真发的请求 + 页面上的题干选项一起抓下来
// ---------------------------------------------------------------------------
// 跑法：ego-browser nodejs < jr/tools/kc-archive-exam.js
// 产物：crawl/exam-archive-<时间戳>.json
//       { capturedAt, page, sniffs:[{url,status,body}], dom:{stems,options,answerish}, tokens }
//
// 什么时候跑：等你自己考完、平台回显卷子之后（本场 81 分不及格 → 平台不回显，跑出来会是空的，
// 那不是脚本坏了，是「没通过就看不到卷」这条规则）。
//
// 为什么用「嗅探页面真发的请求」而不是我猜接口：
//   台账 H16 用 examRecordId 打 exam/front/score-detail 拿到 422/30142 —— 参数形态不对。
//   平台的成绩详情接口到底要什么参数，猜不如让它自己发：页面知道怎么拿，我们只负责记账。
//   同时读一遍 DOM（油猴 481461 的做法：div[data-current$=':content'] + ':options'），两条路互为备份。
//
// 只读：只发页面自己会发的 GET，不点交卷、不提交答案、不改任何计数。
import fs from "node:fs/promises";

const ROOT = "/Users/hgxszhj/x_videos-dowloads/exam-lookup";
const STAMP = new Date().toISOString().replace(/[:.]/g, "-").slice(0, 19);
const OUT = `${ROOT}/crawl/exam-archive-${STAMP}.json`;

const task = await taskSpace(6);
const tabs = await task.tabs();
const cur = tabs.find((t) => /kc\.zhixueyun\.com/.test(t.url || "")) || tabs.find((t) => t.active) || tabs[0];
const page = cur && cur.label ? task.page(cur.label) : task.page("p1");
console.log("用标签页:", cur && cur.label, "|", (cur && cur.url) || "");

// 0) 保证应用真的起来了：hash 路由直跳会让 SPA 卡在「努力加载中」（整页重载 + 懒加载没跑完），
//    所以先看页面有没有内容，没有就回根路径重载，轮询等到有内容再往下走。
async function ensureLoaded() {
  for (let i = 0; i < 3; i++) {
    const t = await page.evaluate(() => ((document.body && document.body.innerText) || "").replace(/\s+/g, " ").trim());
    if (t.length > 20 && !/^努力加载中/.test(t)) return t.slice(0, 60);
    try { await page.goto("https://kc.zhixueyun.com/"); } catch (e) {}
    for (let k = 0; k < 12; k++) {                        // 最多等 12 秒
      await page.waitForTimeout(1000);
      const tt = await page.evaluate(() => ((document.body && document.body.innerText) || "").replace(/\s+/g, " ").trim());
      if (tt.length > 20 && !/^努力加载中/.test(tt)) return tt.slice(0, 60);
    }
  }
  return "";
}
const loaded = await ensureLoaded();
console.log("页面已渲染：", loaded || "（还是空的，下面的结果会不准）");

// 1) 装嗅探器（必须在页面发请求之前装好），再让页面自己走过去
await page.evaluate(() => {
  if (window.__archive) return "already";
  window.__archive = { sniffs: [] };
  const keep = () => true;                       // 先全收（初始化请求也算证据），Python 侧再挑需要的
  const push = (url, status, body) => {
    try {
      if (!keep(String(url))) return;
      const b = String(body || "");
      if (b.length < 20 || !/^[\s]*[\[{]/.test(b)) return;      // 只要 JSON（HTML/JS 不记）
      if (window.__archive.sniffs.length > 80) return;
      window.__archive.sniffs.push({ url: String(url), status, body: String(body).slice(0, 60000) });
    } catch (e) {}
  };
  const of = window.fetch;
  window.fetch = async function (...a) {
    const r = await of.apply(this, a);
    try {
      const url = (a[0] && (a[0].url || a[0])) || "";
      r.clone().text().then((t) => push(url, r.status, t)).catch(() => {});
    } catch (e) {}
    return r;
  };
  const oo = XMLHttpRequest.prototype.open, os = XMLHttpRequest.prototype.send;
  XMLHttpRequest.prototype.open = function (m, u, ...rest) { this.__u = u; return oo.call(this, m, u, ...rest); };
  XMLHttpRequest.prototype.send = function (...a) {
    this.addEventListener("load", () => { try { push(this.__u, this.status, this.responseText); } catch (e) {} });
    return os.apply(this, a);
  };
  return "installed";
});

// 让页面自己走路由：改 hash 而不是 page.goto —— goto 会整页重载，SPA 起不来会一直停在「努力加载中」
const routes = ["#/exam/exam", "#/exam/exam/front/score-detail"];
for (const r of routes) {
  try { await page.evaluate((h) => { location.hash = h; }, r); } catch (e) {
    try { await page.goto("https://kc.zhixueyun.com/" + r); } catch (e2) {}      // 兜底
  }
  await page.waitForTimeout(4000);
  const txt = await page.evaluate(() => (document.body.innerText || "").slice(0, 200));
  console.log(`  路由 ${r} → ${(txt || "").replace(/\s+/g, " ").slice(0, 90) || "(空)"}`);
}

const out = await page.evaluate((stamp) => {
  const txt = (e) => (e ? (e.innerText || e.textContent || "").replace(/\s+/g, " ").trim() : "");
  const pick = (sel) => [...document.querySelectorAll(sel)].map(txt).filter((s) => s && s.length > 1);
  const dom = {
    stems: [...pick("div[data-current$=':content']"), ...pick("div[data-current$='choise:content']")],
    options: [...pick("div[data-current$=':options']"), ...pick("div[data-current$='choise:options']")],
    answerish: [...pick("[class*='answer']"), ...pick("[class*='right']"), ...pick("[class*='correct']")]
      .filter((s) => s.length < 300),
    title: document.title, url: location.href,
    bodyHead: txt(document.body).slice(0, 400),
  };
  const toks = [];
  for (let i = 0; i < localStorage.length; i++) {
    const k = localStorage.key(i), v = localStorage.getItem(k) || "";
    if (v.length > 10 && v.length < 200) toks.push({ k, v: v.replace(/^"|"$/g, "").slice(0, 80) });
  }
  return { capturedAt: new Date().toISOString(), stamp, page: location.href,
           sniffs: (window.__archive || { sniffs: [] }).sniffs, dom, tokens: toks.slice(0, 20) };
}, STAMP);

await fs.writeFile(OUT, JSON.stringify(out, null, 1), "utf8");
console.log(`抓到的响应 ${out.sniffs.length} 条 ｜ DOM 题干 ${out.dom.stems.length} 条 / 选项 ${out.dom.options.length} 条 → ${OUT}`);
for (const s of out.sniffs.slice(0, 12)) {
  console.log(`  [${s.status}] ${s.url.replace(/^https?:\/\/[^/]+/, "").slice(0, 96)}  ${s.body.replace(/\s+/g, " ").slice(0, 90)}`);
}
if (!out.dom.stems.length && !out.sniffs.length) {
  console.log("提示：这一趟什么都没抓到。若你刚考完但分数是不及格，平台本来就不回显卷子（i18n zxy.202205）；");
  console.log("      只有及格的场次才会给答题详情，那之后再跑一次这个脚本。");
}
