#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""answer_chain —— 四级兜底链：题库 → 原文直证 → 多引擎 → Claude Code

为什么要分级：每一级的「可信度」和「成本」都不一样，混在一起就说不清了。

    ① bank      本地题库命中               可信：高   成本：0        有原文/人工核验
    ② verbatim  原件里的「X 是 核心」直证   可信：高   成本：~0.3s    排序/填空题专用
    ③ web       ≥2 个引擎一致             可信：中   成本：3–8s     依赖搜索引擎不被限流
    ④ llm       Claude Code 读证据包作答   可信：中低 成本：30–90s    模型判断，必须人工确认

约定：
- 只有 ①② 算「已验证答案」，可直接用于刷题/复习；
- ③④ 只是线索，进待审（pending.jsonl）与「收编」页，绝不自动写库；
- 任一级给出答案即停止；每级失败都记在 tiers 里，便于事后核对为什么没答出来。
"""
import re
import sys
import time

import ask_ctx
import llm_claude
import solve


def chain(q, web=False, llm=False, budget=180, verbose=False):
    """返回 dict：{answer, tier, source, evidence, confidence, tiers[], single, seconds}"""
    t0 = time.time()
    out = {"answer": None, "tier": "none", "source": "", "evidence": "",
           "confidence": "", "tiers": [], "single": None, "note": ""}
    if not (q or "").strip():
        out["note"] = "题目为空"
        return out

    # ① 题库
    a = solve.ask(q, do_web=False, verbose=False)
    row_hit = a.get("answer")
    out["tiers"].append({"tier": "bank", "ok": bool(row_hit),
                         "detail": (row_hit or "未命中")})
    if row_hit:
        out.update(answer=row_hit, tier="bank", source=a.get("source", ""),
                   evidence=(a.get("evidence") or [{}])[0].get("quote", ""),
                   confidence="高", seconds=round(time.time() - t0, 2))
        return out

    # ② 原文直证（排序/填空题的决定性一步，免费）
    try:
        v, items = ask_ctx.verbatim_answer(q)
    except Exception as e:                                  # 原件缺失等，不拖垮链路
        v, items = None, []
        out["tiers"].append({"tier": "verbatim", "ok": False, "detail": "异常：%s" % e})
    else:
        out["tiers"].append({"tier": "verbatim", "ok": bool(v),
                             "detail": v or "原件里凑不齐空位映射"})
    if v:
        out.update(answer=v, tier="verbatim", source="原件直证",
                   evidence=(items[0]["quote"] if items else ""),
                   confidence="高", seconds=round(time.time() - t0, 2))
        return out

    # ③ 多引擎一致
    if web:
        w = a.get("web") or solve.ask(q, do_web=True, verbose=False).get("web") or {}
        out["tiers"].append({"tier": "web", "ok": bool(w.get("answer")),
                             "detail": "%s｜摘要 %s 条｜%s" % (
                                 w.get("answer") or w.get("note") or "无定论",
                                 (w.get("search") or {}).get("snippets"),
                                 "/".join(w.get("sources") or []) or "无来源")})
        out["single"] = w.get("single")
        if w.get("answer"):
            out.update(answer=w["answer"], tier="web", source=w.get("source", "多引擎一致"),
                       evidence=w.get("quote") or "", confidence="中",
                       seconds=round(time.time() - t0, 2))
            return out

    # ④ Claude Code 读证据作答（慢、走订阅额度，只作最后一跳）
    if llm:
        if time.time() - t0 > budget:
            out["tiers"].append({"tier": "llm", "ok": False, "detail": "超出预算 %ss，未调用" % budget})
        else:
            try:
                ctx = ask_ctx.render(ask_ctx.pack(q))
            except Exception as e:
                ctx = "（证据包生成失败：%s）\n题目：%s" % (e, q)
            ctx += "\n\n【联网摘要】\n" + _snippets_text(a.get("web") or {})
            text, err = llm_claude.ask_claude(ctx, timeout=max(30, int(budget - (time.time() - t0))))
            ans, ev, conf = llm_claude.parse(text)
            out["tiers"].append({"tier": "llm", "ok": bool(ans),
                                 "detail": err or (ans or "证据不足")})
            if ans:
                out.update(answer=ans, tier="llm", source="claude", evidence=ev,
                           confidence=conf or "中", seconds=round(time.time() - t0, 2))
                return out
            if err:
                out["note"] = err
    out["seconds"] = round(time.time() - t0, 2)
    return out


def _snippets_text(web):
    sn = web.get("snippets") or []
    if not sn:
        return "（本轮搜索引擎没有取到摘要%s）" % (
            "：" + "/".join(web.get("engines_resting") or []) + " 冷却中"
            if (web.get("search") or {}).get("engines_resting") else "")
    L = []
    for i, s in enumerate(sn[:8], 1):
        L.append("[%d] %s｜%s｜%s" % (i, s.get("engine", ""), s.get("title", ""), (s.get("text") or "")[:300]))
    return "\n".join(L)


def selftest():
    """不联网、不调模型：验证分级短路与降级。"""
    q_sort = ("云改数转智惠战略，把握（）是核心、（）是动力、（）是根本、（）是基础、"
              "（）是底色、（）是枢纽的内涵要义。（排序题）A. 云网融合 B. 改革开放 "
              "C. 科技创新 D. 干部人才 E. 数智化平台 F. 绿色安全")
    r = chain(q_sort)
    tiers = [t["tier"] for t in r["tiers"]]
    assert r["tier"] == "verbatim" and r["answer"] == "CBDAFE", r
    assert tiers[0] == "bank" and tiers[1] == "verbatim", tiers

    # 题库命中必须短路在①，不能白跑后三级
    r2 = chain("有限空间作业前应通风多长时间？A. 5分钟 B. 10分钟 C. 20分钟 D. 30分钟")
    assert r2["tier"] == "bank" and len(r2["tiers"]) == 1, r2

    # 归零短路：假题目、不联网、不调模型 → tier=none，且不抛异常
    r3 = chain("zzzz 这道题不存在 zzzz？A. 甲 B. 乙 C. 丙 D. 丁", web=False, llm=False)
    assert r3["tier"] in ("none", "verbatim"), r3
    assert r3["answer"] is None or isinstance(r3["answer"], str)

    # 模型答案必须走解析（含「证据不足」不当作答案）
    assert llm_claude.parse("答案：证据不足\n依据：缺材料\n可信度：低")[0] is None
    print("answer_chain selftest 通过：题库短路 / 原文直证 CBDAFE / 无解降级 / 解析拒绝「证据不足」")
    return 0


if __name__ == "__main__":
    if "--selftest" in sys.argv:
        sys.exit(selftest())
    args = [x for x in sys.argv[1:] if not x.startswith("-")]
    if not args:
        print(__doc__)
        sys.exit(2)
    r = chain(args[0], web="--web" in sys.argv, llm="--llm" in sys.argv)
    print("答案：%s" % (r["answer"] or "（没答出来）"))
    print("层级：%s｜来源：%s｜可信度：%s｜%ss" % (r["tier"], r["source"], r["confidence"], r["seconds"]))
    if r["evidence"]:
        print("依据：%s" % r["evidence"])
    for t in r["tiers"]:
        print("  - %-9s %s %s" % (t["tier"], "OK " if t["ok"] else "×  ", t["detail"]))
