#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""llm_claude —— 把 Claude Code 当成「会读原文的答题器」调用（headless）

只做一件事：给一段证据 + 一句严格指令，拿回它的回答文本。
不判定、不落库——落不落库由 answer_chain / 收编页决定。

    from llm_claude import ask_claude
    out = ask_claude("证据包…")        # 返回 (text, err)；err 非空表示没跑成

为什么用 claude 而不是调 API：本题库常在「题库没有、搜索引擎也没摘要」时卡住，
而 Claude Code 能在本机读 docs_new_text 原件（宣贯要点/教材/法规）自己找依据。
代价是慢（30–90s）且走订阅额度，所以只在人工单题场景用。
"""
import os
import re
import shutil
import subprocess

BIN = os.environ.get("CLAUDE_BIN") or shutil.which("claude") or os.path.expanduser("~/.local/bin/claude")
TIMEOUT = int(os.environ.get("CLAUDE_TIMEOUT", "150"))
MODEL = os.environ.get("CLAUDE_MODEL", "")          # 留空用 claude 默认

PROMPT = """你是考试答题器。只依据下面这份「证据包」回答，不要使用你自己的常识补充。

要求：
1. 单选题 → 只给一个字母；多选题 → 字母连写（如 ABD）；排序题/填空题 → 按空位顺序给字母串（如 CBDAFE）。
2. 必须引用证据里的原句（哪份文件/哪个章节）。
3. 证据不足就只输出：证据不足|<缺什么材料>，不要猜。

输出格式（必须严格三行，不要别的）：
答案：<字母或 证据不足>
依据：<原句或缺失说明>
可信度：高|中|低

=== 证据包开始 ===
{ctx}
=== 证据包结束 ==="""


def ask_claude(ctx, timeout=None):
    """返回 (text, err)。err 非空 = 没跑成（未安装/超时/退出码非 0）。"""
    if not BIN or not os.path.exists(BIN):
        return "", "claude CLI 未找到（brew install claude-code 或设置 CLAUDE_BIN）"
    cmd = [BIN, "-p", PROMPT.replace("{ctx}", ctx[:12000])]
    if MODEL:
        cmd += ["--model", MODEL]
    try:
        r = subprocess.run(cmd, capture_output=True, text=True,
                           timeout=timeout or TIMEOUT, cwd=os.path.dirname(os.path.abspath(__file__)))
    except subprocess.TimeoutExpired:
        return "", "claude 超时（%ss）" % (timeout or TIMEOUT)
    except OSError as e:
        return "", "claude 启动失败：%s" % e
    out = (r.stdout or "").strip()
    if r.returncode != 0 and not out:
        return "", "claude 退出码 %s：%s" % (r.returncode, (r.stderr or "")[:200])
    return out, ""


ANSWER_RE = re.compile(r"答案[：:]\s*([A-Fa-f]{1,8}|证据不足)")


def parse(text):
    """从三行输出里抠出 (答案, 依据, 可信度)。抠不到返回 (None, '', '')。"""
    if not text:
        return None, "", ""
    m = ANSWER_RE.search(text)
    ans = m.group(1) if m else None
    if ans and ans != "证据不足":
        ans = ans.upper()
    ev = ""
    me = re.search(r"依据[：:]\s*(.+)", text)
    if me:
        ev = me.group(1).strip()[:300]
    conf = ""
    mc = re.search(r"可信度[：:]\s*(高|中|低)", text)
    if mc:
        conf = mc.group(1)
    return (None if ans == "证据不足" else ans), ev, conf


def selftest():
    cases = [
        ("答案：C\n依据：宣贯要点（三）科技创新是核心\n可信度：高", "C"),
        ("答案：CBDAFE\n依据：六条小标题\n可信度：高", "CBDAFE"),
        ("答案：abd\n依据：条例第十二条\n可信度：中", "ABD"),
        ("答案：证据不足\n依据：缺《涉密人员保密管理办法》\n可信度：低", None),
        ("我看不出来", None),
    ]
    for text, want in cases:
        got = parse(text)[0]
        assert got == want, "解析不符：%r → %r != %r" % (text, got, want)
    # 未安装时也必须返回 (\"\", err) 而不是抛异常
    t, err = ask_claude("x", timeout=1) if not shutil.which("claude") else ("", "skipped")
    assert isinstance(t, str) and isinstance(err, str)
    print("llm_claude selftest 通过：%d 组解析 + 降级返回字符串" % len(cases))
    return 0


if __name__ == "__main__":
    import sys
    sys.exit(selftest() if "--selftest" in sys.argv else 0)
