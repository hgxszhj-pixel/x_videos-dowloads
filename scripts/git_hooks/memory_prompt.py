"""commit-msg memory-prompt: non-blocking soft reminder when commit signals
insight but no memory file was touched today.

设计依据 (2026-10-12): CLAUDE.md 记忆系统新增「自动化模式: agent 生成 + 人 review」
本模块是该模式的提交时 detector —— 不阻塞 commit,只在 stderr 软提示。

触发条件 (全部满足才提示):
  1. 提交 message 含学习/修复/重构类关键词 (fix/refactor/learn/发现/教训/规则/改进/...)
  2. 今日 (本地时区 0:00 至今) memory/ 目录无 .md 文件被修改
  3. 本次 commit 不是 docs: / chore: / style: / test: 纯文档类 (避免噪声)

软提示: stderr 打印,exit 0;人不响应也能完成 commit。
"""
from __future__ import annotations

import re
import sys
from datetime import datetime
from pathlib import Path

# 关键词:触发提示的提交信号 (commit subject 或 body 命中其一)
INSIGHT_SIGNALS = re.compile(
    r"\b(fix|refactor|perf|feat|发现|教训|规则|改进|沉淀|洞察|"
    r"learned|takeaway|gotcha|pitfall|invariant|constraint)\b",
    re.IGNORECASE,
)
# 纯文档/样式/测试类:跳过 (避免每次 docs:commit 都提示)
DOC_ONLY_TYPES = re.compile(r"^(docs|chore|style|test|ci|build)(\(.+\))?!?:", re.IGNORECASE)

# memory 目录: 与 MEMORY.md 平级,文件命名 <slug>-YYYY-MM-DD.md 或 .md
PROJECT_MEMORY_GLOBS = [
    # project-specific memory
    "/Users/hgxszhj/.claude/projects/-Users-hgxszhj-x-videos-dowloads/memory/*.md",
    # user-level preferences / decisions
    "/Users/hgxszhj/memory/preferences.md",
    "/Users/hgxszhj/memory/decisions.md",
]


def _memory_touched_today() -> bool:
    today = datetime.now().date()
    for pattern in PROJECT_MEMORY_GLOBS:
        for p in sorted(Path("/").glob(pattern.lstrip("/"))):
            try:
                mtime = datetime.fromtimestamp(p.stat().st_mtime).date()
                if mtime == today:
                    return True
            except (FileNotFoundError, PermissionError):
                continue
    return False


def check_memory_prompt(commit_msg: str) -> str | None:
    """Return a stderr-friendly soft prompt, or None to stay silent.

    The caller (commit-msg hook) is responsible for printing to stderr and
    never blocking the commit on this output.
    """
    # Extract subject (first non-empty line, up to first blank)
    lines = [l for l in commit_msg.splitlines() if l.strip()]
    if not lines:
        return None
    subject = lines[0]

    # Skip doc-only / chore-only types
    if DOC_ONLY_TYPES.match(subject):
        return None

    # Need an insight signal somewhere in subject or first 5 body lines
    head = subject + "\n" + "\n".join(lines[1:6])
    if not INSIGHT_SIGNALS.search(head):
        return None

    # No memory touched today → soft prompt
    if _memory_touched_today():
        return None

    return (
        "\n[memory-prompt] 本 commit 含学习/改进信号,但 memory/ 今日无更新。\n"
        "  考虑: 沉淀一条洞察到 memory/<slug>-2026-10-12.md (2-3 句即可)\n"
        "  或: 显式 noop (无新洞察) — 本提示不阻塞 commit\n"
    )


if __name__ == "__main__":
    # Manual smoke: read commit msg from stdin, print prompt or nothing
    msg = sys.stdin.read()
    out = check_memory_prompt(msg)
    if out:
        print(out, file=sys.stderr)
        sys.exit(0)
    print("[memory-prompt] no signal (skip)", file=sys.stderr)
