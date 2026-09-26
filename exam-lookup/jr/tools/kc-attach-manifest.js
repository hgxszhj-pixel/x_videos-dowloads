// 把所有已采集专题（crawl/subjects/*.json）里的附件 id 解析成可下载直链，产出 crawl/attach-manifest-new.json
// 形状与 crawl/attach-manifest.json 一致，直接喂 jr/tools/kc-download-attach.py <manifest>
// 只读 GET（preview 是元数据查询，不写平台任何东西）
import fs from "node:fs/promises";

const DIR = "/Users/hgxszhj/x_videos-dowloads/exam-lookup/crawl/subjects";
const OUT = "/Users/hgxszhj/x_videos-dowloads/exam-lookup/crawl/attach-manifest-new.json";
const files = (await fs.readdir(DIR)).filter((f) => f.endsWith(".json"));

const task = await taskSpace(6);
const page = await task.newPage();
await page.goto("https://kc.zhixueyun.com/#/home-v?id=128558");
await page.waitForTimeout(3500);

const targets = [];
for (const f of files) {
  const d = JSON.parse(await fs.readFile(`${DIR}/${f}`, "utf8"));
  const tag = d.tag || f.replace(/\.json$/, "");
  for (const ch of d.chapters || []) for (const s of ch.sections || []) {
    if (!s.attachmentId) continue;
    targets.push({ subject: tag, name: s.name || s.attachmentId, sectionName: s.name || "", chapter: ch.chapter || "",
                   sectionType: s.sectionType, attachmentId: s.attachmentId, isVideo: s.sectionType === 2 });
  }
}
console.log("待解析附件 " + targets.length + " 个（跨 " + files.length + " 个专题）");

// 已解析过的（cache）直接复用，不再打接口：这样重跑只补新专题，也避免一次 evaluate 塞 600 个 id
let cached = [];
try { cached = JSON.parse(await fs.readFile(OUT, "utf8")); } catch (e) {}
const cacheById = new Map(cached.filter((x) => x.url && x.attachmentId).map((x) => [x.attachmentId, x]));
const todo = targets.filter((t) => !cacheById.has(t.attachmentId));
console.log("缓存里已有 " + cacheById.size + " 个直链，本次只解析 " + todo.length + " 个");
const resolved = [...cached.filter((x) => x.url && cacheById.has(x.attachmentId))];
const BATCH = 40;                       // 一次 evaluate 塞 40 个：600 个一起塞会撞 15s 的页面求值超时（实测踩到）
let done = 0;
for (let i = 0; i < todo.length; i += BATCH) {
  const batch = todo.slice(i, i + BATCH);
  const part = await page.evaluate(`(async () => {
  const raw = localStorage.getItem("token") || ""; let tk = raw;
  try { const t = JSON.parse(raw); tk = t.access_token || t.accessToken || ""; } catch (e) {}
  const H = { version: "12.1.1", authorization: "Bearer__" + tk, mucompanyidheader: "f32e65d0-fe3b-40d3-a025-4480a1808746" };
  const list = ${JSON.stringify(batch)};
  const out = [];
  for (const it of list) {
    try {
      const r = await fetch("/api/v1/tools-center-v2/file-cloud/preview?id=" + it.attachmentId, { credentials: "include", headers: H });
      const t = await r.text();
      let j = null; try { j = JSON.parse(t); } catch (e) {}
      if (j && j.url) out.push(Object.assign({}, it, { url: "https://kc.zhixueyun.com" + j.url, size: j.size,
        extention: j.extention, filename: j.filename, translateFlag: j.translateFlag }));
      else out.push(Object.assign({}, it, { err: "preview " + r.status + " " + t.slice(0, 80) }));
    } catch (e) { out.push(Object.assign({}, it, { err: String(e).slice(0, 80) })); }
  }
  return out;
})()`);
  resolved.push(...part);
  console.log("   批次 " + (done + part.length) + "/" + todo.length + "（成功 " + part.filter((x) => x.url).length + "）");
  done += part.length;
}

const good = resolved.filter((x) => x.url);
const bad = resolved.filter((x) => !x.url);
await fs.writeFile(OUT, JSON.stringify(good, null, 1), "utf8");
console.log("解析成功 " + good.length + " 个（直链可裸下），失败 " + bad.length + " 个 → " + OUT);
const byTag = {};
for (const g of good) { byTag[g.subject] = byTag[g.subject] || { n: 0, mb: 0 }; byTag[g.subject].n++; byTag[g.subject].mb += (g.size || 0) / 1048576; }
for (const [k, v] of Object.entries(byTag)) console.log("   " + k.padEnd(24) + v.n + " 个 ｜ " + v.mb.toFixed(1) + " MB");
for (const b of bad.slice(0, 5)) console.log("   × " + b.subject + " " + (b.name || "").slice(0, 40) + " → " + b.err);
await page.close();
