#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
ledger.py —— 逆向台账：把「试过什么 / 请求原文 / 原始返回 / 结论」落成一张可复算的表
=====================================================================================
  python3 ledger.py            生成 crawl/逆向台账.md + .csv
  python3 ledger.py selftest   自检（FAIL 会 exit 1）

为什么要有这个：逆向过程散在十几个 scratch 脚本 + 终端输出里，过两个月没人记得哪条试过、
结论是什么。这份台账把「假设」和「证据」绑在一起：唯一的事实来源是 crawl/evidence.json
（由 jr/tools/kc-ledger-probe.js 现场重打一遍手写落盘），本脚本只做汇总与校验，不编数据。

复现：ego-browser nodejs < jr/tools/kc-ledger-probe.js   → crawl/evidence.json
"""
import csv
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
EVID = os.path.join(HERE, "crawl/evidence.json")
OUT_MD = os.path.join(HERE, "crawl/逆向台账.md")
OUT_CSV = os.path.join(HERE, "crawl/逆向台账.csv")
PROBE = "jr/tools/kc-ledger-probe.js"

# 每条假设的结论（人工判读，写在代码里是为了可复核：改了这里，md 和 csv 一起变）
VERDICT = {
    "h01": "机制成立：机构级开关确实由服务端下发，值 = 0（关）→ 前端守卫据此把路由 replace 到 no-support",
    "h02": "同一份配置换个端点读，值一致（= 0）→ 不是单端点故障",
    "h03": "**服务端返空数组**：题库列表接口通、鉴权过，但一台题都没有 → 这才是「屋里是空的」的直接证据",
    "h04": "换 businessType 再查，仍是空 → 不是参数没给对",
    "h05": "题库目录只有文件夹名（培训名），没有题目 → 目录层存在、内容层为空",
    "h06": "另一类目录同样只有「其他」一个空壳",
    "h07": "题库文件夹接口能列出真实文件夹（如「2026年新疆公司试题」）→ 账号能看到目录，看不到里面的题",
    "h08": "题目分页返回**全 null 空壳**（字段齐全、值全空）→ 典型的「路由在、数据不给」",
    "h09": "另一个题目分页端点直接 500 → 这条路由对该机构未开",
    "h10": "start-practice 传 recordId=null 返回 []：服务端认这个请求、不报错，但没有练习记录可给",
    "h11": "打完 start-practice 后练习列表仍为空 → **无副作用**（这条排除了「我们探测动作污染了账号」的担心）",
    "h12": "题量统计对单个 guideStudyId 返 []（带逗号分隔的多个 id 才有数），说明它按块批量算",
    "h13": "考试详情可读，且带监考参数（allowExamTimes=3、duration=60）→ 考试模块本身是通的",
    "h14": "缺必填参数 → 422 errorCode 900000（平台统一「缺参」码）",
    "h15": "422 errorCode **30142**（= 参数形态对、但给的是 examId，服务端要的是别的 id）→ 与上一轮记的 30113 不一致，以本轮为准",
    "h16": "422 errorCode 30142：成绩详情要的也不是这个 examRecordId 形态 → 未通过状态下这条路走不通",
    "h17": "错题本缺 projectId → 422 900000；拿到 projectId 才有下文",
    "h18": "对照组：带上 mucompanyidheader 结果不变（200 空）→ 之前怀疑「是 org 头把路由打成 404」，证伪",
}

# 非接口侧的发现（没法用脚本复现，出处写清）
NON_API = [
    ("JS 静态", "题库路由守卫原码（simulate-practise chunk）",
     'beforeRouteEnter: (t,e,n)=>n(t=>{ruleConfig({key:"COMPANY_PRACTICE_SWITCH_CONFIG"}).then(e=>{+e.data.value||t.$router.replace("/question-bank/no-support")})})',
     "开关为 0 时前端主动跳 no-support —— 与 h01/h02 实测值 0 完全对上"),
    ("JS 静态", "i18n 状态码",
     'exam-record-status: 5→待评卷 6→及格 7→不及格 9→良好 10→优秀；zxy.202205 =「您未通过该考试，无法查看考试详情」',
     "用户那场 81 分 = status 7 = **不及格**（及格线在 81 以上）；未通过不回显答卷"),
    ("网上工具", "snchengqi/course-robot（★212，已归档，GitHub 本体只剩 LICENSE）",
     "fork（vcfee/finddream2023/csk666）里残留 src/core/exam.js：只做破切屏监听（Object.defineProperty 把 window.onblur 的 set 置空）+ 破防复制",
     "同平台最出名的工具也只是「监考侧软限制」，没有任何题库直取能力"),
    ("网上工具", "greasyfork 油猴 481461《QFZ 网大自动学习机》",
     "交卷后到 #/exam/exam/front/score-detail 抓 div[data-current='exam/exam/question/types/answer/choise:content']（题干）+ :options（选项）存 localStorage",
     "唯一给出「拿答案」可行姿势的方案，前提是**已经交卷**；对我们这场（未通过）不成立"),
    ("GitHub 调研", "题库直取类仓库检索",
     "zhixueyun / 知学云 / mylearning.cn / 题库+爬虫 等关键词全查过，无任何「知学云题库直取」项目",
     "不是我们没找到，是这类工具不存在 —— 数据不在前端，谁也没法从客户端拿"),
]

BOUNDARY = [
    ("不做", "调开考接口试探「第 4 次会不会被拒」", "调它就真的消耗一次真实考试额度（allowExamTimes=3，已用 1）"),
    ("不做", "切屏次数 / 换机换 IP / 考试次数 的绕过", "这三条是监考控制不是访问控制，绕过实质是篡改考核记录；且这是综维高危作业安全认证"),
    ("不做", "伪造不属于自己的 recordId、重放旧请求、借他人账号", "利用漏洞攻击内部系统；且平台在每个请求上都做服务端校验（h14-h17 的 422/30142 就是证据）"),
    ("已做", "只读探测（本台账全部 18 条）", "只 GET 不 POST 提交；h11 专门验证了探测动作没有产生练习记录"),
]


def load():
    if not os.path.exists(EVID):
        sys.exit("缺 %s —— 先跑：ego-browser nodejs < %s" % (EVID, PROBE))
    return json.load(open(EVID, encoding="utf-8"))


def render(d):
    rows = d["results"]
    cap = d["capturedAt"].replace("T", " ")[:16].replace("Z", "") + "Z"
    L = ["# 逆向台账 · mylearning / kc.zhixueyun.com（假设 → 请求 → 原始返回 → 结论）", "",
         "现场抓取：**%s**　|　页面 `%s`　|　token 来源 `%s`" % (cap, d["page"], d["token"]["src"]), "",
         "复现：`ego-browser nodejs < %s` → 重写 `crawl/evidence.json`，再 `python3 ledger.py` 重出本表。" % PROBE,
         "全部为只读 GET；没有一条 POST 提交，没有点过「开始考试 / 交卷」。", "",
         "## 一、接口假设（%d 条，全部现场复现）" % len(rows), "",
         "| # | 阶段 | 假设 | 请求 | 状态 | errorCode | 原始返回（前 120 字） | 结论 |",
         "|---|---|---|---|---|---|---|---|"]
    for r in rows:
        L.append("| %s | %s | %s | `%s` | %s | %s | `%s` | %s |" % (
            r["id"].upper(), r["phase"], r["hyp"], r["url"], r["status"], r["errorCode"] or "-",
            r["body"].replace("|", "\\|")[:120], VERDICT.get(r["id"], "（未写结论）")))
    L += ["", "## 二、非接口侧的发现（出处写清，脚本无法复现）", "",
          "| 类别 | 对象 | 证据原文 | 结论 |", "|---|---|---|---|"]
    for a, b, c, e in NON_API:
        L.append("| %s | %s | %s | %s |" % (a, b, str(c).replace("|", "\\|"), e))
    L += ["", "## 三、边界（哪些不做，以及为什么）", "", "| 状态 | 事项 | 理由 |", "|---|---|---|"]
    for a, b, c in BOUNDARY:
        L.append("| %s | %s | %s |" % (a, b, c))
    L += ["", "## 四、复现踩过的坑（别再踩）", "",
          "1. **token 取错 → 401/404，别误判成「接口不存在」**。`localStorage.token` 是 JSON（`access_token` 在里面，32 字符）；"
          "但同一个 storage 里还有 `businessId`、`save_cookie`、`userInfoRes` 等十来个疑似值。"
          "做法：先拿 `/api/v1/human/todo-task/count` 验证——200 的那个才是真 token（脚本已内置这段挑选逻辑）。",
          "2. **路径别写双重前缀**。`/api/v1/` + `/api/v1/exam/practice/front/list` = `/api/v1//api/v1/exam/...` → 平台回**纯文本 404**，"
          "和「接口真的不存在」长得一样。本脚本第一版就栽在这，18 条里 16 条假 404。",
          "3. **401 和 404 含义不同**：401 = 鉴权没通过（带 `errorCode:0/message:\"Access is Denied\"`）；"
          "纯文本 `404 ` = 路径没这条路由。看到 404 先查自己拼的 URL，再看平台。",
          "4. **`mucompanyidheader` 不是必须**（h18 对照：带不带结果一样），但带上也不会错 —— 别把它当成 404 的替罪羊（我怀疑过一次，被对照实验证伪）。", ""]
    L += ["## 五、这张表说明什么（一句话）", "",
          "题库路线在**服务端**关上：目录层能看到（h05/h06/h07），内容层一律空壳或空数组（h03/h04/h08/h11），"
          "开关值实测为 0（h01/h02），前端守卫据此跳转（第二节原码）。"
          "客户端能做的是「复刻调用去拿服务端愿意给的数据」，这是本台账 18 条里 12 条 200 的来历；"
          "客户端不能做的是「让服务端给出它没有的数据」。", ""]
    open(OUT_MD, "w", encoding="utf-8").write("\n".join(L) + "\n")
    with open(OUT_CSV, "w", encoding="utf-8") as fh:
        w = csv.writer(fh)
        w.writerow(["id", "阶段", "假设", "请求", "状态", "errorCode", "原始返回", "结论"])
        for r in rows:
            w.writerow([r["id"].upper(), r["phase"], r["hyp"], r["url"], r["status"], r["errorCode"],
                        r["body"], VERDICT.get(r["id"], "")])
    n_ok = sum(1 for r in rows if r["status"] == 200)
    print("台账已生成：%d 条假设（%d 条 200 / %d 条非 200）+ %d 条非接口发现 -> %s"
          % (len(rows), n_ok, len(rows) - n_ok, len(NON_API), OUT_MD))
    return rows


def selftest():
    d = load()
    rows = d["results"]
    assert len(rows) >= 15, "证据条数太少：%d" % len(rows)
    missing = [r["id"] for r in rows if r["id"] not in VERDICT]
    assert not missing, "这些假设没写结论：%s" % missing
    # 关键三条必须与证据一致（防止有人改了 md 却没改证据）
    by = {r["id"]: r for r in rows}
    assert by["h01"]["body"].find('"value":"0"') >= 0, "h01 的开关值不是 0，机制结论要重写"
    assert by["h03"]["status"] == 200 and '"recordCount":0' in by["h03"]["body"], "h03 应 200 且空"
    assert by["h11"]["status"] == 200 and '"recordCount":0' in by["h11"]["body"], "h11 无副作用验证失败"
    assert by["h15"]["errorCode"] == "30142", "h15 的 errorCode 变了（结论里的 30142 要同步）"
    assert by["h18"]["status"] == by["h03"]["status"], "h18 对照结论失效"
    out = render(d)
    assert len(out) == len(rows)
    txt = open(OUT_MD, encoding="utf-8").read()
    for sec in ("## 一、接口假设", "## 二、非接口侧的发现", "## 三、边界", "## 四、复现踩过的坑"):
        assert sec in txt, "台账缺章节：%s" % sec
    for c in ("h01".upper(), "h11".upper(), "h18".upper()):
        assert c in txt, "台账缺 %s" % c
    print("selftest OK：%d 条假设全部有结论、h01 开关=0、h03/h11 空数组、h15=30142、h18 对照成立；"
          "台账四节齐全；证据时间 %s" % (len(rows), d["capturedAt"][:16]))


if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1] == "selftest":
        selftest()
    else:
        render(load())
