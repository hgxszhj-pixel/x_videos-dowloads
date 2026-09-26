// 逆向台账复现脚本：先「动态挑一个能用的 token」（拿校验接口验过再用），再打整张只读接口表
// 全部只读 GET，不碰开考/交卷/提交。
// 跑法：ego-browser nodejs < jr/tools/kc-ledger-probe.js
// 产物：crawl/evidence.json（每条含 id/假设/请求/状态/errorCode/原始返回前 220 字 + 选中的 token 来源）
//
// 为什么要「动态挑 token」：localStorage 里有好几个疑似 token（token / businessId / save_cookie…），
// 形状也不一样（JSON 对象里的 access_token、裸串）。挑错一个，平台会把 exam 路由回 401/404，
// 让人误判成「接口不存在」。唯一可靠做法：拿一个有鉴权的校验接口试，200 的那个才是真 token。
import fs from "node:fs/promises";

const ROOT = "/Users/hgxszhj/x_videos-dowloads/exam-lookup";
const ORG = "f32e65d0-fe3b-40d3-a025-4480a1808746";
const EXAM = "cb390efe-2928-47dd-90d6-aab787cc7567";          // 用户那场综装维高危作业安全生产考试
const RECID = "40e5247d-1747-4448-9515-ed11f1295750";         // 那场的答卷记录 id
const GSID = "e2d02e3c-7b55-42ac-ae37-a8bbe7782565";          // 真 guideStudyId（杆上作业规范）
const E = "exam/";   // 相对 /api/v1/ 的路径，循环里统一加前缀。
// 坑 3：这条要是写成 "/api/v1/exam/"，下面再拼一次 "/api/v1/" 就变成 /api/v1//api/v1/exam/... → 平台回纯文本 404，
// 看起来像「接口不存在」，其实是自己把路径写重了。

const BATTERY = [
  ["h01", "A 开关", "题库模块被前端路由守卫挡住，守卫读服务端开关", `system/rule-config/key?key=COMPANY_PRACTICE_SWITCH_CONFIG&organizationId=${ORG}`],
  ["h02", "A 开关", "另一个读开关的端点（getkey）返回同一个值", `system/rule-config/getkey?key=COMPANY_PRACTICE_SWITCH_CONFIG&organizationId=${ORG}`],
  ["h03", "B 数据", "题库列表（随机练习 businessType=0）有没有数据", `${E}practice/front/list?page=1&pageSize=10&businessType=0`],
  ["h04", "B 数据", "题库列表（businessType=1）有没有数据", `${E}practice/front/list?page=1&pageSize=10&businessType=1`],
  ["h05", "B 数据", "题库目录（type=1）", `${E}practice/front/depot-list?type=1`],
  ["h06", "B 数据", "题库目录（type=2）", `${E}practice/front/depot-list?type=2`],
  ["h07", "B 数据", "题库文件夹清单", `${E}question-depot`],
  ["h08", "B 数据", "题目分页（缺 depotId）", `${E}question/front/page?page=1&pageSize=10`],
  ["h09", "B 数据", "临时题分页", `${E}temp-question/front/page?guideStudyId=${GSID}&page=1&pageSize=10`],
  ["h10", "C 副作用", "start-practice（recordId=null）", `${E}temp-question/start-practice?guideStudyId=${GSID}&recordId=null`],
  ["h11", "C 副作用", "打过 start-practice 后练习列表是否仍为空（无副作用验证）", `${E}practice/front/list?page=1&pageSize=10&businessType=0`],
  ["h12", "D 有数据", "题量统计（唯一稳定给真数据的接口）", `${E}temp-question/answer-statistics?guideStudyIds=${GSID}`],
  ["h13", "E 答卷", "考试详情（证明 exam 模块可用）", `${E}exam/front/find-by-ids?ids=${EXAM}`],
  ["h14", "E 答卷", "按 examId 查答卷记录", `${E}exam-record/front/page?examId=${EXAM}`],
  ["h15", "E 答卷", "按 examId 取答卷答案信息", `${E}exam/front/get-exam-answer-info/${EXAM}`],
  ["h16", "E 答卷", "按 examRecordId 取成绩详情答题卡", `${E}exam/front/score-detail?examRecordId=${RECID}`],
  ["h17", "F 错题", "错题本（缺 projectId）", `${E}wrong-question/list?page=1&pageSize=10`],
  ["h18", "Z 对照", "对照：同一条题库列表 URL 带上 mucompanyidheader", `${E}practice/front/list?page=1&pageSize=10&businessType=0`],
];

const task = await taskSpace(6);
const tabs = await task.tabs();
const cur = tabs.find((t) => /kc\.zhixueyun\.com/.test(t.url || "")) || tabs.find((t) => t.active) || tabs[0];
const page = cur && cur.label ? task.page(cur.label) : task.page("p1");
try { await page.goto("https://kc.zhixueyun.com/#/practice-wrong"); } catch (e) {}
await page.waitForTimeout(3000);
console.log("页面:", await page.url(), "| 标签:", cur && cur.label);

const out = await page.evaluate(
  async ({ battery, org }) => {
    const cands = [];
    for (let i = 0; i < localStorage.length; i++) {
      const k = localStorage.key(i), v = localStorage.getItem(k) || "";
      try {
        const j = JSON.parse(v);
        const t = j.access_token || j.accessToken || j.token || "";
        if (typeof t === "string" && t.length > 10) cands.push([k + "→access_token", t]);
      } catch (e) {
        if (v.length > 10 && v.length < 200) cands.push([k + "(裸串)", v.replace(/^"|"$/g, "")]);
      }
    }
    const H_of = (tk, withOrg) => ({ Authorization: "Bearer__" + tk, Version: "12.1.1",
      "X-Requested-With": "XMLHttpRequest", Accept: "application/json, text/plain, */*",
      ...(withOrg ? { mucompanyidheader: org } : {}) });
    let picked = null;
    const tried = [];
    for (const [src, tk] of cands.slice(0, 20)) {
      for (const withOrg of [false, true]) {
        try {
          const r = await fetch("/api/v1/human/todo-task/count", { credentials: "include", headers: H_of(tk, withOrg) });
          const t = await r.text();
          tried.push({ src, withOrg, s: r.status, b: t.replace(/\s+/g, " ").slice(0, 60) });
          if (r.status === 200 && /recordCount|count/i.test(t)) { picked = { src, tk, withOrg }; break; }
        } catch (e) { tried.push({ src, withOrg, s: 0, err: String(e).slice(0, 50) }); }
      }
      if (picked) break;
    }
    if (!picked) return { error: "没找到能用的 token", tried };
    const H = H_of(picked.tk, picked.withOrg);

    const res = [];
    for (const [id, phase, hyp, path] of battery) {
      const url = "/api/v1/" + path;
      const hh = id === "h18" ? { ...H, mucompanyidheader: org } : H;
      try {
        const r = await fetch(url, { credentials: "include", headers: hh });
        const t = await r.text();
        const m = t.match(/"(?:errorCode|code)"\s*:\s*"?(\d+)"?/);
        res.push({ id, phase, hyp, url, status: r.status, errorCode: m ? m[1] : "",
                   body: t.replace(/\s+/g, " ").slice(0, 220) });
      } catch (e) {
        res.push({ id, phase, hyp, url, status: 0, errorCode: "", body: "", err: String(e).slice(0, 80) });
      }
    }
    return { capturedAt: new Date().toISOString(), page: location.href, token: picked,
             tokenCandidates: tried, results: res };
  },
  { battery: BATTERY, org: ORG },
);

if (out.error) { console.log("FAIL:", out.error); console.log(JSON.stringify(out.tried, null, 1)); process.exit(1); }
await fs.writeFile(`${ROOT}/crawl/evidence.json`, JSON.stringify(out, null, 1), "utf8");
console.log(`token: ${out.token.src}（带 org 头=${out.token.withOrg}）｜ 抓 ${out.results.length} 条 -> crawl/evidence.json`);
for (const r of out.results) {
  console.log(`  [${String(r.status).padEnd(3)}] ${r.id.padEnd(4)} ${r.errorCode ? "err" + r.errorCode : "      "} ${r.body.slice(0, 118)}`);
}
