#!/usr/bin/env python3
"""open_loops schema validator for HANDOFF.md.

规则 (与 open-loops-skillstate-style-2026-10-07 配套的机器执行):
  - Active 段: 每条 slot 必须显式 4 字段 (owner / blocker / next / touched)
  - Blocked / Waiting 段: 每条 slot 必须显式 3 字段 (waiting-on / eta / touched)
  - Just Closed 段: 每条 slot 必须含日期 YYYY-MM-DD
  - 字段缺失 → 拒绝 commit (open_loops 是 working memory,字段缺失 = state corruption)

设计 (仿 handoff_size_guard.py):
- 检查 staged version (`git show :HANDOFF.md`),不是 working tree
- 只在 HANDOFF.md 是当前 commit 的一部分时才检查 (避免无谓触发)
- 与 [[open-loops-skillstate-style]] 协同: 人工 schema 习惯 + 机器拦截兜底

Why: 2026-10-07 SKILL.state 论文启发 — 显式 state slot 比 free-text 更利于推理保留,
字段缺失会让 owner/blocker/touched 漂移成不可校验的 free-form,失去 schema 优势。
小模型 (Gemma-4-31B T=100) 68% 错误是 "Premature State Overwrite / Deletion" —
显式 schema 校验拦截同类问题。

How to apply (Hook 视角):
- commit-msg hook 在 handoff_size_guard 之后调用
- 返回 (ok, msg); ok=False 时 commit 被拒 (exit 1)
- 错误通过 stderr 输出,不影响 commit 通过

阈值调优记录:
- 当前: Active 必填 4 字段 / Blocked 必填 3 字段 / Just Closed 必填日期
- 与 SKILL.state 论文 5.7 节 "Schema Comprehension / Type Coercion" 错误对齐:
  - 不强制类型 (type coercion 留给人工)
  - 强制 key 存在 + 非空 (防止 overwrite/deletion)
"""
from __future__ import annotations

import re
import subprocess
import sys
from datetime import date

HANDOFF_FILENAME = "HANDOFF.md"

# 段标题 (精确匹配)
SECTION_ACTIVE = "## Open Loops — Active"
SECTION_BLOCKED = "## Open Loops — Blocked / Waiting"
SECTION_CLOSED = "## Just Closed"

# Active slot 必填字段 (4 个)
ACTIVE_REQUIRED = ["owner", "blocker", "next", "touched"]
# Blocked slot 必填字段 (3 个)
BLOCKED_REQUIRED = ["waiting-on", "eta", "touched"]
# Just Closed 必填日期 YYYY-MM-DD
CLOSED_DATE_RE = re.compile(r"\((\d{4}-\d{2}-\d{2})\)")

# touched 日期不能超过 N 天 (软警告 — 不阻塞)
STALENESS_WARN_DAYS = 14


def get_staged_handoff_text() -> str | None:
    """Return staged HANDOFF.md content, or None if not staged / not in repo."""
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

    return result.stdout


def parse_slots(section_body: str) -> list[tuple[str, str]]:
    """Extract slot entries from a section body.

    Returns list of (title, full_text) tuples.
    Slot = "- **<title>**" line until next "- **" or end of section.
    """
    slots = []
    current_title = None
    current_lines: list[str] = []

    for line in section_body.splitlines():
        stripped = line.strip()
        if stripped.startswith("- **") and "**" in stripped[4:]:
            if current_title is not None:
                slots.append((current_title, "\n".join(current_lines)))
            # extract title between ** **
            m = re.match(r"-\s+\*\*(.+?)\*\*", stripped)
            current_title = m.group(1) if m else stripped
            current_lines = [stripped]
        elif current_title is not None:
            current_lines.append(stripped)

    if current_title is not None:
        slots.append((current_title, "\n".join(current_lines)))

    return slots


def extract_field(text: str, field: str) -> str | None:
    """Extract field value from slot text. Field separator is " · " or "—".

    Examples:
        "owner: 用户 · blocker: ... · next: ... · touched: 2026-10-04"
        → owner = "用户"
    """
    # 匹配 "field: <value>" 直到下一个 " · " 或行尾
    pattern = rf"{re.escape(field)}\s*[::]\s*([^·\n]+)"
    m = re.search(pattern, text)
    if m:
        return m.group(1).strip()
    return None


def check_active_section(section_body: str) -> list[str]:
    """Validate Active section. Return list of error messages (empty = OK)."""
    errors = []
    slots = parse_slots(section_body)
    if not slots:
        errors.append(f"  - [Active] section is empty (no slots found)")
        return errors

    for title, text in slots:
        missing = [f for f in ACTIVE_REQUIRED if not extract_field(text, f)]
        if missing:
            errors.append(
                f"  - [Active] '{title}': missing fields {missing} "
                f"(required: {ACTIVE_REQUIRED})"
            )

    return errors


def check_blocked_section(section_body: str) -> list[str]:
    """Validate Blocked section. Return list of error messages (empty = OK)."""
    errors = []
    slots = parse_slots(section_body)
    if not slots:
        return errors  # empty is OK (no blocked items)

    for title, text in slots:
        missing = [f for f in BLOCKED_REQUIRED if not extract_field(text, f)]
        if missing:
            errors.append(
                f"  - [Blocked] '{title}': missing fields {missing} "
                f"(required: {BLOCKED_REQUIRED})"
            )

    return errors


def check_closed_section(section_body: str) -> list[str]:
    """Validate Just Closed section. Return list of error messages (empty = OK)."""
    errors = []
    slots = parse_slots(section_body)
    if not slots:
        return errors  # empty is OK

    for title, text in slots:
        if not CLOSED_DATE_RE.search(text):
            errors.append(
                f"  - [Just Closed] '{title}': missing date in '(YYYY-MM-DD)' format"
            )

    return errors


def extract_section_body(text: str, section_header: str) -> str:
    """Extract body between section_header and the next ## header (any) or EOF.

    终止条件: 下一个 `\\n## ` 起始(任何 ## 一级标题),确保 3 段 (Active/Blocked/Closed)
    之间不互相吞并。
    """
    start = text.find(section_header)
    if start < 0:
        return ""
    start += len(section_header)

    # find next ## header (any) after start — use regex for exact newline+## boundary
    m = re.search(r"\n## ", text[start:])
    if m:
        end = start + m.start()
    else:
        end = len(text)
    return text[start:end]


def check_openloops_schema() -> tuple[bool, str]:
    """Check staged HANDOFF.md open_loops schema. Return (ok, message).

    ok=False → commit should be rejected.
    ok=True + msg != "" → warning emitted but commit passes.
    ok=True + msg == "" → no issue, silent pass.
    """
    content = get_staged_handoff_text()
    if content is None:
        return True, ""

    # If file has no Open Loops section at all (e.g. not yet migrated), skip
    if "Open Loops" not in content and "Just Closed" not in content:
        return True, ""

    # Extract sections (note: also detect legacy "待办 / Open Items")
    if SECTION_ACTIVE not in content:
        # Legacy flat format — don't block, just warn
        if "待办 / Open Items" in content or "## Open Items" in content:
            return True, (
                "\n⚠️  HANDOFF.md still uses legacy flat 'Open Items' format. "
                "Migrate to SKILL.state 3-section style "
                "(Active / Blocked / Just Closed) per "
                "[[open-loops-skillstate-style-2026-10-07]].\n"
            )
        return True, ""  # no open_loops yet

    # section boundaries — each section ends at the next ## header (any)
    active_body = extract_section_body(content, SECTION_ACTIVE)
    blocked_body = extract_section_body(content, SECTION_BLOCKED)
    closed_body = extract_section_body(content, SECTION_CLOSED)

    errors = []
    errors.extend(check_active_section(active_body))
    errors.extend(check_blocked_section(blocked_body))
    errors.extend(check_closed_section(closed_body))

    if errors:
        msg = (
            f"\n❌ HANDOFF.md open_loops schema violations ({len(errors)}):\n\n"
            + "\n".join(errors)
            + "\n\nFix per [[open-loops-skillstate-style-2026-10-07]]: "
            "each slot must have ALL required fields explicitly. "
            "Active: owner/blocker/next/touched. "
            "Blocked: waiting-on/eta/touched. "
            "Just Closed: (YYYY-MM-DD) date in title.\n"
        )
        return False, msg

    # Soft warning: staleness check (≥ STALENESS_WARN_DAYS)
    today = date.today()
    stale = []
    for body in (active_body, blocked_body):
        for title, text in parse_slots(body):
            touched_str = extract_field(text, "touched")
            if touched_str:
                m = re.search(r"(\d{4}-\d{2}-\d{2})", touched_str)
                if m:
                    try:
                        touched_date = date.fromisoformat(m.group(1))
                        days = (today - touched_date).days
                        if days >= STALENESS_WARN_DAYS:
                            stale.append(f"{title} ({days}d)")
                    except ValueError:
                        pass

    if stale:
        return True, (
            f"\n⚠️  HANDOFF.md open_loops staleness warning "
            f"(touched > {STALENESS_WARN_DAYS}d): {stale}. "
            f"Consider close, downgrade, or touch.\n"
        )

    return True, ""


if __name__ == "__main__":
    ok, msg = check_openloops_schema()
    if msg:
        print(msg, file=sys.stderr)
    if not ok:
        sys.exit(1)
