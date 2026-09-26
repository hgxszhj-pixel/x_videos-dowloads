// 读只地深挖 cms.mylearning.cn 的技能认证/认证中心几个专区页：
//   1) 真浏览器渲染后 dump 全部链接（静态 fetch 抓到 0 item 的那几个页，是 JS/接口驱动的）
//   2) 记录页面自己发的接口请求 → 在页内只读 GET 重放一遍，把 JSON 存下来
// 跑法：ego-browser nodejs < jr/tools/cms-harvest.js
// 只读：全程 GET，不点考试入口、不提交任何表单
import fs from "node:fs/promises";

const OUT = "/Users/hgxszhj/x_videos-dowloads/exam-lookup/crawl/cms-render.json";
const PAGES = [
  "https://cms.mylearning.cn/safe/topic/resource/2024/skill/pc.html",
  "https://cms.mylearning.cn/safe/topic/resource/2024/skill/standard_pc.html",
  "https://cms.mylearning.cn/safe/topic/resource/2024/skill/notice_pc.html",
  "https://cms.mylearning.cn/safe/topic/resource/2026/rzcx/pc.html",
  "https://cms.mylearning.cn/safe/topic/resource/2026/znyfw2026/pc.html",
  "https://cms.mylearning.cn/safe/topic/resource/2026/kdzc/pc.html",
  "https://cms.mylearning.cn/safe/topic/resource/2025/kdzc2025/pc.html",
  "https://cms.mylearning.cn/safe/topic/resource/2025/znyfw2025/pc.html",
];

const task = await taskSpace(6);
const page = await task.newPage();
await page.cdp("Network.enable", {});
const out = [];

for (const url of PAGES) {
  let rec = { url, ok: false };
  try {
    await page.goto(url);
    await page.waitForTimeout(3000);
    rec = await page.evaluate(`(() => {
      const abs = (u) => { try { return new URL(u, location.href).href; } catch (e) { return u; } };
      const links = [...document.querySelectorAll("a[href]")].map(a => ({ t: (a.innerText||"").trim().slice(0,60), h: a.href }));
      const files = links.filter(l => /\\.(pdf|docx?|pptx?|xlsx?|zip|rar|mp4)(\\?|$)/i.test(l.h)).map(l => l.h);
      // 数据驱动的列表：把带 data-* / onclick 的元素也扫一遍，很多页用 div 做入口
      const clicky = [...document.querySelectorAll("[data-url],[data-href],[data-id],[onclick]")]
        .slice(0, 200).map(e => ({ tag: e.tagName, cls: (e.className||"").toString().slice(0,40), text: (e.innerText||"").trim().slice(0,60),
          data: JSON.stringify(e.dataset||{}).slice(0,160), onclick: (e.getAttribute("onclick")||"").slice(0,120) }));
      return { ok: true, title: document.title, text: (document.body.innerText||"").replace(/\\s+/g," ").slice(0, 3000),
        nlinks: links.length, links: links.slice(0, 400), files: [...new Set(files)], clicky: clicky.slice(0, 60) };
    })()`);
    let evs = [];
    try { evs = await page.events(); } catch (e) {}
    const calls = [];
    for (const e of evs || []) {
      const m = e.method || (e.event && e.event.method) || "";
      const p = e.params || (e.event && e.event.params) || {};
      const u = p.request?.url || "";
      if (m === "Network.requestWillBeSent" && /cms\\.mylearning\\.cn/.test(u) && !/\\.(js|css|png|jpg|jpeg|gif|svg|woff2?|ico)(\\?|$)/i.test(u)) {
        calls.push({ method: p.request.method, url: u });
      }
    }
    rec.calls = [...new Map(calls.map(c => [c.url, c])).values()].slice(0, 60);
    // 只读重放这些 GET 接口，把 JSON 抓下来（页面里带 cookie 的 fetch）
    rec.api = await page.evaluate(`(async () => {
      const urls = ${JSON.stringify(rec.calls.map(c => c.url))}.filter(u => /\\/api\\/|\\.json(\\?|$)|list|query|page|search/i.test(u));
      const out = [];
      for (const u of urls.slice(0, 25)) {
        try {
          const r = await fetch(u, { credentials: "include", headers: { Accept: "application/json, text/plain, */*" } });
          const t = await r.text();
          out.push({ url: u, status: r.status, len: t.length, head: t.slice(0, 4000) });
        } catch (e) { out.push({ url: u, error: String(e).slice(0, 120) }); }
      }
      return out;
    })()`);
  } catch (e) {
    rec.error = String(e).slice(0, 200);
  }
  out.push(rec);
  console.log("—— " + url.replace("https://cms.mylearning.cn/safe/topic/resource/", "") + " ——");
  console.log("   title:", rec.title || rec.error || "?", "｜渲染后文本", (rec.text || "").length, "字｜链接", rec.nlinks || 0, "｜文件链接", (rec.files || []).length, "｜接口", (rec.calls || []).length, "｜存下 JSON", (rec.api || []).length);
  const texts = (rec.text || "").slice(0, 160).replace(/\s+/g, " ");
  if (texts) console.log("   正文:", texts);
  for (const f of (rec.files || []).slice(0, 5)) console.log("   文件:", f.slice(0, 130));
  for (const c of (rec.calls || []).slice(0, 8)) console.log("   接口:", c.method, c.url.slice(0, 130));
}
await fs.writeFile(OUT, JSON.stringify(out, null, 1), "utf8");
console.log("\n写入", OUT);
await page.close();
