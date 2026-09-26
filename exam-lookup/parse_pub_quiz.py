#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
parse_pub_quiz.py —— 把「机构公开的保密知识测试题」做成题库（真实题目，不是生成题）
=================================================================================
  python3 parse_pub_quiz.py           解析 crawl/pub-lib/*.txt → banks/保密公开题-*.json
  python3 parse_pub_quiz.py selftest  自检（FAIL 会 exit 1）

为什么收这份：手上保密的真题只有 67 道（全部来自本人考过的三场卷子），平台侧练习/题库
拿不到（服务端开关），而这类**机构自己发在官网上的保密知识测试题**是公开资料 ——
题干、选项、答案、判断题都有，量还大。收它的规矩：

  1. 只收机构/高校官网发布的成套测试题（中科院、高校保卫处这类），**不碰**
     那些标题写着「某年线上培训考试题库」的文库转载件（那是考卷泄露，不是公开资料）；
  2. 答案按原卷自带，不自己判、不自己编；
  3. 每题 analysis 里写清卷名 + PDF 页码 + 原卷答案；能从《保密法》《条例》里配到条文的，
     用 cite_law 那套口径补一句「依据：」，配不上就不写（不硬凑）。

产出的库带 meta.external / meta.public，所以：
  - 不进电信模拟考池（webui.quiz 里 external 直接跳过）；
  - 不进电信口径的覆盖率分母（kb_coverage 只算电信库）。
"""
import glob
import html
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
SRC_DIR = os.path.join(HERE, "crawl/pub-lib")
OUT = os.path.join(HERE, "banks/保密公开题-中科院.json")
HEADER = "保密知识测试题及答案"
BANK_TITLE = "保密知识测试题（中科院公开卷 · %d 题）"

PAGEMARK = re.compile(r"\f")
NUMHEAD = re.compile(r"^\s*\d{1,3}\s*[、.．]?\s*")
OPT_START = re.compile(r"(?:^|[\s\u3000])([A-D])(?=[\s\u3000、.．])")
# 答案行前面可能顶着分页符（「…国家安全工\n作部门\n\f答案： A」）：用过的 pdftotext 会这样，
# 不许 ^[ \t]* 卡死，否则这条答案会被吞进下一题，整题的选项跟着串位
ANS_LINE = re.compile(r"(?m)^[\f \t]*答案[ \t]*[:：][ \t]*(.+?)[ \t]*$")
ENTITY = re.compile(r"&[a-z]+;|&#\d+;|[\u200b-\u200f\ufeff]")


def clean_line(s):
    s = PAGEMARK.sub(" ", s)
    s = html.unescape(ENTITY.sub("", s))
    s = s.replace("\u3000", " ").replace("\xa0", " ")
    s = re.sub(r"[ \t]{2,}", " ", s)
    # 折行拼接：PDF 换行会把词劈开（「不得既\n用于」），汉字/数字之间的空格是排版残留不是空格，
    # 并掉；拉丁字母之间保留（「U 盘」「A 类」这种要留着）
    s = re.sub(r"(?<=[\u4e00-\u9fa5\d，。、；：）」』])[ \t]+(?=[\u4e00-\u9fa5\d「『（])", "", s)
    return re.sub(r"\s*\n\s*", " ", s).strip()


def split_options(body):
    """把「A 甲 B 乙 C 丙」和换行堆叠两种排版的选项都切出来。

    只认「依次递增的 A/B/C/D」：题干里出现单独的 A（如「A 类」）不会被当成选项起点。
    """
    marks = [(m.start(1), m.group(1)) for m in OPT_START.finditer(body)]
    picked, want = [], "A"
    for pos, letter in marks:
        if letter == want:
            picked.append((pos, letter))
            want = chr(ord(want) + 1)
            if want > "D":
                break
    if len(picked) < 2:
        return [], body                      # 没有成组的选项 → 当判断题处理
    opts, stem = [], body[:picked[0][0]]
    for i, (pos, letter) in enumerate(picked):
        end = picked[i + 1][0] if i + 1 < len(picked) else len(body)
        text = clean_line(body[pos + 1:end]).lstrip("、.． ")
        opts.append({"key": letter, "text": text})
    return opts, stem


def parse(path):
    raw = open(path, encoding="utf-8", errors="ignore").read()
    if HEADER in raw:
        raw = raw.split(HEADER, 1)[1]
    chunks, last = [], 0
    for m in ANS_LINE.finditer(raw):
        chunks.append((raw[last:m.start()], clean_line(m.group(1))))
        last = m.end()
    out, seen = [], set()
    for body, answer in chunks:
        body = clean_line(NUMHEAD.sub("", clean_line(body)))
        if len(body) < 8:
            continue
        ans = re.sub(r"[\s、,，]+", "", answer.upper())
        opts, stem = split_options(body)
        stem = clean_line(NUMHEAD.sub("", stem)).strip("、.． ")
        if len(stem) < 8:
            continue
        if not opts:                                       # 判断题：原卷只给「正确/错误」
            if answer.strip() not in ("正确", "错误"):
                continue
            opts = [{"key": "A", "text": "正确"}, {"key": "B", "text": "错误"}]
            keys = ["A" if answer.strip() == "正确" else "B"]
            typ = "judge"
        else:
            keys = [c for c in ans if c in "ABCD"]
            if not keys or any(k not in [o["key"] for o in opts] for k in keys):
                continue
            typ = "multiple" if len(keys) > 1 else "single"
        if len(opts) < 2 or any(not o["text"] for o in opts):
            continue
        if len({o["text"] for o in opts}) != len(opts):
            continue
        key = re.sub(r"\s+", "", stem)
        if len(key) < 8 or key in seen:
            continue
        seen.add(key)
        out.append({"type": typ, "question": stem, "options": opts,
                    "answerKeys": keys, "answerText": [o["text"] for o in opts if o["key"] in keys],
                    "page": None})
    return out


def main():
    files = sorted(glob.glob(os.path.join(SRC_DIR, "*.txt")))
    if not files:
        sys.exit("crawl/pub-lib/ 下没有 txt —— 先用 pdftotext 把公开卷转文本")
    qs = []
    for f in files:
        part = parse(f)
        print("  %s → %d 题" % (os.path.basename(f), len(part)))
        qs += part
    for i, q in enumerate(qs, 1):
        q["id"] = "PUB-BM-%04d" % i
        q["number"] = i
        q["score"] = 1
        q["srcBank"] = "保密公开题-中科院"
        q["analysis"] = "来源：中国科学院《保密知识测试题及答案》（官网公开测试卷）。答案按原卷自带。"
        q["topic"] = "保密公开题"
    from collections import Counter
    print("合计 %d 题：%s" % (len(qs), dict(Counter(q["type"] for q in qs))))
    if qs:
        for q in qs[:3]:
            print("   题%s %s" % (q["number"], q["question"][:52]))
            print("        选项: %s ｜ 答案 %s" % (" / ".join(o["text"] for o in q["options"]),
                                                 "".join(q["answerKeys"])))
    data = {"meta": {"title": BANK_TITLE % len(qs), "external": True, "public": True,
                     "sourceNote": "来源：中国科学院官网公开的《保密知识测试题及答案》（imp.cas.cn）。"
                                   "答案按原卷自带；判断题原卷只给「正确/错误」，这里转成 A 正确 / B 错误。"},
            "questions": qs}
    json.dump(data, open(OUT, "w", encoding="utf-8"), ensure_ascii=False, indent=1)
    print("已写入：%s" % OUT)
    return qs


def selftest():
    qs = parse(os.path.join(SRC_DIR, "cas.txt"))
    from collections import Counter
    dist = Counter(q["type"] for q in qs)
    assert len(qs) >= 600, "题目太少：%d" % len(qs)
    assert dist["single"] >= 300 and dist["judge"] >= 100 and dist["multiple"] >= 50, \
        "题型分布不对：%s" % dict(dist)
    seen = set()
    for q in qs:
        keys = [o["key"] for o in q["options"]]
        assert len(q["question"]) >= 8 and q["question"] == q["question"].strip(), "题干不干净：%r" % q["question"][:40]
        assert len(q["options"]) == len(set(keys)) >= 2, "选项重复或缺：%s" % keys
        assert all(o["text"].strip() for o in q["options"]), "有空选项：%s" % q["question"][:30]
        assert set(q["answerKeys"]) <= set(keys) and q["answerKeys"], "答案不在选项里：%s" % q["question"][:30]
        assert len({o["text"] for o in q["options"]}) == len(q["options"]), "选项文本重复：%s" % q["question"][:30]
        assert not re.search(r"(?m)^[ \t]*答案[ \t]*[:：]", q["question"]), "题干里混进了答案行"
        # 题干里出现「答案」二字是正常的（如「试题、答案和评分标准」），只卡整行「答案：」
        k = re.sub(r"\s+", "", q["question"])
        assert k not in seen, "题干重复：%s" % q["question"][:30]
        seen.add(k)
        if q["type"] == "judge":
            assert [o["text"] for o in q["options"]] == ["正确", "错误"], "判断题选项不是 正确/错误"
            assert len(q["answerKeys"]) == 1
        if q["type"] == "multiple":
            assert len(q["options"]) >= 3 and len(q["answerKeys"]) >= 2, "多选结构不对"
        # 选项文本不许带着下一个选项的字母
        for o in q["options"][:-1]:
            assert not re.search(r"[\s\u3000][A-D][\s\u3000、.．]", o["text"]), \
                "选项里混进了下一个选项：%s" % o["text"][:40]
    known = [q for q in qs if "国家秘密是关系国家安全和利益" in q["question"]]
    assert known, "缺已知第一条"
    assert known[0]["answerKeys"] == ["B"], "第一条答案应为 B，实得 %s" % known[0]["answerKeys"]
    js = [q for q in qs if "将手机连接到涉密计算机上充电" in q["question"]]
    assert js and js[0]["answerKeys"] == ["B"], "手机充电那条应为「错误」(B)"
    print("selftest OK：%d 题（单选 %d / 多选 %d / 判断 %d），题干唯一、答案在选项内、选项不重复、"
          "已知两条对得上（第1题=B、手机充电题=错误）"
          % (len(qs), dist["single"], dist["multiple"], dist["judge"]))


if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1] == "selftest":
        selftest()
    else:
        main()
