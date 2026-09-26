#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""在官方教材 PDF 文字里搜用户 6 道存疑题的确数。

搜的是 PDF 原文，不是我的推断 —— pdftotext -layout 会带多余空格，先压平空白再匹配。
"""
import glob, os, re, sys

D = "/Users/hgxszhj/x_videos-dowloads/exam-lookup/jr/out/pdf"

# 题号 -> (说明, 一组正则)
QUERIES = [
    ("20 脚扣上杆随身工具总重量 5kg", r"(工具|器具|材料)[^。；\n]{0,30}(重量|不得超过|超过)[^。；\n]{0,30}|不得超过[^。；\n]{0,12}(公斤|千克|kg)|5\s*(公斤|千克|kg)"),
    ("21 定向钻孔钻机与电力线安全距离 2.5m", r"定向钻|非开挖|顶管[^。；\n]{0,40}(电力|电缆)[^。；\n]{0,40}|2\.5\s*(米|m|M)"),
    ("26 培训档案保存期限 3年", r"培训档案|档案[^。；\n]{0,20}保存[^。；\n]{0,20}|不少于\s*\d+\s*年"),
    ("38 水平桩/中心桩不得高出路面 1cm", r"水平桩|中心桩|高程桩|标桩[^。；\n]{0,40}|高出路面|路面[^。；\n]{0,20}(cm|厘米)"),
    ("44 安全带每2年1次承重测试", r"安全带[^。；\n]{0,60}(试验|测试|检验|承重)|(试验|测试|检验)[^。；\n]{0,20}(周期|每年|每\s*\d+\s*年)"),
    ("54 组织地市分公司应急演练", r"应急演练|演练[^。；\n]{0,40}(每年|每\s*\d+\s*(次|月|季))|地市[^。；\n]{0,20}演练"),
    ("73/35 有限空间气体复测间隔 15/30分钟", r"有限空间|气体[^。；\n]{0,30}(检测|复测|监测)|每\s*\d+\s*分钟[^。；\n]{0,20}(复测|检测|记录)"),
]

FILES = []
for f in glob.glob(os.path.join(D, "*.txt")):
    t = open(f, encoding="utf-8", errors="ignore").read()
    t = re.sub(r"[ \t\u00a0\u3000]+", "", t)      # 压平空格（pdftotext -layout 会插很多）
    FILES.append((os.path.basename(f)[:-4], t))
print("载入 %d 份教材文字，合计 %d 万字\n" % (len(FILES), sum(len(t) for _, t in FILES) // 10000))

for label, pat in QUERIES:
    print("=" * 76)
    print("【%s】" % label)
    hits = 0
    for name, txt in FILES:
        for m in re.finditer(pat, txt):
            s = max(0, m.start() - 90); e = min(len(txt), m.end() + 90)
            print("  [%s]" % name[:34])
            print("     …%s…" % txt[s:e].replace("\n", " "))
            hits += 1
            if hits >= 6: break
        if hits >= 6: break
    if not hits:
        print("  （这些教材里没搜到相关条文）")
    print()
