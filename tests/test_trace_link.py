#!/usr/bin/env python3
"""
test_trace_link.py — trace_link.py 单测

≥4 case 覆盖核心逻辑:
  1. 无 trace_dir → no-op
  2. 有 trace_dir → 注入 Ref-trace 行
  3. 已存在 Ref-trace → no-op(不重复注入)
  4. commit msg 不以 \n 结尾 → 自动补空行
"""

import os
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPTS_DIR = Path(__file__).resolve().parent.parent / "scripts"
sys.path.insert(0, str(SCRIPTS_DIR))

from trace_link import inject_ref_trace  # noqa: E402


class TestNoTraceDir(unittest.TestCase):
    """Case 1: trace_dir 不存在 → no-op"""

    def test_returns_unchanged_when_no_trace(self):
        with tempfile.TemporaryDirectory() as tmp:
            fake_root = Path(tmp) / "no-such-trace"
            env_backup = os.environ.get("X_VIDEOS_TRACE_ROOT")
            os.environ["X_VIDEOS_TRACE_ROOT"] = str(fake_root)
            try:
                msg = "fix: foo\n\nIntent: 修复 foo 行为\n"
                new, injected = inject_ref_trace(msg)
                self.assertFalse(injected)
                self.assertEqual(new, msg)
            finally:
                if env_backup is not None:
                    os.environ["X_VIDEOS_TRACE_ROOT"] = env_backup
                else:
                    os.environ.pop("X_VIDEOS_TRACE_ROOT", None)


class TestInjectRefTrace(unittest.TestCase):
    """Case 2: 有 trace_dir → 注入 Ref-trace 行"""

    def test_appends_ref_trace_when_trace_exists(self):
        with tempfile.TemporaryDirectory() as tmp:
            today = __import__("datetime").datetime.now().strftime("%Y-%m-%d")
            today_dir = Path(tmp) / today
            today_dir.mkdir()
            trace_file = today_dir / "abc123-gui-single.ndjson"
            trace_file.write_text("{}\n", encoding="utf-8")

            msg = "fix: foo\n\nIntent: 修复 foo 行为\n"
            new, injected = inject_ref_trace(msg, trace_path=trace_file)
            self.assertTrue(injected)
            self.assertIn("Ref-trace:", new)
            self.assertIn(str(trace_file), new)


class TestAlreadyHasRefTrace(unittest.TestCase):
    """Case 3: 已存在 Ref-trace → no-op,不重复注入"""

    def test_skips_when_ref_trace_present(self):
        with tempfile.TemporaryDirectory() as tmp:
            today = __import__("datetime").datetime.now().strftime("%Y-%m-%d")
            today_dir = Path(tmp) / today
            today_dir.mkdir()
            trace_file = today_dir / "xyz.ndjson"
            trace_file.write_text("{}\n", encoding="utf-8")

            msg = "fix: foo\n\nIntent: 修复 foo 行为\n\nRef-trace: /old/path.ndjson\n"
            new, injected = inject_ref_trace(msg, trace_path=trace_file)
            self.assertFalse(injected)
            self.assertEqual(new, msg)
            self.assertEqual(new.count("Ref-trace:"), 1)


class TestNoTrailingNewline(unittest.TestCase):
    """Case 4: commit msg 不以 \\n 结尾 → 自动补空行保持美观"""

    def test_adds_blank_line_when_no_trailing_newline(self):
        with tempfile.TemporaryDirectory() as tmp:
            today = __import__("datetime").datetime.now().strftime("%Y-%m-%d")
            today_dir = Path(tmp) / today
            today_dir.mkdir()
            trace_file = today_dir / "abc.ndjson"
            trace_file.write_text("{}\n", encoding="utf-8")

            msg_no_newline = "fix: foo\n\nIntent: 修复 foo 行为"
            new, injected = inject_ref_trace(msg_no_newline, trace_path=trace_file)
            self.assertTrue(injected)
            self.assertIn("\n\nRef-trace:", new)


if __name__ == "__main__":
    unittest.main()
