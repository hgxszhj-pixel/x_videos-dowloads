#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
cite_law.py —— 给题库逐题配「条款号 + 原文摘录」（通用版：电信 + 保密）
=========================================================================================
  python3 cite_law.py                # 两个语料都配，写回题库（law 字段 + 解析末尾）
  python3 cite_law.py 电信 | 保密      # 只跑一侧
  python3 cite_law.py selftest       # 自检（FAIL exit 1）：摘录必须能在原文里逐字查到
  python3 cite_law.py 只看            # 只打印，不写题库
  python3 cite_law.py 候选            # 没配上的题出候选条款清单（不写题库）

为什么不用 kb_coverage 那套 6 字窗：它命中的是「哪份文件里有这句话」，而且 6 字窗会落到隔壁条款
（甚至落到法条开头的修订记录上 —— 那是误命中）。这里改成：
  ① 只认「按第N条分条」的语料（安全生产法/管理办法/管理办法解读/维护规程节选、保密法/实施条例），
     先按「第N条」切成条款单元；
  ② 探针取 24→8 字的最长公共窗（越长越可信），命中的是**具体某一条**；
  ③ 摘录直接取该条原文，自检断言它能在这几个文件里逐字查到（不许编）。
四条通道，越往下越保守：
  长窗命中 → 结构实测/沿革实测（从文件本身数出来）→ 人工核·锚词（写死条款 + 锚词，运行时断言）
  → 候选清单（只出 md，不动题库；配不上的题明确写原因）
判据：长窗 ≥14 字记「高」，10~13 记「中」，8~9 记「低」；低置信仍然给出，但表里标出来让你自己核。
"""
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ARTICLE = re.compile(r"第([一二三四五六七八九十百零]{1,7})条")
LINE_ART = re.compile(r"^[ \t　]*第([一二三四五六七八九十百零]{1,7})条", re.M)
CN = {"零": 0, "一": 1, "二": 2, "三": 3, "四": 4, "五": 5, "六": 6, "七": 7, "八": 8, "九": 9}
UNIT = {"十": 10, "百": 100}


def cn2int(s):
    """条号转数字（用来判递增）：九十 → 90，一百一十四 → 114"""
    total = cur = 0
    for ch in s:
        if ch in CN:
            cur = CN[ch]
        elif ch in UNIT:
            cur = (cur or 1) * UNIT[ch]
            total += cur
            cur = 0
    return total + cur
STRIP = re.compile(r"[\s　“”\"'（）()、，。；：！？《》【】…—\-·]")

CORPORA = {
    "电信": {
        "banks": ["banks/comm-safety-50.json", "banks/dtzx-safety.json", "banks/dtzx-yungai.json",
                  "banks/docs-otn-transport.json", "banks/docs-bigdata-ai.json",
                  "banks/docs-tianyi-security.json", "banks/教材原题-线路安全.json"],
        "sources": [("安全生产法（2021修正）", "jr/out/法规/中华人民共和国安全生产法（2021修正）.txt"),
                    # 解读版是 PPT 转文字（正文被拆成碎片、条号重复 8 处），当不了条款依据，不收
                    ("管理办法（试行）", "jr/out/官方原件/中国电信综合维护及装维作业安全生产管理办法（试行）.txt"),
                    ("维护规程节选", "jr/out/pdf/现场综合化维护认证课件-（第三篇  维护规程节选）.txt")],
        "md": "crawl/电信真题-法条对照.md", "draft": "crawl/电信真题-待配条文.md",
        # 电信侧只有「安全卷」是从《管理办法》《安全生产法》出的题；docs-*、云改那几本卷子的
        # 出处是教材正文（没有「第N条」结构），定位靠 kb_pages.py 的页码索引 —— 所以命中率下限放低
        # 命中率天然低：电信侧只有「安全卷」（dtzx-safety 62 题）的出处在《管理办法》《安全生产法》里，
        # docs-*、云改那几本卷子的出处是教材正文（没有「第N条」结构）→ 它们的依据走 kb_pages.py 的页码索引
        "min_rate": 0.05,
    },
    "保密": {
        "banks": ["banks/baomi-2024.json", "banks/baomi-2026.json", "banks/baomi-我的错题.json"],
        "sources": [("保守国家秘密法", "jr/out/baomi/法规/中华人民共和国保守国家秘密法(2024修订).txt"),
                    ("保守国家秘密法实施条例", "jr/out/baomi/法规/中华人民共和国保守国家秘密法实施条例(2024修订).txt")],
        "md": "crawl/保密真题-法条对照.md", "draft": "crawl/保密真题-待配条文.md",
        "min_rate": 0.5,
    },
}

# 人工核过的少数题：长窗配不上（题干是概念问法，条文用的是另一套措辞），但条款能靠一个「锚词」钉死。
# 写法要求：锚词必须真出现在那一条里，运行时断言（不许凭记忆写）。键＝(库文件名, 题号)。
CURATED = {
    ("baomi-2024.json", 20): ("保守国家秘密法", "十五", "保密事项范围", "「保密事项范围」的定义条 —— 题干问的就是它的定义"),
    ("baomi-2024.json", 23): ("保守国家秘密法", "三", "坚持中国共产党对保守国家秘密", "党管保密入法的原文（条里插了「（以下简称保密）」夹注，锚词取到这里）"),
    ("baomi-2024.json", 26): ("保守国家秘密法实施条例", "二十一", "国家秘密标志应当标注密级、保密期限", "国家秘密标志的规定条"),
    ("baomi-2024.json", 30): ("保守国家秘密法实施条例", "五", "不得将依法应当公开的事项确定为国家秘密", "「不得确定为国家秘密」的反面规定"),
    ("baomi-2024.json", 33): ("保守国家秘密法实施条例", "十九", "不能确定的，应当确定解密条件", "保密期限/解密时间/解密条件三种形式"),
    ("baomi-2024.json", 40): ("保守国家秘密法实施条例", "五十三", "离岗离职前，应当接受保密提醒谈话", "离职离岗前要做什么"),
    ("baomi-2024.json", 19): ("保守国家秘密法", "五十七", "没收违法所得", "法律责任章里「没收违法所得」的原文"),
    # 电信侧：数字型规定基本写在《管理办法》正文或附件1《作业规范》里，措辞与题干不同但对得上
    ("comm-safety-50.json", "4"): ("管理办法（试行）", "十三", "坠落高度2米以上", "高处作业的定义（第十三条「高空作业（坠落高度2米以上）」）"),
    ("dtzx-safety.json", "22"): ("管理办法（试行）", "十六", "系统自动提前36个月提醒复审", "电子资质台账的复审提醒（第十六条）"),
    ("dtzx-safety.json", "62"): ("管理办法（试行）·附件1", "", "松紧度以能伸入1～2根手指为宜", "安全带佩戴方法（附件1 二、安全防护用品配备）"),
    # 2026 卷（20 题，100 分）—— 长窗配不上的概念/判断型题，逐条人工核到条
    ("baomi-2026.json", 17): ("保守国家秘密法", "六十四", "适用工作秘密管理办法采取必要的保护措施",
                              "判断题「工作秘密…不需要管理」是错的 —— 第六十四条要求采取必要保护措施"),
    ("baomi-2026.json", 19): ("保守国家秘密法", "三十一", "将涉密信息系统、涉密信息设备接入互联网及其他公共信息网络",
                              "物理隔离的法源：第三十一条禁止接入互联网及在两者间交换信息"),
    ("baomi-2026.json", 20): ("保守国家秘密法实施条例", "四十二", "按照先审查、后公开的原则，对拟公开的信息逐项进行保密审查",
                              "宣传报道材料也要先审查后公开（第四十二条）"),
}

# 明确配不上条文的题：写清原因，不给假候选（硬凑的条款比「没有依据」更坏）
NO_CLAUSE = {
    ("dtzx-safety.json", "44"): "原文口径与题库答案**不一致**：附件1「二、安全防护用品配备」写的是「绝缘工具每季度进行1次绝缘测试、"
                                "安全帽每半年检查1次帽壳强度、安全带每2年进行1次承重测试」，本题官方答案给的是「绝缘工具每半年1次、"
                                "安全帽每年检查1次、安全带每2年1次」—— 三个数里两个对不上，不配依据（考前按题库官方答案记）",
    ("dtzx-safety.json", "21"): "题干问「非开挖定向钻孔铺管」与电力线的安全距离（2.5m），原文里 2.5m 只出现在「三线交越核查」和"
                                "「高压线下方或附近作业」两处（都是另一个场景）→ 语境对不上，不配依据",
    ("baomi-2024.json", 1): "「本次修订新增/修改了什么」的内容概览题 —— 法条正文里没有对应段落，"
                            "真正该引的是《保守国家秘密法》修订说明（没入库）",
    ("baomi-2024.json", 7): "「应党而生、伴党而行、为党而兴」是政治表述，不是法条规定，法条里查不到",
    # 2026 卷：4 道不配（3 道党史典故 + 1 道期限值不在本法里），另 13 道自动配、3 道人工核 → 16/20
    ("baomi-2026.json", 3): "党史典故题（中共一大用「平安家信」发会议通知）—— 保密法/实施条例正文里没有这段史实，"
                            "出处是党史读物，不是保密法规",
    ("baomi-2026.json", 5): "党史典故题（1934 年战略转移「分批移交防务，秘密撤离」）—— 同上，法条里查不到",
    ("baomi-2026.json", 7): "党史典故题（谢觉哉日记用「○○○○○」代替「共产主义者」）—— 同上，法条里查不到",
    ("baomi-2026.json", 15): "「一般涉密人员的脱密期为不少于1年」这个**期限数值本身不在**《保密法》《实施条例》正文里"
                             "（两法只说「实行脱密期管理」）；具体期限出自《涉密人员保密管理办法》，该办法未入库 → "
                             "不硬配第四十六条（那一条只说「实行脱密期管理」，撑不起「1年」这个答案）",
}


def flat(t):
    """去空白与标点再比：题干里有空框（　）、法条里有断行，不归一就永远对不上"""
    return STRIP.sub("", t or "")


def flat_q(t, aliases):
    """题干版本：还要抹掉法名本身。不抹的话，题干里一提法名就会命中《实施条例》第一条那句
    「根据《中华人民共和国保守国家秘密法》…的规定」——那是立法依据的样板话，不是考点
    （实测题 1/4/9/19 都上过这个当）。"""
    for a in aliases:
        t = t.replace(a, "")
    return flat(t)


def is_boiler(body):
    """立法依据的样板条（「根据X，制定本…」）：它出现在每部法开头，命中它等于没命中"""
    h = flat(body)[:40]
    return h.startswith("根据") and re.search(r"(制定|规定|修订)", h) is not None


def split_articles(raw):
    """按「第N条」切条款，但要挡掉正文里的**交叉引用**（「依照第九十条的规定追究…」也会匹配到
    「第九十条」，切出来是个碎片，还会把上一条正文截断 —— 安全生产法/保密法里都有）。
    收条的两条判据：① 行首的条标记，且条号严格递增；② 非行首但条号递增且到下一标记 ≥25 字。
    顺带把每条正文在「附件：」处截断：附件名会被题干蹭到，造成「第二十四条」这种无意义命中。"""
    starts = {m.start() for m in LINE_ART.finditer(raw)}
    marks = [(m.start(), m.group(1)) for m in ARTICLE.finditer(raw)]
    acc, last = [], 0
    for i, (pos, num) in enumerate(marks):
        nxt = marks[i + 1][0] if i + 1 < len(marks) else len(raw)
        gap = len(flat(raw[pos:nxt]))
        # 行首的条标记直接收（多文档汇编里条号会重新从头排，不能一律要求递增）；
        # 非行首的要递增且够长 —— 挡交叉引用
        if pos in starts or (cn2int(num) > last and gap >= 25):
            acc.append((pos, num))
            last = max(last, cn2int(num))
    out = []
    for i, (pos, num) in enumerate(acc):
        end = acc[i + 1][0] if i + 1 < len(acc) else len(raw)
        body = re.split(r"附件[：:]", raw[pos:end])[0]
        if len(flat(body)) >= 12:
            out.append((num, body))
    return out


def read_sources(corpus):
    """→ [(别名, 条号, 归一化正文, 原文)]"""
    out = []
    for alias, rel in corpus["sources"]:
        raw = open(os.path.join(HERE, rel), encoding="utf-8").read()
        for num, body in split_articles(raw):
            if is_boiler(body):
                continue
            if any(a == alias and n == num for a, n, _, _ in out):
                raise AssertionError("《%s》切出重复条号「%s」—— 切条规则要再收一收" % (alias, num))
            out.append((alias, num, flat(body), body))
    return out


def source_struct(corpus):
    """每个语料源文件自身的结构：几章几条（给「共有（N章M条）」这类题一个可复算的依据）"""
    res = []
    for alias, rel in corpus["sources"]:
        raw = open(os.path.join(HERE, rel), encoding="utf-8").read()
        n_ch = {m.group(1) for m in re.finditer(r"第([一二三四五六七八九十]{1,3})章", raw)}
        n_art = len(split_articles(raw))
        if n_ch and n_art:
            res.append((alias, len(n_ch), n_art))
    return res


def source_history(corpus):
    """每个语料源文件开头的沿革记录：第几次修订/修正"""
    res = []
    for alias, rel in corpus["sources"]:
        head = re.sub(r"\s+", "", open(os.path.join(HERE, rel), encoding="utf-8").read()[:400])
        n = len(set(re.findall(r"第[一二三四五六七八九十]次(?:修订|修正)", head)))
        if n:
            res.append((alias, n, head[:120]))
    return res


def cite(qtext, arts, aliases):
    """最长公共窗命中哪一条：窗越长越可信"""
    q = flat_q(qtext, aliases)
    best = None
    for L in (24, 20, 16, 14, 12, 10, 8):
        if len(q) < L:
            continue
        for s in range(0, len(q) - L + 1):
            w = q[s:s + L]
            for name, num, fbody, body in arts:
                if w in fbody:
                    best = {"law": name, "num": num, "score": L, "body": body}
                    break
            if best:
                break
        if best:
            break
    if best:
        best["conf"] = "高" if best["score"] >= 14 else ("中" if best["score"] >= 10 else "低")
    return best


STRUCT = re.compile(r"(\d)\s*章\s*(\d+)\s*条")


def structural(q, corpus):
    """题干/选项/答案里写了「N章M条」并且跟源文件实测一致 → 记一条结构依据（不是猜）"""
    blob = " ".join([q.get("question") or ""] + [str(o) for o in (q.get("answerText") or [])])
    m = STRUCT.search(flat(blob))
    if not m:
        return None
    for alias, n_ch, n_art in source_struct(corpus):
        if (int(m.group(1)), int(m.group(2))) == (n_ch, n_art):
            return {"law": "%s（全法结构）" % alias, "num": "%d章%d条" % (n_ch, n_art), "score": 0,
                    "conf": "结构实测", "body": "本文件实测：共 %d 章 %d 条" % (n_ch, n_art),
                    "label": "共 %d 章 %d 条（结构实测）" % (n_ch, n_art)}
    return None


def history_cite(q, corpus):
    """「第（）次修订/修正」这种题：数文件开头那句沿革，答案里的数字要跟数出来的一致"""
    ans = " ".join(str(o) for o in (q.get("answerText") or [])) + "".join(q.get("answerKeys") or [])
    digits = set(re.findall(r"\d", flat(ans)))
    for alias, n, head in source_history(corpus):
        if str(n) in digits or "一二三四五六七八九十"[n - 1] in flat(ans):
            return {"law": "%s（沿革）" % alias, "num": "修订记录", "score": 12, "conf": "沿革实测",
                    "body": "文件开头沿革记录：%s" % head, "label": "修订记录（沿革实测）"}
    return None


def curated(bank, q, arts, cname="电信"):
    """人工核过的题：按锚词在指定条款里找，找不到就报错（不静默跳过）。
    题库里的题号有 int 也有 str，两种键都试。"""
    base = os.path.basename(bank)
    ent = CURATED.get((base, q.get("number"))) or CURATED.get((base, str(q.get("number"))))
    global cur_corpus
    cur_corpus = (cname, corpus_obj)
    if not ent:
        return None
    law, num, anchor, why = ent
    if re.search(r"·附件[123]$", law) or law.endswith("·附件"):
        for alias, label, sent in sent_units(CORPORA[cur_corpus[0]], cur_corpus[0]):
            if alias == law and flat(anchor) in sent:
                return {"law": law, "num": label, "score": len(anchor), "body": sent,
                        "conf": "人工核·锚词", "why": why, "label": label}
        raise AssertionError("第%s题：锚词「%s」在《%s》附件里找不到" % (q.get("number"), anchor, law))
    for name, n, fbody, body in arts:
        if name == law and n == num:
            # 锚词按归一化后的文本比：原文里会插「（以下简称保密）」这类夹注、还用「，」
            assert flat(anchor) in fbody, \
                "第%s题：锚词「%s」不在《%s》第%s条里 —— 原文变了，重新核" % (q.get("number"), anchor, law, num)
            return {"law": law, "num": num, "score": len(anchor), "body": body,
                    "conf": "人工核·锚词", "why": why, "label": "第%s条" % num}
    raise AssertionError("第%s题：找不到《%s》第%s条" % (q.get("number"), law, num))


NUMTOK = re.compile(r"\d+(?:\.\d+)?\s*(?:kg|KG|千克|公斤|米|m|M|分钟|小时|个月|年|次|级|层|伏|kV|V|mA|℃|%|天|月|周|人|台|处|个|张|支|条|只|套|件|根|名)")
SEC_CN = re.compile(r"(?m)^[ \t　]*([一二三四五六七八九十]{1,2})、\s*(\S[^\n]{0,26})$")
SEC_NUM = re.compile(r"(?m)^[ \t　]*(\d{1,2})\s*[．.]\s*(\S[^\n]{0,26})$")


def numbers_in(text):
    return {m.group(0).replace(" ", "") for m in NUMTOK.finditer(text or "")}


def _tidy_title(s):
    t = re.sub(r"^\s*[（(]?\d{1,2}[）)]?\s*[．.、]?\s*", "", s or "")
    t = re.sub(r"^[一二三四五六七八九十]{1,2}、\s*", "", t)
    return re.sub(r"[。：:，,;；]\s*$", "", t).strip()


def _title(s):
    """只认「像标题」的短串：包装后的正文行（1.每天出工前，检查…）不算标题"""
    t = _tidy_title(s)
    return t if 2 <= len(t) <= 14 and not re.search(r"[，。；：！？]", t) else ""


def appendix_blocks(raw):
    """附件区（「附件：」之后）按「一、」「N．」小节切开 —— 综维装维的数字型规定基本都在这里，
    它没有「第N条」编号，所以按小节定位。→ [(附件名, 小节标签, 该节全文)]"""
    i = raw.find("附件：")
    region = raw[i:] if i >= 0 else ""
    if not region:
        return []
    marks = [(m.start(), m.group(0).strip()) for m in SEC_CN.finditer(region)]
    if not marks:
        marks = [(m.start(), m.group(0).strip()) for m in SEC_NUM.finditer(region)]
    if not marks:
        return []
    out = []
    for k, (pos, head) in enumerate(marks):
        end = marks[k + 1][0] if k + 1 < len(marks) else len(region)
        body = region[pos:end]
        if "口诀" in head or "口诀" in body[:80]:
            doc = "附件2"
        elif "平安歌" in head or "平安歌" in body[:80]:
            doc = "附件3"
        else:
            doc = "附件1"
        top = _title(head) or _tidy_title(head)[:14]
        inn = ""
        for m in SEC_NUM.finditer(body):
            t = _title(m.group(0))
            if t and t != top:
                inn = t
                break
        label = "%s › %s" % (top, inn) if inn else top
        out.append((doc, label, body))
    return out


_UNITS = {}


def sent_units(corpus, cname):
    """→ [(出处别名, 出处标签, 归一化句子)]：条文型按条切句，附件区按小节切句。
    用来做「数值锚定」——题里考的那个数是多少，就到原文里把写着这个数的那句话挑出来。"""
    if cname in _UNITS:
        return _UNITS[cname]
    out = []
    for alias, rel in corpus["sources"]:
        raw = open(os.path.join(HERE, rel), encoding="utf-8").read()
        for num, body in split_articles(raw):
            for line in re.split(r"(?<=。)", body):      # flat() 会把「。」也抹掉，所以先按原文切句
                line = flat(line)
                if len(line) >= 10:
                    out.append((alias, "第%s条" % num, line))
        for doc, label, text in appendix_blocks(raw):
            # 逐句定位：标签取「这句前面最近的那个 N．小标题」，不是整块里的第一个小标题
            subs = [(m.start(), _title(m.group(0))) for m in SEC_NUM.finditer(text)]
            subs = [(pos, t) for pos, t in subs if t]
            for m in re.finditer(r"[^。]+。?", text):
                line = flat(m.group(0))
                if len(line) < 10:
                    continue
                # 只标记到「一、」这一级：往里再精确定位（最近的 N．小标题）会错 ——
                # 原文的小节顺序与题目顺序无关（安全带那句被排在「动火作业配置」标题之后）。
                # 粗一点但一定对，比精一点但可能错强。
                lab = label.split(" › ")[0]
                out.append(("%s·%s" % (alias, doc), lab, line))
    _UNITS[cname] = out
    return out


def longest_window(a, b, lo=8, hi=24):
    """a 与 b 的最长公共窗（不超过 hi 字）"""
    n = min(len(a), len(b), hi)
    for L in range(n, lo - 1, -1):
        for i in range(0, max(0, len(a) - L + 1)):
            if a[i:i + L] in b:
                return L
    return 0


def numeric_cite(q, corpus, cname, aliases, min_win=8):
    """数值锚定：答案里的数字（5kg / 2.5m / 30分钟…）必须真出现在原文某句里，
    且题干与那句有 ≥8 字的连续重合 → 才认。定位标签是「哪一条 / 哪一小节」。"""
    ans = " ".join(str(o) for o in (q.get("answerText") or []))
    toks = numbers_in(ans) or numbers_in(q.get("question") or "")
    if not toks:
        return None
    stem = flat_q(q.get("question") or "", aliases)
    best = None
    for alias, label, s in sent_units(corpus, cname):
        if not any(t in s for t in toks):
            continue
        w = longest_window(stem, s)
        if w >= min_win and (best is None or w > best[0]):
            best = (w, alias, label, s)
    if not best:
        return None
    w, alias, label, s = best
    return {"law": alias, "num": label, "score": w, "conf": "数值锚定",
            "body": s, "label": label, "token": sorted(toks)[0]}


def candidates(q, arts, aliases, topn=3):
    """没配到条文的题：给候选条款（不写题库，只出清单让人点）。
    打分＝题干+答案里那些 4~8 字片段在条款里出现的总长度，片段越长、命中越多越靠前。"""
    text = flat_q((q.get("question") or "") + " " + " ".join(str(o) for o in (q.get("answerText") or [])), aliases)
    frags = set()
    for n in (8, 7, 6, 5, 4):
        for i in range(0, max(0, len(text) - n + 1)):
            frags.add(text[i:i + n])
    scored = []
    for name, num, fbody, body in arts:
        hit = [f for f in frags if f in fbody]
        if not hit:
            continue
        scored.append((sum(len(f) for f in hit), max(hit, key=len), name, num, body))
    scored.sort(reverse=True)
    return scored[:topn]


def qtext(q):
    return (q.get("question") or "") + " " + " ".join(
        (o.get("text") or "") if isinstance(o, dict) else str(o) for o in (q.get("options") or []))


def run(which=None, write=True):
    out = {}
    global corpus_obj
    for cname, corpus in CORPORA.items():
        corpus_obj = corpus
        if which and cname != which:
            continue
        arts = read_sources(corpus)
        aliases = [a for a, _ in corpus["sources"]]
        assert len(arts) > 30, "%s 语料切条失败，只切出 %d 条" % (cname, len(arts))
        rows = []
        for bf in corpus["banks"]:
            path = os.path.join(HERE, bf)
            j = json.load(open(path, encoding="utf-8"))
            hit = 0
            for q in j["questions"]:
                c = (cite(qtext(q), arts, aliases) or structural(q, corpus)
                     or history_cite(q, corpus) or curated(path, q, arts)
                     or numeric_cite(q, corpus, cname, aliases))
                if c:
                    hit += 1
                    q["law"] = {"source": "《%s》" % c["law"],
                                "article": c.get("label") or "第%s条" % c["num"],
                                "excerpt": re.sub(r"\s+", "", c["body"])[:150],
                                "confidence": c["conf"], "matchLen": c["score"]}
                else:
                    q.pop("law", None)
                base = re.sub(r"｜依据：.*$", "", q.get("analysis") or "")
                short = re.sub(r"\s+", "", c["body"]) if c else ""
                q["analysis"] = base + ("" if not c else "｜依据：《%s》%s：%s%s"
                                        % (c["law"], c.get("label") or "第%s条" % c["num"], short[:110],
                                           "…" if len(short) > 110 else ""))
                rows.append((os.path.basename(bf), q.get("number"), q.get("question"), c))
            if write:
                json.dump(j, open(path, "w", encoding="utf-8"), ensure_ascii=False, indent=1)
            print("%s：%d/%d 配到条款" % (bf, hit, len(j["questions"])))
        out[cname] = (rows, corpus)
        write_md(cname, corpus, rows)
        print("%s 合计 %d 题配到条款（其中高置信 %d）；对照表：%s"
              % (cname, sum(1 for r in rows if r[3]), sum(1 for r in rows if r[3] and r[3]["conf"] == "高"), corpus["md"]))
    return out


def write_md(cname, corpus, rows):
    L = ["# %s真题 → 条款对照（条号级，自动生成）" % cname, "",
         "生成：`python3 cite_law.py %s`。判据：探针取 24→8 字**最长公共窗**，命中的是具体条款；" % cname,
         "「高」=14 字以上，「中」=10~13，「低」=8~9（低置信仍列出，标出来让你自己核）。",
         "「结构实测 / 沿革实测」是从源文件本身数出来的（章条数、修订记录），「人工核·锚词」是写死条款 + 锚词并在运行时断言。", "",
         "| 库 | 题号 | 题干（截断） | 条款 | 置信 | 原文摘录 |", "|---|---|---|---|---|---|"]
    for bf, num, stem, c in rows:
        stem = (stem or "")[:34].replace("|", "\\|")
        if c:
            L.append("| %s | %s | %s | 《%s》%s | %s | %s |"
                     % (bf, num, stem, c["law"], c.get("label") or "第%s条" % c["num"], c["conf"],
                        re.sub(r"\s+", "", c["body"])[:70].replace("|", "\\|")))
        else:
            L.append("| %s | %s | %s | 未命中 | — | — |" % (bf, num, stem))
    open(os.path.join(HERE, corpus["md"]), "w", encoding="utf-8").write("\n".join(L) + "\n")


# 官方公布的条数（用来钉住切条规则：切错条必然在这里露馅）
KNOWN_ARTICLES = {"保守国家秘密法": 65, "保守国家秘密法实施条例": 73,
                  "安全生产法（2021修正）": 119, "管理办法（试行）": 24}


# 官方公布的条数（用来钉住切条规则：切错条必然在这里露馅）
KNOWN_ARTICLES = {"保守国家秘密法": 65, "保守国家秘密法实施条例": 73,
                  "安全生产法（2021修正）": 119, "管理办法（试行）": 24}


def selftest():
    for cname, corpus in CORPORA.items():
        arts = read_sources(corpus)
        for alias, n in KNOWN_ARTICLES.items():
            got = sum(1 for a, _, _, _ in arts if a == alias)
            if got:
                assert got == n, "%s《%s》只切出 %d 条（官方 %d 条）—— 切条规则被改坏了" % (cname, alias, got, n)
    out = run()
    assert out, "没有跑任何语料"
    for cname, (rows, corpus) in out.items():
        # 摘录必须逐字出现在源文件里（条文型与附件小节型一视同仁：都是源文件的子串）
        ftexts = [flat(open(os.path.join(HERE, rel), encoding="utf-8").read()) for _, rel in corpus["sources"]]
        bad = []
        for bf, num, stem, c in rows:
            if not c:
                continue
            key = (bf, num)
            if c["conf"] in ("结构实测", "沿革实测"):
                assert c["body"].startswith("本文件实测") or "沿革记录" in c["body"], \
                    "%s 第%s题 实测型依据的写法不对" % (bf, num)
            elif c["conf"] == "人工核·锚词":
                assert c["body"], "%s 第%s题 人工核的依据没有原文" % (bf, num)
                assert key in CURATED or (bf, str(num)) in CURATED, \
                    "%s 第%s题 标了人工核却不在 CURATED 表里" % (bf, num)
            elif c["conf"] == "数值锚定":
                if not any(flat(c["body"]) in t for t in ftexts):
                    bad.append("%s 第%s题 数值锚定的句子不在原文里" % (bf, num))
                if not c.get("token") or c["token"] not in c["body"]:
                    bad.append("%s 第%s题 数值锚定的句子里没有那个数（%s）" % (bf, num, c.get("token")))
                if not c.get("label"):
                    bad.append("%s 第%s题 数值锚定没有小节标签" % (bf, num))
            else:
                if not any(flat(c["body"]) in t for t in ftexts):
                    bad.append("%s 第%s题 摘录不在原文里（%s）" % (bf, num, c["law"]))
                if "次会议" in c["body"][:60]:
                    bad.append("%s 第%s题 命中会议记录/沿革（误命中）" % (bf, num))
                if ARTICLE.search(c["body"]) is None:
                    bad.append("%s 第%s题 条号取不到" % (bf, num))
        assert not bad, "有问题：%s" % bad[:3]
        got = sum(1 for r in rows if r[3])
        rate = 1.0 * got / len(rows) if rows else 0.0
        assert corpus["min_rate"] <= rate <= 0.95, \
            "%s 命中率 %.0f%% 超出预期区间（%.0f~95%%），先看是不是切条切坏了" % (cname, 100 * rate, 100 * corpus["min_rate"])
        nums = {r[3]["num"] for r in rows if r[3] and r[3]["conf"] not in ("结构实测", "沿革实测")}
        assert len(nums) >= 8, "%s 命中的出处只有 %d 个不同值，像是全落到同一处了" % (cname, len(nums))
        confs = {}
        for r in rows:
            if r[3]:
                confs[r[3]["conf"]] = confs.get(r[3]["conf"], 0) + 1
        print("selftest OK %s：%d/%d 题配到依据 %s，摘录都能在原文里逐字查到，出处 %d 个不同值"
              % (cname, got, len(rows), confs, len(nums)))


def draft():
    """给没配到条款的题出候选清单（只写 md，不动题库 —— 猜的东西不进去）"""
    for cname, corpus in CORPORA.items():
        arts = read_sources(corpus)
        aliases = [a for a, _ in corpus["sources"]]
        miss = []
        for bf in corpus["banks"]:
            for q in json.load(open(os.path.join(HERE, bf), encoding="utf-8"))["questions"]:
                if not q.get("law"):
                    miss.append((bf, q))
        L = ["# %s真题 · 还没配上条款的题（候选条款，待你确认）" % cname, "",
             "生成：`python3 cite_law.py 候选`。**这些候选没有写进题库** —— 长窗匹配配不上的题，",
             "多半是「整本书讲了什么」这类概览题，条款里没有一段连续的对应表述。",
             "下面是按「题干+正确答案里的 4~8 字片段命中条款」排出来的候选，供你自己翻原文核对；",
             "你点哪条我就写哪条（`law` 字段 + 解析末尾）。", ""]
        for bf, q in miss:
            L += ["## %s · 第 %s 题　%s" % (bf, q["number"], re.sub(r"\s+", "", q["question"])), "",
                  "正确答案：%s　%s" % ("".join(q.get("answerKeys") or []), " / ".join(q.get("answerText") or [])), ""]
            base = os.path.basename(bf)
            nc = NO_CLAUSE.get((base, q["number"])) or NO_CLAUSE.get((base, str(q["number"])))
            if nc:
                L += ["**不配条款**：%s" % nc, ""]
                continue
            cands = candidates(q, arts, aliases)
            if not cands:
                L += ["（一个候选都提不出来 —— 这题是整本书/整部法的概览，不算法条依据）", ""]
                continue
            L += ["| 候选 | 条款 | 命中片段 | 原文摘录 |", "|---|---|---|---|"]
            for score, frag, name, num, body in cands:
                L.append("| %d（%d 字） | 《%s》第%s条 | `%s` | %s |"
                         % (score, score, name, num, frag, re.sub(r"\s+", "", body)[:60]))
            L.append("")
        open(os.path.join(HERE, corpus["draft"]), "w", encoding="utf-8").write("\n".join(L) + "\n")
        print("待配条款 %d 题 → %s" % (len(miss), corpus["draft"]))


if __name__ == "__main__":
    arg = sys.argv[1] if len(sys.argv) > 1 else ""
    if arg == "selftest":
        selftest()
    elif arg == "候选":
        draft()
    elif arg == "只看":
        run(write=False)
    elif arg in CORPORA:
        run(which=arg, write=True)
    else:
        run(write=True)
