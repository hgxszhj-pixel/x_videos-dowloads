#!/usr/bin/env python3
"""kb_db —— 培训资料的单一存储：SQLite（清单 + chunk 文本 + bigram 倒排）一个文件搞定。

为什么不用 pickle + JSON + 目录三件套：目录会漂（实测 235 个 txt vs 177 条在册），
pickle 不能增量（改一篇要整库重建）。SQLite 一个事务写进去，冷启动 mmap 按需读。

表（PRAGMA user_version = SCHEMA_VERSION 记录结构版本）:
    documents(id, att UNIQUE, name, course, course_id, domain, ext, size, url, sha1,
              chars, synced_at, gone_at)          -- 清单 + 同步状态，合一
    chunks(id, doc_id, seq, text)                -- 每 700 字一块
    postings(bigram PRIMARY KEY, chunk_ids BLOB) -- int32 数组，一次查所有命中

用法:
    python3 kb_db.py migrate [--dry-run]   # 从现有 catalog + 文本目录建库（含孤岛处置）
    python3 kb_db.py add <文件...>         # 把本地资料（法规原文/内部规范）灌进库，重跑即替换
    python3 kb_db.py import-banks [--prune] # 把 banks/*.json 灌进 questions 表，重跑不重复
    python3 kb_db.py check-banks           # 校验 questions 表与 banks/*.json 是否一致（定时哨兵）
    python3 kb_db.py export [--out x.json] # 导出快照给 Rust 侧 / 人读
    python3 kb_db.py stats
    python3 kb_db.py selftest              # 红绿自检：写入→查询→更新→查询 + 题库导入幂等
"""
import argparse
import array
import hashlib
import json
import os
import re
import sqlite3
import sys
import time

PROJ = os.path.expanduser("~/x_videos-dowloads/exam-lookup")
OUT = os.path.join(PROJ, "jr/out")
DB_PATH = os.path.join(OUT, "docs.sqlite")
TEXT_DIR = os.path.join(OUT, "docs_new_text")
ORPHAN_DIR = os.path.join(OUT, "docs_new_text_orphans")
CATALOG = os.path.expanduser("~/develop_software/exam-lookup-rs/docs/platform-training-docs.json")
CHUNK = 700
SCHEMA_VERSION = 2          # v2：+ questions 表（题库从 banks/*.json 灌入，JSON 降为导入格式）

sys.path.insert(0, PROJ)
from kb_docs_index import cjk_bigrams, chunks_of, normalize, query_terms  # noqa: E402

SCHEMA = """
CREATE TABLE IF NOT EXISTS documents(
  id INTEGER PRIMARY KEY, att TEXT UNIQUE, name TEXT, course TEXT, course_id TEXT,
  domain TEXT, ext TEXT, size INTEGER, url TEXT, sha1 TEXT, chars INTEGER,
  synced_at TEXT, gone_at TEXT);
CREATE TABLE IF NOT EXISTS chunks(id INTEGER PRIMARY KEY, doc_id INTEGER NOT NULL,
  seq INTEGER, text TEXT);
CREATE INDEX IF NOT EXISTS idx_chunks_doc ON chunks(doc_id);
CREATE TABLE IF NOT EXISTS postings(bigram TEXT PRIMARY KEY, chunk_ids BLOB);
-- 题库：banks/*.json 的唯一落地点。JSON 从此只是导入/导出格式，不再是被直接读取的真相源。
-- extra 存其余字段（myWrong/examWhen/law/confidence/warn…）原样 JSON，保真不丢字段、加字段不用改表。
CREATE TABLE IF NOT EXISTS questions(
  bank TEXT NOT NULL, qid TEXT NOT NULL,
  number INTEGER, type TEXT, question TEXT NOT NULL,
  options TEXT, answer_keys TEXT, answer_text TEXT, analysis TEXT,
  topic TEXT, category TEXT, volume TEXT, page INTEGER,
  extra TEXT, sha1 TEXT, updated_at TEXT,
  PRIMARY KEY(bank, qid));
CREATE INDEX IF NOT EXISTS idx_questions_bank ON questions(bank);
CREATE INDEX IF NOT EXISTS idx_questions_qid ON questions(qid);
"""


def ensure_schema(db):
    """补齐缺表。SCHEMA 全是 IF NOT EXISTS，所以这里不靠版本号判断——实测过：
    旧脚本会把 user_version 写成 2，而 questions 表并不存在，纯按版本号就会漏建（踩过一次）。"""
    ver = db.execute("PRAGMA user_version").fetchone()[0]
    db.executescript(SCHEMA)
    if ver < SCHEMA_VERSION:
        db.execute("PRAGMA user_version=%d" % SCHEMA_VERSION)
        db.commit()
    return ver, SCHEMA_VERSION


def connect(path=None, readonly=False):
    """path 必须运行时解析：写成默认参数会在 import 时绑定，测试/迁移时改不动。"""
    path = path or DB_PATH
    if readonly:
        try:
            db = sqlite3.connect("file:%s?mode=ro" % path, uri=True, timeout=10)
            db.execute("PRAGMA user_version").fetchone()   # 探活：ro 连接是懒打开的
        except sqlite3.OperationalError:
            # WAL 库在 -shm 不在时，只读连接会报 unable to open database file（SQLite 已知行为，
            # 后台进程实测必踩）。退化成读写连接，调用方的只读语义不变，代码里没有写操作。
            db = sqlite3.connect(path, timeout=10)
    else:
        db = sqlite3.connect(path, timeout=30)
        db.execute("PRAGMA journal_mode=WAL")
        db.execute("PRAGMA synchronous=NORMAL")
    db.execute("PRAGMA mmap_size=268435456")
    if not readonly:
        ensure_schema(db)          # v1 库在这里自动补 questions 表；只读连接不改库
    return db


# ---------- 写入 ----------
def _postings_of(db, doc_id):
    """该文档现占用的 (bigram -> chunk_ids) 快照，用于增量回退。

    ponytail: 用旧 chunk 文本反推 bigram 再定点查，避免扫全表 141k 行；
    若将来单文档大到几万字，升级路径是从 chunks 表加一列存 bigram 列表。
    """
    rows = db.execute("SELECT id, text FROM chunks WHERE doc_id=?", (doc_id,)).fetchall()
    if not rows:
        return {}
    ids = {r[0] for r in rows}
    grams = set()
    for _, text in rows:
        grams |= cjk_bigrams(text)
    out = {}
    grams = list(grams)
    for i in range(0, len(grams), 500):
        part = grams[i:i + 500]
        for g, blob in db.execute("SELECT bigram, chunk_ids FROM postings WHERE bigram IN (%s)"
                                  % ",".join("?" * len(part)), part):
            arr = array.array("i", blob)
            keep = [c for c in arr if c in ids]
            if keep:
                out[g] = (arr, keep)
    return out


def upsert_document(db, meta, text, chunk_size=CHUNK):
    """写入或替换一份文档：sha1 或 url 变了就重灌 chunk 与倒排。返回 (doc_id, 动作)。"""
    att = meta.get("att") or hashlib.sha1((meta.get("name") or "").encode()).hexdigest()[:16]
    row = db.execute("SELECT id, sha1, url FROM documents WHERE att=?", (att,)).fetchone()
    new_sha = meta.get("sha1")
    if row and new_sha and row[1] == new_sha and row[2] == meta.get("url"):
        return row[0], "unchanged"

    with db:  # 单事务：清单与倒排要么一起生效，要么都不生效
        if row:
            doc_id = row[0]
            old = _postings_of(db, doc_id)
            for g, (arr, keep) in old.items():
                rest = [c for c in arr if c not in keep]
                if rest:
                    db.execute("UPDATE postings SET chunk_ids=? WHERE bigram=?",
                               (array.array("i", rest).tobytes(), g))
                else:
                    db.execute("DELETE FROM postings WHERE bigram=?", (g,))
            db.execute("DELETE FROM chunks WHERE doc_id=?", (doc_id,))
            db.execute("""UPDATE documents SET name=?,course=?,course_id=?,domain=?,ext=?,size=?,
                          url=?,sha1=?,chars=?,synced_at=?,gone_at=NULL WHERE id=?""",
                       (meta.get("name"), meta.get("course"), meta.get("course_id"),
                        meta.get("domain"), meta.get("ext"), meta.get("size"), meta.get("url"),
                        new_sha, len(text or ""), time.strftime("%Y-%m-%d %H:%M:%S"), doc_id))
            action = "replaced"
        else:
            cur = db.execute("""INSERT INTO documents(att,name,course,course_id,domain,ext,size,url,
                                sha1,chars,synced_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)""",
                             (att, meta.get("name"), meta.get("course"), meta.get("course_id"),
                              meta.get("domain"), meta.get("ext"), meta.get("size"), meta.get("url"),
                              new_sha, len(text or ""), time.strftime("%Y-%m-%d %H:%M:%S")))
            doc_id = cur.lastrowid
            action = "inserted"

        base = db.execute("SELECT COALESCE(MAX(id), -1) + 1 FROM chunks").fetchone()[0]
        touched = {}
        for seq, ch in enumerate(chunks_of(text or "", chunk_size)):
            cid = base + seq
            db.execute("INSERT INTO chunks(id,doc_id,seq,text) VALUES(?,?,?,?)", (cid, doc_id, seq, ch))
            for g in cjk_bigrams(ch):
                touched.setdefault(g, []).append(cid)
        # 合并进 postings：命中的 bigram 读→并→写（每 bigram 一次，块数少，够快）
        for g, cids in touched.items():
            r = db.execute("SELECT chunk_ids FROM postings WHERE bigram=?", (g,)).fetchone()
            merged = array.array("i", r[0]) if r else array.array("i")
            merged.extend(cids)
            if r:
                db.execute("UPDATE postings SET chunk_ids=? WHERE bigram=?", (merged.tobytes(), g))
            else:
                db.execute("INSERT INTO postings(bigram,chunk_ids) VALUES(?,?)", (g, merged.tobytes()))
    return doc_id, action


def mark_gone(db, att, when=None):
    with db:
        db.execute("UPDATE documents SET gone_at=? WHERE att=? AND gone_at IS NULL",
                   (when or time.strftime("%Y-%m-%d %H:%M:%S"), att))


# ---------- 查询 ----------
def search(db, query, topk=5):
    """与 pickle 版同样的打分：bigram 按 1/(1+df) 加权累加。返回 [(doc_id, text, score)]。"""
    grams = list(cjk_bigrams(query_terms(query)))
    if not grams:
        return []
    rows = db.execute("SELECT bigram, chunk_ids FROM postings WHERE bigram IN (%s)"
                      % ",".join("?" * len(grams)), grams).fetchall()
    score = {}
    for g, blob in rows:
        arr = array.array("i", blob)
        w = 1.0 / (1 + len(arr))
        for c in arr:
            score[c] = score.get(c, 0.0) + w
    top = sorted(score.items(), key=lambda kv: -kv[1])[:topk]
    if not top:
        return []
    ids = [c for c, _ in top]
    texts = dict(db.execute("SELECT id, text FROM chunks WHERE id IN (%s)"
                            % ",".join("?" * len(ids)), ids).fetchall())
    docs = dict(db.execute("SELECT c.id, c.doc_id FROM chunks c WHERE c.id IN (%s)"
                           % ",".join("?" * len(ids)), ids).fetchall())
    return [(docs[c], texts[c], s) for c, s in top if c in texts and c in docs]


def search_with_cite(db, query, topk=5):
    """带出处的检索：返回 [{doc, course, domain, text, score}]。"""
    hits = search(db, query, topk)
    meta = {}
    if hits:
        ids = list({d for d, _, _ in hits})
        for row in db.execute("SELECT id, name, course, domain FROM documents WHERE id IN (%s)"
                              % ",".join("?" * len(ids)), ids):
            meta[row[0]] = row[1:]
    return [{"doc": meta.get(d, ("", "", ""))[0], "course": meta.get(d, ("", "", ""))[1],
             "domain": meta.get(d, ("", "", ""))[2], "text": t, "score": s}
            for d, t, s in hits]


# ---------- 迁移 ----------
def _catalog_by_att():
    if not os.path.exists(CATALOG):
        return {}
    return {d["att"]: d for d in json.load(open(CATALOG, encoding="utf-8"))["docs"]}


def _text_files():
    if not os.path.isdir(TEXT_DIR):
        return []
    return [f for f in sorted(os.listdir(TEXT_DIR)) if f.endswith(".txt")]


def cmd_migrate(a):
    cat = _catalog_by_att()
    state_path = os.path.join(OUT, "docs_state.json")
    state = (json.load(open(state_path, encoding="utf-8"))["docs"]
             if os.path.exists(state_path) else {})
    files = _text_files()
    # 文件名前缀 = attachmentId；同一个 att 在旧命名（x.pdf.txt）与新命名（x.pdf.pdf.txt）下会各有
    # 一份文本 → 每个 att 只留一份（优先 state 登记的那个文件名，否则取最新 mtime），其余进孤岛目录。
    by_att = {}
    for f in files:
        att = f.split("_", 1)[0]
        by_att.setdefault(att, []).append(f)
    plan, orphans, dupes = [], [], 0
    for att, names in sorted(by_att.items()):
        if att not in cat and att not in state:
            for f in names:
                orphans.append((f, open(os.path.join(TEXT_DIR, f), encoding="utf-8", errors="ignore").read()))
            continue
        reg = os.path.basename((state.get(att) or {}).get("file") or "").rsplit(".", 1)[0]
        best = next((n for n in names if n[:-4] == reg), None) or max(
            names, key=lambda n: os.path.getmtime(os.path.join(TEXT_DIR, n)))
        text = open(os.path.join(TEXT_DIR, best), encoding="utf-8", errors="ignore").read()
        plan.append((att, best, text))
        for f in names:
            if f != best:
                orphans.append((f, open(os.path.join(TEXT_DIR, f), encoding="utf-8", errors="ignore").read()))
                dupes += 1
    print("  同 att 重复文本去重：%d 份移出" % dupes)

    print("迁移计划：catalog %d 条 | state %d 条 | 文本文件 %d 个" % (len(cat), len(state), len(files)))
    print("  可入库 %d 份 | 孤岛（无清单条目）%d 份" % (len(plan), len(orphans)))
    dup = 0
    seen = {}
    for att, f, text in plan:
        h = hashlib.sha1(normalize(text).encode()).hexdigest()
        if h in seen:
            dup += 1
            print("  重复正文：%s ≡ %s" % (f[:48], seen[h][:48]))
        else:
            seen[h] = f
    for f, text in orphans[:8]:
        print("  孤岛：%s (%d 字)" % (f[:60], len(text)))
    if orphans:
        print("  孤岛处置：移到 %s 并在迁移日志记一行（不删、不进索引）" % ORPHAN_DIR)

    if a.dry_run:
        print("--dry-run：未写库")
        return 0

    if os.path.exists(DB_PATH):
        os.rename(DB_PATH, DB_PATH + ".bak-%d" % int(time.time()))
    db = connect(DB_PATH)
    t0 = time.time()
    inserted = replaced = 0
    log_lines = []
    for att, f, text in plan:
        # 清单给展示字段，state 给 sha1/file（catalog 里没有 sha1，别让 cat 覆盖掉 state）
        meta = dict(cat.get(att) or {})
        meta.update({k2: v2 for k2, v2 in (state.get(att) or {}).items() if v2 is not None})
        meta["att"] = att
        # sha1 一律以磁盘文件为准：state 里的值可能是坏的（迁移/手改），不能当真相
        cand = os.path.join(OUT, state.get(att, {}).get("file") or "")
        if not os.path.exists(cand):
            cand = os.path.join(OUT, "docs_new", os.path.basename(f))
        if os.path.exists(cand):
            meta["sha1"] = hashlib.sha1(open(cand, "rb").read()).hexdigest()
        _, action = upsert_document(db, meta, text)
        inserted += action == "inserted"
        replaced += action == "replaced"
        log_lines.append("%s %s %s" % (action, att, (meta.get("name") or f)[:70]))
    # 清单里有、但抽不出文本的（扫描版 PDF / 图片 / 压缩包）也要进 documents 表：
    # 它们是「在册但无正文」，不是「下架」——只有平台真的不见了才算 gone。
    no_text = 0
    for att in cat:
        if att in {p[0] for p in plan}:
            continue
        meta = dict(cat[att])
        meta.update({k2: v2 for k2, v2 in (state.get(att) or {}).items() if v2 is not None})
        meta["att"] = att
        cand = os.path.join(OUT, state.get(att, {}).get("file") or "")
        if os.path.exists(cand):
            meta["sha1"] = hashlib.sha1(open(cand, "rb").read()).hexdigest()
        _, action = upsert_document(db, meta, "")
        no_text += action != "unchanged"
        log_lines.append("no-text %s %s" % (att, (meta.get("name") or "")[:70]))
    print("  在册无正文（仍是清单成员）：%d 份" % no_text)
    os.makedirs(ORPHAN_DIR, exist_ok=True)
    for f, _ in orphans:
        src = os.path.join(TEXT_DIR, f)
        if os.path.exists(src):
            os.rename(src, os.path.join(ORPHAN_DIR, f))
        log_lines.append("orphan %s" % f)
    with open(os.path.join(OUT, "docs_migrate.log"), "w", encoding="utf-8") as fp:
        fp.write("\n".join(log_lines) + "\n")
    db.close()
    print("迁移完成：新增 %d / 替换 %d / 孤岛移出 %d | %.1fs | %s（%.1f MB）"
          % (inserted, replaced, len(orphans), time.time() - t0, DB_PATH,
             os.path.getsize(DB_PATH) / 1048576))
    return 0


def cmd_export(a):
    db = connect(DB_PATH, readonly=True)
    docs = [dict(zip(("att", "name", "course", "course_id", "domain", "ext", "size", "url",
                      "sha1", "chars", "synced_at", "gone_at"), r))
            for r in db.execute("""SELECT att,name,course,course_id,domain,ext,size,url,sha1,
                                   chars,synced_at,gone_at FROM documents ORDER BY domain,name""")]
    out = {"schema_version": SCHEMA_VERSION, "generated_at": time.strftime("%Y-%m-%d %H:%M:%S"),
           "docs": docs}
    path = os.path.expanduser(a.out) if a.out else CATALOG
    json.dump(out, open(path, "w", encoding="utf-8"), ensure_ascii=False, indent=1)
    db.close()
    print("export: %d 条 → %s" % (len(docs), path))
    return 0


def cmd_stats(a):
    db = connect(DB_PATH, readonly=True)
    q = lambda s: db.execute(s).fetchone()[0]
    print("db %s (%.1f MB) schema_version=%d" % (
        DB_PATH, os.path.getsize(DB_PATH) / 1048576, q("PRAGMA user_version")))
    print("documents=%d（在册 %d / 下架 %d）chunks=%d postings=%d 字数=%.0f 万" % (
        q("SELECT COUNT(*) FROM documents"), q("SELECT COUNT(*) FROM documents WHERE gone_at IS NULL"),
        q("SELECT COUNT(*) FROM documents WHERE gone_at IS NOT NULL"), q("SELECT COUNT(*) FROM chunks"),
        q("SELECT COUNT(*) FROM postings"), q("SELECT COALESCE(SUM(chars),0) FROM documents") / 10000))
    db.close()
    return 0


def cmd_selftest(a):
    """红绿自检：临时库 → 写两份 → 查得到 → 更新其中一份 → 旧内容查不到、新内容查得到、另一份不受影响。"""
    tmp = "/tmp/kb_db_selftest.sqlite"
    for p in (tmp, tmp + "-wal", tmp + "-shm"):
        if os.path.exists(p):
            os.remove(p)
    db = connect(tmp)
    upsert_document(db, {"att": "A1", "name": "甲文档", "sha1": "s1"}, "登杆作业必须系好安全带，脚扣不得攀登方形水泥杆。")
    upsert_document(db, {"att": "A2", "name": "乙文档", "sha1": "s2"}, "挖掘土石方应从上而下进行，不得掏挖。")
    db.commit()
    hits = search(db, "挖掘土石方")
    assert hits and "挖掘土石方" in hits[0][1], "查询应命中乙文档：%r" % (hits,)
    upsert_document(db, {"att": "A1", "name": "甲文档", "sha1": "s1b"}, "新的甲文档内容：安全带应高挂低用。")
    db.commit()
    old = search_with_cite(db, "脚扣")
    new = search_with_cite(db, "高挂低用")
    other = search_with_cite(db, "挖掘土石方")
    assert all("脚扣" not in h["text"] for h in old), "替换后旧内容不该还能查到：%r" % (old,)
    assert new and "高挂低用" in new[0]["text"], "替换后新内容应可查"
    assert other and "挖掘土石方" in other[0]["text"], "另一份文档不该被影响"
    n = db.execute("SELECT COUNT(*) FROM chunks WHERE doc_id=1").fetchone()[0]
    assert n == 1, "替换后甲文档应只有 1 块，实际 %d" % n
    db.close()

    # ---- 题库导入：幂等 / 内容指纹 / 删题 ----------------------------------
    import tempfile
    d = tempfile.mkdtemp()
    bp = os.path.join(d, "自测题库.json")
    q1 = {"id": "T-1", "number": 1, "type": "single", "question": "脚扣不得攀登什么杆？",
          "options": [{"key": "A", "text": "方形水泥杆"}], "answerKeys": ["A"],
          "answerText": "方形水泥杆", "topic": "线路", "myWrong": True}
    q2 = {"id": "T-2", "question": "挖掘土石方应如何？",
          "options": [{"key": "A", "text": "从上而下"}, {"key": "B", "text": "掏挖"}],
          "answerKeys": ["A"], "answerText": "从上而下", "confidence": "低"}

    def dump(qs):
        json.dump({"schema_version": 1, "meta": {"title": "自测"}, "questions": qs},
                  open(bp, "w", encoding="utf-8"), ensure_ascii=False)

    dump([q1, q2])
    qdb = connect(os.path.join(d, "q.sqlite"))
    assert validate_bank(bp) == [], "合格题库不该报错：%r" % validate_bank(bp)
    # 坏数据必须当场报错，且指名到题（格式契约）
    badq = dict(q1)
    badq.pop("answerKeys")
    dump([badq])
    assert any("缺字段 answerKeys" in x for x in validate_bank(bp)), "缺 answerKeys 应被抓"
    assert any("第 1 题" in x for x in validate_bank(bp)), "报错应指名到题"
    dump([dict(q1, answerKeys=["Z"])])
    assert any("不在选项" in x for x in validate_bank(bp)), "答案不在选项里应被抓"
    dump([dict(q1, type="single", answerKeys=["A"])])
    assert validate_bank(bp) == [], "单选 1 答案应通过"
    dump([dict(q1, type="multiple", answerKeys=["A"])])
    assert any("type='multiple'" in x for x in validate_bank(bp)), "多选只有 1 个答案应被抓"
    dump([dict(q1, options=[{"key": "A"}])])
    assert any("选项必须是" in x for x in validate_bank(bp)), "选项缺 text 应被抓"
    dump([q1, dict(q1)])
    assert any("id 在本库内重复" in x for x in validate_bank(bp)), "同库 id 重复应被抓"
    # ordering：answerKeys 是顺序（>=2、不重复）；重复字母必须报红，否则排序题会静默错答
    dord = dict(q1, id="T-ORD", type="ordering", answerKeys=["A", "A"],
                options=[{"key": "A", "text": "甲"}, {"key": "B", "text": "乙"}])
    dump([dord])
    assert any("不能有重复字母" in x for x in validate_bank(bp)), "排序题重复字母没报红"
    dump([dict(dord, answerKeys=["B", "A"])])
    assert validate_bank(bp) == [], "合法的排序题应通过：%r" % validate_bank(bp)
    dump([dict(dord, answerKeys=["A"])])
    assert any("应有 2" in x for x in validate_bank(bp)), "排序题只有 1 个答案应被抓"
    # 排序 = 多选题的变体：multiple + ordered=true 走同一套规则（合法通过 / 重复字母报红）
    dmul = dict(q1, id="T-MULORD", type="multiple", ordered=True, answerKeys=["B", "A"],
                options=[{"key": "A", "text": "甲"}, {"key": "B", "text": "乙"}])
    dump([dmul])
    assert validate_bank(bp) == [], "multiple+ordered 应通过：%r" % validate_bank(bp)
    dump([dict(dmul, answerKeys=["A", "A"])])
    assert any("不能有重复字母" in x for x in validate_bank(bp)), "multiple+ordered 重复字母没报红"
    # blank（填空）：不看 options/answerKeys，只看 answerText；空位数必须和题干对得上
    dblank = {"id": "T-BL", "type": "blank", "question": "国家秘密的密级分为（）、（）、（）三级。",
              "options": [], "answerKeys": [], "answerText": ["绝密", "机密", "秘密"]}
    dump([dblank])
    assert validate_bank(bp) == [], "合法填空题应通过：%r" % validate_bank(bp)
    dump([dict(dblank, answerText=["绝密", "机密"])])
    assert any("题干有 3 个空" in x for x in validate_bank(bp)), "少给答案应被抓"
    dump([dict(dblank, answerText=[])])
    assert any("必须在 answerText 里给各空的答案" in x for x in validate_bank(bp)), "填空题没答案应被抓"
    # 契约锁：schema 文件与 REQUIRED_Q 必须一致，改一边不改另一边就红
    sch = json.load(open(SCHEMA_FILE, encoding="utf-8"))
    assert set(sch["properties"]["questions"]["items"]["required"]) == set(REQUIRED_Q), \
        "schema/bank.schema.json 与 kb_db.REQUIRED_Q 字段清单不一致"
    assert set(sch["properties"]["questions"]["items"]["properties"]["options"]["items"]["required"]) \
        == set(REQUIRED_OPT), "schema 与 REQUIRED_OPT 不一致"
    assert sch["properties"]["schema_version"]["const"] == BANK_SCHEMA_VERSION, "schema_version 常量不一致"
    # 导入侧同样拦：坏题库一行都不该写进去
    dump([badq])
    try:
        import_bank(qdb, bp)
        raise AssertionError("坏题库应被 import_bank 拒绝")
    except ValueError:
        pass
    assert qdb.execute("SELECT COUNT(*) FROM questions").fetchone()[0] == 0, "被拒的题库不该写入任何行"
    dump([q1, q2])
    assert import_bank(qdb, bp) == (2, 0, 0, 0), "首次导入应为 2 新增"
    assert import_bank(qdb, bp) == (0, 0, 2, 0), "重跑必须全未变（幂等）"
    assert qdb.execute("SELECT COUNT(*) FROM questions").fetchone()[0] == 2, "幂等失败：出现重复行"
    assert check_banks(qdb, d) == 0, "刚导入完 check 应为绿"
    extra = json.loads(qdb.execute("SELECT extra FROM questions WHERE qid='T-1'").fetchone()[0])
    assert extra.get("myWrong") is True, "extra 应保真存非核心字段：%r" % extra
    q1["answerText"] = "方形水泥杆（改）"
    dump([q1, q2])
    assert import_bank(qdb, bp) == (0, 1, 1, 0), "改一题应为 1 更新 / 1 未变"
    assert check_banks(qdb, d) == 0, "改完导入后 check 应转绿"
    dump([q1])                                          # json 里删掉 T-2
    assert import_bank(qdb, bp) == (0, 0, 1, 1), "删题应只报告 1 条待删"
    assert qdb.execute("SELECT COUNT(*) FROM questions").fetchone()[0] == 2, "默认不该删数据"
    assert check_banks(qdb, d) == 1, "json 删了题而库里还有，check 必须报红"
    import_bank(qdb, bp, prune=True)
    assert qdb.execute("SELECT COUNT(*) FROM questions").fetchone()[0] == 1, "--prune 应真的删掉"
    assert check_banks(qdb, d) == 0, "prune 后 check 应转绿"
    # 整个题库文件被挪走（不只是库里删一题）：prune_missing_banks 必须清掉它的行
    gone = prune_missing_banks(qdb, keep_names={"别的库.json"})
    assert gone and gone[0][1] == 1, "整库消失没被清理：%r" % (gone,)
    assert qdb.execute("SELECT COUNT(*) FROM questions").fetchone()[0] == 0, "清理后应为 0 行"
    qdb.close()
    for p in (tmp, tmp + "-wal", tmp + "-shm"):
        if os.path.exists(p):
            os.remove(p)
    print("SELFTEST PASS：语料写入/替换/隔离 + 题库导入幂等/指纹/删题 + 格式契约（含 schema 字段锁）全过")
    return 0



# ---------- 评估（口径与 kb_docs_index.py 完全一致，便于对照） ----------
def _bank(bank_path):
    bank = json.load(open(bank_path, encoding="utf-8"))
    return bank if isinstance(bank, list) else (bank.get("questions") or bank.get("items") or [])


def _idf(db, g, nchunks):
    df = db.execute("SELECT LENGTH(chunk_ids)/4 FROM postings WHERE bigram=?", (g,)).fetchone()
    return 1.0 + (nchunks / (1 + (df[0] if df else 0))) ** 0.5


def eval_bank(db, bank_path, cite_out=None):
    qs = _bank(bank_path)
    located = answered = 0
    cites = []
    for q in qs:
        hits = search_with_cite(db, q.get("question") or "", topk=5)
        if not hits:
            continue
        qg = cjk_bigrams(query_terms(q.get("question")))
        located += len(qg & cjk_bigrams(hits[0]["text"])) >= 3
        at = " ".join(q.get("answerText") or [])
        win = {at[i:i + 4] for i in range(max(0, len(at) - 3))}
        hit = next((h for h in hits if win and any(w in h["text"] for w in win)), None)
        if hit:
            answered += 1
            if cite_out is not None:
                cites.append({"id": q.get("id"), "question": q.get("question"), "answer": at,
                              "doc": hit["doc"], "course": hit["course"], "snippet": hit["text"][:300]})
    n = len(qs)
    print("  ① 定位到相关段落: %d/%d = %.0f%%" % (located, n, 100.0 * located / max(1, n)))
    print("  ② 能锚到正确选项（可给出处）: %d/%d = %.0f%%" % (answered, n, 100.0 * answered / max(1, n)))
    if cite_out and cites:
        json.dump(cites, open(cite_out, "w"), ensure_ascii=False, indent=1)
        print("     出处 → %s（%d 条）" % (cite_out, len(cites)))
    return located, answered, n


def eval_options(db, bank_path):
    qs = _bank(bank_path)
    nchunks = db.execute("SELECT COUNT(*) FROM chunks").fetchone()[0]
    ok = 0
    misses = []
    for q in qs:
        hits = search_with_cite(db, q.get("question") or "", topk=5)
        if not hits:
            continue
        ctx_g = cjk_bigrams(" ".join(h["text"] for h in hits))
        scores = {}
        for o in (q.get("options") or []):
            g = cjk_bigrams(o.get("text") or "")
            if g:
                scores[o["key"]] = sum(_idf(db, x, nchunks) for x in g if x in ctx_g)
        if not scores:
            continue
        pred = max(scores, key=scores.get)
        if pred in (q.get("answerKeys") or []):
            ok += 1
        else:
            misses.append("%s(选%s/答%s)" % (q.get("id"), pred, "/".join(q.get("answerKeys") or [])))
    n = len(qs)
    print("  ③ 四选一答题准确率: %d/%d = %.0f%%" % (ok, n, 100.0 * ok / max(1, n)))
    if misses:
        print("     错例:", ", ".join(misses[:6]))
    return ok, n


def cmd_eval(a):
    db = connect(DB_PATH, readonly=True)
    print("题库 %s（SQLite 引擎）" % os.path.basename(a.bank))
    eval_bank(db, os.path.expanduser(a.bank), a.cite_out)
    eval_options(db, os.path.expanduser(a.bank))
    db.close()
    return 0


def cmd_bench(a):
    """冷启动 / 查询 / 增量更新：三个数就是这次换格式的全部理由。"""
    bank = os.path.join(PROJ, "banks/dtzx-safety.json")
    qs = _bank(bank)[:20]
    t0 = time.time(); db = connect(DB_PATH, readonly=True); search(db, qs[0]["question"]); cold = time.time() - t0
    ts = []
    for q in qs:
        a0 = time.time(); search(db, q["question"], 5); ts.append(time.time() - a0)
    ts.sort()
    db.close()
    print("冷启动（开库到首次查询）: %.0f ms" % (cold * 1000))
    print("单题检索 top5: p50 %.1f ms | p90 %.1f ms" % (ts[len(ts) // 2] * 1000, ts[int(len(ts) * 0.9)] * 1000))
    text = "增量更新测试文本。登杆前应检查电杆完好情况，脚扣不得攀登方形水泥杆。"
    t0 = time.time()
    db = connect(DB_PATH)
    upsert_document(db, {"att": "BENCH-TMP", "name": "增量测试", "sha1": "bench-%d" % time.time()}, text)
    db.commit(); db.close()
    inc = time.time() - t0
    db = connect(DB_PATH)
    with db:
        db.execute("DELETE FROM chunks WHERE doc_id=(SELECT id FROM documents WHERE att='BENCH-TMP')")
        db.execute("DELETE FROM documents WHERE att='BENCH-TMP'")
    db.close()
    print("增量：新增 1 篇（1 chunk + 倒排合并）%.0f ms，无需整库重建" % (inc * 1000))
    print("库体积 %.1f MB" % (os.path.getsize(DB_PATH) / 1048576))
    return 0

# ---------- 题库导入：banks/*.json → questions 表 ----------
QBANKS = os.path.join(PROJ, "banks")
BANK_SCHEMA_VERSION = 1
SCHEMA_FILE = os.path.join(PROJ, "schema/bank.schema.json")
REQUIRED_Q = ("id", "question", "options", "answerKeys", "answerText")   # 与 schema 文件同一份契约
REQUIRED_OPT = ("key", "text")
CORE_Q = {"id", "number", "type", "question", "options", "answerKeys", "answerText",
          "analysis", "topic", "category", "volume", "page"}

TYPE_RULES = {"single": (1, 1), "multiple": (2, None), "judge": (1, 1),
              "ordering": (2, None)}   # type -> (最少答案数, 最多)；ordering = multiple 的排序变体（兼容别名）


def is_ordered(q):
    """排序题 = 多选题的变体：type=multiple 且 ordered=true（老数据写 type=ordering 也算）。"""
    return bool(q.get("ordered")) or str(q.get("type") or "") == "ordering"
# blank（填空）：答案在 answerText，按空位顺序；options / answerKeys 都不要求


BLANK_MARK_RE = __import__("re").compile(r"[（(]\s*[)）]|_{2,}")


def _blank_marks(question):
    """题干里有几个空位（（）/( )/____ 都算）。"""
    return len(BLANK_MARK_RE.findall(str(question or "")))


def _blank_answers(q):
    """填空题的答案清单：answerText 里的非空项（顺序 = 空位顺序）。"""
    at = q.get("answerText")
    items = at if isinstance(at, list) else ([at] if at else [])
    return [str(x).strip() for x in items if str(x or "").strip()]


def validate_bank(path):
    """题库文件的格式契约，返回问题清单（空=合格）。坏数据的报错要指名到题，别只说文件坏了。

    为什么手写而不装 jsonschema：形状只有这一份，装个库不值当（本机也没装）。
    schema/bank.schema.json 是给编辑器/Rust 侧看的同一份契约，selftest 有一条断言锁着两边字段清单。
    """
    name = os.path.basename(path)
    try:
        data = json.load(open(path, encoding="utf-8"))
    except json.JSONDecodeError as e:
        return ["%s：不是合法 JSON（%s）" % (name, e)]
    if not isinstance(data, dict):
        return ["%s：顶层必须是对象（schema_version + meta + questions），不是裸数组" % name]

    bad = []
    ver = data.get("schema_version")
    if ver != BANK_SCHEMA_VERSION:
        bad.append("%s：schema_version 应为 %d，实际 %r" % (name, BANK_SCHEMA_VERSION, ver))
    if not str((data.get("meta") or {}).get("title") or "").strip():
        bad.append("%s：meta.title 缺失或为空" % name)
    qs = data.get("questions")
    if not isinstance(qs, list) or not qs:
        return bad + ["%s：questions 必须是非空数组" % name]

    seen = set()
    for i, q in enumerate(qs, 1):
        if not isinstance(q, dict):
            bad.append("%s 第 %d 题：不是对象" % (name, i))
            continue
        qid = str(q.get("id") or "").strip()
        tag = "%s 第 %d 题%s" % (name, i, "（%s）" % qid if qid else "")
        for k in REQUIRED_Q:
            if k not in q:
                bad.append("%s：缺字段 %s" % (tag, k))
        if not qid:
            bad.append("%s：id 为空" % tag)
        elif qid in seen:
            bad.append("%s：id 在本库内重复" % tag)
        seen.add(qid)
        if not str(q.get("question") or "").strip():
            bad.append("%s：question 为空" % tag)

        qtype = str(q.get("type") or "")
        opts = q.get("options")
        keys = []
        if qtype == "blank":
            # 填空：没有选项；答案按空位排在 answerText 里，空位数必须和题干里的（）对得上
            if opts not in (None, []) and not isinstance(opts, list):
                bad.append("%s：type=blank 的 options 应为空数组或省略" % tag)
            blanks = _blank_answers(q)
            if not blanks:
                bad.append("%s：type=blank 必须在 answerText 里给各空的答案" % tag)
            n_mark = _blank_marks(q.get("question"))
            if n_mark and blanks and n_mark != len(blanks):
                bad.append("%s：题干有 %d 个空，answerText 给了 %d 个答案" % (tag, n_mark, len(blanks)))
        elif not isinstance(opts, list) or not opts:
            bad.append("%s：options 必须是非空数组" % tag)
        else:
            for o in opts:
                if not isinstance(o, dict) or any(k not in o for k in REQUIRED_OPT):
                    bad.append("%s：选项必须是 {key, text}，实际 %r" % (tag, o))
                    break
                keys.append(str(o.get("key")))

        ak = q.get("answerKeys")
        if qtype == "blank":
            if ak not in (None, []) and not isinstance(ak, list):
                bad.append("%s：type=blank 的 answerKeys 应为空数组或省略" % tag)
        elif not isinstance(ak, list) or not ak:
            bad.append("%s：answerKeys 必须是非空数组" % tag)
        else:
            miss = [a for a in ak if keys and str(a) not in keys]
            if miss:
                bad.append("%s：答案 %r 不在选项 %r 里" % (tag, miss, keys))
            if is_ordered(q) and len(set(map(str, ak))) != len(ak):
                bad.append("%s：排序题（multiple+ordered）的 answerKeys 是顺序，不能有重复字母（实际 %r）" % (tag, ak))
            lo, hi = TYPE_RULES.get("multiple" if is_ordered(q) else str(q.get("type") or ""), (1, None))
            if len(ak) < lo or (hi and len(ak) > hi):
                bad.append("%s：type=%r 应有 %d%s 个答案，实际 %d 个"
                           % (tag, q.get("type"), lo, "" if not hi else "~%d" % hi, len(ak)))

        at = q.get("answerText")
        if at is None or (isinstance(at, (str, list)) and not at):
            bad.append("%s：answerText 为空" % tag)
    return bad


def bank_files(banks_dir=None):
    d = banks_dir or QBANKS
    return [os.path.join(d, f) for f in sorted(os.listdir(d)) if f.endswith(".json")] \
        if os.path.isdir(d) else []


def validate_banks(banks_dir=None):
    """全量校验，返回 {文件名: [问题…]}（只含有问题的文件）。"""
    out = {}
    for p in bank_files(banks_dir):
        bad = validate_bank(p)
        if bad:
            out[os.path.basename(p)] = bad
    return out



def _q_sha1(q):
    """内容指纹：算「会改变答案/题面」的字段。type 和 ordered 也算——
    单选改排序、排序改多选，答案语义全变，改动不能被判成「未变」。"""
    blob = json.dumps([q.get("question"), q.get("options"), q.get("answerKeys"), q.get("answerText"),
                       q.get("type"), bool(q.get("ordered"))],
                      ensure_ascii=False, sort_keys=True, separators=(",", ":"))
    return hashlib.sha1(blob.encode()).hexdigest()


def bank_questions(path):
    data = json.load(open(path, encoding="utf-8"))
    return (data if isinstance(data, list) else (data.get("questions") or data.get("items") or []))


def import_bank(db, path, prune=False):
    """导入一个题库文件。返回 (新增, 更新, 未变, 待删)。

    幂等靠 sha1：内容没变就一行都不写；改了才 UPDATE，所以反复跑不会产生重复行。
    """
    name = os.path.basename(path)
    bad = validate_bank(path)          # 信任边界：格式不合格就别进库，错在哪题都点出来
    if bad:
        raise ValueError("题库不合格，未写入：\n  " + "\n  ".join(bad[:10]) +
                         ("\n  …… 共 %d 处" % len(bad) if len(bad) > 10 else ""))
    qs = bank_questions(path)
    ins = upd = same = 0
    seen = set()
    with db:
        for i, q in enumerate(qs, 1):
            qid = str(q.get("id") or "").strip()
            if not qid:
                # 信任边界：缺 id 直接报错，不拿题号猜（猜出来的 id 下次就对不上了）
                raise ValueError("%s 第 %d 题没有 id" % (name, i))
            if qid in seen:
                raise ValueError("%s 内 id 重复：%s" % (name, qid))
            seen.add(qid)
            at = q.get("answerText")
            row = (name, qid, q.get("number"), q.get("type"), q.get("question") or "",
                   json.dumps(q.get("options") or [], ensure_ascii=False),
                   json.dumps(q.get("answerKeys") or [], ensure_ascii=False),
                   at if isinstance(at, str) else " ".join(at or []),
                   q.get("analysis"), q.get("topic"), q.get("category"), q.get("volume"),
                   q.get("page"),
                   json.dumps({k: v for k, v in q.items() if k not in CORE_Q}, ensure_ascii=False),
                   _q_sha1(q), time.strftime("%Y-%m-%d %H:%M:%S"))
            old = db.execute("SELECT sha1 FROM questions WHERE bank=? AND qid=?", (name, qid)).fetchone()
            if old and old[0] == row[-2]:
                same += 1
                continue
            db.execute("""INSERT INTO questions(bank,qid,number,type,question,options,answer_keys,
                            answer_text,analysis,topic,category,volume,page,extra,sha1,updated_at)
                          VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)
                          ON CONFLICT(bank,qid) DO UPDATE SET
                            number=excluded.number, type=excluded.type, question=excluded.question,
                            options=excluded.options, answer_keys=excluded.answer_keys,
                            answer_text=excluded.answer_text, analysis=excluded.analysis,
                            topic=excluded.topic, category=excluded.category, volume=excluded.volume,
                            page=excluded.page, extra=excluded.extra, sha1=excluded.sha1,
                            updated_at=excluded.updated_at""", row)
            if old:
                upd += 1
            else:
                ins += 1
        stale = [r[0] for r in db.execute(
            "SELECT qid FROM questions WHERE bank=? AND qid NOT IN (%s)" % ",".join("?" * len(seen)),
            (name, *seen))] if seen else [r[0] for r in db.execute(
                "SELECT qid FROM questions WHERE bank=?", (name,))]
        if prune and stale:
            db.executemany("DELETE FROM questions WHERE bank=? AND qid=?", [(name, q) for q in stale])
    return ins, upd, same, len(stale)


def prune_missing_banks(db, keep_names):
    """banks/ 里已经没有的库，把它的题从 questions 表里删掉（只删整库消失的）。

    为什么单独一个函数：import_bank 只能 prune「同一个文件里被删掉的题」，
    整个题库文件被挪走（比如临时停用某套题）时它压根不知道这个库存在。
    返回 [(bank, 删了几行)]。
    """
    keep = {str(x) for x in keep_names}
    out = []
    for (bank,) in db.execute("SELECT DISTINCT bank FROM questions").fetchall():
        if bank in keep:
            continue
        n = db.execute("SELECT COUNT(*) FROM questions WHERE bank=?", (bank,)).fetchone()[0]
        db.execute("DELETE FROM questions WHERE bank=?", (bank,))
        out.append((bank, n))
    db.commit()
    return out


def check_banks(db=None, banks_dir=None):
    """哨兵：questions 表与 banks/*.json 是否一致。一致时一个字都不输出（给定时任务用）。"""
    own = db is None
    db = db or connect(readonly=True)
    banks_dir = banks_dir or QBANKS
    bad = []
    files = {f: os.path.join(banks_dir, f) for f in sorted(os.listdir(banks_dir))
             if f.endswith(".json")} if os.path.isdir(banks_dir) else {}
    for name, p in files.items():
        qs = bank_questions(p)
        want = {str(q.get("id")) for q in qs if q.get("id")}
        have = {r[0] for r in db.execute("SELECT qid FROM questions WHERE bank=?", (name,))}
        if want - have:
            bad.append("%s：库里少 %d 题（如 %s）" % (name, len(want - have), sorted(want - have)[:3]))
        if have - want:
            bad.append("%s：库里有 %d 题 json 里已没有（如 %s）" % (name, len(have - want), sorted(have - want)[:3]))
        drift = 0
        for q in qs:
            r = db.execute("SELECT sha1 FROM questions WHERE bank=? AND qid=?",
                           (name, str(q.get("id") or ""))).fetchone()
            if r and r[0] != _q_sha1(q):
                drift += 1
        if drift:
            bad.append("%s：%d 题改过没导入" % (name, drift))
    for name in {r[0] for r in db.execute("SELECT DISTINCT bank FROM questions")} - set(files):
        bad.append("questions 表里有 banks/ 已不存在的题库：%s" % name)
    if own:
        db.close()
    for b in bad:
        print("✗ " + b)
    return 1 if bad else 0


def cmd_import_banks(a):
    paths = ([os.path.expanduser(p) for p in a.paths] if a.paths else
             [os.path.join(QBANKS, f) for f in sorted(os.listdir(QBANKS)) if f.endswith(".json")])
    db = connect()
    t0 = time.time()
    tot = [0, 0, 0, 0]
    for p in paths:
        ins, upd, same, stale = import_bank(db, p, a.prune)
        tot = [tot[0] + ins, tot[1] + upd, tot[2] + same, tot[3] + stale]
        print("  %-34s 新 %3d 改 %3d 未变 %4d%s" % (os.path.basename(p)[:34], ins, upd, same,
                                                    (" 待删 %d" % stale) if stale else ""))
    if a.prune and not a.paths:                      # 全量导入 + --prune：连「整个库被挪走」也一起清
        gone = prune_missing_banks(db, [os.path.basename(p) for p in paths])
        for bank, n in gone:
            print("  已清掉整库：%-30s %d 题（banks/ 里已无此文件）" % (bank[:30], n))
    n_all = db.execute("SELECT COUNT(*) FROM questions").fetchone()[0]
    n_bank = db.execute("SELECT COUNT(DISTINCT bank) FROM questions").fetchone()[0]
    orphans = db.execute("""SELECT COUNT(*) FROM answers a WHERE NOT EXISTS
                            (SELECT 1 FROM questions q WHERE q.bank=a.bank AND q.qid=a.qid)""").fetchone()[0]
    db.close()
    print("\nquestions：%d 库 / %d 题｜本次 新 %d 改 %d 未变 %d 待删 %d｜%.1fs"
          % (n_bank, n_all, *tot, time.time() - t0))
    if orphans:
        print("注意：answers 里有 %d 行对不上 questions（题库改名/删题留下的），核对清单会看不到它们" % orphans)
    if tot[3] and not a.prune:
        print("提示：%d 道题在 banks/*.json 里已不存在；确认后加 --prune 才删" % tot[3])
    return 0


def cmd_validate_banks(a):
    bad = validate_banks()
    if not bad:
        print("格式契约通过：%d 个题库文件全部合格（schema_version=%d）"
              % (len(bank_files()), BANK_SCHEMA_VERSION))
        return 0
    n = 0
    for name, problems in bad.items():
        print("✗ %s" % name)
        for p in problems[:8]:
            print("    " + p)
        n += len(problems)
        if len(problems) > 8:
            print("    …… 共 %d 处" % len(problems))
    print("\n共 %d 个文件 / %d 处问题" % (len(bad), n))
    return 1


def cmd_check_banks(a):
    return check_banks()


def cmd_add(a):
    """把本地文件（法规原文 / 内部规范 / 教材 txt）灌进语料库。

    为什么需要：2026-10-27 实测，196 道保密题「本地推不出来」的根因不是语料里没有依据，
    而是保密法原文躺在 jr/out/baomi/法规/ 里、没进 docs.sqlite（换了索引没回灌）。
    灌入后同一批题覆盖率 ≥50% 的从 17 道跳到 322 道 —— 差的是索引，不是资料。
    """
    import glob
    import subprocess as sp
    paths = []
    for p in a.paths:
        paths.extend(sorted(glob.glob(os.path.expanduser(p))) or [os.path.expanduser(p)])
    db = connect()
    done = 0
    for p in paths:
        if not os.path.isfile(p):
            print("跳过（不存在）%s" % p)
            continue
        if p.lower().endswith(".pdf"):
            text = sp.run(["pdftotext", "-layout", p, "-"], capture_output=True
                          ).stdout.decode("utf-8", "ignore")
        elif p.lower().endswith((".txt", ".md")):
            text = open(p, encoding="utf-8", errors="ignore").read()
        else:
            print("跳过（只认 txt/md/pdf）%s" % p)
            continue
        meta = {"att": "local-" + hashlib.sha1(os.path.basename(p).encode()).hexdigest()[:12],
                "name": os.path.basename(p), "course": a.course, "domain": a.domain,
                "ext": os.path.splitext(p)[1].lstrip("."), "size": os.path.getsize(p),
                "url": "file://" + os.path.abspath(p),
                "sha1": hashlib.sha1(text.encode()).hexdigest()}
        doc_id, action = upsert_document(db, meta, text)
        print("  %-8s %6d 字  doc_id=%3d  %s" % (action, len(text), doc_id, os.path.basename(p)))
        done += action != "unchanged"
    n = db.execute("SELECT COUNT(*) FROM documents").fetchone()[0]
    c = db.execute("SELECT COUNT(*) FROM chunks").fetchone()[0]
    db.close()
    print("写入 %d 份 → 库现状 %d 份文档 / %d 块" % (done, n, c))
    print("接着跑：python3 kb_answer_resolve.py resolve --bank <题库完整路径>…"
          " → python3 kb_review_queue.py build")
    return 0


def main():
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    m = sub.add_parser("migrate"); m.add_argument("--dry-run", action="store_true"); m.set_defaults(fn=cmd_migrate)
    d = sub.add_parser("add")
    d.add_argument("paths", nargs="+", help="文件或通配符（txt/md/pdf）")
    d.add_argument("--course", default="法规原文")
    d.add_argument("--domain", default="法规")
    d.set_defaults(fn=cmd_add)
    e = sub.add_parser("export"); e.add_argument("--out"); e.set_defaults(fn=cmd_export)
    i = sub.add_parser("import-banks")
    i.add_argument("paths", nargs="*", help="指定题库 json；省略=导入 banks/ 全量")
    i.add_argument("--prune", action="store_true", help="删掉 banks 里已不存在的题（默认只报告）")
    i.set_defaults(fn=cmd_import_banks)
    c = sub.add_parser("check-banks"); c.set_defaults(fn=cmd_check_banks)
    vb = sub.add_parser("validate-banks"); vb.set_defaults(fn=cmd_validate_banks)
    s = sub.add_parser("stats"); s.set_defaults(fn=cmd_stats)
    t = sub.add_parser("selftest"); t.set_defaults(fn=cmd_selftest)
    v = sub.add_parser("eval")
    v.add_argument("--bank", default=os.path.join(PROJ, "banks/dtzx-safety.json"))
    v.add_argument("--cite-out")
    v.set_defaults(fn=cmd_eval)
    b = sub.add_parser("bench"); b.set_defaults(fn=cmd_bench)
    a = ap.parse_args()
    return a.fn(a)


if __name__ == "__main__":
    sys.exit(main())
