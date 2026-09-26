#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
parse_textbook_quiz.py —— 把教材里的「小节测试」抽成刷题库（教材原题）
======================================================================
  python3 parse_textbook_quiz.py            生成 banks/教材原题-线路安全.json
  python3 parse_textbook_quiz.py selftest   自检（FAIL 会 exit 1）

现状：手上 30 多本教材里，只有《第五篇 线路安全作业规范》印了成组的练习题（4 组共 8 题，
单选 2 / 多选 3 / 判断 3）。**教材只印题、不印答案** —— 所以这里的答案是逐题从同一本正文里
推出来的，每题都带「依据原文 + 页码」，可回查。别把这些答案当教材官方答案。

第 7 题（下人孔多选）是个真陷阱：选项 A 说「孔内有积水必须使用潜水泵抽水」，
而正文 p28 写的是「严禁使用潜水泵抽水」—— 所以 A 是错的。这也是坚持逐题回查正文的原因。
"""
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import kb_pages                                                   # noqa: E402

OUT = os.path.join(HERE, "banks/教材原题-线路安全.json")
BOOK_KEY = "第五篇"
HEAD = re.compile(r"（(单选题|多选题|判断题)）\s*(\d+)\s*[、.]")
FOOTER = re.compile(r"云网运营部.*?$|目录|目\s+录|小节测试")

# 逐题判定的答案与依据（key = 题干的特征词，防止顺序一变就错位）
ANSWER = [
    ("跨越高速公路", "single", ["C"], ["500"],
     "p4「若跨越高速公路作业时，必须在距离作业点500m处设置安全警示标志」", 4),
    ("电力线附近作业时有哪些注意事项", "multiple", ["A", "B", "C", "D"],
     ["戴绝缘手套", "穿绝缘鞋", "使用绝缘钳子", "事先联系电力部门"],
     "p5「在电力线附近作业时，必须事先联系电力部门停止送电，并有专人看管，确认停电后再作业，作业时仍须戴绝缘手套，穿绝缘鞋和使用绝缘钳子」", 5),
    ("梯子靠在吊线", "single", ["B"], ["0.5m"],
     "p10「梯子靠在吊线时，其顶端至少应高出吊线0.5m（梯子顶部装有钩的除外）」", 10),
    ("扔抛工具和材料", "judge", ["B"], ["错"],
     "p8「高处作业人员与地面人员之间不准扔抛工具和材料」", 8),
    ("关于拆换杆线作业", "multiple", ["A", "B", "C", "D"],
     ["注意环境安全统一指挥", "先检查电杆根部", "不在原洞更换电杆", "渐渐松放"],
     "p24 拆换杆线作业须知四条：注意环境安全、统一指挥／拆除线缆前检查电杆根部／不在原洞更换电杆时先立新杆／拆除吊线时渐渐松放", 24),
    ("坐滑车前应检查吊板", "judge", ["A"], ["对"],
     "p22「检查吊板的绳索、吊钩的牢固程度，如吊钩已磨损1/4时，不准使用」", 22),
    ("下人孔作业", "multiple", ["B", "C", "D"],
     ["气体检测", "梯子不准搭在电缆上", "先行通风"],
     "p28：A 错 —— 正文写的是「严禁使用潜水泵抽水」；B/C/D 分别是「应使用仪器对孔内气体进行适时检测」「不准把梯子搭在孔内电缆上，严禁蹬踏电缆线或电缆托架」「必须先行通风，确认没有易燃、有毒有害气体，方可下孔作业」", 28),
    ("管道线缆敷设作业前", "judge", ["A"], ["对"],
     "p29「作业前, 必须检查使用的各种机具,确保齐备完好；作业时，工具不可随意替代」", 29),
]


def book_pages():
    for b, pages in kb_pages.books():
        if BOOK_KEY in b:
            return b, pages
    sys.exit("没找到《%s》这本书" % BOOK_KEY)


def clean(t):
    t = FOOTER.sub(" ", t)
    return re.sub(r"\s+", " ", t).strip()


def tight(t):
    """PDF 按行折行会在汉字之间插空格（「扔抛工具和材 料」），去掉；数字/字母旁边的空格保留（「距离作业点 500m」）"""
    return re.sub(r"(?<=[\u4e00-\u9fff\u3001\uff08\uff09])\s+(?=[\u4e00-\u9fff\u3001\uff08\uff09])", "", t)


def parse():
    name, pages = book_pages()
    full = clean(" ".join(pages))
    out = []
    for hm in HEAD.finditer(full):
        kind, num = hm.group(1), int(hm.group(2))
        seg = full[hm.end():hm.end() + 800]
        nxt = HEAD.search(seg)
        if nxt:
            seg = seg[:nxt.start()]
        seg = seg.split("云网运营部")[0]          # 页脚之后是页码/目录，直接砍掉（否则最后一个选项会被吞）
        # 选项：A、xxx B、xxx …（选项里可能含换行/页码残留，已 clean 过）
        opts = []
        for om in re.finditer(r"([ABCD])\s*[、.]\s*([^ABCD]{1,160}?)(?=\s*[ABCD]\s*[、.]|$)", seg):
            opts.append({"key": om.group(1), "text": tight(re.sub(r"\s+", " ", om.group(2))).strip(" ，,。;；")})
        stem = seg[:seg.find("A、") if "A、" in seg else len(seg)]
        stem = tight(re.sub(r"\s+", " ", stem)).strip(" ，,。;；")
        if kind == "判断题":                                    # 判断题的「A、对 B、错」不是选项文本，统一成对/错
            opts = [{"key": "A", "text": "对"}, {"key": "B", "text": "错"}]
            stem = re.sub(r"（\s*）\s*$", "（　）", stem)
        out.append({"kind": kind, "num": num, "stem": stem, "options": opts})
    return name, pages, out


def build():
    name, pages, parsed = parse()
    assert len(parsed) == len(ANSWER), "解析出 %d 题，答案表有 %d 条 —— 别硬跑，先对齐" % (len(parsed), len(ANSWER))
    qs = []
    for i, (q, (key, kind, keys, texts, ev, page)) in enumerate(zip(parsed, ANSWER), 1):
        assert key in q["stem"].replace(" ", ""), "第 %d 题题干对不上答案表（找 %s，题干：%s）" % (i, key, q["stem"][:36])
        assert page <= len(pages), "依据页码越界：p%d > %d 页" % (page, len(pages))
        assert len(keys) >= 1
        qs.append({
            "id": "TEXTBOOK-L%02d" % i, "number": i, "type": kind,
            "typeLabel": {"single": "单选（教材题）", "multiple": "多选（教材题）", "judge": "判断（教材题）"}[kind],
            "question": q["stem"], "options": q["options"], "answerKeys": keys, "answerText": texts,
            "analysis": "依据：%s（%s 第%d页）" % (ev, "第五篇 线路安全作业规范", page),
            "topic": "线路安全作业规范（教材小节测试）", "derived": True,
            "warn": "教材原题，答案由正文推导（教材未印答案）",
            "srcBank": "教材原题-线路安全",
        })
    data = {"meta": {"title": "教材原题 · 线路安全作业规范 小节测试（答案由正文推导）",
                     "derived": True,
                     "sourceNote": "解析自《第五篇 线路安全作业规范》（%s）里的 4 组「小节测试」共 %d 题；"
                                   "教材未印答案，每题答案是逐题回查同一本正文推出来的，analysis 里有依据原文+页码。" % (name.split(" ┄ ")[-1], len(qs))},
            "questions": qs}
    json.dump(data, open(OUT, "w", encoding="utf-8"), ensure_ascii=False, indent=1)
    print("已生成：%s —— %d 题（单选 2 / 多选 3 / 判断 3）" % (OUT, len(qs)))
    for q in qs:
        print("  题%d [%s] %s" % (q["number"], q["typeLabel"], q["question"][:52]))
        print("        答案 %s ｜ %s" % ("".join(q["answerKeys"]), q["analysis"][:78]))
    return qs


def selftest():
    qs = build()
    assert len(qs) == 8, "题数不对：%d" % len(qs)
    assert [q["type"] for q in qs] == ["single", "multiple", "single", "judge", "multiple", "judge", "multiple", "judge"], \
        "题型顺序跟 ANSWER 表不一致：%s" % [q["type"] for q in qs]
    for q in qs:
        need = 2 if q["type"] == "judge" else 4
        assert len(q["options"]) == need, "题%d 选项数不对：%d" % (q["number"], len(q["options"]))
        assert set(q["answerKeys"]) <= {o["key"] for o in q["options"]}, "题%d 答案不在选项里" % q["number"]
        assert re.search(r"第\d+页", q["analysis"]), "题%d 没写依据页码" % q["number"]
        assert q["derived"] and q["warn"], "题%d 没标注「答案由正文推导」" % q["number"]
    # 关键陷阱题：第 7 题的 A（潜水泵）必须是错的
    q7 = qs[6]
    assert "潜水泵" in q7["options"][0]["text"] and "A" not in q7["answerKeys"], \
        "第 7 题没按正文判定（正文 p28：严禁使用潜水泵抽水）"
    assert "严禁" in q7["analysis"]
    # 题干要完整（不是被页码截断的半句）
    for q in qs:
        assert 8 <= len(q["question"]) <= 120, "题%d 题干长度异常：%s" % (q["number"], q["question"][:40])
        assert not q["question"].startswith(("目", "云网", "A、")), "题%d 题干混进了页脚/目录" % q["number"]
    print("selftest OK：8 道教材原题（单选 2 / 多选 3 / 判断 3），选项数正确、答案在选项内、每题带依据页码；"
          "陷阱题第 7 题按正文判 A 错（p28「严禁使用潜水泵抽水」）")


if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1] == "selftest":
        selftest()
    else:
        build()
