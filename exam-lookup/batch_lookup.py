#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""批量检索存疑题，结果追加到 lookup_results.txt"""
import subprocess, re, html, urllib.parse, random, time, os, sys

UA = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36"
OUT = "lookup_results.txt"

QUERIES = [
 ("Q21 定向钻与电力线距离", "非开挖 定向钻 铺管 钻机 电力线 安全距离 米 以上"),
 ("Q22 电子资质台账复审提醒", "电子资质台账 系统 提前 提醒 特种作业人员 复审 个月"),
 ("Q26 培训档案保存期限", "从业人员安全教育培训档案 保存期限 不得少于 3年"),
 ("Q29 重大风险整改验收", "重大风险 整改完成 验收 属地安全管理部门 恢复作业"),
 ("Q38 水平桩高出路面", "通信管道 中心桩 水平桩 不得高出路面 1cm"),
 ("Q39 每日班会五必查", "每日班会 五必查 人员 车辆 工具 环境 天气 安全口诀"),
 ("Q44 安全带承重试验", "安全带 承重试验 每2年 每半年 通信 电力 安全"),
 ("Q54 省公司云网运营部职责", "省公司 云网运营部 职责 组织 应急演练 综维装维"),
 ("Q62 安全带腰带松紧", "安全带 腰带 松紧度 能伸入 1-2根手指 为宜"),
 ("Q74 安全带距杆梢", "安全带 兜挂 距杆梢 50厘米 以下 上杆"),
 ("Q77 安全积分", "综维装维 人员 安全积分 每人每年 12分 扣1-3分"),
]

def curl(url):
    for i in range(3):
        try:
            r = subprocess.run(["curl","-sSL","-m","30","-A",UA,
                                "-H","Accept-Language: zh-CN,zh;q=0.9",
                                "-b","/tmp/exam_lookup_cookies.txt",
                                "-c","/tmp/exam_lookup_cookies.txt", url],
                               capture_output=True, text=True, timeout=45)
            if r.stdout and len(r.stdout) > 3000:
                return r.stdout
        except Exception:
            pass
        time.sleep(2)
    return ""

def clean(s):
    return re.sub(r'\s+',' ', html.unescape(re.sub(r'<[^>]+>','', s))).strip()

def parse(h):
    out = []
    for blk in re.split(r'<li class="res-list"', h)[1:]:
        m = re.search(r'<h3[^>]*>\s*<a[^>]*href="([^"]+)"[^>]*>(.*?)</a>', blk, re.S)
        if not m: continue
        d = re.search(r'class="res-desc"[^>]*>(.*?)</p>', blk, re.S)
        out.append((clean(m.group(2)), clean(d.group(1)) if d else ""))
    if not out:
        for m in re.finditer(r'<h3[^>]*>\s*<a[^>]*href="([^"]+)"[^>]*>(.*?)</a>(.*?)(?=<h3|$)', h, re.S):
            out.append((clean(m.group(2)), clean(m.group(3))[:220]))
    return out

with open(OUT, "a", encoding="utf-8") as f:
    for tag, q in QUERIES:
        f.write("\n" + "="*72 + "\n" + tag + "\n查询: " + q + "\n")
        got = False
        for eng, tmpl in [("360","https://www.so.com/s?q={q}"),
                          ("搜狗","https://www.sogou.com/web?query={q}")]:
            h = curl(tmpl.format(q=urllib.parse.quote(q)))
            if not h or ("验证" in h and len(h) < 9000):
                continue
            for t, s in parse(h)[:6]:
                f.write(f"[{eng}] {t[:88]}\n        {s[:230]}\n")
                got = True
            if got: break
            time.sleep(random.uniform(3,5))
        if not got:
            f.write("(未取到结果)\n")
        f.flush()
        time.sleep(random.uniform(3,5))
print("done ->", os.path.abspath(OUT))
