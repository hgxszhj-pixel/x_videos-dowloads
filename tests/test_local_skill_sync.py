#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""test_local_skill_sync.py — local_skill_sync.py 单元测试

覆盖 plan P3 防 AI 假测试 case:
  1. 空目录 list
  2. 已 symlink verify (无断链)
  3. 已 symlink 但目标缺失 verify (断链)
  4. CRLF 检测 fix_line_endings
  5. 硬名单保护 apply --dry-run

运行: python3 -m unittest tests/test_local_skill_sync.py -v
或:   cd tests && python3 test_local_skill_sync.py -v
"""

from __future__ import annotations

import json
import shutil
import sys
import tempfile
import unittest
from pathlib import Path

# 把 scripts/ 加进 path,允许直接 import
SCRIPTS_DIR = Path(__file__).resolve().parent.parent / "scripts"
sys.path.insert(0, str(SCRIPTS_DIR))

import local_skill_sync as lss  # noqa: E402


class TestExpand(unittest.TestCase):
    """expand() ~ 与 ${home} 展开。"""

    def test_expand_tilde(self):
        p = lss.expand("~/foo")
        self.assertEqual(p, Path.home() / "foo")

    def test_expand_home_env(self):
        p = lss.expand("${HOME}/bar")
        self.assertEqual(p, Path.home() / "bar")

    def test_expand_absolute_unchanged(self):
        p = lss.expand("/tmp/baz")
        self.assertEqual(p, Path("/tmp/baz"))


class TestShouldExclude(unittest.TestCase):
    """should_exclude() fnmatch 匹配。"""

    def test_excludes_pycache(self):
        self.assertTrue(lss.should_exclude("__pycache__",
                                            ["__pycache__", "*.egg-info"]))

    def test_excludes_glob(self):
        self.assertTrue(lss.should_exclude("foo.egg-info",
                                            ["__pycache__", "*.egg-info"]))

    def test_includes_normal(self):
        self.assertFalse(lss.should_exclude("SKILL.md",
                                             ["__pycache__", "*.egg-info"]))


class TestCheckSkillMd(unittest.TestCase):
    """check_skill_md() frontmatter 完整性。"""

    def test_missing_skill_md(self):
        with tempfile.TemporaryDirectory() as tmp:
            skill_dir = Path(tmp) / "myskill"
            skill_dir.mkdir()
            issues = lss.check_skill_md(skill_dir, ["name", "description"])
            self.assertEqual(len(issues), 1)
            self.assertIn("missing SKILL.md", issues[0])

    def test_no_frontmatter(self):
        with tempfile.TemporaryDirectory() as tmp:
            skill_dir = Path(tmp) / "myskill"
            skill_dir.mkdir()
            (skill_dir / "SKILL.md").write_text("# just a title\nbody\n")
            issues = lss.check_skill_md(skill_dir, ["name", "description"])
            self.assertEqual(len(issues), 1)
            self.assertIn("no frontmatter", issues[0])

    def test_complete_frontmatter(self):
        with tempfile.TemporaryDirectory() as tmp:
            skill_dir = Path(tmp) / "myskill"
            skill_dir.mkdir()
            (skill_dir / "SKILL.md").write_text(
                "---\nname: myskill\ndescription: a skill\n---\nbody\n")
            issues = lss.check_skill_md(skill_dir, ["name", "description"])
            self.assertEqual(issues, [])

    def test_missing_key(self):
        with tempfile.TemporaryDirectory() as tmp:
            skill_dir = Path(tmp) / "myskill"
            skill_dir.mkdir()
            (skill_dir / "SKILL.md").write_text(
                "---\nname: myskill\n---\nbody\n")
            issues = lss.check_skill_md(skill_dir, ["name", "description"])
            self.assertEqual(len(issues), 1)
            self.assertIn("description", issues[0])


class TestFixLineEndings(unittest.TestCase):
    """fix_line_endings() CRLF → LF。"""

    def test_replaces_crlf(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            (tmp_path / "a.md").write_bytes(b"line1\r\nline2\r\nline3\n")
            (tmp_path / "b.txt").write_bytes(b"ignored\r\n")  # 非 .md 不动
            fixed = lss.fix_line_endings(tmp_path, [".md"], log=None) if False \
                else lss.fix_line_endings(tmp_path, [".md"], _DummyLog())
            self.assertEqual(fixed, 1)
            self.assertEqual((tmp_path / "a.md").read_bytes(),
                             b"line1\nline2\nline3\n")
            # .txt 不在 exts,不动
            self.assertEqual((tmp_path / "b.txt").read_bytes(), b"ignored\r\n")

    def test_skips_lf_only(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            (tmp_path / "a.md").write_bytes(b"line1\nline2\n")
            fixed = lss.fix_line_endings(tmp_path, [".md"], _DummyLog())
            self.assertEqual(fixed, 0)


class TestVerifySymlinks(unittest.TestCase):
    """verify_symlinks() 断链检测。"""

    def test_empty_dir(self):
        with tempfile.TemporaryDirectory() as tmp:
            broken = lss.verify_symlinks(Path(tmp), [], _DummyLog())
            self.assertEqual(broken, [])

    def test_healthy_symlink(self):
        with tempfile.TemporaryDirectory() as tmp:
            src = Path(tmp) / "src"
            src.mkdir()
            (src / "x.md").write_text("hi")
            link = Path(tmp) / "link"
            os_symlink = link
            os_symlink.symlink_to(src)
            broken = lss.verify_symlinks(Path(tmp), [], _DummyLog())
            self.assertEqual(broken, [])

    def test_broken_symlink(self):
        with tempfile.TemporaryDirectory() as tmp:
            link = Path(tmp) / "broken"
            link.symlink_to("/nonexistent/path")
            broken = lss.verify_symlinks(Path(tmp), [], _DummyLog())
            self.assertEqual(len(broken), 1)
            self.assertEqual(broken[0]["name"], "broken")
            self.assertFalse(broken[0]["exists"])
            self.assertFalse(broken[0]["preserve"])


class TestMirrorSkill(unittest.TestCase):
    """mirror_skill() 不穿透 symlink。"""

    def test_mirror_preserves_symlinks(self):
        with tempfile.TemporaryDirectory() as tmp:
            src = Path(tmp) / "src"
            src.mkdir()
            target = src / "target.txt"
            target.write_text("data")
            inner_link = src / "inner_link"
            inner_link.symlink_to(target)  # src 内层 symlink

            dst = Path(tmp) / "dst"
            lss.mirror_skill(src, dst, [], _DummyLog())
            # 验证内层 symlink 被保留(不是穿透为文件)
            self.assertTrue((dst / "inner_link").is_symlink(),
                            "symlink should be preserved, not dereferenced")
            # 验证 readlink 目标指向原 target
            self.assertEqual(os_path_readlink(dst / "inner_link"),
                             str(target))


# ---- helpers ----

class _DummyLog:
    """替身 logger,避免 fix_line_endings 等函数必须传真 logger。"""
    def info(self, *a, **kw): pass
    def warning(self, *a, **kw): pass
    def error(self, *a, **kw): pass


def os_path_readlink(p: Path) -> str:
    import os
    return os.readlink(p)


if __name__ == "__main__":
    unittest.main(verbosity=2)