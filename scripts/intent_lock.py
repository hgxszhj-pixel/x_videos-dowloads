#!/usr/bin/env python3
"""Intent Lock — 校验 commit message 含一句话意图声明

机制（参考 spec-superflow 的 Intent Lock 概念，本地最小化实现）:
- commit message 必须包含一行 `Intent: <一句话>`
- Intent 长度 10-200 字符，太短挡掉 vague commit，太长挡掉段落
- Conventional Commits 前缀（feat/fix/refactor/test/perf/docs/chore）保持不变

用法:
    from intent_lock import validate_intent
    ok, msg = validate_intent(commit_message_text)
    if not ok:
        print(msg, file=sys.stderr)
        sys.exit(1)
"""
from __future__ import annotations

import re
import sys

INTENT_RE = re.compile(r"^Intent:\s*(.+)$", re.MULTILINE)
MIN_LEN = 10
MAX_LEN = 200


def extract_intent(msg: str) -> str | None:
    """从 commit message 中提取 Intent 行，没有返回 None。"""
    m = INTENT_RE.search(msg)
    return m.group(1).strip() if m else None


def validate_intent(msg: str) -> tuple[bool, str]:
    """校验 commit message 是否含有效 Intent。

    Returns:
        (ok, message): ok=True 通过；ok=False 拒绝并给出原因。
    """
    intent = extract_intent(msg)
    if not intent:
        return False, (
            "❌ Missing Intent Lock.\n"
            "   Add a line to your commit message:\n"
            "\n"
            "       Intent: <一句话锁定本次变更意图>\n"
            "\n"
            "   例:\n"
            "       Intent: 移除 adaptive sampler 死代码，不改变下载 API"
        )
    if len(intent) < MIN_LEN:
        return False, (
            f"❌ Intent too vague ({len(intent)} chars): '{intent}'\n"
            f"   必须 ≥{MIN_LEN} 字符。写清楚: 要做什么、不做什么。"
        )
    if len(intent) > MAX_LEN:
        return False, (
            f"❌ Intent too long ({len(intent)} chars).\n"
            f"   写一句话（≤{MAX_LEN} 字符），不要写成段落。"
        )
    return True, f"✓ Intent: {intent}"


def main() -> int:
    """CLI 入口: 从 stdin 读 commit message，校验后退出。"""
    msg = sys.stdin.read()
    ok, out = validate_intent(msg)
    print(out, file=sys.stderr)
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())