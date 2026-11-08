#!/usr/bin/env python3
"""apply_adjudication —— 把「逐题读原文」的判定结果写入题库与报告。

判定口径（只认原文，不认题库/文件里的答案）：
    supported        原文直接支持该答案（引原句）
    partial          原文部分支持（说明缺哪一块）
    unknown          原文没有相关信息
    order-pending    排序题需人工对照
"""
import json
import os
import shutil
import sys
import time

PROJ = os.path.expanduser("~/x_videos-dowloads/exam-lookup")
OUT = os.path.join(PROJ, "jr/out")
BANK = os.path.join(PROJ, "banks/dtzx-yungai.json")
DOC = "【学习资料】云改数转智惠战略宣贯要点.pdf"

# 题号 → (判定, 原文依据句/说明)
V = {
    1: ("supported", "云改数转智惠战略是…在云改数转战略基础上的继承与发展"),
    2: ("supported", "深刻理解云改数转智惠战略，必须准确把握其中蕴含的政治性要求"),
    3: ("supported", "战略的本质是体现以人民为中心的发展思想"),
    4: ("supported", "个人（2C）方面，顺应个人信息服务 AI 化趋势"),
    5: ("supported", "要把科技创新作为企业转型和价值创造的核心"),
    6: ("supported", "集中力量突破息壤“第一科技”关键核心技术"),
    7: ("supported", "按照“网是基础、云为核心、智惠共生、云网智融”的原则"),
    8: ("supported", "全面推动数据中心向 AIDC 演进"),
    9: ("supported", "AI 是数智化平台的“神经中枢”"),
    10: ("supported", "全集团要全面拥抱 AI，集团内部所有流程都必须用 AI 重新跑一遍"),
    11: ("supported", "改革开放是“关键一招”，核心是打破阻碍高质量发展的体制机制障碍"),
    12: ("partial", "珠峰计划=面向顶尖人才（科学家引领、硕博士为主）；「产数技术人才」对应的是五岳计划，选项 A 混了两者表述"),
    13: ("supported", "强化 AI 应用，增强识别、预警和处置能力，赋能综合反诈治理"),
    14: ("supported", "政企（2G）方面，从数字政务升级到智慧政务"),
    15: ("supported", "积极探索“耐心资本”、科研费用“包干制”等支持政策"),
    16: ("supported", "海外（2O）方面：从基础连接升级到战新融合"),
    17: ("supported", "研发攻击/防守智能体，进一步扩大安全大模型部署范围"),
    18: ("supported", "省公司要切实承担起自研产品规模推广的主体责任"),
    19: ("supported", "建立“销售、交付、运营”一体化服务"),
    20: ("supported", "三个基本原则为：服务集团战略全局／找准位置明确功能定位和主责主业／建立市场化机制；「追求短期利润最大化」不在其中"),
    21: ("supported", "战略是…在云改数转战略基础上的继承与发展；是一个系统、有机的整体；本质是体现以人民为中心；旗帜鲜明讲政治…集中体现"),
    22: ("supported", "党建统领四条：服务党和国家工作大局／增强核心功能提升核心竞争力／树立鲜明价值取向和评价标准／以高质量党建引领保障高质量发展"),
    23: ("supported", "精确适配产品和服务：个人（2C）、家庭（2H）、行业（2B）、政企（2G）、海外（2O）"),
    24: ("supported", "推进解决方案全面升级，加强融 AI、融量子、融安全、融绿色"),
    25: ("supported", "加强应用基础研究，在云及云网融合、网络、人工智能、安全/量子方面"),
    26: ("supported", "实现通智超量一体化的云服务能力"),
    27: ("supported", "坚决防范涉网涉数涉诈风险（原文无“涉外”）"),
    28: ("supported", "肩负着汇聚、整合、调度、流转与服务的核心功能"),
    29: ("supported", "一体化推动政企、市场、渠道、云网等体系改革"),
    30: ("supported", "一是建强班子／二是优化结构／三是提高能力／四是锤炼作风"),
    31: ("supported", "加快连接升级…加快终端 AI 升级…加快平台融通升级（原文无“资费套餐升级”）"),
    32: ("supported", "推动核心操作自主实施、产数融合交付自主完成、核心系统自主研发运营（原文无“市场营销策略自主制定”）"),
    33: ("supported", "空天地融合、高中低轨协同、高中低频联动；5G-A 载波聚合升级；打造高中低速协同的物联网"),
    34: ("supported", "加强规划、采购、建设、运营各环节全流程绿色节能"),
    35: ("supported", "围绕“环境安全可靠、数据治理有效、模型算法可信、内容依法合规、应用风险可控”的目标"),
    36: ("supported", "三个基本原则：服务集团战略全局／在战略全局中找准位置、明确功能定位和主责主业／建立市场化机制（原文无“全员参与”）"),
    37: ("supported", "做到增存一体、营维一体、营服一体、线上线下一体"),
    38: ("partial", "原文只明确「揭榜挂帅」等灵活项目负责人选拔与管理机制，及“非红线即可为”容错机制；「赛马机制」在原文未出现"),
    39: ("supported", "在 MaaS／DaaS／PaaS／IaaS 方面，攻关…关键技术（四层均有单独段落）"),
    40: ("supported", "推进“量子+通信”“量子+云网”“量子+平台”（原文无“量子+终端”）"),
    41: ("supported", "原文为“在云改数转战略基础上的继承与发展”，与“全新、独立的战略”相悖，故题干错误"),
    42: ("supported", "各党务部门…如何利用人工智能提升自身工作能力和水平，杜绝置身事外，主动担当作为"),
    43: ("supported", "特别是利用好 AI 赋能，加强模型、系统和流程建设，治理能力水平更加智慧精准"),
    44: ("supported", "坚持向上赋能…推动自研产品“能用尽用、应用尽用”"),
    45: ("supported", "原文为“网是基础、云为核心”，题干说成“云是基础、网为核心”，故错误"),
    46: ("unknown", "原文只说“逐步从共生走向融合”，未出现“3.0阶段”“云网一体、按需定制、智能敏捷”“云随网动”等表述，无法判定"),
    47: ("supported", "数智化平台是 IT、CT、DT 融合的平台，是数据开放、能力开放的平台"),
    48: ("supported", "数字化是智能化升级的基石…大型国企需先补齐数字化短板"),
    49: ("supported", "改革和开放是一体的，开放是重大战略…（题干“对内改革是另一回事”与之相悖）"),
    50: ("supported", "面向产数技术人才实施“五岳计划”；珠峰计划是面向顶尖人才（题干混淆，故错误）"),
    51: ("supported", "推动现有队伍从 CT 向 IT 转型，将 IT 专业作为人才队伍建设的重要方向"),
    52: ("supported", "提高自助服务占比…引导客户从传统的人工服务转为使用自助服务"),
    53: ("supported", "推动从“AI+生产工作”向“数字员工+自然人员工”转变；加快发展“数字员工”新质生产力"),
    54: ("supported", "海外（2O）方面…聚焦“一带一路”加大出海突破力度"),
    55: ("supported", "狠抓业务管理，加强内部监管，坚决防范“内鬼”"),
    56: ("supported", "内部云…实现 IT 和 CT 业务统一承载，向云原生、智能化全面发展"),
    57: ("supported", "在安全方面…努力成为值得信任的数字社会底座"),
    58: ("supported", "集团产品委员会…充分发挥统筹指导作用；各级产品委员会…做好协同和衔接"),
    59: ("supported", "要落实“严实快”的工作作风"),
    60: ("partial", "原文有“人机协同是未来发展主要趋势”“数字员工+自然人员工”，但无“AI 不是替代员工，而是赋能员工”原句，属间接支撑"),
    61: ("order-pending", "六项内涵顺序需人工对照原文（科技创新-改革开放-干部人才-云网融合-绿色安全-数智化平台）"),
}


def main():
    data = json.load(open(BANK, encoding="utf-8"))
    qs = data if isinstance(data, list) else (data.get("questions") or [])
    before = {q.get("id"): list(q.get("answerKeys") or []) for q in qs}

    n = 0
    for q in qs:
        # 题库题号 YG-xxxx 与文件题号一一对应（同为 60 题、同序）
        try:
            no = int(str(q.get("id", "")).split("-")[-1])
        except ValueError:
            continue
        if no not in V:
            continue
        verdict, why = V[no]
        q["docVerdict"] = verdict
        q["docEvidence"] = why
        q["docEvidenceDoc"] = DOC
        q["docCheckedAt"] = time.strftime("%Y-%m-%d")
        n += 1

    assert all(before[k] == next(x.get("answerKeys") for x in qs if x.get("id") == k) for k in before), "答案被改动"

    bak = BANK + ".bak-doc-" + time.strftime("%Y%m%d-%H%M%S")
    shutil.copy(BANK, bak)
    json.dump(data, open(BANK, "w", encoding="utf-8"), ensure_ascii=False, indent=1)
    print("写入 %d 道题的原文判定（备份 %s）" % (n, os.path.basename(bak)))

    cnt = {}
    for v, _ in V.values():
        cnt[v] = cnt.get(v, 0) + 1
    print("判定分布：", cnt)

    md = [os.path.join(OUT, "doc_adjudication.md")][0]
    with open(md, "w", encoding="utf-8") as fp:
        fp.write("# 逐题原文判定（只认原文，不认题库答案）\n\n")
        fp.write("依据文档：%s\n\n" % DOC)
        fp.write("| 判定 | 题数 |\n|---|---|\n")
        for k in ("supported", "partial", "unknown", "order-pending"):
            if cnt.get(k):
                fp.write("| %s | %d |\n" % (k, cnt[k]))
        fp.write("\n## 明细\n\n| # | 判定 | 答案 | 原文依据 |\n|---|---|---|---|\n")
        for no in sorted(V):
            v, why = V[no]
            fp.write("| %d | %s | %s | %s |\n" % (no, v, "", why.replace("|", "/")))
    print("报告 →", md)
    return 0


if __name__ == "__main__":
    sys.exit(main())
