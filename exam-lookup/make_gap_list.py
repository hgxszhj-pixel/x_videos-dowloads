#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""给收编页剩下那 23 条生成「该去哪儿补」清单。

不猜答案、不写库。只做三件事：
1) 每条算 3-gram 命中率最高的 3 份原件（哪怕分数很低也照实给分数）；
2) 带上已有的联网摘要（供点选项时比对）；
3) 分成「有选项，点一下就能收编」和「只有题干，得先找卷面」两档。
"""
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import ask_ctx                                        # noqa: E402
import plugins.harvest as hv                          # noqa: E402

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "jr/out/缺题面清单-20261028.md")


EVIDENCE = [
    (1, "《安全生产法》从业人员超过多少人应设安全生产管理机构（A.100/B.200/C.300/D.500）", "A · 100 人",
     "《中国电信湖北公司安全生产管理办法（2024）》：「从业人员及外包人员超过一百人的生产经营单位，应设置安全生产监督管理机构或者配备专职安全生产监督管理人员」",
     "强"),
    (2, "保守国家秘密法的密级分为几级（A.两级/B.三级/C.四级/D.五级）", "B · 三级",
     "《中华人民共和国保守国家秘密法（2024修订）》第十四条：「国家秘密的密级分为绝密、机密、秘密三级。」",
     "强"),
    (3, "云改数转智惠：核心/动力/根本/基础（A.云网融合 B.改革开放 C.科技创新 D.干部人才 E.数智化平台 F.绿色安全）", "C · B · D · A",
     "《云改数转智惠战略宣贯要点》八条标题：（三）科技创新是核心（四）云网融合是基础（五）绿色安全是底色（六）数智化平台是枢纽（七）改革开放是动力（八）干部人才是根本",
     "强"),
    (4, "通信机房内接地电阻应不大于多少欧姆（A.1/B.4/C.10/D.30）", "需你判断：原文口径不一",
     "《综维工程师技能认证教材》：「C 类局站接地地阻应小于 1Ω，D 类局站接地地阻应小于 5Ω」；另一份《云网风险隐患》写光交接箱「接地电阻≤10欧姆」——选项里没有 5Ω，凭机房类别才定得下来",
     "弱"),
]

def best_files(stem, k=3):
    """全原件扫一遍，给 gram 命中率 top-k（分数低也照实报，不假装找到了）。"""
    g = ask_ctx._grams(ask_ctx._stem(stem))
    if not g:
        return []
    rows = []
    for d in ask_ctx.TEXT_DIRS:
        if not os.path.isdir(d):
            continue
        for fn in sorted(os.listdir(d)):
            if not fn.endswith(".txt"):
                continue
            txt, grams = ask_ctx._file_txt_grams(os.path.join(d, fn))
            if txt is None or not grams:
                continue
            rows.append((len(g & grams) / len(g), fn))
    rows.sort(reverse=True)
    return rows[:k]


def main():
    items = hv.list_items(limit=100)
    usable = [x for x in items if x["usable"]]
    stem_only = [x for x in items if not x["usable"]]

    L = []
    L.append("# 待收编 23 条：去哪儿补题面 / 点了就能收编\n")
    L.append("> 生成于 2026-10-28。**不包含任何我猜的答案**——凡题库和本地语料都没有依据的，这里只给「最可能出自哪份原件」+ 命中率，答案要你对照原件/卷面确认。\n")
    L.append("## 一、有选项，点一下就能收编（%d 条）\n" % len(usable))
    L.append("在网页「收编」页按顺序点选项即可；下面附已有的联网摘要，供比对。\n")
    L.append("| # | 题干 | 选项 | 摘要（供比对） |")
    L.append("|---|---|---|---|")
    for i, it in enumerate(usable, 1):
        opts = " / ".join("%s.%s" % (o["key"], o["text"][:14]) for o in it["options"][:6])
        ev = (it["evidence"] or [""])[0]
        ev = re.sub(r"\s+", " ", ev)[:120].replace("|", "/")
        L.append("| %d | %s | %s | %s |" % (i, it["stem"][:60].replace("|", "/"), opts.replace("|", "/"), ev))

    L.append("\n## 一·5、已有原文依据（等你点头再点，我不替你入库）\n")
    L.append("| # | 题干 | 建议答案 | 原文依据 | 强度 |")
    L.append("|---|---|---|---|---|")
    for row in EVIDENCE:
        L.append("| %d | %s | **%s** | %s | %s |" % row)
    L.append("\n两条重复项（题目不同写法、答案相同）建议只留一条：`2d885fd83592` 与 `32e719303231`；"
             "`31b16e009a71` 与 `aa31dd7b46ca`。\n")

    L.append("\n## 二、只有题干，得先找到卷面才能收编（%d 条）\n" % len(stem_only))
    L.append("「命中率」= 题干 3-gram 在该原件里出现的比例，<0.3 基本等于无关，只当线索方向看。\n")
    for i, it in enumerate(stem_only, 1):
        L.append("### %d. %s" % (i, it["stem"][:100]))
        L.append("")
        rows = best_files(it["stem"])
        if rows and rows[0][0] >= 0.15:
            L.append("| 可能的原件 | 命中率 |")
            L.append("|---|---|")
            for score, fn in rows:
                L.append("| `%s` | %.2f |" % (fn.replace("|", "/"), score))
        else:
            L.append("_全 178 份原件都没命中（最高 %.2f）——本地资料里真的没有，需要从卷面/教材纸面补。_"
                     % (rows[0][0] if rows else 0.0))
        ev = (it["evidence"] or [])
        if ev:
            L.append("")
            L.append("联网摘要：`%s`" % re.sub(r"\s+", " ", ev[0])[:160])
        L.append("")

    L.append("## 怎么用\n")
    L.append("1. 第一档：打开 http://127.0.0.1:8765/ 「收编」页，按摘要比对点选项 → 直接入库。")
    L.append("2. 第二档：命中率 ≥0.5 的，打开对应原件（`jr/out/docs_new_text/`）搜题干关键词，把带选项的原文整段粘进卡片「补题面并收编」。")
    L.append("3. 第二档里标「都没命中」的，只有找到卷面/教材才能补——别凭印象填。")
    open(OUT, "w", encoding="utf-8").write("\n".join(L) + "\n")
    print("写出 %s（%d 行）· 有选项 %d 条 / 只有题干 %d 条" % (OUT, len(L), len(usable), len(stem_only)))
    for it in stem_only:
        rows = best_files(it["stem"])
        top = "%.2f %s" % (rows[0][0], rows[0][1][:28]) if rows else "无"
        print("  · %-46s → %s" % (it["stem"][:44], top))


if __name__ == "__main__":
    main()
