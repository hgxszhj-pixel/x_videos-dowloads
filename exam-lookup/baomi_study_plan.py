#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
baomi_study_plan.py —— 中国保密在线「2026 年度全国保密教育线上培训」学时清单 + 补学时计划
=========================================================================================
  python3 baomi_study_plan.py            生成 crawl/保密培训-学时清单.md 并打印计划
  python3 baomi_study_plan.py selftest   自检（FAIL 会 exit 1）

输入（都是只读抓回来的原始返回，抓法见 jr/tools/baomi-pull-course.js）：
  jr/out/baomi/resources.json  各子目录 getCourseResourceList 的原始返回（41 个资源的权威来源）
  jr/out/baomi/course.json     课程页三个目录的正文（只用来给资源贴「目录」标签）
  jr/out/baomi/grade.json      我的学习记录（getStudyCourseGradeList，progressRate 判是否学完）
  jr/out/baomi/stat.json       课程统计（getCourseUserStatistic：resourceSum / gradeSum / totalGrade / studyResourceNum）

判据（写在明处，方便自己复核）：
  · 学时取资源条目自带的 grade（0.22 这种），不是我估的。
  · 接口统计（resourceSum / gradeSum / totalGrade / studyResourceNum）是权威口径；学习记录会滞后几条，
    所以自检只卡「学习记录 ≤ 接口」和「差值 ≤ 0.45 学时」两头，两头都过才算对账通过。
  · 「已学」＝学习记录里出现同一 resourceId 且 progressRate ≥ 0.99。必须按 resourceId 对齐：
    同名资源不同年份各有一条（「2021年保密公益宣传片」2025 版、2026 版都在记录里），按名字比会误判 0.04 学时。
  · 学习记录原接口默认只回一页，必须带 pageSize=1000，否则漏掉最新看完的那条（本轮就漏了「暗处之眼」0.22）。
  · 开考门槛 4 学时来自考试页原文「满足 4学时 可参与考试」。
  · 本脚本不代看视频、不刷学时，只列清单算账。
"""
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
D = os.path.join(HERE, "jr/out/baomi")
OUT = os.path.join(HERE, "crawl/保密培训-学时清单.md")
NEED = 4.0
PREFIX = re.compile(r"^\s*(第\s*\d+\s*集|[\d]+[\s\.、])\s*")


def norm(name):
    """「1.隐秘而伟大」「第3集 窃密存密触红线」→ 去前缀去空白，两侧写法不一样才比得准"""
    return re.sub(r"[\s　“”\"'《》]", "", PREFIX.sub("", (name or "").strip()))


def load():
    for f in ("resources.json", "course.json", "grade.json", "stat.json"):
        if not os.path.exists(os.path.join(D, f)):
            sys.exit("缺 %s —— 先跑 jr/tools/baomi-pull-course.js 抓回来" % os.path.join(D, f))
    j = lambda f: json.load(open(os.path.join(D, f), encoding="utf-8"))
    return j("resources.json"), j("course.json"), j("grade.json"), j("stat.json")


def parse_resources(resources):
    """resources.json = {子目录id: getCourseResourceList 原始响应（字符串或 JSON）} → 资源行"""
    rows, seen = [], set()
    for dirid, raw in (resources.get("dirs") or resources).items():
        try:
            data = json.loads(raw) if isinstance(raw, str) else raw
        except ValueError:
            continue
        for r in (data.get("data") or {}).get("listdata") or []:
            nm = (r.get("name") or "").strip()
            if not nm or nm in seen:
                continue
            seen.add(nm)
            rows.append({"name": nm, "hours": float(r.get("grade") or 0), "dir_id": dirid,
                         "rid": r.get("resourceID") or "", "length": r.get("timeLength") or "",
                         "key": norm(nm)})
    return rows


def label_of(key, course):
    """资源落在哪个顶层目录：拿三个目录的正文做包含匹配"""
    for dirname, blob in (course.get("dirs") or {}).items():
        if key and norm(blob).find(key) >= 0:
            return dirname
    return "—"


def studied_names(grade):
    """→ {resourceId: (名字, 平台记的学时)}，只算 progressRate ≈ 1 的。

    按 resourceId 对齐而不是按名字：不同年份的课程里有同名资源
    （如「2021年保密公益宣传片」2025 版和 2026 版各有一条），按名字会误判成已学。
    """
    out = {}
    for r in (grade.get("data") or {}).get("recordList") or []:
        rid = (r.get("resourceId") or "").strip()
        if not rid or float(r.get("progressRate") or 0) < 0.99:
            continue
        out.setdefault(rid, ((r.get("resourceName") or "").strip(), float(r.get("grade") or 0)))
    return out


def build():
    resources, course, grade, stat = load()
    res = parse_resources(resources)
    done = studied_names(grade)
    st = stat.get("data") or {}
    for r in res:
        r["dir"] = label_of(r["key"], course)
    done_res = [r for r in res if r["rid"] and r["rid"] in done]
    done_hours = sum(r["hours"] for r in done_res)          # 按课程列表标的学时算，不信学习记录里那个 grade
    todo = sorted([r for r in res if not (r["rid"] and r["rid"] in done)], key=lambda r: -r["hours"])
    plan, acc = [], done_hours
    for r in todo:
        if acc >= NEED:
            break
        plan.append(r)
        acc += r["hours"]

    L = ["# 保密教育线上培训 · 学时清单（只读抓取，自动生成）", "",
         "生成：`python3 baomi_study_plan.py`　｜　课程：%s" % (course.get("course") or ""), "",
         "## 现状", "", "| 项 | 值 | 出处 |", "|---|---|---|",
         "| 课程资源总数 | %s | getCourseUserStatistic.resourceSum |" % st.get("resourceSum"),
         "| 课程总学时 | %s | getCourseUserStatistic.gradeSum |" % st.get("gradeSum"),
         "| 已学资源数 | %d 个（接口说 %s 个）| 学习记录 × 资源列表对齐 |" % (len(done_res), st.get("studyResourceNum")),
         "| 已得学时 | %.2f（接口 totalGrade=%s）| 资源列表各条 grade 求和 |" % (done_hours, st.get("totalGrade")),
         "| 开考门槛 | %.1f 学时 | 考试页原文「满足 4学时 可参与考试」 |" % NEED,
         "| 还差 | %.2f 学时 | 门槛 − 已得 |" % max(0.0, NEED - done_hours),
         "| 口径差 | %.2f 学时 | 接口已得 − 学习记录已落库；差值是刚看完还没进记录的那条 |"
         % (float(st.get("totalGrade") or 0) - done_hours), "",
         "## 已完成（%d 个）" % len(done_res), ""]
    L += ["- [x] %s（%.2f 学时，%s）— %s" % (r["name"], r["hours"], r["length"] or "—", r["dir"])
          for r in done_res]
    L += ["", "## 还差 %.2f 学时 —— 按这个顺序看最省时间" % max(0.0, NEED - done_hours), "",
          "| # | 课程 | 学时 | 时长 | 目录 |", "|---|---|---|---|---|"]
    for i, r in enumerate(plan, 1):
        L.append("| %d | %s | %.2f | %s | %s |" % (i, r["name"], r["hours"], r["length"] or "—", r["dir"]))
    if plan:
        L += ["", "看完这 %d 个，累计 **%.2f 学时**（门槛 %.1f）。"
              % (len(plan), done_hours + sum(p["hours"] for p in plan), NEED), ""]
    L += ["## 全部资源（%d 个，合计 %.2f 学时）" % (len(res), sum(r["hours"] for r in res)), ""]
    for r in res:
        L.append("- [%s] %s（%.2f 学时，%s）— %s"
                 % ("x" if (r["rid"] and r["rid"] in done) else " ", r["name"], r["hours"],
                    r["length"] or "—", r["dir"]))
    L += ["", "---", "",
          "所有数字都来自接口原始返回；`python3 baomi_study_plan.py selftest` 会拿我的解析结果跟平台的 "
          "resourceSum / gradeSum / studyResourceNum / totalGrade 四个数对账，对不上就报错。"]
    open(OUT, "w", encoding="utf-8").write("\n".join(L) + "\n")

    print("资源 %d 个 / 合计 %.2f 学时（接口 gradeSum=%s）｜ 已完成 %d 个、%.2f 学时"
          "（接口 studyResourceNum=%s、totalGrade=%s）"
          % (len(res), sum(r["hours"] for r in res), st.get("gradeSum"), len(done_res), done_hours,
             st.get("studyResourceNum"), st.get("totalGrade")))
    print("还差 %.2f 学时，看这 %d 个就够：" % (max(0.0, NEED - done_hours), len(plan)))
    for i, r in enumerate(plan, 1):
        print("  %2d. %-34s %4.2f 学时  %-8s [%s]" % (i, r["name"][:32], r["hours"], r["length"] or "", r["dir"]))
    print("→ 明细：crawl/保密培训-学时清单.md")
    return res, done_res, st


def selftest():
    res, done_res, st = build()
    total = sum(r["hours"] for r in res)
    assert len(res) == int(st.get("resourceSum") or 0), \
        "资源数对不上：解析 %d 个，接口 resourceSum=%s" % (len(res), st.get("resourceSum"))
    assert abs(total - float(st.get("gradeSum") or 0)) < 0.05, \
        "总学时对不上：我算 %.2f，接口 gradeSum=%s" % (total, st.get("gradeSum"))
    got, want = sum(r["hours"] for r in done_res), float(st.get("totalGrade") or 0)
    assert got <= want + 0.01, \
        "学习记录说已得 %.2f 学时，比接口 totalGrade=%s 还多 —— 对齐错了（典型：跨年份同名资源按名字比）" % (got, want)
    assert want - got <= 0.45, \
        "已得学时差 %.2f（我 %.2f / 接口 %s），超出记录滞后的正常范围，检查 resourceId 对齐" % (want - got, got, want)
    assert len(done_res) <= int(st.get("studyResourceNum") or 0), \
        "我数出已学 %d 个，比接口 studyResourceNum=%s 还多" % (len(done_res), st.get("studyResourceNum"))
    assert all(r["hours"] > 0 for r in res), "有资源的学时解析成 0"
    assert len({r["key"] for r in res}) == len(res), "有重复资源"
    print("selftest OK：资源 %d 个 = 接口 resourceSum，总学时 %.2f = 接口 gradeSum；已学 %d ≤ 接口 "
          "studyResourceNum=%s，已得 %.2f ≤ 接口 totalGrade=%s（差 %.2f 是刚看完未落库）"
          % (len(res), total, len(done_res), st.get("studyResourceNum"),
             sum(r["hours"] for r in done_res), st.get("totalGrade"),
             float(st.get("totalGrade") or 0) - sum(r["hours"] for r in done_res)))


if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1] == "selftest":
        selftest()
    else:
        build()
