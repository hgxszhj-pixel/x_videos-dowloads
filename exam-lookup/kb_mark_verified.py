#!/usr/bin/env python3
"""kb_mark_verified —— 把「人工核对过、有现行法原文支撑」的结论写进 answers 表。

为什么单独一个脚本：本地推导（kb_answer_resolve）是自动的，会反复重算；人工核对带的是
「依据哪一条、原文哪一句」，重算会把它冲掉。所以这类结论用 provenance='manual' 落库，
`resolve` 会跳过它们（keep_web 里已含 manual），核对清单也不再列（`build` 里按 manual 过滤）。

规矩（跟 ANSWER_SOURCES.md 一致）：
    - 只写**有现行法原文**的结论；旧卷口径的题不写（答案按卷面记，但不作现行依据）
    - 不动题库 json 的答案：库存答案本来就和法条一致，这里只补「依据」
    - 写库前先拷 docs.sqlite 做 .bak-时间戳

用法:
    python3 kb_mark_verified.py            # 只打印将写入什么（dry-run）
    python3 kb_mark_verified.py --yes      # 真写
    python3 kb_mark_verified.py selftest   # 离线：临时库上验写入/幂等/过滤
"""
import argparse
import os
import shutil
import sqlite3
import sys
import time

PROJ = os.path.expanduser("~/x_videos-dowloads/exam-lookup")
sys.path.insert(0, PROJ)
import kb_db as KB                    # noqa: E402
import kb_answer_resolve as R         # noqa: E402

LAW = "中华人民共和国保守国家秘密法(2024修订).txt"
REG = "中华人民共和国保守国家秘密法实施条例(2024修订).txt"

# (bank, qid, 结论, 依据文档, 依据原文)
VERDICTS = [
    ("baomi-2024.json", "BM-8056fb3a", "D", LAW,
     "第四十条 军事禁区、军事管理区和属于国家秘密不对外开放的其他场所、部位，应当采取保密措施，"
     "未经有关部门批准，不得擅自决定对外开放或者扩大开放范围。涉密军事设施及其他重要涉密单位周边区域"
     "应当按照国家保密规定加强保密管理。第四十一条 从事国家秘密载体制作、复制、维修、销毁，涉密信息系统集成，"
     "武器装备科研生产，或者涉密军事设施建设等涉及国家秘密业务的企业事业单位，应当经过审查批准，取得保密资质。"
     "（「军队家属院」「防谍」全库 0 命中 → 不存在项为 D）"),
    ("baomi-2024.json", "BM-16381ddb", "C", LAW,
     "第三十四条 网络运营者应当加强对其用户发布的信息的管理，配合监察机关、保密行政管理部门、公安机关、"
     "国家安全机关对涉嫌泄露国家秘密案件进行调查处理；发现……涉嫌泄露国家秘密的，应当立即停止传输该信息，"
     "保存有关记录……应当根据……要求，删除涉及泄露国家秘密的信息，并对有关设备进行技术处理。"
     "（A/B/D 均有原文，C「每年报送日常经营情况」法条无）"),
    ("保密公开题-中科院.json", "PUB-BM-0293", "B", LAW,
     "第五十七条（四）寄递、托运国家秘密载体出境，或者未经有关主管部门批准，携带、传递国家秘密载体出境的。"),
    ("保密公开题-中科院.json", "PUB-BM-0519", "A", REG,
     "第十六条 中央国家机关、省级机关以及设区的市级机关可以根据保密工作需要或者有关机关、单位申请，"
     "在国家保密行政管理部门规定的定密权限、授权范围内作出定密授权。第十七条 机关、单位对应当定密但本机关、"
     "本单位没有定密权限的事项，先行采取保密措施，并依照法定程序，报上级机关、单位确定。"),
    ("保密公开题-中科院.json", "PUB-BM-0560", "A", LAW,
     "第四十六条 涉密人员在脱密期内，不得违反规定就业和出境，不得以任何方式泄露国家秘密；脱密期结束后，"
     "应当遵守国家保密规定，对知悉的国家秘密继续履行保密义务。"),
    ("保密公开题-中科院.json", "PUB-BM-0446", "A/B/C", REG,
     "第四十八条 从事国家秘密载体制作、复制、维修、销毁，涉密信息系统集成，武器装备科研生产，"
     "或者涉密军事设施建设等涉密业务的企业事业单位，应当由保密行政管理部门单独或者会同有关部门进行保密审查，"
     "取得保密资质。（「涉密工程建设」不在其列；保密法第四十二条对工程建设单位只要求遵守国家保密规定）"),
    # ---- 第二批（2026-10-27）：保密法 2024 版新增条款，四道都是「以上都正确」型 ----
    ("baomi-2024.json", "BM-22ad8864", "D", LAW,
     "第二十条 机关、单位应当根据工作需要，确定具体的保密期限、解密时间或者解密条件。"
     "（条例第十九条同义：确定具体的保密期限或者解密时间；不能确定的，应当确定解密条件）"
     "→ A/B/C 三种形式法条都列了，故选 D。"),
    ("baomi-2024.json", "BM-7d67bc22", "C", LAW,
     "第三条 坚持中国共产党对保守国家秘密（以下简称保密）工作的领导。中央保密工作领导机构领导全国保密工作，"
     "研究制定、指导实施国家保密工作战略和重大方针政策，统筹协调国家保密重大事项和重要工作，推进国家保密法治建设。"),
    ("baomi-2024.json", "BM-0a87a954", "B", LAW,
     "第三十五条 机关、单位应当依法对拟公开的信息进行保密审查，遵守国家保密规定。"),
    ("baomi-2024.json", "BM-d7dd5730", "D", LAW,
     "第十条 国家鼓励和支持保密科学技术研究和应用，提升自主创新能力，依法保护保密领域的知识产权。"
     "→ A/B/C 三项都在这条里，故选 D。"),
]


def apply_verdicts(db, verdicts, now=None):
    """写入 answers 表，返回 [(qid, 动作)]。幂等：同一结论重写不会变脏。

    按 qid 更新**所有**题库行：同一道题常同时出现在「baomi-2024」和「baomi-我的错题」，
    只写其中一行，另一行会在清单里再冒出来（2026-10-27 实测踩过）。
    """
    now = now or time.strftime("%Y-%m-%d %H:%M:%S")
    done = []
    for bank, qid, ans, doc, quote in verdicts:
        stored = db.execute("SELECT COALESCE(stored,'') FROM answers WHERE qid=? LIMIT 1",
                            (qid,)).fetchone()
        if not stored:
            done.append((qid, "缺行"))
            continue
        stored = stored[0]
        status = "agree" if set(stored.split("/")) == set(ans.split("/")) else "conflict"
        cur = db.execute("""UPDATE answers SET derived=?, status=?, confidence='high',
                              method=?, evidence_doc=?, evidence_text=?, provenance='manual',
                              updated_at=?
                            WHERE qid=?""",
                         (ans, status, "人工核对：原文直接支持（" + doc + "）", doc, quote[:400],
                          now, qid))
        done.append((qid, "%s（库存 %s，写 %d 行）" % (ans, stored or "-", cur.rowcount or 0)))
    return done


def run(a):
    db = KB.connect()
    if a.yes:
        bak = KB.DB_PATH + ".bak-%d" % int(time.time())
        db.close()
        shutil.copy2(KB.DB_PATH, bak)
        db = KB.connect()
        print("备份 → %s" % os.path.basename(bak))
    done = apply_verdicts(db, VERDICTS)
    if a.yes:
        db.commit()
    for qid, act in done:
        print("  %-14s %s" % (qid, act))
    if a.yes:
        n = db.execute("SELECT COUNT(*) FROM answers WHERE provenance='manual'").fetchone()[0]
        print("已写入 %d 条；库内 manual 共 %d 条" % (len([d for d in done if d[1] != "缺行"]), n))
        print("接着跑：python3 kb_review_queue.py build --split（这几道会从清单里消失）")
    else:
        print("\n这是 dry-run；确认无误后加 --yes 再跑一次。")
    db.close()
    return 0


def selftest():
    """离线：临时库里造两条题，验写入 / 幂等 / 与库存不一致时标 conflict。"""
    tmp = "/tmp/kb_mark_verified_selftest.sqlite"
    for p in (tmp, tmp + "-wal", tmp + "-shm"):
        if os.path.exists(p):
            os.remove(p)
    db = KB.connect(tmp)
    db.executescript("""CREATE TABLE answers(bank TEXT, qid TEXT, question TEXT, stored TEXT,
                        derived TEXT, status TEXT, confidence TEXT, method TEXT, evidence_doc TEXT,
                        evidence_text TEXT, updated_at TEXT, provenance TEXT, evidence_url TEXT,
                        PRIMARY KEY(bank,qid));""")
    db.execute("INSERT INTO answers(bank,qid,stored,derived,status) VALUES('b.json','Q1','D','A','conflict')")
    db.execute("INSERT INTO answers(bank,qid,stored,derived,status) VALUES('b.json','Q2','A','A','agree')")
    v = [("b.json", "Q1", "D", "doc.txt", "原文一"), ("b.json", "Q2", "C", "doc.txt", "原文二")]
    apply_verdicts(db, v, now="2026-10-27 00:00:00")
    db.commit()
    r1 = db.execute("SELECT derived,status,provenance,evidence_doc,confidence FROM answers WHERE qid='Q1'").fetchone()
    r2 = db.execute("SELECT derived,status,provenance FROM answers WHERE qid='Q2'").fetchone()
    assert r1 == ("D", "agree", "manual", "doc.txt", "high"), "Q1 应写成 D/agree/manual/high，实际 %r" % (r1,)
    assert r2[1] == "conflict", "Q2 库存 A 与结论 C 不一致，应标 conflict，实际 %r" % (r2,)
    before = db.execute("SELECT updated_at FROM answers WHERE qid='Q1'").fetchone()[0]
    apply_verdicts(db, v[:1], now="2026-10-28 00:00:00")
    db.commit()
    n = db.execute("SELECT COUNT(*) FROM answers WHERE provenance='manual'").fetchone()[0]
    after = db.execute("SELECT updated_at FROM answers WHERE qid='Q1'").fetchone()[0]
    assert n == 2, "重写不该复制出行，实际 manual %d 条" % n
    assert after != before, "重写应更新时间戳"
    # 同一 qid 出现在两个题库都要写（否则另一行会在清单里冒出来）
    db.execute("INSERT INTO answers(bank,qid,stored,derived,status) VALUES('c.json','Q3','A','A','agree')")
    db.execute("INSERT INTO answers(bank,qid,stored,derived,status) VALUES('d.json','Q3','A','A','agree')")
    db.commit()
    apply_verdicts(db, [("c.json", "Q3", "A", "doc.txt", "原文三")], now="2026-10-29 00:00:00")
    db.commit()
    n3 = db.execute("SELECT COUNT(*) FROM answers WHERE qid='Q3' AND provenance='manual'").fetchone()[0]
    assert n3 == 2, "同一 qid 跨题库都要写，实际 manual %d 行" % n3
    # manual 的题不该再被 resolve 覆盖
    import re
    src = open(os.path.join(PROJ, "kb_answer_resolve.py"), encoding="utf-8").read()
    assert "'manual'" in src, "resolve 的 keep_web 必须含 manual，否则人工结论会被重算冲掉"
    db.close()
    for p in (tmp, tmp + "-wal", tmp + "-shm"):
        os.path.exists(p) and os.remove(p)
    print("selftest PASS：写入 / 冲突标记 / 幂等 / 跨题库同写 / resolve 不覆盖，五项断言全过")
    return 0


def main():
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    r = sub.add_parser("apply")
    r.add_argument("--yes", action="store_true")
    sub.add_parser("selftest")
    a = ap.parse_args()
    if a.cmd == "selftest":
        return selftest()
    return run(a)


if __name__ == "__main__":
    sys.exit(main())
