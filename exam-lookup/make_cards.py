#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
make_cards.py —— 给「背题」页做更多材料（问答体之外的料也能背）
=============================================================
  python3 make_cards.py            生成 crawl/数字考点-问答.json、crawl/考点解析-问答.json
  python3 make_cards.py selftest   自检（FAIL 会 exit 1）

为什么要有它：背题页原来只有《网络和信息安全应知应会手册》那一份 343 条（平台上能抽到的
问答体材料只有那 4 本），材料太少。但「背题」不必局限在问答体 —— 只要有「问题面 + 答案面」
就能背，而这两类料全是原文，不需要编：

  ① 数字考点卡：`crawl/数字考点速查.csv` 里 441 条含数字的规范句（安全管理规章原文）
     → 问题 = 把那个数字挖掉的句子，答案 = 完整原句 + 书名页码。
     注意这里能用满 441 条：生成题（make_drill.py）受「同单位至少 3 个真实干扰项」限制
     只出得了 60 道，而卡片不需要选项，所以这些数字全都能背。
  ② 考点解析卡：语料里「X 是指 …」「X 包括：…」这类定义/清单句 → 问题 = 「X 是指什么？」，答案 = 原句。

两条硬规矩：
  - 答案只能是**原文整句**，不缩写、不改写、不编（挖空句本身就是原文去掉一个数字）；
  - 每条都带出处（书名 + 页码），背的时候能回查。

产出的文件名必须含「问答」二字 —— webui.py 的背题页就是按 `crawl/*问答*.json` 扫的，
所以这份料会自动出现在背题页的材料下拉里，不需要改后端。
"""
import csv
import json
import os
import re
import sqlite3
import sys
import collections

HERE = os.path.dirname(os.path.abspath(__file__))
CSV = os.path.join(HERE, "crawl/数字考点速查.csv")
OUT_NUM = os.path.join(HERE, "crawl/数字考点-问答.json")
OUT_DEF = os.path.join(HERE, "crawl/考点解析-问答.json")
OUT_PUB = os.path.join(HERE, "crawl/保密公开题-问答.json")
BLANK = "（　）"

NORM_TITLE = re.compile(r"安全|作业|规程|规范|手册|管理办法|教材|认证|解读|条款|标准")
JUNK_TERM = re.compile(r"^(主要|问题|工作|情况|内容|要求|条件|方式|措施|方法|该项|下列|以下|如下|上述|"
                       r"其中|其他|有关|相关|各种|部分|一般|同时|并且|如果|因此|所以|可以|必须|不得)$")
DEF_PAT = re.compile(r"^([\u4e00-\u9fa5A-Za-z0-9（）()·]{3,14})\s*(?:，)?\s*(?:是指|指的是|系指|称为|叫做|简称)")
LIST_PAT = re.compile(r"^([\u4e00-\u9fa5A-Za-z0-9（）()·]{3,14})\s*(?:主要)?包括\s*[：:]")
SPLIT = re.compile(r"[。；\n]")
BAD_SENT = re.compile(r"见下表|见本章|参见|如下表|详见|同上|如图|见附件")


def flat(s):
    """压空白：CSV 里的句子是按页抽的，空格位置跟语料库不一致，比对前先压平。"""
    return re.sub(r"\s+", "", s or "")


def load_corpus():
    """语料库整句表：既能给 CSV 的截断句找回完整原句，也是定义卡的来源。"""
    db = sqlite3.connect(os.path.join(HERE, "kb.sqlite3"))
    out = []
    for title, page, text in db.execute("select title, page, text from docs where length(text) > 120"):
        for seg in SPLIT.split(text or ""):
            seg = re.sub(r"\s+", " ", seg).strip()
            if 18 <= len(seg) <= 260:
                out.append({"title": title or "", "page": page, "s": seg, "f": flat(seg)})
    return out


def full_sentence(corpus, sentence):
    """CSV 里的句子有时被截断（「…，根」），拿前缀去语料库把完整句找回来；找不到就跳过这条。"""
    key = flat(sentence)[:16]
    if len(key) < 12:
        return None
    best = None
    for c in corpus:
        if c["f"].startswith(key) and (best is None or len(c["s"]) > len(best["s"])):
            best = c
    return best


def num_cards(corpus):
    """数字考点 → 卡。问题面把数字挖掉，答案面是完整原句。"""
    rows = list(csv.DictReader(open(CSV, encoding="utf-8")))
    out, seen = [], set()
    for r in rows:
        num, sent = (r.get("num") or "").strip(), (r.get("sentence") or "").strip()
        if not num or not sent:
            continue
        full = full_sentence(corpus, sent)
        if full:
            # 页码优先用语料库块自带的；那块没有页码（如平台附件）就退回 CSV 记的页码
            text, page, book = full["s"], full["page"] or r.get("page"), full["title"]
        elif re.search(r"[。；）]$", sent):
            text, page, book = sent, r.get("page"), r.get("book")   # 语料库对不上但 CSV 这句本身是完整句
        else:
            continue                                   # CSV 里被截断、语料库又找不回来 → 宁缺勿错
        pat = re.compile(r"\s*".join(map(re.escape, num)))     # 「4 公斤」/「4公斤」两种写法都认
        m = pat.search(text)
        if not m:
            continue                                   # 原句里没有这个数 → 对不上，跳过
        q = (text[:m.start()] + BLANK + text[m.end():]).strip()
        if len(flat(q)) < 12:
            continue
        key = flat(q)
        if key in seen:
            continue
        seen.add(key)
        out.append({"num": len(out) + 1, "chapter": (r.get("category") or "其它").strip() or "其它",
                    "page": page, "q": q,
                    "a": "%s　——　%s" % (num, text),
                    "book": book, "tested": (r.get("bank_questions") or "").strip(),
                    "src": "数字考点速查.csv"})
    return out


def def_cards(corpus):
    """「X 是指 …」「X 包括：…」→ 卡。只取定义/清单句，且出处要对得上命题范围。"""
    out, seen = [], set()
    for c in corpus:
        if not NORM_TITLE.search(c["title"]):
            continue
        s = c["s"]
        if BAD_SENT.search(s):
            continue
        m = DEF_PAT.match(s) or LIST_PAT.match(s)
        if not m:
            continue
        term = re.sub(r"^[（(]?[一二三四五六七八九十\d]{1,3}[）)、.．]?\s*", "", m.group(1).strip())
        if (len(term) < 3 or len(term) > 12 or JUNK_TERM.match(term)
                or re.search(r"[的了吗呢（）()]|是否|怎么|为什么|如何|第[一二三四五六七八九十]", term)):
            continue                                   # 小节标签（「（一）高风险与特殊场景作业」）不是知识点名
        q = "%s 是指什么？（%s）" % (term, re.sub(r"（.*?）", "", c["title"])[:22])
        key = flat(q)
        if key in seen:
            continue
        seen.add(key)
        out.append({"num": len(out) + 1, "chapter": re.sub(r"（.*?）", "", c["title"])[:26] or "其它",
                    "page": c["page"], "q": q, "a": s, "book": c["title"], "src": "语料库定义句"})
    return out


def bank_cards(bank_file, only_public=True):
    """题库 → 卡：题目面是题干 + 选项，答案面是正确答案 + 依据。给背题页多一份大料。

    只默认收「机构公开卷」这类（meta.public）—— 电信那几套是刷题页的主体，
    再复制一份到背题页只是同一批题出现两遍。
    """
    path = os.path.join(HERE, "banks", bank_file)
    if not os.path.exists(path):
        return []
    d = json.load(open(path, encoding="utf-8"))
    meta = d.get("meta") or {}
    if only_public and not meta.get("public"):
        return []
    TYPE = {"single": "单项选择", "multiple": "多项选择", "judge": "判断"}
    out = []
    for q in d.get("questions") or []:
        opts = "\n".join("  %s. %s" % (o.get("key"), o.get("text")) for o in (q.get("options") or []))
        keys = "".join(q.get("answerKeys") or [])
        ans = "答案：%s %s" % (keys, "；".join(q.get("answerText") or []))
        out.append({"num": q.get("number") or len(out) + 1,
                    "chapter": TYPE.get(q.get("type"), q.get("type") or ""),
                    "page": None, "q": q.get("question", "") + ("\n" + opts if opts else ""),
                    "a": ans, "book": meta.get("title") or bank_file,
                    "src": os.path.basename(path)})
    return out


def write_cards(path, title, note, items):
    data = {"meta": {"title": title, "sourceNote": note,
                     "generatedFrom": "make_cards.py（原文成卡，非问答体材料）"},
            "questions": [{"chapter": x["chapter"], "num": x["num"], "page": x["page"],
                           "q": x["q"], "a": x["a"], "_book": x.get("book", ""),
                           "_src": x.get("src", ""), "_tested": x.get("tested", "")}
                          for x in items]}
    json.dump(data, open(path, "w", encoding="utf-8"), ensure_ascii=False, indent=1)
    return data


def main():
    corpus = load_corpus()
    print("语料库整句: %d 条" % len(corpus))
    nc, dc = num_cards(corpus), def_cards(corpus)
    bc = bank_cards("保密公开题-中科院.json")
    write_cards(OUT_NUM, "数字考点（安全管理规章原文 · %d 条）" % len(nc),
                "来源：crawl/数字考点速查.csv（441 条含数字的规范句，出处为书名 + 页码）。"
                "问题面是原文挖掉数字，答案面是完整原句 —— 生成题需要干扰项所以只出 60 道，"
                "卡片不需要选项，所以数字考点能全量背。带「已考」标记的说明题库里考过。", nc)
    write_cards(OUT_DEF, "考点解析（语料原文定义句 · %d 条）" % len(dc),
                "来源：本机语料库（kb.sqlite3）里「X 是指…」「X 包括：…」这类定义/清单句，"
                "只取安全管理规章/教材/手册，答案就是原文整句，不改写。", dc)
    write_cards(OUT_PUB, "保密公开卷（中科院 · 看题看答案 · %d 道）" % len(bc),
                "来源：banks/保密公开题-中科院.json（中国科学院官网公开的保密知识测试题）。"
                "背题页这一份是「看题 → 看答案」的走法，跟刷题页的判分走法共用同一批题。", bc)
    for path, items in ((OUT_NUM, nc), (OUT_DEF, dc), (OUT_PUB, bc)):
        ch = collections.Counter(x["chapter"] for x in items)
        print("已写 %s：%d 条 ｜ 章节 %d 个（%s）"
              % (os.path.basename(path), len(items), len(ch),
                 "、".join("%s %d" % kv for kv in ch.most_common(4))))
    if nc:
        print("  数字卡示例: %s → %s" % (nc[0]["q"][:52], nc[0]["a"][:52]))
    if dc:
        print("  解析卡示例: %s → %s" % (dc[0]["q"][:52], dc[0]["a"][:52]))
    if bc:
        print("  公开卷卡示例: %s → %s" % (bc[0]["q"][:44].replace("\n", " / "), bc[0]["a"][:44]))


def selftest():
    corpus = load_corpus()
    assert len(corpus) > 3000, "语料库整句太少：%d" % len(corpus)
    nc, dc = num_cards(corpus), def_cards(corpus)
    # 441 条数字考点里只有 221 条是「完整句 + 语料库里能对上原文」——其余是平台附件表格里
    # 被抽断的碎片（「灭：动火范围 10 米内应具备 2 个 4 公斤灭火器，根」），成不了卡，宁缺勿错
    assert len(nc) >= 200, "数字卡太少：%d" % len(nc)
    # 只有 22 条：语料里的定义句本来就少（多数规章是「必须/不得」祈使句，不是「X 是指 Y」），
    # 放宽到「X 是 …」会把「测验水体是否有电」这种半句当知识点收进来，所以宁少不乱
    assert len(dc) >= 15, "解析卡太少：%d" % len(dc)
    for tag, items in (("数字卡", nc), ("解析卡", dc)):
        seen = set()
        for it in items:
            q, a = it["q"].strip(), it["a"].strip()
            assert len(flat(q)) >= 12, "%s 题干太短：%r" % (tag, q[:40])
            assert len(flat(a)) >= 12, "%s 答案太短：%r" % (tag, a[:40])
            assert it["chapter"] and str(it["num"]).isdigit(), "%s 缺章节或序号：%r" % (tag, it["num"])
            assert flat(q) not in seen, "%s 题干重复：%r" % (tag, q[:40])
            seen.add(flat(q))
            assert "[object" not in q and "None" not in q, "%s 题干里有脏值：%r" % (tag, q[:40])
    for it in nc:                                   # 挖空句：空框在，答案里必须能找回那个数字
        assert BLANK in it["q"], "数字卡没挖空：%r" % it["q"][:50]
        assert it["a"].startswith(it["a"].split("　")[0].strip()), "答案面格式不对"
        num = it["a"].split("　")[0].strip()
        # 答案面里必须能找到这个数字（原文可能写作「200 kg」，压掉空白比）
        assert num and flat(num) in flat(it["a"]), "答案面丢了数字：%r" % it["a"][:50]
        assert it["page"], "数字卡缺页码：%r" % it["q"][:40]
        # 把答案填回空框，必须等于原句（证明挖空与答案是同一句）
        assert BLANK in it["q"]
        filled = flat(it["q"].replace(BLANK, num))
        assert filled in flat(it["a"].split("——")[1]), "填回数字后对不上原句：%r" % it["q"][:40]
    for it in dc:
        assert "是指什么？" in it["q"], "解析卡问法不对：%r" % it["q"][:40]
        assert not it["q"].startswith("是指"), "解析卡把词组切坏了：%r" % it["q"][:40]
    books = {it["book"] for it in dc}
    assert len(books) >= 5, "解析卡来源太集中：%s" % list(books)[:5]
    bc = bank_cards("保密公开题-中科院.json")
    assert len(bc) >= 600, "公开卷卡太少：%d" % len(bc)
    assert all(x["q"].strip() and x["a"].startswith("答案：") for x in bc), "公开卷卡有空的题目或答案面"
    print("selftest OK：数字卡 %d 条（章节 %d 个，全带页码与已考标记）、解析卡 %d 条（来自 %d 份材料）、"
          "公开卷卡 %d 道；"
          "挖空句填回数字 == 原文整句、题干唯一、答案非空都验过"
          % (len(nc), len({x["chapter"] for x in nc}), len(dc), len(books), len(bc)))


if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1] == "selftest":
        selftest()
    else:
        main()
