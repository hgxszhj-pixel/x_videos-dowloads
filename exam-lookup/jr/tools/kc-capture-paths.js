// 抓应用自己发的请求：比对「我们猜的接口路径」和「前端真正打的路径」
// 跑法：ego-browser nodejs < jr/tools/kc-capture-paths.js
import fs from "node:fs/promises";

const task = await taskSpace(6);
const tabs = await task.tabs();
const cur = tabs.find((t) => /kc\.zhixueyun\.com/.test(t.url || "")) || tabs.find((t) => t.active) || tabs[0];
const page = cur && cur.label ? task.page(cur.label) : task.page("p1");

await page.cdp("Network.enable", {});
const routes = ["#/exam/exam/list", "#/exam/exam/exam-detail/cb390efe-2928-47dd-90d6-aab787cc7567", "#/exam/my-exam", "#/practice-wrong"];
for (const r of routes) {
  try { await page.goto("https://kc.zhixueyun.com/" + r); } catch (e) {}
  await page.waitForTimeout(3500);
}
let evs = [];
try { evs = await page.events(); } catch (e) { console.log("events() 失败:", String(e).slice(0, 80)); }
const rows = [];
for (const e of evs || []) {
  const m = e.method || (e.event && e.event.method) || "";
  const p = e.params || (e.event && e.event.params) || {};
  if (m === "Network.requestWillBeSent" && /\/api\/|zhixueyun/.test(p.request?.url || "")) {
    rows.push({ url: p.request.url, method: p.request.method, ver: (p.request.headers || {}).Version || (p.request.headers || {}).version || "" });
  }
}
const uniq = [...new Map(rows.map((r) => [r.method + " " + r.url, r])).values()];
console.log("抓到应用自己发的接口调用 " + uniq.length + " 条：");
for (const r of uniq) console.log(`  ${r.method.padEnd(4)} ver=${String(r.ver).padEnd(8)} ${r.url.replace("https://kc.zhixueyun.com", "")}`);
await fs.writeFile("/Users/hgxszhj/x_videos-dowloads/exam-lookup/crawl/_app-paths.json", JSON.stringify(uniq, null, 1), "utf8");
