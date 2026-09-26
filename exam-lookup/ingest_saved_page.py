#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""把「从考试平台保存下来的页面」变成题库。

适用输入（任一种，直接喂文件）：
  · Ctrl+S 保存的 .html（网页，全部）
  · 页面上 Ctrl+A 复制出来的 .txt / .md
  · 打印成 PDF 后用 pdftotext 转的 .txt

能自动识别的几种常见排版：
  A) 题干 + A. B. C. D. 选项 + 「正确答案：X」/「参考答案 X」/「答案：X」
  B) 题干 + 选项 + 「我的答案：X」+「正确答案：Y」+「解析：…」   ← 错题本常见
  C) 题干 + 选项 + 「【答案】A、B、C」

用法：
  python3 ingest_saved_page.py 保存的页面.html --title "综维装维错题本" --out banks/xxx.json
  python3 ingest_saved_page.py 页面.html --dry-run      # 只看解析结果，不写文件
"""
import argparse, html as ihtml, json, os, re, sys

QSPLIT = re.compile(r"(?m)^\s*(\d{1,3})\s*[、.．)）]\s*(.+)$")
OPAT = re.compile(r"^\s*([A-F])\s*[、.．)）]\s*(.+)$")
# 答案的多种写法：正确答案/参考答案/答案/【答案】/我的答案
ANS_PATS = [
    r"(?:正确答案|参考答案|标准答案)\s*[:：]?\s*([A-F](?:\s*[、,，/]?\s*[A-F])*|正确|错误|对|错)",
    r"【答案】\s*([A-F](?:\s*[、,，/]?\s*[A-F])*|正确|错误|对|错)",
    r"(?<![我])答案\s*[:：]\s*([A-F](?:\s*[、,，/]?\s*[A-F])*|正确|错误|对|错)",
]


def to_text(raw, is_html):
    if not is_html:
        return raw
    h = re.sub(r"(?is)<(script|style|noscript).*?</\1>", "\n", raw)
    h = re.sub(r"(?i)<br\s*/?>|</(p|div|li|tr|h[1-6])>", "\n", h)
    h = re.sub(r"(?s)<[^>]+>", " ", h)
    h = ihtml.unescape(h)
    return re.sub(r"[ \t\u3000]+", " ", h)


def parse(text):
    qs, cur = [], None
    for ln in text.split("\n"):
        ln = ln.rstrip()
        if not ln.strip():
            continue
        mo = OPAT.match(ln)
        if mo and cur is not None:
            cur["options"].append({"key": mo.group(1), "text": mo.group(2).strip()})
            continue
        ans = None
        for p in ANS_PATS:
            m = re.search(p, ln)
            if m:
                ans = m.group(1)
                break
        if ans and cur is not None:
            a = re.sub(r"[^A-F]", "", ans)
            if not a:                        # 判断题写法
                a = "A" if ans in ("正确", "对") else "B"
                cur["judge"] = True
            cur["raw_answer"] = ans
            cur["keys"] = sorted(set(a))
            cur["line"] = ln.strip()
            continue
        if ("解析" in ln or "我的答案" in ln) and cur is not None:
            cur["line"] = (cur.get("line", "") + " | " + ln.strip())[:400]
            continue
        mq = QSPLIT.match(ln)
        if mq:
            if cur and cur["options"]:
                qs.append(cur)
            cur = {"num": int(mq.group(1)), "stem": mq.group(2).strip(),
                   "options": [], "raw_answer": "", "keys": [], "judge": False}
            continue
        if cur is not None and not cur["options"]:
            cur["stem"] += " " + ln.strip()          # 题干折行
    if cur and cur["options"]:
        qs.append(cur)
    return qs


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("file")
    ap.add_argument("--title", default="")
    ap.add_argument("--out", default="")
    ap.add_argument("--dry-run", action="store_true")
    a = ap.parse_args()

    raw = open(a.file, encoding="utf-8", errors="ignore").read()
    text = to_text(raw, a.file.lower().endswith((".html", ".htm", ".xml")))
    qs = parse(text)

    withA = [q for q in qs if q["keys"]]
    judge = [q for q in withA if q["judge"] or len(q["options"]) == 2]
    print("解析出题目: %d 道   其中带答案: %d" % (len(qs), len(withA)))
    print("  选项数分布: %s" % sorted({len(q["options"]) for q in qs}))
    if not withA:
        print("\n⚠ 没解析到答案。可能是：页面没保存完整（答案要登录才显示），")
        print("  或该页排版不在已支持的三种之内。把文件发我，我按实际版式加规则。")
    for q in withA[:6]:
        print("\n  [%d] %s" % (q["num"], q["stem"][:70]))
        for o in q["options"]:
            print("      %s %s. %s" % ("✔" if o["key"] in q["keys"] else " ", o["key"], o["text"][:56]))
        print("      -> %s   （原文：%s）" % ("/".join(q["keys"]), q["raw_answer"]))
    if a.dry_run or not withA:
        return
    bank = {"meta": {"title": a.title or os.path.basename(a.file),
                     "questionCount": len(withA), "volumes": 1,
                     "sourceNote": "来源：%s（考试平台保存页解析）" % os.path.basename(a.file)},
            "questions": [{
                "id": "SAVED-%04d" % q["num"], "volume": 1,
                "type": "judge" if q["judge"] else ("multiple" if len(q["keys"]) > 1 else "single"),
                "typeLabel": "判断题" if q["judge"] else ("多选题" if len(q["keys"]) > 1 else "单选题"),
                "number": q["num"], "question": q["stem"],
                "options": q["options"], "answerKeys": q["keys"],
                "answerText": [o["text"] for o in q["options"] if o["key"] in q["keys"]],
                "topic": a.title or "", "category": a.title or "", "score": 1,
                "analysis": "来源：%s | 原文：%s" % (os.path.basename(a.file), q["line"]),
            } for q in withA]}
    out = a.out or "banks/saved-%s.json" % re.sub(r"\W+", "-", os.path.basename(a.file))[:24]
    os.makedirs(os.path.dirname(out) or ".", exist_ok=True)
    json.dump(bank, open(out, "w", encoding="utf-8"), ensure_ascii=False, indent=1)
    print("\n-> %s  (%d 题)" % (out, len(withA)))


if __name__ == "__main__":
    main()
