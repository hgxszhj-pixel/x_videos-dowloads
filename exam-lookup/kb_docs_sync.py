#!/usr/bin/env python3
"""kb_docs_sync —— 平台培训资料「更新引擎」：刷新清单 → 差量下载 → 抽文本 → 重建索引 → 记录状态。

为什么需要它：平台上的课程和附件会变（新班开课、附件替换、专题下架），清单和语料如果靠手抓，
一周就旧了；而且每次全量重抓 190MB 没必要。这个引擎只做差量：

    sync   = 刷新清单（调 zhixueyun-kc-video 的 kc_docs_collect.py）→ 与 state 比对
             → 只下新增/变更的附件 → 抽文本 → 重建 2-gram 索引 → 覆盖 exam-lookup-rs 的清单文档
    status = 看上次同步时间、文档数、过期天数
    diff   = 只看差量，不下载

状态文件 ~/x_videos-dowloads/exam-lookup/jr/out/docs_state.json 记录每个 attachmentId 的
{name, course, domain, size, url, file, text_chars, sha1}；时间戳写在 updated_at。

用法:
    python3 kb_docs_sync.py sync                 # 在线：要求 9224 调试 Chrome 已登录
    python3 kb_docs_sync.py sync --offline       # 离线：用已有清单，只补缺失的下载/文本/索引
    python3 kb_docs_sync.py status
    python3 kb_docs_sync.py diff

定时（每周一次即可，平台更新没那么快）:
    hermes cronjob 或系统 cron：
    0 7 * * 1 cd ~/x_videos-dowloads/exam-lookup && python3 kb_docs_sync.py sync >> jr/out/docs_sync.log 2>&1
"""
import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
import time

HOME = os.path.expanduser("~")
PROJ = os.path.join(HOME, "x_videos-dowloads/exam-lookup")
OUT = os.path.join(PROJ, "jr/out")
DOCS_DIR = os.path.join(OUT, "docs_new")
TEXT_DIR = os.path.join(OUT, "docs_new_text")
STATE = os.path.join(OUT, "docs_state.json")
LOG = os.path.join(OUT, "docs_sync.log")
CATALOG = os.path.join(HOME, "develop_software/exam-lookup-rs/docs/platform-training-docs.json")
CATALOG_MD = os.path.join(HOME, "develop_software/exam-lookup-rs/docs/platform-training-docs.md")
COLLECT = os.path.join(HOME, ".hermes/skills/media/zhixueyun-kc-video/scripts/kc_docs_collect.py")
INDEX = os.path.join(PROJ, "kb_docs_index.py")


def load_state():
    if os.path.exists(STATE):
        return json.load(open(STATE, encoding="utf-8"))
    return {"updated_at": None, "docs": {}}


def save_state(st):
    st["updated_at"] = time.strftime("%Y-%m-%d %H:%M:%S")
    json.dump(st, open(STATE, "w", encoding="utf-8"), ensure_ascii=False, indent=1)


def log(msg):
    line = "[%s] %s" % (time.strftime("%Y-%m-%d %H:%M:%S"), msg)
    print(line)
    with open(LOG, "a", encoding="utf-8") as fp:
        fp.write(line + "\n")


def min_bytes(ext):
    """txt/md/csv 可能只有几十字节（比如一段演示环境地址），不能按 1KB 门槛判失败。"""
    return 1 if (ext or "").lower() in ("txt", "md", "csv") else 1024


def file_for(d):
    import re
    ext = d.get("ext") or "bin"
    safe = re.sub(r'[\\/:*?"<>|]', "_", "%s_%s.%s" % (d["att"], (d["name"] or "")[:40], ext))
    return os.path.join(DOCS_DIR, safe)


def sha1_of(path):
    h = hashlib.sha1()
    with open(path, "rb") as fp:
        for blk in iter(lambda: fp.read(1 << 20), b""):
            h.update(blk)
    return h.hexdigest()


def refresh_catalog(offline):
    """在线：跑一次收集脚本刷新 catalog；离线：直接用现有 catalog。"""
    if offline:
        if not os.path.exists(CATALOG):
            sys.exit("离线模式但 catalog 不存在：%s" % CATALOG)
        log("catalog: offline（沿用 %s）" % time.strftime("%Y-%m-%d %H:%M", time.localtime(os.path.getmtime(CATALOG))))
        return json.load(open(CATALOG, encoding="utf-8"))
    tmp = os.path.join(OUT, "_catalog_fresh")
    os.makedirs(tmp, exist_ok=True)
    log("catalog: 调 %s 刷新清单…" % os.path.basename(COLLECT))
    r = subprocess.run([sys.executable, COLLECT, "--port", "9224", "--out", tmp],
                       capture_output=True, text=True)
    if r.returncode != 0 or not os.path.exists(os.path.join(tmp, "docs.json")):
        log("catalog 刷新失败（%s）；退回现有 catalog" % (r.stderr or r.stdout)[-200:].strip())
        return json.load(open(CATALOG, encoding="utf-8"))
    fresh = json.load(open(os.path.join(tmp, "docs.json"), encoding="utf-8"))
    log("catalog: 刷新 ok，courses=%s docs=%d" % (fresh.get("courses"), len(fresh["docs"])))
    return fresh


def cmd_diff(a):
    cat = refresh_catalog(a.offline) if a.online else json.load(open(CATALOG, encoding="utf-8"))
    st = load_state()
    new = [d for d in cat["docs"] if d["att"] not in st["docs"]]
    gone = [k for k in st["docs"] if k not in {d["att"] for d in cat["docs"]}]
    changed = [d for d in cat["docs"] if d["att"] in st["docs"]
               and (st["docs"][d["att"]].get("size") != d.get("size")
                    or st["docs"][d["att"]].get("url") != d.get("url"))]
    print("清单 %d 条 | 新增 %d | 变更 %d | 下架 %d" % (len(cat["docs"]), len(new), len(changed), len(gone)))
    for d in new[:10]:
        print("  + [%s] %s" % (d["domain"], d["name"][:52]))
    for d in changed[:10]:
        print("  ~ [%s] %s（%s → %s bytes）" % (d["domain"], d["name"][:40],
                                                st["docs"][d["att"]].get("size"), d.get("size")))
    for k in gone[:10]:
        print("  - %s" % st["docs"][k].get("name", k)[:52])
    return 0


def cmd_sync(a):
    t0 = time.time()
    cat = refresh_catalog(a.offline)
    st = load_state()
    cur = {d["att"]: d for d in cat["docs"]}
    new_ids = [k for k in cur if k not in st["docs"]]
    changed_ids = [k for k in cur if k in st["docs"]
                   and (st["docs"][k].get("size") != cur[k].get("size")
                        or st["docs"][k].get("url") != cur[k].get("url"))]
    gone_ids = [k for k in st["docs"] if k not in cur]
    # 第三类差量：state 与主库记录的 sha1 不一致 → 说明有人手改过文件/状态，或上次写到一半
    import kb_db as KB
    drift_ids = []
    if os.path.exists(KB.DB_PATH):
        _db = KB.connect(KB.DB_PATH, readonly=True)
        db_sha = dict(_db.execute("SELECT att, sha1 FROM documents").fetchall())
        _db.close()
        drift_ids = [k for k in cur
                     if k in db_sha and db_sha[k] and db_sha[k] != (st["docs"].get(k) or {}).get("sha1")]
    log("diff: 清单 %d | 新增 %d | 变更 %d | 下架 %d | 状态漂移 %d"
        % (len(cur), len(new_ids), len(changed_ids), len(gone_ids), len(drift_ids)))

    os.makedirs(DOCS_DIR, exist_ok=True)
    fetched = failed = skipped = 0
    bytes_new = 0
    for k in new_ids + changed_ids:
        d = cur[k]
        path = file_for(d)
        if os.path.exists(path) and os.path.getsize(path) >= min_bytes(d.get("ext")) and k not in changed_ids:
            skipped += 1
        else:
            r = subprocess.run(["curl", "-s", "--max-time", "180", "-o", path, d["url"]],
                               capture_output=True)
            if r.returncode != 0 or not os.path.exists(path) or os.path.getsize(path) < min_bytes(d.get("ext")):
                failed += 1
                log("  FAIL fetch %s" % (d.get("name") or k)[:50])
                continue
            bytes_new += os.path.getsize(path)
            fetched += 1
        json.dump(d, open(path + ".json", "w"), ensure_ascii=False)
        st["docs"][k] = {"name": d.get("name"), "course": d.get("course"), "domain": d.get("domain"),
                         "size": d.get("size"), "url": d.get("url"), "ext": d.get("ext"),
                         "file": os.path.relpath(path, OUT), "sha1": sha1_of(path),
                         "synced_at": time.strftime("%Y-%m-%d %H:%M:%S")}
    for k in gone_ids:
        st["docs"][k]["gone_at"] = time.strftime("%Y-%m-%d %H:%M:%S")
    log("fetch: 新下 %d（%.1f MB）| 已有 %d | 失败 %d" % (fetched, bytes_new / 1048576, skipped, failed))

    # 抽文本 + 写入 SQLite 主库。只碰本次新增/变更的文档——这就是换掉 pickle 的理由：
    # pickle 一次改动要整库重建（6.6s 起），SQLite 单文档 3ms。
    import kb_docs_index as KI
    db_exists = os.path.exists(KB.DB_PATH)
    todo = list(cur) if not db_exists else sorted(set(new_ids + changed_ids + drift_ids))
    if todo:
        db = KB.connect()
        os.makedirs(TEXT_DIR, exist_ok=True)
        wrote = 0
        t_write = time.time()
        for k in todo:
            v = st["docs"].get(k) or {}
            path = os.path.join(OUT, v.get("file") or "")
            if not os.path.exists(path):
                continue
            meta = dict(cur[k])
            meta["sha1"] = sha1_of(path)
            v["sha1"] = meta["sha1"]          # 自愈：state 里的 sha1 被改坏会被这里纠正
            text = KI.normalize(KI.text_of(path, meta.get("ext") or ""))
            open(os.path.join(TEXT_DIR, os.path.basename(path).rsplit(".", 1)[0] + ".txt"),
                 "w", encoding="utf-8").write(text)
            _, action = KB.upsert_document(db, meta, text)
            v["text_chars"] = len(text)
            wrote += action != "unchanged"
        for k in gone_ids:
            KB.mark_gone(db, k)
        db.close()
        log("db: 变动 %d 份 / 处理 %d 份，%.1fs → %s" % (
            wrote, len(todo), time.time() - t_write, os.path.basename(KB.DB_PATH)))
    else:
        log("db: 无变化，跳过")
    if a.legacy_pickle:
        log("legacy-pickle: 重建 docs_index.pkl（只作对照基线）…")
        subprocess.run([sys.executable, INDEX, "index"], capture_output=True)

    # 覆盖项目里的清单（离线时 catalog 已是同一份）
    if not a.offline and os.path.exists(os.path.join(OUT, "_catalog_fresh", "docs.md")):
        shutil.copy(os.path.join(OUT, "_catalog_fresh", "docs.md"), CATALOG_MD)
        shutil.copy(os.path.join(OUT, "_catalog_fresh", "docs.json"), CATALOG)
        log("catalog: 已覆盖 %s 与 .json" % os.path.basename(CATALOG_MD))

    save_state(st)
    alive = sum(1 for v in st["docs"].values() if not v.get("gone_at"))
    with_text = sum(1 for v in st["docs"].values() if v.get("text_chars"))
    log("DONE %.1fs | 在册 %d（有文本 %d）| 下架 %d | 状态 %s" % (
        time.time() - t0, alive, with_text, len(gone_ids), os.path.basename(STATE)))
    return 0


def cmd_status(a):
    st = load_state()
    if not st.get("updated_at"):
        print("从未同步过")
        return 1
    age = (time.time() - time.mktime(time.strptime(st["updated_at"], "%Y-%m-%d %H:%M:%S"))) / 86400
    alive = [v for v in st["docs"].values() if not v.get("gone_at")]
    with_text = [v for v in alive if v.get("text_chars")]
    print("上次同步: %s（%.1f 天前）" % (st["updated_at"], age))
    print("在册 %d 份 | 有文本 %d 份 | 下架 %d 份 | 合计 %.1f MB" % (
        len(alive), len(with_text), len(st["docs"]) - len(alive),
        sum(v.get("size") or 0 for v in alive) / 1048576))
    if os.path.exists(os.path.join(OUT, "docs.sqlite")):
        try:
            import kb_db as KB
            db = KB.connect(KB.DB_PATH, readonly=True)
            n_chunks = db.execute("SELECT COUNT(*) FROM chunks").fetchone()[0]
            n_bigrams = db.execute("SELECT COUNT(*) FROM postings").fetchone()[0]
            ver = db.execute("PRAGMA user_version").fetchone()[0]
            size = os.path.getsize(KB.DB_PATH) / 1048576
            db.close()
            print("主库 docs.sqlite: %.1f MB | schema v%d | %d chunk / %d bigram" % (size, ver, n_chunks, n_bigrams))
        except Exception as e:
            print("主库读取失败:", e)
    if age > 7:
        print("⚠ 超过 7 天没同步，建议跑一次 sync")
    return 0


def main():
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    for name, fn in (("sync", cmd_sync), ("diff", cmd_diff), ("status", cmd_status)):
        p = sub.add_parser(name)
        p.add_argument("--offline", action="store_true", help="不刷新清单，用现有 catalog（不需要登录）")
        p.add_argument("--online", action="store_true", help="diff 时也先刷新清单")
        p.add_argument("--legacy-pickle", action="store_true", help="顺带重建 docs_index.pkl（对照基线用）")
        p.set_defaults(fn=fn)
    a = ap.parse_args()
    return a.fn(a)


if __name__ == "__main__":
    sys.exit(main())
