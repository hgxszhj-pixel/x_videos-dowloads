#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""机构/高校官网公开的「带答案」测试卷 → banks/*.json（题库）+ crawl/*问答*.json（背题卡）。

收件规矩（用户定的）：只收机构/高校官网自己发布的公开卷，答案按原卷自带；
不碰文库转载件（renrendoc / book118 那类「某年考试题库」＝考卷泄露）；答案不自己判也不自己编。

四种格式各一个解析器，都是零依赖（docx 用 zipfile 自己解）：
  1. shanwei —— 政府附件 docx：一题一段，题干（）+ 同行选项 + 答案标记。
  2. sduwh  —— 高校网页：选项一行一个，「(正确答案)」单独一行贴在正确选项后面；多选就是每个正确项各一个标记。
  3. xawl   —— 高校网页：答案写在题干括号里「自( B )起施行」，选项一行一个（页面有两份拷贝，按题号去重）。
  4. ankang —— 政府网页：纯问答（题干 + 「答：…」），出背题卡而不是题库。

用法：
    python3 parse_pub_banks.py            # 解析全部来源并写文件（末尾打印质量自查）
    python3 parse_pub_banks.py check      # 只打印每个来源出多少题，不写文件
    python3 parse_pub_banks.py selftest   # 用内置样例断言各解析器
"""
import collections
import html
import json
import os
import re
import sys
import zipfile

BASE = "/Users/hgxszhj/x_videos-dowloads/exam-lookup"
PUB = os.path.join(BASE, "crawl/pub-lib")
BANKS = os.path.join(BASE, "banks")
CRAWL = os.path.join(BASE, "crawl")

SP = r"[ \t\u3000]+"
MARKSTR = r"[（(]\s*正确答案\s*[)）]"          # 全角/半角括号都要认
ANS_COLON = re.compile(r"正确答案\s*[:：]\s*([A-D]{1,4})")


def clean(s):
    s = html.unescape(s)
    s = re.sub(r"[\u00a0\u3000]", " ", s)
    s = re.sub(SP, " ", s)
    return s.strip()


def html_text(path):
    """网页 → 按标签断行的纯文本。

    必须「每个标签断一行」：这些页面把题干、每个选项分别放在各自的标签里，
    只认 <br>/<p> 会把选项粘成一坨，选项就拆不出来了。
    """
    s = open(path, encoding="utf-8", errors="ignore").read()
    s = re.sub(r"<(script|style)[^>]*>.*?</\1>", " ", s, flags=re.S | re.I)
    s = html.unescape(re.sub(r"<[^>]+>", "\n", s))
    s = re.sub(r"[\u00a0\u3000]", " ", s)
    s = re.sub(r"[ \t]+", " ", s)
    return "\n".join(ln.strip() for ln in s.split("\n"))


def docx_text(path):
    """docx → 段落文本（只认 word/document.xml，够用且零依赖）"""
    with zipfile.ZipFile(path) as z:
        x = z.read("word/document.xml").decode("utf-8", "ignore")
    x = re.sub(r"</w:p>", "\n", x)
    x = re.sub(r"<w:tab[^>]*/>", " ", x)
    return "\n".join(ln.strip() for ln in html.unescape(re.sub(r"<[^>]+>", "", x)).split("\n"))


OPT_SPLIT = re.compile(r"(?<![\w\u4e00-\u9fa5])([A-D])\s*[、.．,，:：]\s*")


def split_options(s):
    """把「A、xx B、yy C、zz」拆成 [('A','xx'), ...]；只认 ≥2 个选项且字母不重复的串。
    拆不出来时返回 (None, 原串)，调用方必须判类型。"""
    hits = list(OPT_SPLIT.finditer(s))
    if len(hits) < 2:
        return None, s
    stem = s[: hits[0].start()].strip()
    opts = []
    for i, h in enumerate(hits):
        end = hits[i + 1].start() if i + 1 < len(hits) else len(s)
        txt = clean(s[h.end():end])
        if not txt:
            return None, s
        opts.append({"key": h.group(1), "text": txt})
    keys = [o["key"] for o in opts]
    if len(set(keys)) != len(keys):
        return None, s
    return stem, opts


def mk(qtype, stem, opts, keys, src, topic, note):
    texts = {o["key"]: o["text"] for o in opts}
    return {
        "type": qtype,
        "question": clean(stem),
        "options": opts,
        "answerKeys": list(keys),
        "answerText": [texts.get(k, "") for k in keys],
        "page": None,
        "id": "PUB-0000",
        "number": 0,
        "score": 1,
        "srcBank": src,
        "analysis": "来源：%s。答案按原卷自带；只做选项拆分与题型归一，不改答案。" % note["srcNote"],
        "topic": topic,
    }


def finalize(items, note):
    """去重（同一题干只留第一道）+ 只留答案落在选项里的题，顺手报丢弃数。"""
    seen, out, dropped = set(), [], 0
    for q in items:
        stem = re.sub(r"\s+", "", q["question"])
        keys = {o["key"] for o in q["options"]}
        if not q["answerKeys"] or not set(q["answerKeys"]) <= keys or stem in seen:
            dropped += 1
            continue
        seen.add(stem)
        q["number"] = len(out) + 1
        q["id"] = "%s-%04d" % (note.get("prefix", "PUB"), q["number"])
        out.append(q)
    if dropped:
        print("   [%s] 去重/答案对不上选项，丢掉 %d 题（宁缺勿错）" % (note.get("key", "?"), dropped))
    return out


# ─────────────────── 1. 汕尾市政府 docx（一题一段，同行选项） ───────────────────
def parse_shanwei(text, note):
    """答案标记两种写法，都很坑：
      a) 贴在正确选项文本后面：「……的组成部分（正确答案） C.错项 D.错项」
         —— 标记后面的字母是**错误**选项，绝不能当答案。
      b) 标记后面把答案选项重述一遍：「（正确答案）D.以上都是」
    所以先判断是哪种，据此把标记（b 连重述的选项一起）抠掉，用剩下的干净文本拆选项；
    再回到原文定答案 —— (a) 看哪个选项文本紧跟标记，(b) 看标记后面紧跟哪个字母。
    """
    out = []
    for para in text.split("\n"):
        para = clean(para)
        if len(para) < 12 or "正确答案" not in para:
            continue
        m = ANS_COLON.search(para)
        if m:
            stem, opts = split_options(para[: m.start()])
            keys = list(m.group(1))
        else:
            # 先按主写法 (a) 来：选项文本紧跟标记 → 这一项就是答案
            stem, opts = split_options(clean(re.sub(MARKSTR, " ", para)))
            keys = []
            if isinstance(opts, list):
                for o in opts:
                    if re.search(re.escape(o["text"]) + r"\s*" + MARKSTR, para):
                        keys.append(o["key"])
            if not keys:
                # (a) 定不了才判 (b)：标记后面把答案选项重述了一遍（重述会让选项字母重复，得先把重述段抠掉）
                body2 = clean(re.sub(MARKSTR + r"(?:\s*[A-D]\s*[.、．]\s*[^A-D]{1,60})*", " ", para))
                stem2, opts2 = split_options(body2)
                if isinstance(opts2, list) and opts2:
                    stem, opts = (stem2 or stem), opts2
                    for mm in re.finditer(MARKSTR + r"\s*([A-D])\s*[.、．]\s*([^A-D]{0,60})", para):
                        k, txt = mm.group(1), clean(mm.group(2))
                        for o in opts:
                            if o["key"] == k and (txt.startswith(o["text"][:10]) or o["text"].startswith(txt[:10])):
                                keys.append(k)
        if not isinstance(opts, list) or not opts or not stem or len(stem) < 6:
            continue
        keys = [k for k in dict.fromkeys(keys) if k in {o["key"] for o in opts}]
        if not keys:
            continue
        out.append(mk("multiple" if len(keys) > 1 else "single", stem, opts, keys,
                      note["bank"], note["topic"], note))
    return finalize(out, note)


# ───────── 2. 山东大学（威海）网页：一行一个选项 +「(正确答案)」单独一行 ─────────
QHEAD = re.compile(r"^\s*(\d{1,3})\s*[.、．]\s*(.+)$")
OPTLINE = re.compile(r"^\s*([A-D])\s*[、.．]\s*(.+)$")
MARK_ONLY = re.compile(r"^\s*[（(]?\s*正确答案\s*[)）]?\s*$")


def parse_correct_mark(text, note):
    blocks, cur = [], None
    for raw in text.split("\n"):
        ln = clean(raw)
        if not ln or ln == "*":
            continue
        if MARK_ONLY.match(ln):
            if cur and cur["opts"]:
                cur["keys"].append(cur["opts"][-1]["key"])
            continue
        m = QHEAD.match(ln)
        if m and not OPTLINE.match(ln) and len(m.group(2)) > 6:
            if cur:
                blocks.append(cur)
            stem = re.sub(r"[\[（(]?\s*(单选题|多选题|判断题)\s*[\]）)]?\s*$", "", m.group(2))
            qtype = "multiple" if "多选" in ln else ("judge" if "判断" in ln else "single")
            cur = {"stem": stem, "opts": [], "keys": [], "type": qtype}
            continue
        m = OPTLINE.match(ln)
        if m and cur is not None and not cur.get("closed"):
            if m.group(1) in {o["key"] for o in cur["opts"]}:
                cur["closed"] = True             # 字母重复 = 这题选项列完了
                continue
            cur["opts"].append({"key": m.group(1), "text": clean(m.group(2))})
            continue
    if cur:
        blocks.append(cur)

    out = []
    for c in blocks:
        keys = [k for k in dict.fromkeys(c["keys"]) if k in {o["key"] for o in c["opts"]}]
        if len(c["opts"]) < 2 or not keys or len(c["stem"]) < 6:
            continue
        if len(keys) == len(c["opts"]) and c["type"] != "multiple":
            # 选项全被标记、题干又没写「多选」：分不清是真多选还是页面把标记渲染乱了 → 丢
            continue
        out.append(mk("multiple" if len(keys) > 1 else c["type"], c["stem"], c["opts"], keys,
                      note["bank"], note["topic"], note))
    return finalize(out, note)


# ───────── 3. 西安文理学院网页：答案在题干括号里（页面有两份拷贝） ─────────
STEMLINE = re.compile(r"^\s*(\d{1,3})\s*[.、．]\s*(.+)$")
PAREN_ANS = re.compile(r"[（(]\s*([A-D]{1,4})\s*[)）]")


def parse_paren(text, note):
    merged = []                                  # 「8\n.保密技术…」这种被折开的题号先并回来
    for ln in (clean(x) for x in text.split("\n")):
        if merged and re.fullmatch(r"\d{1,3}", merged[-1]) and re.match(r"^[.、．]", ln):
            merged[-1] = merged[-1] + ln
        else:
            merged.append(ln)

    blocks, cur = [], None
    for ln in merged:
        if not ln:
            continue
        m = STEMLINE.match(ln)
        ans = PAREN_ANS.search(ln)
        if m and ans and len(m.group(2)) > 6:
            if cur:
                blocks.append(cur)
            cur = {"stem": PAREN_ANS.sub("（ ）", m.group(2)), "opts": [], "keys": list(ans.group(1))}
            continue
        m = re.match(r"^\s*([A-D])\s*[.、．]\s*(.+)$", ln)
        if m and cur is not None and not cur.get("closed"):
            if m.group(1) in {o["key"] for o in cur["opts"]}:
                cur["closed"] = True             # 选项字母重复 = 一轮选项结束，后面的不属于这题
                continue
            cur["opts"].append({"key": m.group(1), "text": clean(m.group(2))})
    if cur:
        blocks.append(cur)

    out = []
    for c in blocks:
        keys = [k for k in dict.fromkeys(c["keys"]) if k in {o["key"] for o in c["opts"]}]
        if len(c["opts"]) < 2 or not keys:
            continue
        out.append(mk("multiple" if len(keys) > 1 else "single", c["stem"], c["opts"], keys,
                      note["bank"], note["topic"], note))
    return finalize(out, note)


# ─────────────── 4. 安康市档案局网页：问答 → 背题卡 ───────────────
def parse_qa(text, note):
    chunks = re.split(r"(?m)^\s*(\d{1,3})\s*[.、．]\s*", text)
    items = []
    for i in range(1, len(chunks) - 1, 2):
        parts = re.split(r"答\s*[:：]", clean(chunks[i + 1]), maxsplit=1)
        if len(parts) != 2:
            continue
        q, a = clean(parts[0]), clean(parts[1])
        if len(q) < 6 or len(a) < 12:
            continue
        items.append({"chapter": note.get("chapter", "问答"), "num": len(items) + 1, "page": None,
                      "q": q, "a": a, "_book": note["title"], "_src": note["srcNote"], "_tested": ""})
    return items


# ───────────────────────────── 来源清单 ─────────────────────────────
SRC_NOTE_WANGAN = "汕尾市人民政府官网公开附件《网络安全知识题库》（shanwei.gov.cn）"
SRC_NOTE_SDUWH = "山东大学（威海）国家安全人民防线宣传教育网公开的《国家安全知识题目》（psb.wh.sdu.edu.cn）"
SRC_NOTE_XAWL = "西安文理学院保卫处官网公开的《保密知识测试题及答案》（xawl.edu.cn）"
SRC_NOTE_ANKANG = "安康市档案局官网公开的《保密知识问答25题》（saac.ankang.gov.cn）"

SOURCES = [
    {"key": "网络安全-汕尾", "file": "shanwei-网络安全题库.docx", "parser": parse_shanwei, "kind": "bank",
     "bank": "公开题-网络安全知识-汕尾市政府", "title": "网络安全知识题库（汕尾市政府公开附件 · 带答案）",
     "topic": "网络安全公开题", "prefix": "PUB-WA", "srcNote": SRC_NOTE_WANGAN},
    {"key": "国家安全-山大威海", "file": "sduwh-国家安全知识.htm", "parser": parse_correct_mark, "kind": "bank",
     "bank": "公开题-国家安全知识-山东大学威海", "title": "国家安全知识题目（山东大学威海 · 带正确答案标记）",
     "topic": "国家安全公开题", "prefix": "PUB-GA", "srcNote": SRC_NOTE_SDUWH},
    {"key": "保密-西安文理", "file": "xawl-保密知识测试题.html", "parser": parse_paren, "kind": "bank",
     "bank": "公开题-保密知识-西安文理学院", "title": "保密知识测试题及答案（西安文理学院官网）",
     "topic": "保密公开题", "prefix": "PUB-XA", "srcNote": SRC_NOTE_XAWL},
    {"key": "保密问答-安康", "file": "ankang-保密知识问答25题.htm", "parser": parse_qa, "kind": "cards",
     "bank": "公开问答-安康保密25题", "title": "保密知识问答 25 题（安康市档案局公开）",
     "topic": "保密公开问答", "prefix": "PUB-AK", "srcNote": SRC_NOTE_ANKANG, "chapter": "保密知识问答"},
]


def load_text(src):
    p = os.path.join(PUB, src["file"])
    if not os.path.exists(p):
        return None
    return docx_text(p) if p.endswith(".docx") else html_text(p)


def run(write=True):
    report = []
    for src in SOURCES:
        t = load_text(src)
        if t is None:
            report.append((src["key"], None, "缺源文件 " + src["file"]))
            continue
        items = src["parser"](t, src)
        report.append((src["key"], items, ""))
        if not write or not items:
            continue
        if src["kind"] == "bank":
            os.makedirs(BANKS, exist_ok=True)
            path = os.path.join(BANKS, src["bank"] + ".json")
            doc = {"meta": {"title": src["title"], "external": True, "public": True,
                            "sourceNote": "来源：%s。答案按原卷自带（只做选项拆分与题型归一，不判不改）。" % src["srcNote"]},
                   "questions": items}
        else:
            path = os.path.join(CRAWL, src["bank"] + "-问答.json")
            # 标题不带条数：背题页的下拉自己会接「（N 条）」，带了就重复成「（25 条）（25 条）」
            doc = {"meta": {"title": src["title"],
                            "sourceNote": "来源：%s。问面是原题，答面是原文答案。" % src["srcNote"]},
                   "questions": items}
        json.dump(doc, open(path, "w", encoding="utf-8"), ensure_ascii=False, indent=1)
        print("   写出 %s（%d 条）" % (os.path.basename(path), len(items)))
    return report


def check(report):
    ok = True
    for key, items, err in report:
        if items is None:
            print("   %-16s !! %s" % (key, err))
            ok = False
            continue
        c = collections.Counter(x.get("type", "card") for x in items)
        has_opts = bool(items) and "options" in items[0]
        bad = sum(1 for q in items if not set(q["answerKeys"]) <= {o["key"] for o in q["options"]}) if has_opts else 0
        bad += sum(1 for q in items if has_opts and (len(q["answerKeys"]) > 1) != (q["type"] == "multiple"))
        dup = len(items) - len({re.sub(r"\s+", "", q.get("question", q.get("q", ""))) for q in items})
        print("   %-16s %4d 条 %-34s 答案越界/题型不符 %d ｜ 重题 %d" % (key, len(items), dict(c), bad, dup))
        if bad or dup:
            ok = False
    return ok


# ───────────────────────────── selftest ─────────────────────────────
FIX_SHANWEI = """一、单选题
防火墙一般都具有网络地址转换功能,NAT允许多台计算机使用一个（）连接网络: A、Web浏览器 B、IP地址 C、代理服务器 D、服务器名 正确答案：B
网络运营者应当（） A、制定内部安全管理制度 B、确定负责人 C、采取技术措施 D、以上都是 （正确答案）D.以上都是
《网络安全法》在（）中提出建立网络安全标准体系。 A.网络安全支持与促进(正确答案) B.网络运行安全 C.网络信息安全 D.监测预警与应急处置
"""
FIX_MARK = """1.党的十八大以来，()起正式施行。 [单选题]
*
A、2015年7月1日
(正确答案)
B、2000年1月1日
C、1993年2月22日
51.大学生学习国家安全知识的重要意义在于()。
A、履行社会公民职责的需要
(正确答案)
B、此时正是国家安全意识重要形成时期
(正确答案)
C、与在校学习无关
D、只与成年人有关
"""
FIX_PAREN = """一、单项选择题
1.新修订保密法自( B )起施行。
A.2010年4月29日
B.2010年10月1日
C.2010年10月7日
8
.保密技术装备配备方式,包括( AC ) 。
A.强制配备
B.自主配备
C.指导配备
D.统一配备
1.新修订保密法自( B )起施行。
A.2010年4月29日
B.2010年10月1日
C.2010年10月7日
"""
FIX_QA = """1.什么是国家秘密？
答：国家秘密是关系国家安全和利益，依照法定程序确定，在一定时间内只限一定范围的人员知悉的事项。
2.为什么划分密级？
答：为了使属于保密范围的事项成为国家法规认可和保护的国家秘密，便于采取宽严程度不同的保密措施。
"""


def selftest():
    note = {"bank": "x", "topic": "t", "prefix": "T", "srcNote": "测试来源", "title": "测试"}
    sw = parse_shanwei(FIX_SHANWEI, note)
    assert len(sw) == 3, "shanwei 应出 3 题，实际 %d" % len(sw)
    assert sw[0]["answerKeys"] == ["B"] and sw[0]["type"] == "single", sw[0]["answerKeys"]
    assert sw[1]["answerKeys"] == ["D"], sw[1]["answerKeys"]        # 重述式：字母跟在标记后面
    assert sw[2]["answerKeys"] == ["A"], sw[2]["answerKeys"]         # 标记贴选项后面，后面 B/C/D 是错项
    assert sw[0]["options"][1]["text"] == "IP地址", sw[0]["options"]

    mk_ = parse_correct_mark(FIX_MARK, note)
    assert len(mk_) == 2, "correct_mark 应出 2 题，实际 %d" % len(mk_)
    assert mk_[0]["answerKeys"] == ["A"] and mk_[0]["type"] == "single", mk_[0]["answerKeys"]
    assert mk_[1]["type"] == "multiple" and mk_[1]["answerKeys"] == ["A", "B"], mk_[1]["answerKeys"]
    assert "[单选题]" not in mk_[0]["question"], mk_[0]["question"]

    px = parse_paren(FIX_PAREN, note)
    assert len(px) == 2, "paren 应出 2 题（页面第二份拷贝要去重），实际 %d" % len(px)
    assert px[0]["answerKeys"] == ["B"] and px[0]["type"] == "single", px[0]["answerKeys"]
    assert px[1]["type"] == "multiple" and px[1]["answerKeys"] == ["A", "C"], px[1]["answerKeys"]
    assert "（ ）" in px[0]["question"], px[0]["question"]

    qa = parse_qa(FIX_QA, note)
    assert len(qa) == 2, "qa 应出 2 条，实际 %d" % len(qa)
    assert qa[0]["q"].endswith("？") and "国家秘密是关系" in qa[0]["a"], qa[0]
    print("selftest OK：shanwei 3 题 / correct_mark 2 题 / paren 2 题（含去重）/ qa 2 条 都过")


if __name__ == "__main__":
    mode = sys.argv[1] if len(sys.argv) > 1 else "run"
    if mode == "selftest":
        selftest()
    elif mode == "check":
        print("── 各来源出题量 ──")
        sys.exit(0 if check(run(write=False)) else 1)
    else:
        print("── 解析公开卷并写出 ──")
        rep = run(write=True)
        print("── 质量自查 ──")
        sys.exit(0 if check(rep) else 1)
