// 深度爬 cms.mylearning.cn 的技能认证专区：BFS 抓所有专区页 + 汇总文档/课程链接
import fs from "node:fs/promises";

const task = await taskSpace(6);
const tabs = await task.tabs();
const cur = tabs.find((t) => /mylearning\.cn/.test(t.url || "")) || tabs.find((t) => t.active);
const page = cur && cur.label ? task.page(cur.label) : task.page("p1");

const seeds = [
  "https://cms.mylearning.cn/safe/topic/resource/2024/skill/pc.html",
  "https://cms.mylearning.cn/safe/topic/resource/2022/ywgcs/pc.html",
];
const MAX = 80;

const result = await page.evaluate(
  async ({ seeds, MAX }) => {
    const seen = new Set();
    const out = [];
    const queue = [...seeds];
    const isCms = (u) => /^https:\/\/cms\.mylearning\.cn\/safe\/topic\//.test(u);
    while (queue.length && out.length < MAX) {
      const batch = queue.splice(0, 4);
      const got = await Promise.all(
        batch.map(async (u) => {
          if (seen.has(u)) return null;
          seen.add(u);
          try {
            const r = await fetch(u, { credentials: "include" });
            const html = await r.text();
            const doc = new DOMParser().parseFromString(html, "text/html");
            const links = [...doc.querySelectorAll("a[href]")].map((a) => a.href);
            const files = [...new Set(links.filter((h) => /\.(pdf|docx?|pptx?|xlsx?|zip|mp4)(\?|$)/i.test(h)))];
            const cms = [...new Set(links.filter(isCms))];
            const kc = [...new Set(links.filter((h) => /kc\.zhixueyun\.com/.test(h)))];
            return {
              url: u,
              http: r.status,
              title: (doc.title || "").trim(),
              text: (doc.body.innerText || "").replace(/\s+/g, " ").slice(0, 400),
              files,
              cms,
              kc,
            };
          } catch (e) {
            return { url: u, err: String(e).slice(0, 100) };
          }
        }),
      );
      for (const g of got) {
        if (!g) continue;
        out.push(g);
        for (const n of g.cms || []) if (!seen.has(n)) queue.push(n);
      }
    }
    return out;
  },
  { seeds, MAX },
);

const file = "/Users/hgxszhj/x_videos-dowloads/exam-lookup/crawl/cms_pages.json";
await fs.mkdir("/Users/hgxszhj/x_videos-dowloads/exam-lookup/crawl", { recursive: true });
await fs.writeFile(file, JSON.stringify(result, null, 1), "utf8");

const fileLinks = result.flatMap((p) => (p.files || []).map((f) => ({ page: p.url, f })));
const kcLinks = [...new Set(result.flatMap((p) => p.kc || []))];
console.log(JSON.stringify({ pages: result.length, fileLinks: fileLinks.length, kcLinks: kcLinks.length, saved: file }, null, 1));
console.log("=== 页面标题 ===");
console.log(result.map((p) => (p.title || p.url).slice(0, 46)).join(" | ").slice(0, 1600));
console.log("=== 前 15 个文档链接 ===");
console.log(JSON.stringify(fileLinks.slice(0, 15), null, 1).slice(0, 1500));
console.log("=== 前 12 个课程/专区链接 ===");
console.log(kcLinks.slice(0, 12).join("\n"));
