// 中国保密在线（baomi.org.cn）：把本人历史考试记录整卷拉下来存档
// ---------------------------------------------------------------------------
// 跑法：ego-browser nodejs < jr/tools/baomi-pull-records.js
// 产物：jr/out/baomi/examRecord.json（原始返回，供 parse_baomi_records.py 解析）
//
// 为什么是这条接口：/portal/main-api/v2/activity/exam/examRecord.do 是**本人考试记录**接口，
// 每条记录的 content 字段就是那场整卷（题干/选项/官方答案/我当时的作答/对错）。
// 只用自己考过的卷子；不调 getExamContentData.do（那是「开始考试」后才拿得到的题），
// 不点「进入考试」、不交卷、不消耗次数、不改任何计数。
//
// 身份：请求头带 token（= localStorage.token），不带也能通 cookies，但带上更稳。
import fs from "node:fs/promises";

const ROOT = "/Users/hgxszhj/x_videos-dowloads/exam-lookup";
const OUT = `${ROOT}/jr/out/baomi/examRecord.json`;

const task = await taskSpace(6);
const tabs = await task.tabs();
const tab = tabs.find((t) => /baomi\.org\.cn/.test(t.url || "")) || tabs[tabs.length - 1];
if (!/baomi\.org\.cn/.test(tab.url || "")) {
  console.log("当前标签不是 baomi.org.cn：", tab.url);
  console.log("先在 ego 里打开 https://www.baomi.org.cn 并登录，再跑本脚本。");
  process.exit(1);
}
const page = task.page(tab.label);

const out = await page.evaluate(async () => {
  const U = "/portal/main-api/v2/activity/exam/examRecord.do?pageNum=1&pageSize=50";
  const tries = [];
  for (const h of [{ token: localStorage.getItem("token") || "" }, {}]) {
    try {
      const r = await fetch(U, { credentials: "include", headers: h });
      const b = await r.text();
      tries.push({ h: Object.keys(h).join("+") || "无头", s: r.status, n: b.length, ok: /recordList/.test(b) });
      if (r.status === 200 && /recordList/.test(b)) return { body: b, tries };
    } catch (e) { tries.push({ h: "err", s: 0, e: String(e).slice(0, 60) }); }
  }
  return { body: "", tries };
});

if (!out.body) {
  console.log("拉取失败：", JSON.stringify(out.tries));
  console.log("多半是没登录（token 过期）→ 在 ego 里重新登录 baomi.org.cn 再跑。");
  process.exit(1);
}
await fs.mkdir(`${ROOT}/jr/out/baomi`, { recursive: true });
await fs.writeFile(OUT, out.body, "utf8");
const d = JSON.parse(out.body);
const recs = (d.data && d.data.recordList) || [];
console.log(`记录 ${recs.length} 条 → ${OUT}`);
for (const r of recs) {
  let n = 0;
  try { const c = JSON.parse(r.content); n = (c.typeList || []).reduce((a, t) => a + (t.questionList || []).length, 0); } catch (e) {}
  console.log(`  ${r.exam_name || "(无名)"} ｜ ${new Date(Number(r.create_date) || 0).toISOString().slice(0, 10)} ｜ 得分 ${r.score} ｜ ${n} 题`);
}
console.log("下一步：python3 parse_baomi_records.py selftest");
