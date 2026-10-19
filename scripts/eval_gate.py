#!/usr/bin/env python3
"""eval_gate.py — Pre-commit EVAL gate (Harness² 思想本地化)

源自文章《把 Opus 5.5 用成自主工程师》: 完成 = 检查全过 + 证据
本地最小化实现 (bin-only Rust 项目):

触发条件 (与门):
  1) commit 消息体含「完成声明」关键词 (中文 + 英文), 例如:
     - "fixed" / "done" / "passed" / "通过" / "完成" / "搞定" / "修复"
  2) staged 文件包含 Rust 源 (*.rs)

触发动作:
  - cargo fmt --check (快速, <5s)
  - cargo clippy --all-targets -- -D warnings (项目标准 lint)
  - 任一 exit != 0 → 拒绝 commit, 把尾段 stderr 回显给提交者

Escape hatch:
  - commit msg 含 [skip-eval] → 跳过本 gate (留给 doc-only / hotfix)
  - 已签发 VC 的 commit (含 [DID: ..] / [VC: ..]) → 跳过 (签发即证据)

调用:
    from eval_gate import check_eval_gate
    ok, msg = check_eval_gate(commit_msg_text)
    if msg: print(msg, file=sys.stderr)
    if not ok: sys.exit(1)
"""
from __future__ import annotations

import re
import shutil
import subprocess
import sys
from pathlib import Path


# 完成声明关键词 — case-insensitive; 中文直接子串匹配
CLAIM_KEYWORDS_EN = (
    r"\bfix(?:es|ed)?\b",   # fix / fixes / fixed
    r"\bdone\b",
    r"\bpass(?:es|ed)?\b",
    r"\bresolved?\b",
    r"\bcomplete[sd]?\b",
    r"\bimplemented?\b",
    r"\bshipped?\b",
    r"\bsuccessfully\b",
)
CLAIM_KEYWORDS_ZH = (
    "修复", "完成", "通过", "搞定", "解决",
    "实现", "已修", "调通", "落地", "交付",
)
CLAIM_RE = re.compile(
    "|".join(CLAIM_KEYWORDS_EN) + "|" + "|".join(re.escape(w) for w in CLAIM_KEYWORDS_ZH),
    re.IGNORECASE,
)

RUST_EXTS = {".rs"}

# 跳过 marker (escape hatch)
SKIP_MARKER = "[skip-eval]"
EVIDENCE_MARKERS = ("[DID:", "[VC:")


def _git_cached_files() -> list[str]:
    """staged files (added/copied/modified/renamed), empty if not a git context."""
    try:
        out = subprocess.check_output(
            ["git", "diff", "--cached", "--name-only", "--diff-filter=ACMR"],
            text=True,
            stderr=subprocess.DEVNULL,
        )
    except (subprocess.CalledProcessError, FileNotFoundError):
        return []
    return [line.strip() for line in out.splitlines() if line.strip()]


def _strip_trailers(body: str) -> str:
    """去掉 Intent / Ref-trace / DID-VC 这些 trailer 行,只对正文本做关键词扫描。"""
    keep: list[str] = []
    for line in body.splitlines():
        s = line.strip()
        if s.startswith(("Intent:", "Ref-trace:", "[DID:", "[VC:")):
            continue
        keep.append(line)
    return "\n".join(keep)


def _has_completion_claim(body: str) -> bool:
    cleaned = _strip_trailers(body)
    return bool(CLAIM_RE.search(cleaned))


def _filter_rust(files: list[str]) -> list[str]:
    return [f for f in files if Path(f).suffix in RUST_EXTS]


def _run(cmd: list[str], timeout: int = 120, cwd: Path | None = None) -> tuple[int, str, str]:
    """Run a check command; return (exit, stdout_tail, stderr_tail)."""
    try:
        proc = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            timeout=timeout,
            check=False,
            cwd=cwd,
        )
    except FileNotFoundError as e:
        return 127, "", f"command not found: {cmd[0]} ({e})"
    except subprocess.TimeoutExpired as e:
        return 124, "", f"timeout after {timeout}s"
    # 取尾段,避免长输出炸 stderr
    stdout_tail = "\n".join((proc.stdout or "").splitlines()[-30:])
    stderr_tail = "\n".join((proc.stderr or "").splitlines()[-30:])
    return proc.returncode, stdout_tail, stderr_tail


def check_eval_gate(commit_msg: str | None = None) -> tuple[bool, str]:
    """Gate 主函数。返回 (ok, human_msg) — ok=False 时 caller 应该 sys.exit(1)。

    Args:
        commit_msg: 待扫描的 commit 消息文本,None 时从 stdin 读 (兼容独立 CLI 调用)
    """
    # 0. Escape hatch 检查 (在最前,避免无害 doc commit 也被卡)
    msg_text = commit_msg if commit_msg is not None else sys.stdin.read()
    if SKIP_MARKER in msg_text:
        return True, f"🔓 eval-gate skipped via {SKIP_MARKER}"
    if any(m in msg_text for m in EVIDENCE_MARKERS):
        return True, "🔓 eval-gate skipped (commit already carries DID/VC evidence)"

    # 1. cargo 不存在 → warn 但不挡 (用户机器/容器可能有差异)
    if shutil.which("cargo") is None:
        return True, "⚠️  cargo 未找到, eval-gate 跳过 (建议装 Rust toolchain)"

    # 2. 没有 staged rust 文件 → 非代码提交,gate 免触发
    staged = _git_cached_files()
    rust_staged = _filter_rust(staged)
    if not rust_staged:
        return True, f"eval-gate skipped: no staged *.rs (staged={len(staged)} files)"

    # 3. 没有完成声明 → 文档/重构/格式化提交,gate 免触发
    if not _has_completion_claim(msg_text):
        return True, "eval-gate skipped: no completion-claim keyword in commit body"

    # 4. 触发 EVAL: fmt + clippy
    failures: list[str] = []
    repo_root = subprocess.check_output(
        ["git", "rev-parse", "--show-toplevel"], text=True
    ).strip()

    # 4a. cargo fmt --check (整个 crate 格式, ~3s)
    rc, _, err = _run(["cargo", "fmt", "--check"], cwd=Path(repo_root), timeout=30)
    if rc != 0:
        failures.append(f"cargo fmt --check failed:\n{err}")

    # 4b. cargo clippy --all-targets -- -D warnings  (项目 CLAUDE.md 标准命令)
    rc, _, err = _run(
        ["cargo", "clippy", "--all-targets", "--quiet", "--", "-D", "warnings"],
        cwd=Path(repo_root),
        timeout=180,
    )
    if rc != 0:
        failures.append(f"cargo clippy failed:\n{err}")

    if failures:
        body = "\n\n".join(failures)
        return False, (
            "❌ eval-gate 拦截: completion claim + Rust staged = 必须跑过 fmt + clippy\n\n"
            f"{body}\n\n"
            "绕过: 在 commit msg 加 [skip-eval] (保留 Intent Lock)"
        )

    return True, f"✓ eval-gate passed: fmt + clippy on {len(rust_staged)} rust file(s)"


def main() -> int:
    ok, out = check_eval_gate()
    if out:
        print(out, file=sys.stderr)
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
