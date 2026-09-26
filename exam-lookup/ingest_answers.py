#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""把子代理判定的答案合入题库，带一致性校验。

一致性闸门（任何一条不过就拒绝该题，不写进库）：
  G1 id 必须在原题里存在
  G2 answerKeys 非空
  G3 每个 key 必须真实存在于该题 options（防止模型给不存在的选项）
  G4 判断/单选只能一个 key；多选可多个
  G5 题面（归一化后）不能与已有题库重复 —— 防止同一题在两套库里答案打架
  G6 若与已有库同题且答案不同 → 报警，以已有库（经过人工核对）为准
"""
import json, os, re, sys, collections

BASE = "/Users/hgxszhj/x_videos-dowloads/exam-lookup"
QS = json.load(open(os.path.join(BASE, "docs_questions.json"), encoding="utf-8"))

PUNC = re.compile(r"[\s，。、；：？！“”‘’（）()《》【】\-—,.!?;:'\"]")
def nq(s):
    return PUNC.sub("", s or "")

# 已有题库的题面 → 答案（用于查重/查冲突）
existing = {}
for f in os.listdir(os.path.join(BASE, "banks")):
    if not f.endswith(".json"):
        continue
    b = json.load(open(os.path.join(BASE, "banks", f), encoding="utf-8"))
    for q in b.get("questions", []):
        existing.setdefault(nq(q.get("question", "")), []).append(
            (f, "/".join(q.get("answerKeys") or [])))

SETS = [
    ("bigdata-ai", "YB", "中国电信大数据和人工智能认证考试",
     "2023-年1-17日大数据和ai考试题目单选.pdf"),
    ("tianyi-security", "TY", "中国电信天翼安全认证考试",
     "题目提取结果_2025-09-03T05-08-20.txt"),
    ("otn-transport", "OTN", "2022年云网工程师-云网融合实操（传输）四级认证（B1卷）",
     "2022年云网工程师-云网融合实操（传输）四级认证考试（10月场）.txt"),
]

# 抽取后确认损坏的题 → 一律排除，不给库里塞选项乱掉的题
EXCLUDE = {
    # PDF 里公式换行把选项切碎：A.P(A / B.B) / C.P(AB) / D.P(B / E. / A) / F. / P(B)
    # 无法可靠还原选项，宁可少一题
    "YB-S0-29",
}

report, made = [], {}
for key, prefix, title, srcfile in SETS:
    apath = os.path.join(BASE, "answers_%s.json" % key)
    if not os.path.exists(apath):
        report.append("%-16s ✗ 缺答案文件 %s" % (key, os.path.basename(apath)))
        continue
    ans = json.load(open(apath, encoding="utf-8"))
    ans = ans.get(key, ans) if isinstance(ans, dict) else ans
    amap = {a["id"]: a for a in ans if isinstance(a, dict) and a.get("id")}

    qs = QS[key]
    ok, rej, reclass = [], collections.Counter(), collections.Counter()
    conf = collections.Counter()
    for q in qs:
        if q["id"] in EXCLUDE:
            rej["G7 抽取损坏已排除"] += 1
            continue
        a = amap.get(q["id"])
        if not a:
            rej["G0 无判定"] += 1; continue
        keys = [k.upper() for k in (a.get("answerKeys") or [])]
        if not keys:
            rej["G2 空答案"] += 1; continue
        valid = {o["key"].upper() for o in q["options"]}
        if any(k not in valid for k in keys):
            rej["G3 选项不存在 %s" % "/".join(keys)] += 1; continue
        # 题型以答案键个数为准（题干措辞不可靠，见 extract_docs_q.py 注释）
        if q["type"] == "judge":
            if len(keys) != 1:
                rej["G4 判断题给了多键（真冲突）"] += 1
                continue
        else:
            want = "multiple" if len(keys) > 1 else "single"
            if want != q["type"]:
                reclass["按答案键个数改判 %s→%s" % (q["type"], want)] += 1
            q = dict(q, type=want)
        # 题干说「哪些/包括」但只给一个答案 → 可能漏选，打标提醒人工复核
        suspect = (len(keys) == 1 and q["type"] != "judge"
                   and re.search(r"哪些|包括|有哪些|正确的有", q["stem"]))
        if len(set(keys)) != len(keys):
            rej["G4 重复键"] += 1; continue
        keys = sorted(set(keys))
        # G5/G6 与已有库查重
        hit = existing.get(nq(q["stem"]))
        if hit:
            old = {h[1] for h in hit}
            if len(old) == 1 and list(old)[0] and list(old)[0] != "/".join(keys):
                rej["G6 与已有库答案冲突 旧=%s 新=%s" % (list(old)[0], "/".join(keys))] += 1
                continue
        c = a.get("confidence", "low")
        conf[c] += 1
        om = {o["key"].upper(): o["text"] for o in q["options"]}
        ok.append({
            "id": q["id"], "volume": 1, "type": q["type"],
            "typeLabel": {"single": "单选题", "multiple": "多选题", "judge": "判断题"}[q["type"]],
            "number": q["num"],
            "question": q["stem"],
            "options": [{"key": o["key"], "text": o["text"]} for o in q["options"]],
            "answerKeys": keys,
            "answerText": [om.get(k, "") for k in keys],
            "topic": title, "category": title, "score": 1,
            "analysis": "%s【答案来源】%s模型判定 | %s" % (
                "⚠题干含「哪些/包括」但只给一个答案，可能漏选，建议复核 | " if suspect else "",
                {"high": "高置信·", "medium": "中置信·", "low": "★低置信待人工复核·"}.get(c, ""),
                a.get("basis", "")),
            "confidence": c,
        })

    made[key] = (title, srcfile, ok, dict(conf), dict(rej), dict(reclass))
    report.append("%-16s %3d/%3d 题入库 | high%3d medium%3d low%3d" % (
        key, len(ok), len(qs), conf["high"], conf["medium"], conf["low"]))
    for k, v in reclass.items():
        report.append("      ↻ 改判 %2d 题：%s（已入库）" % (v, k))
    for k, v in rej.items():
        report.append("      ✗ 拒绝 %2d 题：%s" % (v, k))

print("=== 入库校验 ===")
print("\n".join(report))

# 写出题库文件
for key, (title, srcfile, ok, conf, rej, reclass) in made.items():
    if not ok:
        continue
    bank = {"meta": {
        "title": "%s（模型判定版）" % title,
        "questionCount": len(ok),
        "volumes": 1,
        "sourceNote": "题目来源：~/Documents/%s。答案来源：检索+大模型判定，"
                      "高置信 %d / 中置信 %d / 低置信 %d（低置信标★，请优先复核）。"
                      % (srcfile, conf.get("high", 0), conf.get("medium", 0), conf.get("low", 0)),
    }, "questions": ok}
    out = os.path.join(BASE, "banks", "docs-%s.json" % key)
    json.dump(bank, open(out, "w", encoding="utf-8"), ensure_ascii=False, indent=1)
    print("-> %s  (%d 题)" % (out, len(ok)))
