#!/usr/bin/env python3
"""把 l3-questions.json 渲染成人看的 md 清单（序号 / 题干 / 选项 / 答案 / 分析 / 可信度）。"""
import json
import os
import sys
from collections import Counter

PROJ = os.path.expanduser("~/x_videos-dowloads/exam-lookup")
OUT = os.path.join(PROJ, "jr/out")
SRC = os.path.join(OUT, "l3-questions.json")
MD = os.path.join(OUT, "2025云网条线大模型L3-题目与答案.md")
LABEL = {"high": "高", "mid": "中", "low": "低（需人工确认）"}


def main():
    d = json.load(open(SRC, encoding="utf-8"))
    qs = sorted(d["questions"], key=lambda q: q["no"])
    types = Counter(q["type"] for q in qs)
    low = [q["no"] for q in qs if q["confidence"] == "low"]

    with open(MD, "w", encoding="utf-8") as f:
        f.write("# 2025 年云网条线大模型 L3 · 题目与答案\n\n")
        f.write("%s\n\n" % d["source_note"])
        f.write("共 **%d 道**（单选 %d / 多选 %d / 判断 %d）。"
                "答案列写的是「分析后认为正确的选项」，多选按字母排序。\n\n"
                % (len(qs), types.get("单选", 0), types.get("多选", 0), types.get("判断", 0)))
        f.write("重复拍摄：3 与 2 同题、27 与 26 同题、49 与 48 同题，已合并。\n\n")
        if low:
            f.write("**低可信度（务必人工确认）**：%s —— 这些题考的是平台内部约定"
                    "（节点/标识符/角色/参数上限），不查产品文档无法确定。\n\n" % "、".join("第%d题" % n for n in low))
        f.write("---\n\n")

        for q in qs:
            f.write("## %d、%s题（%s）\n\n" % (q["no"], q["type"], LABEL.get(q["confidence"], q["confidence"])))
            f.write("**题干**：%s\n\n" % q["stem"])
            for k in sorted(q["options"]):
                mark = " ✅" if k in q["answer"] else ""
                f.write("- %s、%s%s\n" % (k, q["options"][k], mark))
            f.write("\n**答案**：%s\n\n" % "".join(q["answer"]))
            f.write("**分析**：%s\n\n" % q["analysis"])
        f.write("---\n\n")
        f.write("## 使用与边界\n\n")
        f.write("- 题干和选项逐张按截图识别，已人工核对；图上**没有**标准答案，答案与解析是推导结果。\n")
        f.write("- 可信度「高」= 通用技术共识或题干直接可判；「中」= 需要一点判断（多选边界、平台语义）；\n"
                "  「低」= 平台专有约定，只能以产品文档/实际界面为准。\n")
        f.write("- 机器可读版：`jr/out/l3-questions.json`（含 answer/analysis/confidence 字段）。\n")
    print("→ %s（%d 道；低可信 %d 道：%s）" % (MD, len(qs), len(low), low))
    return 0


if __name__ == "__main__":
    sys.exit(main())
