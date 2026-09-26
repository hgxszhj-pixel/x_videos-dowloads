#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
fetch_pub_law.py —— 抓公开法律/条例原文
  《保守国家秘密法》+《实施条例》→ jr/out/baomi/法规/（kb kind=保密法规，只管保密那门考试）
  要加电信口径的公开法规，就在 LAWS 里写一条不带 "dest" 的，会落到 jr/out/法规/（kind=法规）
=====================================================================================
  python3 fetch_pub_law.py            抓 LAWS 里全部（多源取最全，抓完自检）
  python3 fetch_pub_law.py selftest   只校验已有文件

为什么多源：政府网站常把「正文 + 附件」拆开，或把条款塞在 iframe 里；单个源经常只拿到
标题+开头。这里对每部法规抓 N 个源，按「条数够不够 + 关键锚点齐不齐」挑最全的那份，
拿不到就报错退出，不写半截文件（宁缺勿假）。
"""
import html
import os
import re
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
DEST = os.path.join(HERE, "jr/out/法规")                     # 默认落电信那本依据库（kind=法规）
DEST_BAOMI = os.path.join(HERE, "jr/out/baomi/法规")         # 保密教育用（kind=保密法规，不计入电信覆盖率）
UA = ("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 "
      "(KHTML, like Gecko) Chrome/153.0.0.0 Safari/537.36")

LAWS = [
    {
        "file": "中华人民共和国保守国家秘密法(2024修订).txt",
        "dest": DEST_BAOMI,
        "title": "中华人民共和国保守国家秘密法",
        "urls": [
            "http://bmj.wuhan.gov.cn/fgbzk/bmzffgk/202403/t20240308_2369739.shtml",
            "https://mzt.guizhou.gov.cn/xxgk/flfg_5870026/202402/t20240228_83856187.html",
            "https://mzt.xinjiang.gov.cn/xjmzt/c113001/202402/e152261ca43f49cfa16efd9ee757f8ca.shtml",
            "https://www.haian.gov.cn/hasgaj/zcwj/content/8ee8f47a-5b38-48c8-bea1-32d0d023d18e.html",
        ],
        "min_articles": 60,                       # 2024 修订版共 6 章 65 条
        "anchors": ["中国共产党", "保密协议", "第六十五条", "国家秘密的密级"],
    },
    {
        "file": "中华人民共和国保守国家秘密法实施条例(2024修订).txt",
        "dest": DEST_BAOMI,
        "title": "中华人民共和国保守国家秘密法实施条例",
        "urls": [
            "https://www.mee.gov.cn/zcwj/gwywj/202407/t20240723_1082257.shtml",
        ],
        "min_articles": 60,
        "anchors": ["国务院令", "786", "中国共产党", "保密法"],   # 条例用阿拉伯数字写「第786号」
    },
]
ART = re.compile(r"第([一二三四五六七八九十百零]+)条")


def fetch(url):
    p = subprocess.run(["curl", "-sSL", "-m", "30", "-A", UA, url], capture_output=True)
    raw = p.stdout.decode("utf-8", "ignore")
    raw = re.sub(r"(?is)<(script|style).*?</\1>", " ", raw)
    t = re.sub(r"(?s)<[^>]+>", "\n", raw)
    t = html.unescape(html.unescape(t))
    t = re.sub(r"[ \t\r\f\v\u3000]+", " ", t)
    return re.sub(r"\n\s*\n+", "\n", t).strip()


def trim(t, title):
    """掐掉新闻页头尾的杂物：从「第一章」往前走，取最后一次出现的法规标题作为起点。"""
    i = t.find("第一章")
    if i < 0:
        return t
    j = t.rfind(title, 0, i)
    return t[j:] if j > 0 else t


def best_of(law):
    cands = []
    for u in law["urls"]:
        try:
            t = fetch(u)
        except Exception as e:
            print("   抓失败 %s：%s" % (u[:60], e))
            continue
        n_art = len(set(ART.findall(t)))
        miss = [a for a in law["anchors"] if a not in t]
        cands.append((n_art, -len(miss), len(t), u, t, miss))
        print("   %-70s 条数 %3d 缺锚点 %s" % (u[:70], n_art, miss or "无"))
    cands.sort(reverse=True)
    if not cands or cands[0][0] < law["min_articles"] or cands[0][5]:
        return None
    return cands[0]


def run():
    ok = True
    for law in LAWS:
        os.makedirs(law.get("dest", DEST), exist_ok=True)
        print("== %s" % law["title"])
        got = best_of(law)
        if not got:
            print("   ✗ 没有一个源拿到完整原文（条数不够或缺锚点）—— 不写文件")
            ok = False
            continue
        n_art, _, n_char, u, t, _ = got
        t = trim(t, law["title"])
        # 不写「来源头」：那行会把法名塞进正文，之后任何提到「保密法」的题干都能命中它，
        # 覆盖率会虚高（实测 87% → 去掉头之后才是真数）。出处记在文件名 + 本脚本 + HANDOFF。
        path = os.path.join(law.get("dest", DEST), law["file"])
        open(path, "w", encoding="utf-8").write(t)
        with open(path + ".src", "w", encoding="utf-8") as fh:      # 侧面记出处，不进语料
            fh.write(u + "\n")
        print("   ✓ %s ← %s（%d 条 / %d 字）" % (law["file"], u[:60], n_art, n_char))
    return ok


def selftest():
    bad = []
    for law in LAWS:
        path = os.path.join(law.get("dest", DEST), law["file"])
        if not os.path.exists(path):
            bad.append("%s 不存在" % law["file"])
            continue
        t = open(path, encoding="utf-8").read()
        n_art = len(set(ART.findall(t)))
        miss = [a for a in law["anchors"] if a not in t]
        if n_art < law["min_articles"]:
            bad.append("%s 只数到 %d 条（要 ≥%d）" % (law["file"], n_art, law["min_articles"]))
        if miss:
            bad.append("%s 缺锚点 %s" % (law["file"], miss))
        print("  %-52s %3d 条 / %6d 字" % (law["file"], n_art, len(t)))
    assert not bad, "法规原文有问题：%s" % bad
    print("selftest OK：%d 部法规原文齐（条数与关键条款都在）" % len(LAWS))


if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1] == "selftest":
        selftest()
    else:
        run() and selftest()
