#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""剩余未确认题的检索预对"""
import subprocess, re, html, urllib.parse, random, time, os
UA="Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36"
OUT="remaining_lookup.txt"
Q=[
 (21,"非开挖定向钻孔铺管 钻机设备与电力线 安全距离 以上"),
 (27,"安全口诀 风力五级以上 楼顶作业 停止"),
 (29,"重大风险整改完成后 验收合格 方可恢复作业 属地"),
 (30,"酒精 班组内 存放 防爆箱"),
 (39,"每日班会五必查 人员 车辆 工具 环境 天气"),
 (41,"较大风险 车辆轮胎磨损 48小时 整改方案"),
 (42,"一人一档 培训档案 考核时间 培训内容 考核试卷 成绩记录"),
 (44,"防护用品维护与报废 绝缘工具每半年 安全帽 安全带每2年 承重测试"),
 (45,"有限空间作业 严禁未通风未检测 严禁应急救援盲目进入"),
 (46,"杆上作业 材料放置稳妥 工具随手装入工具袋 不得向下抛扔"),
 (48,"地市公司安全检查的重点 工单审批 劳保用品 现场操作 隐患整改闭环"),
 (49,"安全生产专项资金 使用范围 隐患排查 应急管理 安全管理与监督"),
 (50,"涉电场景 安全防护用品 绝缘手套 绝缘电工鞋 验电笔"),
 (51,"光缆激光 一律视为有信号 不得用眼直视"),
 (52,"每日班会一必讲 安全案例 安全要领 安全口诀平安歌"),
 (56,"高风险与特殊场景作业包括 高空 有限空间 涉电"),
 (58,"新装拉线 钢绞线 与吊线型号一致 拉距 回填土夯实"),
 (59,"云网运营条线 外线作业 安全生产事故 分为 死亡 受伤"),
 (60,"安全防护用品采购与验收 核查营业执照 产品合格证 检测报告 禁止三无"),
 (61,"集团云网运营部 统筹推进 风险识别AI能力 远程监控 全国普及"),
 (62,"安全带 腰带 松紧度 能伸入1-2根手指 为宜"),
 (69,"无证或过期人员 系统冻结工单权限"),
 (71,"雷雨天气严禁上杆施工作业"),
]
def curl(u):
    for i in range(2):
        try:
            r=subprocess.run(["curl","-sSL","-m","25","-A",UA,"-H","Accept-Language: zh-CN,zh;q=0.9",
                              "-b","/tmp/exam_lookup_cookies.txt","-c","/tmp/exam_lookup_cookies.txt",u],
                             capture_output=True,text=True,timeout=35)
            if r.stdout and len(r.stdout)>3000: return r.stdout
        except Exception: pass
        time.sleep(1.5)
    return ""
def clean(s): return re.sub(r'\s+',' ',html.unescape(re.sub(r'<[^>]+>','',s))).strip()
def parse(h):
    out=[]
    for blk in re.split(r'<li class="res-list"',h)[1:]:
        m=re.search(r'<h3[^>]*>\s*<a[^>]*href="([^"]+)"[^>]*>(.*?)</a>',blk,re.S)
        if not m: continue
        d=re.search(r'class="res-desc"[^>]*>(.*?)</p>',blk,re.S)
        out.append((clean(m.group(2)), clean(d.group(1)) if d else ""))
    if not out:
        for m in re.finditer(r'<h3[^>]*>\s*<a[^>]*href="([^"]+)"[^>]*>(.*?)</a>(.*?)(?=<h3|$)',h,re.S):
            out.append((clean(m.group(2)), clean(m.group(3))[:200]))
    return out
with open(OUT,"a",encoding="utf-8") as f:
    for n,q in Q:
        f.write(f"\n##{n}## {q}\n"); f.flush(); got=False
        for tmpl in ["https://www.so.com/s?q={q}","https://www.sogou.com/web?query={q}"]:
            h=curl(tmpl.format(q=urllib.parse.quote(q)))
            if not h or ("验证" in h and len(h)<9000): continue
            for t,s in parse(h)[:4]:
                f.write(f"  · {t[:95]}\n    {s[:230]}\n"); got=True
            if got: break
        if not got: f.write("  (无)\n")
        f.flush(); time.sleep(random.uniform(1.5,2.5))
print("done",os.path.abspath(OUT))
