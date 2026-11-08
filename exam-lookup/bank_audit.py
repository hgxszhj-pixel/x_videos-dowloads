#!/usr/bin/env python3
"""bank_audit —— 题库数据质量审计：先从数据内部找出「不用外部源也能判定是错」的答案。

四类可判定的数据错误（都不需要考试宝、不需要推理）：
    E1 答案键不在选项里        answerKeys 有 X，但 options 里没有 X
    E2 跨库同一题答案矛盾      归一化题干相同（或高度相似），answerKeys 不同 → 至少有一边错
    E3 选项重复/缺失           同一题里两个选项文本完全相同，或选项数 < 2
    E4 判断题格式异常          判断题但选项不是 正确/错误 二元

另外量化：
    - 每库题数、单选/多选/判断占比、含数字或期限的题占比（这类最易随政策变而失效）
    - 结合 answers 表：无本地依据占比、冲突占比、high 风险题构成

用法: python3 bank_audit.py [--out jr/out/bank_audit.md]
"""
import argparse
import glob
import json
import os
import re
import sys
from collections import defaultdict

PROJ = os.path.expanduser("~/x_videos-dowloads/exam-lookup")
sys.path.insert(0, PROJ)
import kb_db as KB  # noqa: E402

NUM_RE = re.compile(r"\d+\s*(天|日|小时|分钟|年|月|工作日|米|秒|%|％)")
JUDGE_OPTS = {"正确", "错误", "对", "错", "√", "×"}


def load(path):
    d = json.load(open(path, encoding="utf-8"))
    return d if isinstance(d, list) else (d.get("questions") or d.get("items") or [])


def stem_key(s):
    s = KB.normalize(s or "")
    s = re.sub(r"[（(]\s*[）)]", "", s)
    return re.sub(r"[\s，。；：、？！,.?;:!\"'“”‘’]", "", s)


def is_multi(q):
    t = str(q.get("typeLabel") or q.get("type") or "")
    return ("多" in t) or len(q.get("answerKeys") or []) > 1


def is_judge(q):
    t = str(q.get("typeLabel") or q.get("type") or "")
    if "判断" in t:
        return True
    opts = q.get("options") or []
    texts = {str(o.get("text") or "").strip() for o in opts}
    return len(opts) == 2 and texts and texts <= JUDGE_OPTS


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default=os.path.join(PROJ, "jr/out/bank_audit.md"))
    a = ap.parse_args()

    banks = sorted(f for f in glob.glob(os.path.join(PROJ, "banks/*.json")))
    all_qs = []
    per_bank = []
    for path in banks:
        name = os.path.basename(path)
        qs = load(path)
        multi = sum(1 for q in qs if is_multi(q))
        judge = sum(1 for q in qs if is_judge(q))
        num = sum(1 for q in qs if NUM_RE.search(q.get("question") or ""))
        per_bank.append({"bank": name, "n": len(qs), "multi": multi, "judge": judge,
                         "num": num})
        for q in qs:
            q["__bank"] = name
            all_qs.append(q)

    # ---- E1 答案键不在选项里 ----
    e1 = []
    for q in all_qs:
        keys = {str(k).upper() for k in (q.get("answerKeys") or [])}
        opt_keys = {str(o.get("key") or "").upper() for o in (q.get("options") or [])}
        bad = keys - opt_keys
        if bad:
            e1.append((q, sorted(bad)))

    # ---- E2 跨库同题答案矛盾 ----
    groups = defaultdict(list)
    for q in all_qs:
        k = stem_key(q.get("question"))
        if len(k) >= 12:
            groups[k].append(q)
    e2 = []
    for k, g in groups.items():
        answers = {tuple(sorted(str(x).upper() for x in (q.get("answerKeys") or []))) for q in g}
        answers.discard(tuple())
        if len(answers) > 1:
            # 只在选项文本也一致时才算「同一道题」——否则可能是同名不同题
            opts_sig = {tuple(sorted(str(o.get("text") or "") for o in (q.get("options") or []))) for q in g}
            if len(opts_sig) == 1:
                e2.append((g, sorted(answers)))

    # ---- E3 选项重复/缺失 ----
    e3 = []
    for q in all_qs:
        texts = [str(o.get("text") or "").strip() for o in (q.get("options") or [])]
        if len(texts) < 2 or (len(set(texts)) < len(texts) and texts):
            e3.append(q)

    # ---- E4 判断题异常 ----
    e4 = [q for q in all_qs if is_judge(q) and len(q.get("options") or []) != 2]

    # ---- 结合 answers 表 ----
    db = KB.connect(readonly=True)
    local = {(b, qid): (st, conf) for b, qid, st, conf in
             db.execute("SELECT bank, qid, status, confidence FROM answers")}

    def risk(q):
        v = local.get((q["__bank"], q.get("id") or q.get("number")))
        return v if v else ("?", "?")

    hi = []
    for q in all_qs:
        st, conf = risk(q)
        if st == "conflict" and conf == "high":
            hi.append(q)
    db.close()

    # ---- 报告 ----
    lines = ["# 题库数据质量审计", ""]
    lines.append("生成时间 2026-10-26　题库 %d 个　题目 %d 道" % (len(banks), len(all_qs)))
    lines.append("")
    lines.append("## 一、不依赖外部源就能判定为错的问题")
    lines.append("")
    lines.append("| 类型 | 数量 | 说明 |")
    lines.append("|---|---|---|")
    lines.append("| E1 答案键不在选项里 | %d | answerKeys 指向不存在的选项，机械可判 |" % len(e1))
    lines.append("| E2 跨库同题答案矛盾 | %d | 题干与选项全同、答案不同 → 至少一边错 |" % len(e2))
    lines.append("| E3 选项重复或不足 | %d | 选项少于 2 个，或两个选项文本完全相同 |" % len(e3))
    lines.append("| E4 判断题格式异常 | %d | 判为判断题但选项不是二元 |" % len(e4))
    lines.append("")
    if e1:
        lines.append("### E1 明细（前 10）")
        lines.append("")
        for q, bad in e1[:10]:
            lines.append("- `%s` %s　答案 %s 不在选项 %s 里" % (
                q.get("id"), (q.get("question") or "")[:50],
                ",".join(bad), ",".join(sorted(str(o.get("key")) for o in q.get("options") or []))))
        lines.append("")
    if e2:
        lines.append("### E2 明细（跨库矛盾，前 10）")
        lines.append("")
        for g, answers in e2[:10]:
            lines.append("- %s" % (g[0].get("question") or "")[:70])
            for q in g:
                lines.append("  　- %s `%s` → %s" % (q["__bank"], q.get("id"),
                                                     ",".join(q.get("answerKeys") or [])))
        lines.append("")

    lines.append("## 二、各库构成")
    lines.append("")
    lines.append("| 题库 | 题数 | 多选 | 判断 | 含数字/期限 | 无本地依据 | high 冲突 |")
    lines.append("|---|---|---|---|---|---|---|")
    for b in per_bank:
        name = b["bank"]
        noev = sum(1 for q in all_qs if q["__bank"] == name and risk(q)[1] in ("low", "none"))
        hc = sum(1 for q in all_qs if q["__bank"] == name and risk(q) == ("conflict", "high"))
        lines.append("| %s | %d | %d | %d | %d | %d | %d |" % (
            name, b["n"], b["multi"], b["judge"], b["num"], noev, hc))
    lines.append("")
    lines.append("## 三、high 风险题的构成")
    lines.append("")
    lines.append("- 合计 %d 道：单选 %d／多选 %d／判断 %d" % (
        len(hi), sum(1 for q in hi if not is_multi(q) and not is_judge(q)),
        sum(1 for q in hi if is_multi(q)), sum(1 for q in hi if is_judge(q))))
    num_hi = sum(1 for q in hi if NUM_RE.search(q.get("question") or ""))
    lines.append("- 其中题干含数字/期限（政策一变就失效）：%d 道（%.0f%%）" % (
        num_hi, 100.0 * num_hi / max(1, len(hi))))
    lines.append("")


    # ---- 四、分布类分析：重复度、答案字母偏差、判断题倾向、期限题清单 ----
    uniq = {stem_key(q.get("question")) for q in all_qs if stem_key(q.get("question"))}
    dup = len(all_qs) - len(uniq)

    single = [q for q in all_qs if not is_multi(q) and not is_judge(q)]
    letter = defaultdict(int)
    for q in single:
        for k in (q.get("answerKeys") or []):
            letter[str(k).upper()] += 1
    total_single_ans = sum(letter.values()) or 1

    judge = [q for q in all_qs if is_judge(q)]

    def judge_true(q):
        keys = [str(k).upper() for k in (q.get("answerKeys") or [])]
        for o in q.get("options") or []:
            if str(o.get("key") or "").upper() in keys:
                if str(o.get("text") or "").strip() in ("正确", "对", "√", "是"):
                    return True
        return False

    j_true = sum(1 for q in judge if judge_true(q))

    lines.append("## 四、分布分析")
    lines.append("")
    lines.append("- 唯一题干 **%d** 道，重复题干 **%d** 道（重复率 %.0f%%）" % (
        len(uniq), dup, 100.0 * dup / max(1, len(all_qs))))
    lines.append("- 单选题答案字母分布：%s（总数 %d）" % (
        "／".join("%s %d(%.0f%%)" % (k, v, 100.0 * v / total_single_ans)
                  for k, v in sorted(letter.items())), total_single_ans))
    if judge:
        lines.append("- 判断题 %d 道，其中答案为「正确」%d 道（%.0f%%）" % (
            len(judge), j_true, 100.0 * j_true / len(judge)))
    period = [q for q in all_qs if NUM_RE.search(q.get("question") or "")]
    lines.append("- 题干含数字/期限的题 %d 道（政策一变就失效，需优先核对）" % len(period))
    lines.append("")
    if period:
        lines.append("### 数字/期限题清单")
        lines.append("")
        for q in sorted(period, key=lambda x: x["__bank"]):
            lines.append("- [%s] `%s` %s" % (q["__bank"], q.get("id"),
                                            (q.get("question") or "")[:80]))
        lines.append("")

    md = "\n".join(lines)
    open(a.out, "w", encoding="utf-8").write(md)
    print(md[:2600])
    print("\n...\n报告 → %s" % a.out)
    return 0


if __name__ == "__main__":
    sys.exit(main())
