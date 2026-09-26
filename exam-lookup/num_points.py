#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
num_points.py —— 数字型考点速查表（「几米 / 几公斤 / 几分钟 / 几年」）
========================================================================
零依赖（标准库；PDF 走 kb_pages 的 pdftotext 分页缓存）。

  python3 num_points.py            生成 crawl/数字考点速查.md + .csv
  python3 num_points.py selftest   自检（FAIL 会 exit 1）

为什么：综维装维安全卷的失分点大量是「数字」（5kg、2.5m、30 分钟、3 年、1cm、每季度…），
这些规定散在《管理办法》《规范》和 30 多本教材里，翻书找不快。本表把含数字的规范句抽出来，
标上「出处 + 页码」，再用题库里的题号标出**哪些数字已经考过**，复习直接对着表背。
"""
import glob
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import kb_pages                                              # noqa: E402

TOPICS = [                       # 安全考试的主题分桶：先出现的先认领
    ("高处/上杆作业", r"上杆|登高|杆上|安全带|脚扣|梯子|脚手架|防坠落|吊线|高处作业|杆梢|角杆"),
    ("有限空间作业", r"有限空间|人孔|人手孔|地下室|窖井|检查井|通风|气体检测|呼吸器|监护人"),
    ("动火与消防", r"动火|焊接|切割|灭火器|明火|易燃|易爆|消防|火源|气瓶|氧气"),
    ("电气安全", r"触电|接地|绝缘|验电|带电|停电|高压|电压|漏电|跨步|电弧|短路|kV|千伏|线路侧"),
    ("劳动防护用品", r"安全帽|绝缘鞋|手套|工作服|护目|防毒|防护用品|口罩|反光"),
    ("工器具与设备", r"脚扣|梯子|绳索|葫芦|滑轮|工具|仪表|三脚架|绞盘|抱箍|吊装|检测仪"),
    ("急救与事故处置", r"急救|心肺复苏|包扎|止血|伤员|伤亡|事故|报告|中毒|窒息|骨折|120"),
    ("交通与道路作业", r"交通|道路|车辆|警示|锥|反光|占道|施工标志"),
    ("管理与流程", r"审批|作业票|验收|交底|培训|预案|演练|台账|记录|检查|监护|开工|许可"),
]
OUT_MD = os.path.join(HERE, "crawl/数字考点速查.md")
OUT_SAFETY = os.path.join(HERE, "crawl/考前必背-综维安全数字.md")
OUT_CSV = os.path.join(HERE, "crawl/数字考点速查.csv")

NUM = r"(?:\d+(?:\.\d+)?|[一二三四五六七八九十百千两半]+)"
UNIT = r"(?:kg|KG|Kg|公斤|千克|斤|吨|毫米|cm|CM|厘米|米|m|M|km|千米|公里|伏|kV|KV|V|安|A|毫安|分钟|min|小时|h|秒|天|日|周|月|年|次|人|个|台|处|件|%|‰|度|级)"
CLAUSE = re.compile(r"%(n)s\s*%(u)s" % {"n": NUM, "u": UNIT})
NORM = re.compile(r"不得|不应|至少|不少于|不低于|不超过|不大于|不得超过|不能超过|必须|严禁|禁止|应|每|≤|≥|大于|小于|以内|以上|以下")
SPLIT = re.compile(r"[。；;!?！？\n]")
YEAR = re.compile(r"^(?:19|20)\d{2}(?:年|月|日)?$")          # 「2025年」这种是日期不是量值
UNIT_CANON = {"公斤": "重量", "千克": "重量", "kg": "重量", "斤": "重量", "吨": "重量",
              "毫米": "长度", "mm": "长度", "cm": "长度", "厘米": "长度", "米": "长度", "m": "长度",
              "km": "长度", "千米": "长度", "公里": "长度",
              "分钟": "时间", "min": "时间", "小时": "时间", "h": "时间", "秒": "时间",
              "年": "周期", "月": "周期", "日": "周期", "天": "周期", "周": "周期", "次": "频次",
              "%": "比例", "‰": "比例", "度": "其他", "级": "其他", "人": "人", "个": "个数",
              "台": "个数", "件": "个数", "处": "个数", "v": "电气", "伏": "电气", "kv": "电气",
              "a": "电气", "安": "电气", "毫安": "电气"}
CN = {"零": 0, "半": 0.5, "一": 1, "两": 2, "二": 2, "三": 3, "四": 4, "五": 5, "六": 6, "七": 7, "八": 8, "九": 9, "十": 10}


def to_value(tok):
    """把「20kg」「三十天」「2.5m」里的数值抠成 float（够用就行，不追求中文数字全解）"""
    m = re.match(r"(\d+(?:\.\d+)?)", tok)
    if m:
        return float(m.group(1))
    cn = re.match(r"([零半一两二三四五六七八九十百]+)", tok)
    if not cn:
        return 0.0
    t = cn.group(1)
    if t in CN:
        return float(CN[t])
    if t.startswith("十"):
        return 10.0 + CN.get(t[1:2], 0)
    if "十" in t:
        a, b = t.split("十", 1)
        return CN.get(a, 1) * 10 + (CN.get(b, 0) if b else 0)
    if t.endswith("百"):
        return CN.get(t[0], 1) * 100.0
    return float(CN.get(t[0], 0))


def unit_of(tok):
    m = re.match(r"(?:\d+(?:\.\d+)?|[零半一两二三四五六七八九十百千两]+)\s*(.+)$", tok)
    return UNIT_CANON.get((m.group(1) if m else "").lower(), "其他")


def flat(t):
    return re.sub(r"\s+", "", t or "")


def tokens_in(text):
    """文本里的「数字+单位」，带边界检查：P2MP 里的 2M、cm² 里的 cm 这种不算独立量值。"""
    out = set()
    t = flat(text)
    for m in CLAUSE.finditer(t):
        a, b = m.start(), m.end()
        if a > 0 and (t[a - 1].isalnum() and t[a - 1].isascii()):
            continue
        if b < len(t) and (t[b].isalnum() and t[b].isascii()):
            continue
        tok = m.group(0).lower()
        if not YEAR.match(tok):
            out.add(tok)
    return out


BULLET = re.compile(r"^[\süØ•·▪◦※\-—>·、,，)）(（【】\d.]+")


def clean(s):
    """去掉项目符号/编号前缀和首尾括号，别让表里全是「ü ...」「1个）」这种碎片"""
    s = BULLET.sub("", re.sub(r"\s+", " ", s or "").strip())
    return s.strip("、，,。；;）) ") or s


def short_book(b):
    """书名截到能认出来为止：砍掉【PPT学习材料】这类前缀，截断后别留半个括号"""
    b = re.sub(r"（【[^】]*】", "（", b)
    if re.fullmatch(r"[0-9a-f]{20,}", b.split("（")[0]):
        b = "平台附件 " + b[:8]                      # 平台下发的附件是 hash 名，标一下免得看着像乱码
    if len(b) > 34:
        b = b[:34]
        if "（" in b and "）" not in b:
            b = b.split("（")[0]                     # 先截断再判括号：先判会把「（…）书」这种整名保留下来
    return b


def sentences(page_text):
    """一页切成句子，做空白归一（pdftotext -layout 会在数字和单位之间塞空格）"""
    out = []
    for s in SPLIT.split(page_text):
        s = re.sub(r"[ \t\u3000]+", " ", s).strip()
        if 6 <= len(s) <= 200:
            out.append(s)
    return out


def bank_numbers():
    """题库里出现过的数字+单位 -> [(token, 题号, 文件)]，用于标「已考过」"""
    idx = {}
    for f in sorted(glob.glob(os.path.join(HERE, "banks", "*.json"))):
        name = os.path.basename(f)[:-5]
        j = json.load(open(f, encoding="utf-8"))
        qs = j["questions"] if isinstance(j, dict) and "questions" in j else j
        for q in qs:
            text = (q.get("question") or "")
            for o in q.get("options") or []:
                text += " " + ((o.get("text") or "") if isinstance(o, dict) else str(o))
            for tok in tokens_in(text):
                idx.setdefault(tok, []).append("%s#%s" % (name, q.get("number")))
    return idx


def collect():
    rows = []
    for book, pages_ in kb_pages.books():
        for i, pg in enumerate(pages_, 1):
            for s in sentences(pg):
                if not CLAUSE.search(flat(s)):
                    continue
                toks = tokens_in(s)
                toks = {t for t in toks if not YEAR.match(t)}
                if not toks:
                    continue                                  # 整句只有年份 → 不是量值考点
                norm = bool(NORM.search(s))
                for tok in toks:
                    rows.append({"book": short_book(book.split(" ┄ ")[0]), "file": book.split(" ┄ ")[-1], "page": i,
                                 "sent": clean(s), "token": tok, "norm": norm})
    return rows


def main():
    asked = bank_numbers()
    # 只留「有规范措辞」或「题库考过这个数字」的：否则表里全是「1 个」「2 台」这类噪声
    rows = [r for r in collect() if r["norm"] or r["token"] in asked]
    # 同句去重（同一句在多本教材重复出现时合并页码）
    merged = {}
    for r in rows:
        k = (r["sent"][:70], r["token"])
        m = merged.setdefault(k, {"book": r["book"], "file": r["file"], "pages": [], "sent": r["sent"], "token": r["token"], "norm": r["norm"]})
        if r["page"] not in m["pages"]:
            m["pages"].append(r["page"])
    out = []
    for m in merged.values():
        hits = sorted(set(asked.get(m["token"], [])))
        out.append((unit_of(m["token"]), m["token"], m["sent"], m["book"],
                    "%d" % m["pages"][0] if len(m["pages"]) == 1 else "%d(%d处)" % (m["pages"][0], len(m["pages"])),
                    "; ".join(hits[:6]) + (" …" if len(hits) > 6 else ""), "是" if m["norm"] else "-"))
    prio = ["重量", "长度", "时间", "周期", "频次", "电气", "比例", "人", "个数", "其他"]
    out.sort(key=lambda r: (prio.index(r[0]) if r[0] in prio else 99, to_value(r[1]), r[1], r[3], r[4]))

    n_hit = sum(1 for r in out if r[5])
    lines = ["# 数字型考点速查（含出处 + 页码）", "",
             "生成：`python3 num_points.py`　|　共 **%d** 条含数字的规范句，其中 **%d** 条的数字在题库里出现过" % (len(out), n_hit), "",
             "「出处/页」按 kb_pages 的分页（pdftotext -layout 按 \\\\f 切页，33 本 / 2031 页）。",
             "「题库题号」= 那个数字出现在哪些题的题干/选项里；空白表示题库还没考过这个数字（不等于考试不考）。",
             "「规范性」列 = 句子含「不得/至少/每/不超过」这类规范措辞（纯数据句不算）。", "",
             "| 类别 | 数字 | 原文 | 出处 | 页 | 题库题号 | 规范性 |", "|---|---|---|---|---|---|---|"]
    lines += ["| %s | %s | %s | %s | %s | %s | %s |" % (c, t, s.replace("|", "/"), b.replace("|", "/"), p, q.replace("|", "/"), f)
              for c, t, s, b, p, q, f in out]
    open(OUT_MD, "w", encoding="utf-8").write("\n".join(lines) + "\n")
    with open(OUT_CSV, "w", encoding="utf-8") as fh:
        fh.write("category,num,sentence,book,page,bank_questions,normative\n")
        for r in out:
            fh.write(",".join('"%s"' % str(x).replace('"', "'") for x in r) + "\n")
    print("数字考点：%d 条（其中 %d 条的数字在题库出现过）-> %s / %s" % (len(out), n_hit, OUT_MD, OUT_CSV))
    print("范例：")
    for r in out[:6]:
        print("   %-4s %-7s %s（%s 第%s页）" % (r[0], r[1], r[2][:46], r[3][:18], r[4]))


def topic_of(sent):
    for name, rx in TOPICS:
        if re.search(rx, sent):
            return name
    return "其他"


def safety_list():
    """考前必背单：只留安全主题的数字句，按主题分桶，桶内「题库考过」优先"""
    asked = bank_numbers()
    rows = [r for r in collect() if r["norm"] or r["token"] in asked]
    merged = {}
    for r in rows:
        k = (r["sent"][:70], r["token"])
        m = merged.setdefault(k, {"sent": r["sent"], "token": r["token"], "book": r["book"], "pages": [],
                                  "norm": r["norm"], "topic": topic_of(r["sent"])})
        if r["page"] not in m["pages"]:
            m["pages"].append(r["page"])
    items = []
    for m in merged.values():
        # 只认「安全类题库」的命中：1次/2年 这种通用数字在别的题库里到处出现，标 ✓ 就成了噪声
        hits = sorted(set(h for h in asked.get(m["token"], []) if "safety" in h))
        if m["topic"] == "其他" and not hits:
            continue                                  # 既不是安全主题、安全题库也没考过 → 不进必背单
        items.append({"topic": m["topic"], "token": m["token"], "sent": m["sent"], "book": m["book"],
                      "page": "%d" % m["pages"][0] if len(m["pages"]) == 1 else "%d(%d处)" % (m["pages"][0], len(m["pages"])),
                      "hits": hits, "cat": unit_of(m["token"])})
    order = [t for t, _ in TOPICS] + ["其他"]
    items.sort(key=lambda x: (order.index(x["topic"]), 0 if x["hits"] else 1, to_value(x["token"]), x["token"]))
    n_hit = sum(1 for x in items if x["hits"])
    L = ["# 考前必背 · 综维安全数字（按主题）", "",
         "生成：`python3 num_points.py 必背`　|　**%d 条**，其中 %d 条的数字在题库里出现过（标 ✓ 的是考过的）" % (len(items), n_hit), "",
         "用法：先背带 ✓ 的（题库已考过 = 命题人偏好），再扫本主题其余条目防冷门；「页」= 该规定在你手上那份 PDF 的第几页。", "",
         "来源：《中国电信综合维护及装维作业安全生产管理办法（试行）》+ 解读、YD5201-2014、线路安全作业规范、综维认证教材各分册、视频课件。", ""]
    for t in order:
        grp = [x for x in items if x["topic"] == t]
        if not grp:
            continue
        L += ["## %s　（%d 条，%d 条考过）" % (t, len(grp), sum(1 for x in grp if x["hits"])), "",
              "| ✓ | 数字 | 规定原文 | 出处 | 页 | 题库题号 |", "|---|---|---|---|---|---|"]
        L += ["| %s | %s | %s | %s | %s | %s |" % ("✓" if x["hits"] else "", x["token"], x["sent"].replace("|", "/"),
                                                 x["book"].replace("|", "/"), x["page"], "; ".join(x["hits"][:5]))
              for x in grp]
        L += [""]
    open(OUT_SAFETY, "w", encoding="utf-8").write("\n".join(L) + "\n")
    print("考前必背单：%d 条（其中 %d 条题库考过）-> %s" % (len(items), n_hit, OUT_SAFETY))
    for t in order:
        grp = [x for x in items if x["topic"] == t]
        if grp:
            print("   %-12s %3d 条（%d 考过）  例：%s" % (t, len(grp), sum(1 for x in grp if x["hits"]), grp[0]["sent"][:40]))


def selftest():
    rows = collect()
    assert len(rows) > 300, "抽出的数字句太少：%d" % len(rows)
    toks = {r["token"] for r in rows}
    for must in ("20kg", "200kg", "30分钟", "3年"):
        assert must in toks, "缺关键数字：%s（现有 %d 种）" % (must, len(toks))
    bank = bank_numbers()
    assert len([t for t in toks if t in bank]) >= 5, "和题库交叉命中的数字太少"
    # 页码有效性：抽到的页必须 ≤ 该书总页数
    byfile = {b.split(" ┄ ")[-1]: len(p) for b, p in kb_pages.books()}
    bad = [r for r in rows if r["page"] > byfile.get(r["file"], 10 ** 9)]
    assert not bad, "页码越界：%s" % bad[:2]
    if os.path.exists(OUT_SAFETY):
        st = open(OUT_SAFETY, encoding="utf-8").read()
        assert st.count("\n| ") >= 80, "必背单条目太少：%d" % st.count("\n| ")
        assert "上杆" in st or "有限空间" in st, "必背单缺主题"
    print("selftest OK：数字句 %d 条（含 20kg/200kg/30分钟/3年），与题库交叉命中 %d 个数字，页码均在书内（%d 本 / %d 页）"
          % (len(rows), len([t for t in toks if t in bank]), len(byfile), sum(byfile.values())))


if __name__ == "__main__":
    arg = sys.argv[1] if len(sys.argv) > 1 else ""
    if arg == "selftest":
        selftest()
    elif arg in ("必背", "safety"):
        safety_list()
    else:
        main()
