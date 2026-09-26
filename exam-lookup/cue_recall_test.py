#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""合成召回测试：用用户真实口吻的话（不含技能名）匹配 Recall cues 的触发面"""
import re, sys, pathlib

SK = pathlib.Path.home() / ".hermes/skills/research/answer-verification/SKILL.md"
txt = SK.read_text(encoding="utf-8")
m = re.search(r"## Recall cues\n(.*?)(?=\n## )", txt, re.S)
cues = m.group(1) if m else ""
# 取出每条 cue 的加粗触发串，拆成关键词
trig = re.findall(r"\*\*「(.+?)」\*\*", cues)
def kws(s):
    s = re.sub(r"[（(].*?[)）]", "", s)
    return [k.strip() for k in re.split(r"[／/|、，, ]+", s) if len(k.strip()) >= 2]
KW = {t: kws(t) for t in trig}

CASES = [
 "按照豆包的题目答案我只得了74分的考试成绩？",
 "我这次考试只考了62分，是不是答案不对",
 "丢分和错题数对不上，你再算算",
 "题库不应该是人工的提供题目的一条路，应该有实时检索和自动增加新题目的能力？",
 "我想要能自动加新题目的那种",
 "这种题能不能自动收录进题库",
 "本地调用方便吗？",
 "能不能在我自己电脑上跑起来",
 "在自己机器上用起来麻烦吗",
 "查一下天翼云的官网有没有这类题目的文档和题库最好呀？",
 "去哪儿能找到这套卷的题库",
 "这种题库源好找吗",
 "以后还会有这类的考试，怎么才能提高答题的准确率",
 "这些题明年还要考，怎么长远提高准确率",
 "怎么复习比较好",
 "我给你的题目和答案是豆包的答案",
 "这是AI给的答案卷，靠谱吗",
 "考试宝这类网站应该有比较多的题库？",
 "那个站一直抓不到怎么办",
 "它返回567是什么",
 "是不是被反爬封了",
 "搜到的摘要里没有答案怎么写",
 "答案搜不到啊",
 "题库站把答案藏起来了",
]
hits, rows = 0, []
for c in CASES:
    matched = [t for t, ks in KW.items() if any(k in c for k in ks)]
    if matched: hits += 1
    rows.append((c, matched))
print(f"召回命中 {hits}/{len(CASES)} = {hits/len(CASES):.0%}\n")
for c, ms in rows:
    flag = "OK " if ms else "MISS"
    print(f"  {flag} {c[:44]:46s} -> {ms[0][:34] if ms else '（无 cue 命中）'}")
miss = [c for c, ms in rows if not ms]
print()
print("未命中:", miss if miss else "无")
print("\n现有 cue 触发面:", list(KW))
