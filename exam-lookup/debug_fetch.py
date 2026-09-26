#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""诊断：检索返回的摘要长什么样，答案解析为什么失败"""
import sys
sys.path.insert(0, "/Users/hgxszhj/x_videos-dowloads/exam-lookup")
import solve as S

tests = [
    ("长问句（带选项）", "数字化创新赋能中，人员与资质管理的措施包括（）A.建立电子资质台账 B.录入特种作业人员证件信息 C.系统自动提醒证件复审 D.对无证或过期人员冻结工单权限"),
    ("短问句（去选项）", "数字化创新赋能中 人员与资质管理的措施包括"),
    ("题库站原题式", "人员与资质管理的措施包括 建立电子资质台账 答案"),
    ("判断题", "有限空间作业中 每30分钟复测气体 判断题 答案"),
]
for label, q in tests:
    print("=" * 74)
    print(f"【{label}】{q[:70]}")
    print("=" * 74)
    sn = S.snippets(q)
    print(f"摘要条数: {len(sn)}")
    for eng, t in sn[:4]:
        print(f"  [{eng}] {t[:300]}")
    if not sn:
        print("  ⚠️ 一条都没抓到 —— 抓取或解析坏了")
    a, v, samp = S.vote(sn, is_judge=False)
    print(f"  → 投票结果: 答案={a} 票数={v}")
    print()
