#!/usr/bin/env python3
"""把平台培训文档（187 份清单）变成可检索的语料，并对题库做命中率评估。

四步流水线（每步独立可重跑）:

    python3 kb_docs_index.py fetch    --docs-json <platform-training-docs.json> [--domains 综维,动环,...] [--all]
    python3 kb_docs_index.py extract  [--dir docs_new]
    python3 kb_docs_index.py eval     --bank banks/dtzx-safety.json [--dir docs_new_text]

检索不用 FTS5 的 unicode61（中文整段会当一个 token），改用**CJK 二元组倒排**：
每个 chunk 记下它的汉字 bigram 集合，查询按 bigram 重叠打分（稀有 bigram 权重更高）。
这样不依赖分词器，也不受「题目是改写过的」影响太大。

评估口径（对每道题）:
  ① 定位：top-1 chunk 与题干内容词共享 ≥3 个 bigram
  ② 可作答：top-3 chunk 里出现正确选项的 ≥4 字连续片段
两步都过才算「能给出带出处答案」。
"""
import argparse
import json
import os
import re
import subprocess
import sys
import zipfile
from collections import defaultdict

ROOT = os.path.expanduser("~/x_videos-dowloads/exam-lookup/jr/out")
DOCS_DIR = os.path.join(ROOT, "docs_new")
TEXT_DIR = os.path.join(ROOT, "docs_new_text")
INDEX_PATH = os.path.join(ROOT, "docs_index.pkl")
CHUNK = 700

BOILER = ["下列说法正确的是", "下列说法错误的是", "以下", "下列", "不属于", "不包括", "正确的是",
          "错误的是", "必须", "应当", "应该", "如何", "多久", "多少", "什么", "哪些", "哪个",
          "可以", "不得", "规定", "要求", "标准"]


# ---------- 文本与分词 ----------
def cjk_bigrams(text):
    out = set()
    for run in re.findall(r"[\u4e00-\u9fff]+", text or ""):
        for i in range(len(run) - 1):
            out.add(run[i:i + 2])
    for tok in re.findall(r"[A-Za-z0-9]{2,}", text or ""):
        out.add(tok.lower())
    return out


def query_terms(text):
    s = text or ""
    for b in BOILER:
        s = s.replace(b, " ")
    return s


def normalize(text):
    """pdftotext 常在汉字之间插空格（"挖 掘 土 石 方"），不修掉会让 bigram 全打不中。"""
    t = text or ""
    t = re.sub(r"(?<=[\u4e00-\u9fff])[ \t\u00a0]+(?=[\u4e00-\u9fff])", "", t)
    return re.sub(r"\s+", " ", t).strip()


def chunks_of(text, size=CHUNK):
    text = normalize(text)
    return [text[i:i + size] for i in range(0, len(text), size)] if text else []


# ---------- 1) 下载 ----------
def cmd_fetch(a):
    docs = json.load(open(os.path.expanduser(a.docs_json), encoding="utf-8"))["docs"]
    if a.domains:
        want = {d.strip() for d in a.domains.split(",")}
        docs = [d for d in docs if d["domain"] in want]
    if not a.all:
        docs = docs[:a.limit]
    os.makedirs(DOCS_DIR, exist_ok=True)
    ok = skip = fail = 0
    total = 0
    for i, d in enumerate(docs, 1):
        ext = d["ext"] or "bin"
        name = re.sub(r'[\\/:*?"<>|]', "_", "%s_%s.%s" % (d["att"], d["name"][:40], ext))
        path = os.path.join(DOCS_DIR, name)
        meta = path + ".json"
        if os.path.exists(path) and os.path.getsize(path) > 0:
            skip += 1
            if not os.path.exists(meta):
                json.dump(d, open(meta, "w"), ensure_ascii=False)
            continue
        r = subprocess.run(["curl", "-s", "--max-time", "120", "-o", path, d["url"]],
                           capture_output=True)
        if r.returncode == 0 and os.path.exists(path) and os.path.getsize(path) >= min_bytes(d.get("ext")):
            json.dump(d, open(meta, "w"), ensure_ascii=False)
            total += os.path.getsize(path)
            ok += 1
        else:
            fail += 1
            print("  FAIL", d["name"][:40])
            if os.path.exists(path):
                os.remove(path)
        if i % 10 == 0:
            print("  %d/%d ok=%d skip=%d fail=%d %.0fMB" % (i, len(docs), ok, skip, fail, total / 1048576))
    print("fetch: ok=%d skip=%d fail=%d total=%.1fMB dir=%s" % (ok, skip, fail, total / 1048576, DOCS_DIR))


# ---------- 2) 抽文本 ----------
def text_of(path, ext):
    ext = ext.lower()
    if ext == "pdf":
        out = path + ".txt"
        subprocess.run(["pdftotext", "-enc", "UTF-8", path, out], capture_output=True)
        return open(out, encoding="utf-8", errors="ignore").read() if os.path.exists(out) else ""
    if ext in ("docx", "pptx", "xlsx"):
        try:
            with zipfile.ZipFile(path) as z:
                names = [n for n in z.namelist()
                         if n == "word/document.xml" or n.startswith("ppt/slides/slide") or n == "xl/sharedStrings.xml"]
                parts = []
                for n in names:
                    xml = z.read(n).decode("utf-8", "ignore")
                    parts.append(re.sub(r"<[^>]+>", " ", xml))
                return re.sub(r"\s+", " ", " ".join(parts))
        except Exception as e:
            print("  unzip fail", os.path.basename(path), e)
            return ""
    if ext in ("txt", "csv", "md"):
        return open(path, encoding="utf-8", errors="ignore").read()
    return ""


def cmd_extract(a):
    src = os.path.expanduser(a.dir)
    os.makedirs(TEXT_DIR, exist_ok=True)
    n = chars = 0
    for f in sorted(os.listdir(src)):
        if f.endswith(".json"):
            continue
        p = os.path.join(src, f)
        meta = json.load(open(p + ".json", encoding="utf-8")) if os.path.exists(p + ".json") else {}
        ext = (meta.get("ext") or f.rsplit(".", 1)[-1]).lower()
        if ext not in ("pdf", "docx", "pptx", "xlsx", "txt", "csv", "md"):
            continue
        t = text_of(p, ext)
        if len(t) < 200:
            continue
        out = os.path.join(TEXT_DIR, os.path.basename(p).rsplit(".", 1)[0] + ".txt")
        open(out, "w", encoding="utf-8").write(normalize(t))
        n += 1
        chars += len(t)
    print("extract: %d 篇 / %.1f 万字符 → %s" % (n, chars / 10000, TEXT_DIR))


# ---------- 3) 建索引 ----------
def build_index(text_dir):
    post = defaultdict(set)
    chunks = []
    docs = []
    for f in sorted(os.listdir(text_dir)):
        if not f.endswith(".txt"):
            continue
        meta_path = os.path.join(DOCS_DIR, f.rsplit(".", 1)[0] + "." + "json")
        # 元数据文件名带扩展名，比较宽松地找一遍
        meta = {}
        for cand in sorted(os.listdir(DOCS_DIR)):
            if cand.endswith(".json") and cand.startswith(f.rsplit(".", 1)[0][:40]):
                try:
                    meta = json.load(open(os.path.join(DOCS_DIR, cand), encoding="utf-8"))
                except Exception:
                    pass
                break
        text = open(os.path.join(text_dir, f), encoding="utf-8", errors="ignore").read()
        doc_id = len(docs)
        docs.append({"id": doc_id, "text_file": f, "name": meta.get("name") or f,
                     "course": meta.get("course") or "", "domain": meta.get("domain") or "",
                     "url": meta.get("url") or ""})
        for si, ch in enumerate(chunks_of(text)):
            cid = len(chunks)
            chunks.append({"id": cid, "doc": doc_id, "seq": si, "text": ch})
            for g in cjk_bigrams(ch):
                post[g].add(cid)
    return {"docs": docs, "chunks": chunks, "post": {g: sorted(v) for g, v in post.items()}}


def cmd_index(a):
    idx = build_index(TEXT_DIR)
    import pickle
    pickle.dump(idx, open(INDEX_PATH, "wb"))
    print("index: docs=%d chunks=%d bigrams=%d → %s" % (
        len(idx["docs"]), len(idx["chunks"]), len(idx["post"]), INDEX_PATH))


# ---------- 4) 评估 ----------
def retrieve(idx, query, topk=3):
    grams = cjk_bigrams(query_terms(query))
    if not grams:
        return []
    weight = {g: 1.0 / (1 + len(idx["post"].get(g, ()))) for g in grams}
    score = defaultdict(float)
    for g, w in weight.items():
        for cid in idx["post"].get(g, ()):
            score[cid] += w
    best = sorted(score.items(), key=lambda kv: -kv[1])[:topk]
    return [(idx["chunks"][cid]["doc"], idx["chunks"][cid]["text"], s) for cid, s in best]


def answer_windows(text, n=4):
    return {text[i:i + n] for i in range(max(0, len(text) - n + 1))}


def eval_bank(idx, bank_path, cite_out=None):
    bank = json.load(open(bank_path, encoding="utf-8"))
    qs = bank if isinstance(bank, list) else (bank.get("questions") or bank.get("items") or [])
    located = answered = 0
    cites = []
    for q in qs:
        hits = retrieve(idx, q.get("question") or "")
        if not hits:
            continue
        qg = cjk_bigrams(query_terms(q.get("question")))
        top_doc, top_text, _ = hits[0]
        shared = len(qg & cjk_bigrams(top_text))
        is_located = shared >= 3
        located += is_located
        at = " ".join(q.get("answerText") or [])
        a_win = answer_windows(at, 4)
        hit = next((h for h in hits if a_win and any(w in h[1] for w in a_win)), None)
        if hit:
            answered += 1
            if cite_out is not None:
                cites.append({"id": q.get("id"), "question": q.get("question"),
                              "answer": at, "doc": idx["docs"][hit[0]]["name"],
                              "course": idx["docs"][hit[0]]["course"],
                              "snippet": hit[1][:300]})
    n = len(qs)
    print("题库 %s：%d 题" % (os.path.basename(bank_path), n))
    print("  ① 定位到相关段落: %d/%d = %.0f%%" % (located, n, 100.0 * located / max(1, n)))
    print("  ② 能锚到正确选项（可给出处）: %d/%d = %.0f%%" % (answered, n, 100.0 * answered / max(1, n)))
    if cite_out and cites:
        json.dump(cites, open(cite_out, "w"), ensure_ascii=False, indent=1)
        print("  出处样例 →", cite_out, "(%d 条)" % len(cites))



def idf(idx, g):
    n = len(idx["chunks"])
    df = len(idx["post"].get(g, ()))
    return 1.0 + (n / (1 + df)) ** 0.5


def eval_options(idx, bank_path):
    """真正回答「能不能查到答案」：只看题干检索 top-k 段落，再用段落给四个选项打分选一个。"""
    bank = json.load(open(bank_path, encoding="utf-8"))
    qs = bank if isinstance(bank, list) else (bank.get("questions") or bank.get("items") or [])
    ok = 0
    misses = []
    for q in qs:
        hits = retrieve(idx, q.get("question") or "", topk=5)
        if not hits:
            continue
        ctx = " ".join(h[1] for h in hits)
        ctx_g = cjk_bigrams(ctx)
        scores = {}
        for o in (q.get("options") or []):
            g = cjk_bigrams(o.get("text") or "")
            if g:
                scores[o["key"]] = sum(idf(idx, x) for x in g if x in ctx_g)
        if not scores:
            continue
        pred = max(scores, key=scores.get)
        if pred in (q.get("answerKeys") or []):
            ok += 1
        else:
            misses.append((q.get("id"), (q.get("question") or "")[:26], pred,
                           "/".join(q.get("answerKeys") or [])))
    n = len(qs)
    print("  ③ 四选一答题准确率: %d/%d = %.0f%%" % (ok, n, 100.0 * ok / max(1, n)))
    if misses:
        print("     错例:", ", ".join("%s(选%s/答%s)" % (m[0], m[2], m[3]) for m in misses[:6]))
    return ok, n


def cmd_eval(a):
    import pickle
    idx = pickle.load(open(INDEX_PATH, "rb")) if os.path.exists(INDEX_PATH) else build_index(TEXT_DIR)
    eval_bank(idx, os.path.expanduser(a.bank), a.cite_out)
    eval_options(idx, os.path.expanduser(a.bank))
    if a.baseline_dir:
        base = build_index(os.path.expanduser(a.baseline_dir))
        print("\n--- 基线（旧语料 %s）---" % a.baseline_dir)
        eval_bank(base, os.path.expanduser(a.bank))
        eval_options(base, os.path.expanduser(a.bank))


def main():
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)

    f = sub.add_parser("fetch")
    f.add_argument("--docs-json", required=True)
    f.add_argument("--domains", help="逗号分隔，如 综维,动环,安全生产,通知与制度,云网技术")
    f.add_argument("--limit", type=int, default=60)
    f.add_argument("--all", action="store_true")
    f.set_defaults(fn=cmd_fetch)

    e = sub.add_parser("extract")
    e.add_argument("--dir", default=DOCS_DIR)
    e.set_defaults(fn=cmd_extract)

    i = sub.add_parser("index")
    i.set_defaults(fn=cmd_index)

    v = sub.add_parser("eval")
    v.add_argument("--bank", required=True)
    v.add_argument("--cite-out")
    v.add_argument("--baseline-dir", default=os.path.join(ROOT, "pdf"))
    v.set_defaults(fn=cmd_eval)

    a = ap.parse_args()
    return a.fn(a)


if __name__ == "__main__":
    sys.exit(main())
