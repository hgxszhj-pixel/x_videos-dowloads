#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""两件事：
1) 从教材 PDF 文字里抽出内嵌的「小节测试」题目（平台自带的题，可当题库源）
2) 对用户 6 道存疑题做宽范围深搜（术语/数字两种写法都试）
"""
import glob, json, os, re

D = "/Users/hgxszhj/x_videos-dowloads/exam-lookup/jr/out/pdf"
FILES = []
for f in sorted(glob.glob(os.path.join(D, "*.txt"))):
    t = open(f, encoding="utf-8", errors="ignore").read()
    t = re.sub(r"[ \t\u00a0\u3000]+", "", t)
    FILES.append((os.path.basename(f)[:-4], t))

# ---------- 1. 抽内嵌测试题 ----------
print("=" * 78)
print("一、教材内嵌的「小节测试」题")
print("=" * 78)
qbank = []
for name, txt in FILES:
    # 题号 + 题干 + 选项(A、B、C、D…)
    for m in re.finditer(r"(\d{1,2})[、.．]\s*([^（）()\n]{6,90})[（(]\s*[）)]\s*((?:[A-D][、.．][^A-D\n]{1,60}){2,6})", txt):
        num, stem, opts = m.group(1), m.group(2).strip(), m.group(3)
        ol = re.findall(r"([A-D])[、.．]\s*([^A-D\n]{1,60})", opts)
        if len(ol) < 2:
            continue
        qbank.append({"src": name, "num": num, "stem": stem,
                      "options": [{"k": k, "v": v.strip()} for k, v in ol]})
    # 判断题
    for m in re.finditer(r"(\d{1,2})[、.．]\s*([^（）()\n]{6,90})[（(]\s*[）)]", txt):
        stem = m.group(2).strip()
        if re.search(r"[A-D][、.．]", stem):
            continue
        if any(k in stem for k in ("判断", "正确的有", "说法")):
            qbank.append({"src": name, "num": m.group(1), "stem": stem, "options": []})
print("抽出 %d 道内嵌题（来自教材自测）\n" % len(qbank))
for q in qbank[:12]:
    print("  [%s] %s. %s" % (q["src"][:22], q["num"], q["stem"][:66]))
    for o in q["options"]:
        print("        %s、%s" % (o["k"], o["v"][:56]))
json.dump(qbank, open("/Users/hgxszhj/x_videos-dowloads/exam-lookup/jr/out/教材内嵌题.json", "w",
                      encoding="utf-8"), ensure_ascii=False, indent=1)

# ---------- 2. 存疑题深搜 ----------
DEEP = {
    "20 随身工具总重量": r"随身携带工具[^。]{0,40}|不准超过\s*\d+\s*(Kg|kg|公斤|千克)|总重量",
    "26 培训档案 保存期限": r"培训档案|档案[^。]{0,30}保存|保存[^。]{0,20}(年|期限)|一人一档|档案管理",
    "38 水平桩/中心桩/标桩 高出路面": r"水平桩|中心桩|高程|不得高出路面|高出路面\s*\d+|回土夯实",
    "44 安全带 试验 周期": r"安全带[^。]{0,15}(试验|检验|测试)|可靠性试验|\d+\s*年[^。]{0,10}(试验|检验)|试验[^。]{0,10}一次",
    "54 应急演练 组织": r"应急演练|演练[^。]{0,35}|每年[^。]{0,10}(演练|次)|地市分公司[^。]{0,30}",
    "73/35 气体 复测间隔": r"每\s*\d+\s*分钟|每隔[^。]{0,12}|复测|适时检测|持续检测|气体检测",
}
print("\n" + "=" * 78)
print("二、6 道存疑题深搜（全 21 份教材）")
print("=" * 78)
for label, pat in DEEP.items():
    print("\n【%s】" % label)
    shown = 0
    for name, txt in FILES:
        for m in re.finditer(pat, txt):
            s = max(0, m.start() - 110); e = min(len(txt), m.end() + 110)
            print("  · [%s] …%s…" % (name[:26], txt[s:e].replace("\n", " ")))
            shown += 1
            if shown >= 5: break
        if shown >= 5: break
    if not shown:
        print("  （未命中）")
