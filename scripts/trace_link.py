#!/usr/bin/env python3
"""
trace_link.py — 自动注入 Ref-trace 字段到 commit message

动机: trace instrumentation v1 (commit baeea9c) 落地后,BUG 复发需要按 session_id 关联 trace。
     手工写 "Ref-trace: <session_id>.ndjson" 易忘 → 自动注入。

机制:
- 读 commit message 文件(从 sys.argv[1])
- 扫描 ~/.local/share/x_videos-dowloads-trace/YYYY-MM-DD/ 找今天最新的 .ndjson
- 若找到:在 commit message 末尾 append 一行 `Ref-trace: <绝对路径>/<file>.ndjson`
- 若没找到:no-op,不破坏现有逻辑
- 若 commit message 已含 `Ref-trace:` 行:no-op(避免重复注入)

参考: docs/dna/trace-schema.md | MEMORY.md:local-ndjson-trace-instrumentation.md
"""

from __future__ import annotations

import datetime as _dt
import os
import re
import sys
from pathlib import Path

REF_TRACE_RE = re.compile(r"^Ref-trace:\s*.+$", re.MULTILINE)
DEFAULT_TRACE_ROOT = Path.home() / ".local" / "share" / "x_videos-dowloads-trace"


def find_today_newest_trace(trace_root: Path | None = None) -> Path | None:
    """找今天日期目录下最新的 .ndjson 文件(按 mtime)"""
    root = trace_root or Path(os.environ.get("X_VIDEOS_TRACE_ROOT", DEFAULT_TRACE_ROOT))
    if not root.exists():
        return None
    today_dir = root / _dt.datetime.now().strftime("%Y-%m-%d")
    if not today_dir.exists():
        return None
    ndjsons = sorted(today_dir.glob("*.ndjson"), key=lambda p: p.stat().st_mtime)
    return ndjsons[-1] if ndjsons else None


def inject_ref_trace(commit_msg: str, trace_path: Path | None = None) -> tuple[str, bool]:
    """返回 (new_msg, injected)
    - injected=True 表示本函数实际写入了 Ref-trace 行
    - injected=False 表示已存在 / 无 trace / 无 op
    """
    # 已存在 → no-op
    if REF_TRACE_RE.search(commit_msg):
        return commit_msg, False

    # 找 trace
    path = trace_path or find_today_newest_trace()
    if path is None:
        return commit_msg, False

    # 注入(在末尾追加一行,前面补空行保持 commit message 美观)
    suffix = commit_msg.endswith("\n")
    new_msg = (
        f"{commit_msg}\n\nRef-trace: {path}" if not suffix else f"{commit_msg}\nRef-trace: {path}"
    )
    return new_msg, True


def main() -> int:
    msg_file = sys.argv[1] if len(sys.argv) > 1 else None
    if not msg_file:
        return 0
    msg_path = Path(msg_file)
    if not msg_path.exists():
        return 0

    original = msg_path.read_text(encoding="utf-8")
    new_msg, injected = inject_ref_trace(original)

    if injected:
        msg_path.write_text(new_msg, encoding="utf-8")
        print(f"[trace_link] 注入 Ref-trace: {find_today_newest_trace()}", file=sys.stderr)
    # else: 静默 no-op

    return 0


if __name__ == "__main__":
    sys.exit(main())
