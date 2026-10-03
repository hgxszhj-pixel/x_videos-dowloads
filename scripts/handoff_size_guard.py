#!/usr/bin/env python3
"""HANDOFF.md size guard for commit-msg hook.

规则 (与 handoff-md-streamline-rule.md 配套的机器执行):
  - > 200 行 → 拒绝 commit (强制精简)
  - > 100 行 → 警告但不阻塞 (提示该精简了)

设计:
- 检查 staged version (`git show :HANDOFF.md`),不是 working tree
- 只在 HANDOFF.md 是当前 commit 的一部分时才检查 (避免无谓触发)
- 与 [[handoff-md-streamline-rule]] 协同: 人工精简时习惯 + 机器拦截兜底

Why: 2026-10-03 单文件曾膨胀至 2117 行,首次拆分至 55 行后
需结构化预防再次复发,人工惯例不够。

How to apply (Hook 视角):
- commit-msg hook 在 Intent Lock 之后调用
- 返回 (ok, msg); ok=False 时 commit 被拒 (exit 1)
- 警告通过 stderr 输出,不影响 commit 通过

阈值调优记录:
- 当前: 100 / 200 (软 / 硬)
- 软上限 = "提示但不阻塞"; 硬上限 = "强制精简"
- 真实拆分后稳态 < 80 行, 软上限留 20 行 buffer 给状态变更
"""
from __future__ import annotations

import subprocess
import sys

# 阈值: 软上限 100, 硬上限 200
SOFT_LIMIT = 100
HARD_LIMIT = 200

HANDOFF_FILENAME = "HANDOFF.md"


def get_staged_handoff_lines() -> int | None:
    """Return line count of staged HANDOFF.md, or None if not staged / not in repo.

    None 表示 "这次 commit 不动 HANDOFF.md,跳过检查"。
    """
    try:
        result = subprocess.run(
            ["git", "diff", "--cached", "--name-only"],
            capture_output=True, text=True, check=False,
        )
        staged_files = result.stdout.splitlines()
        if HANDOFF_FILENAME not in staged_files:
            return None
    except (subprocess.SubprocessError, FileNotFoundError):
        return None

    try:
        result = subprocess.run(
            ["git", "show", f":{HANDOFF_FILENAME}"],
            capture_output=True, text=True, check=True,
        )
    except (subprocess.SubprocessError, FileNotFoundError):
        return None

    return len(result.stdout.splitlines())


def check_handoff_size() -> tuple[bool, str]:
    """Check staged HANDOFF.md size. Return (ok, message).

    ok=False → commit should be rejected.
    ok=True + msg != "" → warning emitted but commit passes.
    ok=True + msg == "" → no issue, silent pass.
    """
    lines = get_staged_handoff_lines()
    if lines is None:
        return True, ""

    if lines > HARD_LIMIT:
        return False, (
            f"\n❌ HANDOFF.md too large: {lines} lines (hard limit {HARD_LIMIT})\n\n"
            "Per the handoff-md-streamline-rule, HANDOFF.md must stay focused on\n"
            "project state, not session history. Session entries belong in\n"
            f"docs/handoff/sessions/<date>-<topic>.md.\n\n"
            "Actions:\n"
            "  1. Move session entries from HANDOFF.md to docs/handoff/sessions/\n"
            "  2. Replace with one-line links in HANDOFF.md's session index\n"
            "  3. Re-stage HANDOFF.md and commit again\n"
        )

    if lines > SOFT_LIMIT:
        return True, (
            f"\n⚠️  HANDOFF.md is {lines} lines (soft limit {SOFT_LIMIT}). "
            "Consider streamlining before next commit.\n"
        )

    return True, ""


if __name__ == "__main__":
    ok, msg = check_handoff_size()
    if msg:
        print(msg, file=sys.stderr)
    if not ok:
        sys.exit(1)