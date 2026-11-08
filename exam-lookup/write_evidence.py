#!/usr/bin/env python3
"""write_evidence —— 把「答案出处」写回题库，并把待外部核对的题单列。

依据来自 answer_evidence.json（逐题回锚结果）。只加字段，不改答案：
    evidence            原文原句（最强的那一句）
    evidenceDoc         出处文档名
    evidenceStrength    strong / weak
    answerVerified      核对日期
    evidenceGap         true（无依据，待外部核对）

用法:
    python3 write_evidence.py --dry-run
    python3 write_evidence.py --yes
"""
import argparse
import json
import os
import shutil
import sys
import time

PROJ = os.path.expanduser("~/x_videos-dowloads/exam-lookup")
OUT = os.path.join(PROJ, "jr/out")
BANK = os.path.join(PROJ, "banks/dtzx-yungai.json")
EV = os.path.join(OUT, "answer_evidence.json")
sys.path.insert(0, PROJ)
import kb_db as KB          # noqa: E402
import kb_answer_resolve as R  # noqa: E402

STRONG = "有依据（强）"
WEAK = "有依据（弱）"
TODAY = time.strftime("%Y-%m-%d")


def stem_sim(a, b):
    ga, gb = KB.cjk_bigrams(a), KB.cjk_bigrams(b)
    return len(ga & gb) / len(ga) if ga and gb else 0.0


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--yes", action="store_true")
    ap.add_argument("--dry-run", action="store_true")
    a = ap.parse_args()

    ev = json.load(open(EV, encoding="utf-8"))
    data = json.load(open(BANK, encoding="utf-8"))
    qs = data if isinstance(data, list) else (data.get("questions") or [])
    before = {q.get("id") or q.get("number"): list(q.get("answerKeys") or []) for q in qs}

    matched = strong = weak = gap = 0
    pending = []
    for e in ev:
        if not e.get("evidence") and e["verdict"] != "无依据":
            continue
        best, best_s = None, 0.0
        for q in qs:
            s = stem_sim(e["stem"], q.get("question") or "")
            if s > best_s:
                best, best_s = q, s
        if not best or best_s < 0.62:
            continue
        matched += 1
        if e["verdict"] in (STRONG, WEAK):
            if e["verdict"] == STRONG:
                strong += 1
            else:
                weak += 1
            best["evidence"] = e["evidence"][:300]
            best["evidenceDoc"] = e["doc"]
            best["evidenceStrength"] = "strong" if e["verdict"] == STRONG else "weak"
            best["answerVerified"] = TODAY
            best.pop("evidenceGap", None)
        elif e["verdict"] == "无依据":
            gap += 1
            best["evidenceGap"] = True
            pending.append((e["no"], e["type"], e["ans"], e["stem"]))

    # 答案一字未动
    after = {q.get("id") or q.get("number"): list(q.get("answerKeys") or []) for q in qs}
    changed = [k for k in before if before[k] != after.get(k)]
    assert not changed, "答案被改动了！%s" % changed[:5]

    print("匹配到库内同题 %d 道：写强依据 %d / 弱依据 %d / 标无依据 %d" % (matched, strong, weak, gap))
    print("答案未被改动（%d 题逐题比对一致）" % len(before))

    if a.yes and not a.dry_run:
        bak = BANK + ".bak-" + time.strftime("%Y%m%d-%H%M%S")
        shutil.copy(BANK, bak)
        json.dump(data, open(BANK, "w", encoding="utf-8"), ensure_ascii=False, indent=1)
        print("已写回 %s（备份 %s）" % (os.path.basename(BANK), os.path.basename(bak)))
        check = json.load(open(BANK, encoding="utf-8"))
        cq = check if isinstance(check, list) else (check.get("questions") or [])
        have = sum(1 for q in cq if q.get("evidence"))
        print("回读校验：带 evidence 字段的题 %d 道，JSON 可解析 ✓" % have)
    else:
        print("（dry-run，未写文件）")

    md = os.path.join(OUT, "pending_external_check.md")
    with open(md, "w", encoding="utf-8") as fp:
        fp.write("# 待外部核对的题（本地文档无依据）\n\n")
        fp.write("生成时间 %s　共 %d 道。这批在 187 份平台文档里找不到可引的原文，"
                 "建议走考试宝或人工确认。\n\n" % (TODAY, len(pending)))
        fp.write("| # | 题型 | 现有答案 | 题干 |\n|---|---|---|---|\n")
        for no, t, ans, stem in pending:
            fp.write("| %d | %s | %s | %s |\n" % (no, t or "—", ans or "—", (stem or "")[:60]))
    print("待核清单 → %s（%d 道）" % (md, len(pending)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
