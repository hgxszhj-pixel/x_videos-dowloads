#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
parse_handbook_qa.py —— 把《网络和信息安全应知应会手册》里的问答抽成背题表
======================================================================
  python3 parse_handbook_qa.py            生成 crawl/网信安-应知应会-问答.md + .json
  python3 parse_handbook_qa.py selftest   自检（FAIL 会 exit 1）

来源：kc 平台「网络和信息安全」专区的 4 本手册（crawl/attach/wxaq/*.pdf 及其 pdftotext 文本）。
手册正文就是问答体：「12、关于 XXX 有何要求？ 答：……」——平台侧的练习题取不到（服务端开关），
但这些问答是印在教材里的官方表述，可以当背题材料，也可以当生成题的原文底本。

零依赖；页码来自 PDF 文本里的「N / 220」页标记。
"""
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
SRC_DIR = os.path.join(HERE, "crawl/attach/wxaq")
OUT_MD = os.path.join(HERE, "crawl/网信安-应知应会-问答.md")
OUT_JSON = os.path.join(HERE, "crawl/网信安-应知应会-问答.json")
ACCOUNT = "网络和信息安全（应知应会手册）"

HEAD = re.compile(r"(?:^|\s)(\d{1,3})\s*[、.．]\s*")          # 题号：N、
PAGE = re.compile(r"(\d{1,3})\s*/\s*(\d{2,3})")               # 页标记 12 / 220
ANS = re.compile(r"[？?]\s*答\s*[：:]")
# 中途插进来的页脚/水印/导图碎片，落到答案里就是噪音
JUNK = re.compile(r"<[^>]{0,40}>|应知应会手册|第[一二三四五六七八九十]+章\s*[·•]?\s*\S{0,12}篇|阅读导图|阅读视图")
FLAT = re.compile(r"\s+")


def load_docs():
    """→ [(章名, 全文, [(该页起始偏移, 页码)])]

    必须**整篇**解析、只用偏移记页码：按页切开会让跨页的答案被截断
    （实测第 1 条答案在页边界断成「…勾画了」，后半句「基本的制度框架。」在下一页被丢掉）。
    """
    out = []
    for name in sorted(os.listdir(SRC_DIR)):
        if not name.endswith(".txt") or not name.startswith("应知应会手册"):
            continue
        chapter = re.sub(r"^应知应会手册|（|）|\.txt$", "", name)
        raw = open(os.path.join(SRC_DIR, name), encoding="utf-8", errors="ignore").read()
        starts, off, page = [], 0, 1
        for m in PAGE.finditer(raw):
            starts.append((off, page))
            off = m.end()
            page = int(m.group(1))
        starts.append((off, page))
        out.append((chapter, PAGE.sub(" ", raw), starts))
    return out


def page_at(starts, pos):
    """某个偏移落在哪一页"""
    cur = starts[0][1] if starts else 0
    for off, pg in starts:
        if off > pos:
            break
        cur = pg
    return cur


def tight(t):
    """PDF 折行会在汉字之间插空格（「国 家安全」），去掉；数字/字母旁边的空格保留（「不少于 6 个月」）"""
    return re.sub(r"(?<=[\u4e00-\u9fff\u3001\uff0c\u3002\uff08\uff09\u300a\u300b])\s+(?=[\u4e00-\u9fff\u3001\uff0c\u3002\uff08\uff09\u300a\u300b])", "", t)


def tidy(t):
    t = JUNK.sub(" ", t)
    t = FLAT.sub(" ", t)
    return tight(t).strip(" ：:，,。;；、·•-“”\"")


def parse():
    qs = []
    for chapter, raw, starts in load_docs():
        flat = FLAT.sub(" ", raw)                    # 页标记已就地抹成空格，答案可以跨页往下读
        for m in ANS.finditer(flat):
            qtail = flat[max(0, m.start() - 400):m.start()]
            hm = None
            for x in HEAD.finditer(qtail):
                hm = x
            if not hm:
                continue
            q = tidy(qtail[hm.end():])
            if not (6 <= len(q) <= 160):
                continue
            rest = flat[m.end():]
            nxt = HEAD.search(rest)                  # 答案一直到下一个题号为止（跨页不再截）
            a = tidy(rest[:nxt.start()] if nxt else rest[:400])
            if len(a) < 8:
                continue
            qs.append({"chapter": chapter, "page": page_at(starts, m.start()), "num": int(hm.group(1)), "q": q, "a": a})
    dedup = {}                                        # 同一题可能因重复排版出现两次，留答案更全的
    for x in qs:
        k = (x["chapter"], x["num"])
        if k not in dedup or len(x["a"]) > len(dedup[k]["a"]):
            dedup[k] = x
    return [dedup[k] for k in sorted(dedup, key=lambda k: (k[0], k[1]))]


def build():
    qs = parse()
    assert qs, "一条问答都没抽到，样本文件是不是没了？"
    data = {"meta": {"title": "网络和信息安全 · 应知应会手册问答", "account": ACCOUNT,
                     "sourceNote": "抽自 kc 平台「网络和信息安全」专区 4 本应知应会手册（crawl/attach/wxaq/）。"
                                   "手册正文即问答体；平台侧练习题取不到（服务端开关），这份是印在教材里的官方表述。",
                     "count": len(qs)},
            "questions": qs}
    json.dump(data, open(OUT_JSON, "w", encoding="utf-8"), ensure_ascii=False, indent=1)
    with open(OUT_MD, "w", encoding="utf-8") as f:
        f.write("# 网络和信息安全 · 应知应会手册问答（共 %d 条）\n\n" % len(qs))
        f.write("来源：kc「网络和信息安全」专区 4 本手册（第一章 守底线·法律法规篇 / 第二章 严要求·规章制度篇 / "
                "第三章 强意识·安全常识篇 / 第四章 学技术·基础通识篇）。页码是该手册内部页码。\n\n")
        chapter = None
        for x in qs:
            if x["chapter"] != chapter:
                chapter = x["chapter"]
                f.write("\n## %s\n\n" % chapter)
            f.write("**%d. %s**\n\n答：%s\n\n（p%d）\n\n" % (x["num"], x["q"], x["a"], x["page"]))
    print("已生成：%s / %s —— %d 条问答" % (OUT_MD, OUT_JSON, len(qs)))
    for x in qs[:5]:
        print("  %s p%-3d %s → %s" % (x["chapter"][:6], x["page"], x["q"][:40], x["a"][:40]))
    return qs


def selftest():
    qs = build()
    assert len(qs) >= 300, "问答数异常偏少：%d" % len(qs)
    assert max(len(x["a"]) for x in qs) > 200, "最长答案才 %d 字 —— 跨页的答案又被截了" % max(len(x["a"]) for x in qs)
    for x in qs:
        assert x["q"] and x["a"], "有空的题干或答案"
        assert len(x["q"]) >= 6 and len(x["a"]) >= 8
        assert not re.search(r"\d{1,3}\s*/\s*\d{2,3}", x["a"]), "答案里混进了页标记：%s" % x["a"][:60]
        assert "<" not in x["a"] and "<" not in x["q"], "正文里混进了标签：%s" % x["a"][:60]
    chs = {x["chapter"] for x in qs}
    assert len(chs) == 4, "章数不对：%s" % chs
    hit = [x for x in qs if "恶意代码的种类" in x["q"]]
    assert hit and "病毒" in hit[0]["a"], "已知问答没抽到或答案不对：恶意代码的种类"
    short = [x for x in qs if x["num"] <= 0 or x["num"] > 300]
    assert not short, "题号越界：%s" % short[:3]
    print("selftest OK：%d 条问答（4 章），题干/答案非空、无页标记残留、已知问答抽查命中" % len(qs))


if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1] == "selftest":
        selftest()
    else:
        build()
