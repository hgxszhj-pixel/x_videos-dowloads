#!/usr/bin/env python3
"""kb_review_queue —— 把「本地没有依据的题」排成一份能照着做的人工核对清单。

为什么不靠考试宝：2026-10-27 账号被停用（见 考试宝-停用与替代方案.md），外部答案源已冻结，
剩下 569 道「语料无旁证」只能人工核对。人一天核不了 569 道，所以要排序：
先核「我考场上答错过的」和「库存答案与本地推导打架的」，再把难度低（语料里有近似段落）的排在前面。

排序口径（分数可解释，不掺魔法）：

    +40  我答错过（题库里 myWrong=true —— 这是唯一有真实考场结果的信号）
    +25  库存答案与本地推导不一致（status=conflict）
    +15  语料里能检索到近似段落（有候选出处，核对成本最低）
    +10  来自真题卷（question 带 examWhen / year）
    +10  多选题（选项组合更容易记错）

用法：
    python3 kb_review_queue.py build [--top N]     # → jr/out/review_queue.md + .csv
    python3 kb_review_queue.py selftest            # 不联网，校验清单本身的一致性
"""
import argparse
import csv
import glob
import json
import os
import re
import sqlite3
import sys

PROJ = os.path.expanduser("~/x_videos-dowloads/exam-lookup")
sys.path.insert(0, PROJ)
import kb_db as KB                    # noqa: E402
import kb_answer_resolve as R         # noqa: E402

OUT = os.path.join(PROJ, "jr/out")
SRC = os.path.join(OUT, "no_local_evidence.md")
MD = os.path.join(OUT, "review_queue.md")
CSV_OUT = os.path.join(OUT, "review_queue.csv")
MAT = os.path.join(OUT, "missing_materials.md")

# 「旧卷口径」识别词：这些说法只存在于已废止的 1988 版保密法 / 1990 年实施办法那一代，
# 2024 现行法里一个都没有（2026-10-27 全库检索确认）。命中的题按卷面答案记，但不当作现行口径依据。
OLD_ERA_TERMS = ("保密工作部门", "人事主管部门", "外交信使", "一事一批", "专职人员",
                 "核定的单位", "密级标志", "左上角")

# 缺语料那批题的归堆规则。探针关键词是拿去语料里做原文命中的，命中 0 份就是「资料没进来」。
GROUPS = [
    {"name": "保密法规（保密法 + 实施条例）",
     "slug": "baomi",
     "match": lambda b: "保密" in b or b.startswith("baomi"),
     "probe": ["保守国家秘密法", "保密法实施条例", "保密要害部门", "定密", "涉密人员"],
     "why": "考的是法条原文（密级、保密期限、定密权限、要害部门），语料里一份保密法都没有，"
            "制度类文档只能推出通用话术，推不出条号。",
     "need": [("中华人民共和国保守国家秘密法（2024 修订）", "国家法律法规数据库 flk.npc.gov.cn（公开全文）"),
              ("中华人民共和国保守国家秘密法实施条例", "同上"),
              ("机关单位保密工作管理规定（本单位发文）", "单位内网 / 保密办")]},
    {"name": "网络安全法规（网安法/数安法/个保法/等保）",
     "slug": "wangan",
     "match": lambda b: "网络安全" in b,
     "probe": ["网络安全法", "数据安全法", "个人信息保护法", "网络安全等级保护", "关键信息基础设施"],
     "why": "这几部法在语料里只有零星提及（1~6 份文档），没有全文；题目问的是条文里的具体义务、期限和主管机关。",
     "need": [("中华人民共和国网络安全法", "国家法律法规数据库"),
              ("中华人民共和国数据安全法", "同上"),
              ("中华人民共和国个人信息保护法", "同上"),
              ("GB/T 22239 网络安全等级保护基本要求", "国家标准全文公开系统 openstd.samr.gov.cn")]},
    {"name": "国家安全法规（国安法/反间谍法/情报法）",
     "slug": "guoan",
     "match": lambda b: "国家安全" in b,
     "probe": ["国家安全法", "反间谍法", "国家情报法", "总体国家安全观", "国家安全审查"],
     "why": "反间谍法在语料里 0 命中，国家安全法只有 2 份文档顺带提到；题目考法条与机关职责，通用文档推不出来。",
     "need": [("中华人民共和国国家安全法", "国家法律法规数据库"),
              ("中华人民共和国反间谍法（2023 修订）", "同上"),
              ("中华人民共和国国家情报法", "同上")]},
    {"name": "大数据与人工智能",
     "slug": "bigdata",
     "match": lambda b: "bigdata" in b or "大数据" in b,
     "probe": ["大数据", "人工智能", "数据治理", "机器学习"],
     "why": "主题覆盖不缺（各 50 份文档），缺的是题库要的那种具体口径：平台组件名、参数阈值、实验步骤。",
     "need": [("对应课程的配套题库/实验手册", "企业内部学习平台「我的学习」→ 课程附件")]},
    {"name": "天翼云与云安全",
     "slug": "tianyi",
     "match": lambda b: "tianyi" in b or "天翼" in b,
     "probe": ["天翼云", "云安全", "云平台安全", "等级保护"],
     "why": "产品侧的配置项与责任边界只在天翼云自己的产品文档里，通用安全教材覆盖不到。",
     "need": [("天翼云安全产品文档/白皮书", "天翼云官网文档中心"),
              ("云平台等保测评要求", "等保测评机构提供的模板")]},
    {"name": "OTN 与传输专业",
     "slug": "otn",
     "match": lambda b: "otn" in b or "传输" in b,
     "probe": ["OTN", "光传送网", "波分复用", "光信噪比"],
     "why": "传输专业教材在语料里有，但这些题问的是设备侧规程和具体指标口径。",
     "need": [("OTN 设备维护规程（厂商版）", "厂商/省公司运维规范库"),
              ("传输专业培训教材对应章节", "企业内部学习平台")]},
    {"name": "内部战略与本地制度（云改数转/自测/安全数字）",
     "slug": "internal",
     "match": lambda b: True,
     "probe": ["安全生产", "综维", "装维", "云改数转"],
     "why": "题目来自内部战略宣贯和本单位自测，答案口径只在宣贯材料/年度要点里，公开资料查不到。",
     "need": [("云改数转战略宣贯要点/年度工作要点", "单位内网 / 省公司发文"),
              ("对应自测卷的配套答案或培训课件", "内部学习平台 / 下发单位")]},
]


def parse_source(path=SRC):
    """旧的冻结清单（no_local_evidence.md），只留作对照；队列现在从 answers 表现场算。

    吃过一次亏：法规原文灌进库以后，这份 2026-10-26 的快照还是 569 条，谁照着它做就白做一半。
    """
    rows, bank = [], None
    if not os.path.exists(path):
        return rows
    for line in open(path, encoding="utf-8"):
        m = re.match(r"^##\s+(\S+\.json)", line.strip())
        if m:
            bank = m.group(1)
            continue
        m = re.match(r"^-?\s*`([^`]+)`\s*(.*)$", line.strip())
        if m and bank:
            rows.append({"bank": bank, "qid": m.group(1), "question": m.group(2).strip()})
    return rows


def live_source(db):
    """现场取题：answers 表里每一道有题干的题，覆盖率在 enrich 里现算。"""
    return [{"bank": b, "qid": q, "question": qn} for b, q, qn in
            db.execute("SELECT bank, qid, question FROM answers ORDER BY bank, qid")]


def load_banks(db):
    """bank -> qid -> 题目原始字段。数据源 = docs.sqlite 的 questions 表。

    为什么不再直接读 banks/*.json：题库回家后有两个消费者（刷题端 / 核对清单），
    各自读 json 就会各自漂；统一从库里取，导入时的格式校验与幂等才对清单生效。
    extra 列存的是非核心字段（typeLabel / myWrong / examWhen / year…），原样拆回来。
    """
    out = {}
    for bank, qid, qn, qtype, opts, ak, extra in db.execute(
            "SELECT bank, qid, question, type, options, answer_keys, extra FROM questions"):
        q = {"question": qn, "type": qtype,
             "options": json.loads(opts or "[]"), "answerKeys": json.loads(ak or "[]")}
        q.update(json.loads(extra or "{}"))
        out.setdefault(bank, {})[qid] = q
    return out


def direct_probe(db, question, nchunks, df_ratio=0.05, limit=6):
    """检索兜底：拿题干里最稀有的两个 bigram 直接 LIKE 查块。

    实测吃过亏：题干写「脱密期满后」，法条写「脱密期结束后」——打分把法条排在 top3 之外，
    看着像「缺语料」，其实原文就在库里（《保密法》第四十六条）。倒排打分不做同义兜底，
    这里用两条 LIKE 把这类漏网的捞回来。
    """
    qg = KB.cjk_bigrams(KB.query_terms(question or ""))
    if not qg:
        return []
    lim = max(3, int(nchunks * df_ratio))
    rare = sorted((g for g in qg if R.df_of(db, g) <= lim), key=lambda g: R.df_of(db, g))[:2]
    out = []
    for g in rare:
        for doc, text in db.execute(
                """SELECT d.name, c.text FROM chunks c JOIN documents d ON d.id=c.doc_id
                   WHERE c.text LIKE ? LIMIT ?""", ("%" + g + "%", limit)):
            out.append({"doc": doc, "text": text})
    return out


def norm_q(s):
    return re.sub(r"[\s（）()、，。；;：:？?！!\"'“”‘’]", "", s or "")


def human_when(v):
    """examWhen 有毫秒时间戳也有空值，统一成人看得懂的日期。"""
    s = str(v or "").strip()
    if s.isdigit() and len(s) >= 10:
        import time as _t
        sec = int(s[:13]) / (1000 if len(s) >= 13 else 1)
        try:
            return _t.strftime("%Y-%m-%d", _t.localtime(sec))
        except Exception:
            return s
    return s


def enrich(db, row, banks, nchunks):
    bank, qid = row["bank"], row["qid"]
    q = (banks.get(bank) or {}).get(qid) or {}
    stored = "/".join(q.get("answerKeys") or [])
    r = db.execute("""SELECT stored, derived, status, confidence, method, evidence_doc, evidence_text,
                             COALESCE(provenance,'local')
                      FROM answers WHERE bank=? AND qid=?""", (bank, qid)).fetchone()
    prov = r[7] if r else "local"
    if r:
        stored = stored or r[0] or ""
        derived, status, conf, method, edoc, etext = r[1], r[2], r[3], r[4] or "", r[5] or "", r[6] or ""
    else:
        derived, status, conf, method, edoc, etext = "", "unknown", "none", "", "", ""
    if not derived:
        # 表里有 103 条 status=conflict 但 derived 为空（早期口径留下的），那不是冲突，是没推出来
        status = "unknown"

    # 现场再检索一次：语料更新过，有些题现在能命中近似段落了（那就是最便宜的核对入口）
    hits = KB.search_with_cite(db, row["question"], topk=3) if row["question"] else []
    hits = hits + direct_probe(db, row["question"], nchunks)      # 同义表述的兜底
    cov, best = 0.0, None
    for h in hits:                        # 逐段算覆盖率，取最像的那一段，别把三段拼起来虚抬分数
        c, _ = R.rare_coverage(db, row["question"], h.get("text") or "", nchunks)
        if c >= cov:
            cov, best = c, h
    if best and not edoc:
        edoc = best.get("doc") or ""
        etext = (best.get("text") or "")[:200]

    era = "旧卷" if any(t in (row["question"] or "") for t in OLD_ERA_TERMS) else "现行"
    my_wrong = bool(q.get("myWrong"))
    multi = len(q.get("answerKeys") or []) > 1
    is_exam = bool(q.get("examWhen") or q.get("year"))
    # 稀有词覆盖 ≥50% 视为本地已有依据（不进清单）；30~49% 要人比一眼；<30% 是缺资料
    tier = "需比对" if cov >= 0.3 else "疑似缺语料"
    rec = {
        "bank": bank, "qid": qid, "question": row["question"] or (q.get("question") or ""),
        "options": q.get("options") or [],
        "stored": stored, "derived": derived, "status": status, "confidence": conf,
        "my_wrong": my_wrong, "multi": multi, "is_exam": is_exam,
        "coverage": round(100 * cov), "has_evidence": bool(hits), "tier": tier, "era": era,
        "evidence_doc": edoc, "evidence_text": etext,
        "provenance": prov,
        "type": q.get("typeLabel") or q.get("type") or "",
        "when": human_when(q.get("examWhen") or q.get("year")),
    }
    rec["score"] = score_of(rec)
    return rec


def score_of(r):
    """可解释的排序分：真实考场信号最重，其次才是口径冲突和核对成本。"""
    return (40 * r["my_wrong"] + 25 * (r["status"] == "conflict")
            + 15 * r["has_evidence"] + 10 * r["is_exam"] + 10 * r["multi"])


def build(a):
    db = KB.connect(readonly=True)
    nchunks = db.execute("SELECT COUNT(*) FROM chunks").fetchone()[0]
    banks = load_banks(db)
    src = live_source(db)
    merged = {}
    for r in (enrich(db, x, banks, nchunks) for x in src):
        if r["provenance"] == "manual":    # 已人工核验过的题不再进清单
            continue
        if r["coverage"] >= 50:            # 覆盖率 ≥50% 本地已有依据，不进人工清单
            continue
        k = norm_q(r["question"])          # 同一题会同时出现在「2024 卷」和「我的错题」，按题干去重
        if k and k in merged:
            m = merged[k]
            m["banks"] = m.get("banks") or [m["bank"]]
            if r["bank"] not in m["banks"]:
                m["banks"].append(r["bank"])
            m["my_wrong"] = m["my_wrong"] or r["my_wrong"]
            if r["coverage"] > m["coverage"]:
                for f in ("coverage", "tier", "evidence_doc", "evidence_text", "has_evidence"):
                    m[f] = r[f]
        else:
            r["banks"] = [r["bank"]]
            merged[k or r["qid"]] = r
    rows = list(merged.values())
    for r in rows:                          # 合并后 myWrong 可能由并集带来，分数重算
        r["score"] = score_of(r)
    db.close()
    rows.sort(key=lambda r: (-r["score"], -r["coverage"], r["bank"], r["qid"]))
    if a.top:
        rows = rows[:a.top]

    bands = [("第一批 · 考场答错过的（必核）", lambda r: r["my_wrong"]),
             ("第二批 · 库存与本地推导打架", lambda r: not r["my_wrong"] and r["status"] == "conflict"),
             ("第三批 · 其余（按可核对成本排序）", lambda r: not r["my_wrong"] and r["status"] != "conflict")]

    with open(MD, "w", encoding="utf-8") as f:
        f.write("# 人工核对清单（本地无依据的题）\n\n")
        f.write("生成时间 %s ｜ 共 %d 题 ｜ 口径：answers 表现场重算，稀有词覆盖 <50%% 才进清单\n\n"
                % (__import__("time").strftime("%Y-%m-%d %H:%M"), len(rows)))
        f.write("排序口径：+40 考场答错过 · +25 库存与推导冲突 · +15 有候选段落 · "
                "+10 真题 · +10 多选\n\n")
        tier_n = {t: sum(1 for r in rows if r["tier"] == t) for t in ("需比对", "疑似缺语料")}
        f.write("分级：需比对（稀有词覆盖 30–49%%）%d 题 · 疑似缺语料（<30%%）%d 题\n\n"
                % (tier_n["需比对"], tier_n["疑似缺语料"]))
        f.write("> 核对方法：先看「候选段落」，能直接定答案就改；候选段落不对题就是真缺语料，\n"
                "> 标 `缺语料` 并跳过——不要在没依据的情况下改答案。\n\n")
        for title, pred in bands:
            part = [r for r in rows if pred(r)]
            f.write("## %s（%d 题）\n\n" % (title, len(part)))
            for r in part:
                f.write("### %d ｜ %s / `%s` ｜ %s\n" % (r["score"], "、".join(r["banks"]),
                                                        r["qid"], r["type"] or "-"))
                f.write("%s\n\n" % r["question"].replace("（ ）", "（ ）"))
                for o in r["options"]:
                    f.write("    %s. %s\n" % (o.get("key"), o.get("text")))
                if r["options"]:
                    f.write("\n")
                flags = []
                if r["era"] == "旧卷":
                    flags.append("旧卷口径（1988/1990 版法规，不作现行依据）")
                if r["my_wrong"]:
                    flags.append("我答错过")
                if r["status"] == "conflict":
                    flags.append("库存 `%s` ≠ 推导 `%s`" % (r["stored"] or "-", r["derived"]))
                elif r["derived"]:
                    flags.append("库存 `%s`（本地推导一致 `%s`）" % (r["stored"] or "-", r["derived"]))
                else:
                    flags.append("库存 `%s`（本地未推出）" % (r["stored"] or "-"))
                if r["multi"]:
                    flags.append("多选")
                if r["when"]:
                    flags.append(str(r["when"]))
                f.write("　" + " · ".join(flags) + "\n")
                f.write("　分级 %s · 稀有词覆盖 %d%%\n" % (r["tier"], r["coverage"]))
                if r["has_evidence"] and r["tier"] == "疑似缺语料":
                    # 覆盖 <30% 的段落基本不对题，贴出来只会浪费核对时间
                    f.write("　候选段落：覆盖 %d%%，最接近《%s》，大概率不对题 → 建议标 `缺语料` 跳过\n"
                            % (r["coverage"], r["evidence_doc"]))
                elif r["has_evidence"]:
                    f.write("　候选段落（稀有词覆盖 %d%%）｜ %s\n　　%s\n"
                            % (r["coverage"], r["evidence_doc"], r["evidence_text"].strip()[:220]))
                else:
                    f.write("　候选段落：语料无命中 → 标 `缺语料`\n")
                f.write("\n")

    with open(CSV_OUT, "w", encoding="utf-8", newline="") as f:
        w = csv.writer(f)
        w.writerow(["rank", "score", "bank", "qid", "type", "question", "stored", "derived",
                    "status", "confidence", "tier", "era", "myWrong", "multi", "isExam", "coverage",
                    "evidence_doc", "evidence_text"])
        for i, r in enumerate(rows, 1):
            w.writerow([i, r["score"], "、".join(r["banks"]), r["qid"], r["type"], r["question"],
                        r["stored"], r["derived"], r["status"], r["confidence"], r["tier"], r["era"],
                        int(r["my_wrong"]), int(r["multi"]), int(r["is_exam"]),
                        r["coverage"], r["evidence_doc"], r["evidence_text"].strip()])

    n_ev = sum(1 for r in rows if r["has_evidence"])
    if getattr(a, "split", False):
        # 按主题把「需比对」那批拆成小清单：一次只处理一个库，比翻一份千行清单现实
        for g in GROUPS:
            part = [r for r in rows if group_of(r["banks"]) is g and r["tier"] == "需比对"]
            if not part:
                continue
            path = os.path.join(OUT, "review_%s.md" % g["slug"])
            with open(path, "w", encoding="utf-8") as f:
                f.write("# %s · 待比对清单（%d 道）\n\n" % (g["name"], len(part)))
                f.write("生成时间 %s ｜ 只收「需比对」档（稀有词覆盖 30–49%%）：\n"
                        "候选段落真在讲这道题，比对后能直接定答案；定不了的标 `缺语料` 跳过。\n\n"
                        % __import__("time").strftime("%Y-%m-%d %H:%M"))
                for i, r in enumerate(part, 1):
                    f.write("## %d ｜ %s / `%s`\n%s\n\n" % (i, "、".join(r["banks"]), r["qid"],
                                                          r["question"]))
                    for o in r["options"]:
                        f.write("    %s. %s\n" % (o.get("key"), o.get("text")))
                    f.write("\n　库存 `%s`%s · 覆盖率 %d%%\n" % (
                        r["stored"] or "-",
                        "（与推导 `%s` 冲突）" % r["derived"] if r["status"] == "conflict" else "",
                        r["coverage"]))
                    f.write("　出处：%s\n　　%s\n\n"
                            % (r["evidence_doc"], r["evidence_text"].strip()[:240]))
            print("  拆分 → review_%s.md（%d 道）" % (g["slug"], len(part)))
        old = [r for r in rows if r["era"] == "旧卷"]
        if old:
            path = os.path.join(OUT, "review_old_era.md")
            with open(path, "w", encoding="utf-8") as f:
                f.write("# 旧卷口径题（%d 道）\n\n" % len(old))
                f.write("生成时间 %s\n\n" % __import__("time").strftime("%Y-%m-%d %H:%M"))
                f.write("这些题问的是**已废止两轮的旧法**（1988 版保密法、1990 年实施办法那一代）：\n"
                        "「保密工作部门」「人事主管部门」「外交信使」「一事一批」等说法在 2024 现行法里\n"
                        "一个都没有（全库检索 0 段）。\n\n"
                        "处理口径：答案按平台卷面记（考试就这么考），但**不当作现行保密口径的依据**，\n"
                        "不计入「已核验」。核对时只需确认「卷面答案记对了」，不要拿现法去纠它。\n\n")
                for i, r in enumerate(old, 1):
                    f.write("## %d ｜ %s / `%s` ｜ %s\n%s\n\n" % (
                        i, "、".join(r["banks"]), r["qid"], r["tier"], r["question"]))
                    for o in r["options"]:
                        f.write("    %s. %s\n" % (o.get("key"), o.get("text")))
                    f.write("\n　库存 `%s` · 覆盖率 %d%%\n\n" % (r["stored"] or "-", r["coverage"]))
            print("  拆分 → review_old_era.md（%d 道）" % len(old))
    print("清单 %d 题（按题干去重后）→ %s" % (len(rows), MD))
    print("  %-34s %d 题" % ("第一批 考场答错过", sum(1 for r in rows if r["my_wrong"])))
    print("  %-34s %d 题" % ("第二批 库存≠推导", sum(1 for r in rows
                                                if not r["my_wrong"] and r["status"] == "conflict")))
    for t in ("需比对", "疑似缺语料"):
        print("  %-34s %d 题" % ("分级 " + t, sum(1 for r in rows if r["tier"] == t)))
    print("  %-34s %d 题" % ("有候选段落", n_ev))
    print("→ %s" % CSV_OUT)
    return 0


def selftest():
    """离线校验清单本身：条数一致、字段完整、排序单调、每条都给出「出处或明确无命中」。"""
    fails = []

    def check(name, ok, detail=""):
        print(("  ok   " if ok else "  FAIL ") + name + ("  " + detail if detail else ""))
        if not ok:
            fails.append(name)

    db = KB.connect(readonly=True)
    banks = load_banks(db)
    nchunks = db.execute("SELECT COUNT(*) FROM chunks").fetchone()[0]
    src = live_source(db)
    sample = [enrich(db, r, banks, nchunks) for r in src[:25]]
    nq = db.execute("SELECT COUNT(*) FROM questions").fetchone()[0]
    nbank = db.execute("SELECT COUNT(DISTINCT bank) FROM questions").fetchone()[0]
    db.close()

    check("answers 表能取到题", len(src) > 0, "%d 道" % len(src))
    # 题库来源必须是 questions 表：条数要对得上，且 non-core 字段要原样拆回来（不能丢 myWrong）
    check("题库读的是 questions 表（不是 banks/*.json）",
          len(banks) == nbank and sum(len(v) for v in banks.values()) == nq,
          "%d 库 / %d 题（表里 %d 库 / %d 题）"
          % (len(banks), sum(len(v) for v in banks.values()), nbank, nq))
    jmw = sum(1 for p in glob.glob(os.path.join(PROJ, "banks/*.json"))
              for q in R.bank_questions(p) if q.get("myWrong"))
    dmw = sum(1 for v in banks.values() for q in v.values() if q.get("myWrong"))
    check("json 里的 myWrong 经库后没丢", jmw == dmw, "json %d / 库 %d" % (jmw, dmw))
    check("每条都有题库与题号", all(r["bank"] and r["qid"] for r in src))
    check("每题都拿到题干", all(r["question"] for r in sample))
    check("每题都有「候选段落」或明确无命中",
          all(r["has_evidence"] == bool(r["evidence_doc"]) for r in sample))
    check("分数在 0~100 内", all(0 <= r["score"] <= 100 for r in sample),
          str(sorted({r["score"] for r in sample})))
    if os.path.exists(MD) and os.path.exists(CSV_OUT):
        rows = list(csv.DictReader(open(CSV_OUT, encoding="utf-8")))
        check("清单里没有覆盖率 ≥50% 的题",
              all(int(r["coverage"]) < 50 for r in rows),
              "最高 %s%%" % max((int(r["coverage"]) for r in rows), default="-"))
        ranks = [int(r["rank"]) for r in rows]
        check("名次从 1 连续", ranks == list(range(1, len(ranks) + 1)))
        md_heads = sum(1 for line in open(MD, encoding="utf-8") if line.startswith("### "))
        check("md 条目数等于 CSV 行数", md_heads == len(rows), "%d vs %d" % (md_heads, len(rows)))
        lack = [r for r in rows if r["tier"] == "疑似缺语料"]
        picks = [group_of(r["bank"].split("、")) for r in lack]
        check("每道缺语料题都能归到一个主题", len(picks) == len(lack) and all(picks),
              "%d 道 → %d 个主题" % (len(lack), len({p["name"] for p in picks})))
        check("用到的主题都有「为什么缺 + 补什么」",
              all(p["why"] and p["need"] for p in picks))
        check("era 取值只有「旧卷/现行」", {r["era"] for r in rows} <= {"旧卷", "现行"},
              str({r["era"] for r in rows}))
        check("标成旧卷的题都命中旧法说法",
              all(any(t in r["question"] for t in OLD_ERA_TERMS)
                  for r in rows if r["era"] == "旧卷"))
    else:
        print("  skip  md/csv 尚未生成（先跑 build）")

    print("\nselftest %s" % ("全部通过" if not fails else "失败: " + ", ".join(fails)))
    return 0 if not fails else 1


def group_of(banks):
    """归堆：一道题只进一个主题，避免重复计数。"""
    for g in GROUPS:
        if any(g["match"](b) for b in banks):
            return g
    return GROUPS[-1]


def materials(a):
    """把「缺语料」的题按主题归堆，并对语料做关键词探针，回答「到底缺哪份资料」。

    探针不看题、只看语料：某个法规名在 2787 个块里一次都没出现，那这批题就不是排序问题，
    是资料本身没进来 —— 补一份全文比调十次算法都值。
    """
    rows = [r for r in csv.DictReader(open(CSV_OUT, encoding="utf-8"))
            if r["tier"] == "疑似缺语料"] if os.path.exists(CSV_OUT) else []
    if not rows:
        print("先跑 build 生成 review_queue.csv")
        return 1
    db = KB.connect(readonly=True)
    by_group = {}
    for r in rows:
        g = group_of(r["bank"].split("、"))
        by_group.setdefault(g["name"], []).append(r)
    out, plan = [], []
    for g in GROUPS:
        mine = by_group.get(g["name"]) or []
        if not mine:
            continue
        probe = []
        for kw in g["probe"]:
            n = db.execute("SELECT COUNT(DISTINCT doc_id) FROM chunks WHERE text LIKE ?",
                           ("%" + kw + "%",)).fetchone()[0]
            probe.append((kw, n))
        out.append({"group": g, "n": len(mine), "probe": probe, "rows": mine})
        cost = max(1, len(g["need"]))
        plan.append((len(mine) / cost, g["name"], len(mine), cost))
    db.close()
    out.sort(key=lambda x: -x["n"])
    plan.sort(reverse=True)

    with open(MAT, "w", encoding="utf-8") as f:
        f.write("# 补料清单：缺语料那批题到底缺什么\n\n")
        f.write("生成时间 %s ｜ 覆盖 %d 道「疑似缺语料」的题 ｜ 来源 `review_queue.csv`\n\n"
                % (__import__("time").strftime("%Y-%m-%d %H:%M"), len(rows)))
        f.write("判据：把主题关键词在语料（187 份文档 / 2772 块）里做原文命中计数。\n"
                "**命中 0 份 = 语料里根本没有这份资料**，不是检索没排上来。\n\n")
        f.write("## 优先级（题数 ÷ 需要补的资料份数，越高越划算）\n\n")
        f.write("| 主题 | 缺语料题数 | 需补份数 | 每份资料能覆盖 |\n|---|---|---|---|\n")
        for ratio, name, n, cost in plan:
            f.write("| %s | %d | %d | %.0f 题 |\n" % (name, n, cost, ratio))
        f.write("\n")
        for item in out:
            g, mine = item["group"], item["rows"]
            f.write("## %s（%d 题）\n\n" % (g["name"], item["n"]))
            f.write("语料探针：\n\n")
            for kw, n in item["probe"]:
                f.write("    %-18s %s\n" % (kw, "缺（0 份文档）" if n == 0 else "%d 份文档有提及" % n))
            f.write("\n为什么要补：%s\n\n" % g["why"])
            f.write("建议补的资料：\n\n")
            for name, src in g["need"]:
                f.write("    - 《%s》\n      获取：%s\n" % (name, src))
            f.write("\n代表题（最前面的 3 道）：\n\n")
            for r in mine[:3]:
                f.write("    - `%s` %s\n" % (r["qid"], r["question"][:70]))
            f.write("\n")
        f.write("## 补完之后怎么用\n\n")
        f.write("    把新文档放进资料目录 → python3 kb_docs_sync.py sync（在线）或直接\n"
                "    python3 kb_docs_index.py 重建索引 → python3 kb_answer_resolve.py resolve\n"
                "    → python3 kb_review_queue.py build（这批题会从「疑似缺语料」升到本地已有依据，不再进清单）\n")

    print("补料清单 → %s" % MAT)
    for ratio, name, n, cost in plan:
        print("  %-16s %3d 题 / %d 份资料 → 每份覆盖 %2.0f 题" % (name, n, cost, ratio))
    return 0


def main():
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    b = sub.add_parser("build")
    b.add_argument("--top", type=int, default=0)
    b.add_argument("--split", action="store_true", help="按主题另存几份小清单（只看需比对的题）")
    sub.add_parser("materials")
    sub.add_parser("selftest")
    a = ap.parse_args()
    if a.cmd == "build":
        return build(a)
    if a.cmd == "materials":
        return materials(a)
    return selftest()


if __name__ == "__main__":
    sys.exit(main())
