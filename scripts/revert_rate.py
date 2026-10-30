#!/usr/bin/env python3
"""revert_rate.py — 回滚率指标（把 Anthropic 的北极星搬过来）

来源: Anthropic《How the Claude Code team does code review》把 **回滚率**
作为整套审查流程的北极星指标 —— 6→9 月合并 PR 数翻倍以上,回滚率反而降了 40%+。
理由很直接: 「审查通过率」会被 reviewer 刷高(放宽标准就上去了),
但「上线后被回退」骗不了人,它是外部世界对这次改动的最终判决。

    回滚率 = (窗口内被回退的 commit 数) / (窗口内 commit 总数)

## 口径:只认真信号,不做文本猜测

2026-10-30 首版用 subject 关键字(revert / roll back / 回滚 / 撤回 / 回退)判定,
**实测 3/3 全是误报**:

    a426d392  fix(docs): SKILL.md 回滚策略描述准确化      ← 说的是「回滚策略」这个概念
    f56e705e  docs(handoff): WikiSkill 决策回滚日志        ← 同上
    7f6dc15d  feat(download): X 下载速度自适应 (空标题回退…)  ← 「回退」= fallback,不是回滚

所以现在**只认 git 自己写的机器信号**: `git revert` 生成的 commit 正文里
必然有一行 `This reverts commit <sha>.`。这一行没有歧义、可复算、不依赖语言。

手工写的 revert 没有可靠文本特征,只能作为**候选**单独列出(--include-declared),
默认**不计入**分母。宁可少算,不要假绿。

用法:
    python3 scripts/revert_rate.py                  # 默认近 30 天
    python3 scripts/revert_rate.py --days 90
    python3 scripts/revert_rate.py --json
    python3 scripts/revert_rate.py --include-declared
    python3 scripts/revert_rate.py --max-rate 0.05 # 超过 5% 则 exit 2(可用于门禁)

## 时间窗口:不用 git 的 --since 相对写法

git 的 `--since` 走 `approxidate`,对裸 "Nd" 的解析**不可靠**:
实测 `--since=90d` 给出 231 条、`--since=365d` 直接给出 **0 条**(不是"没有更老的提交")。
所以本脚本用 Python 算出 ISO 时间戳再传给 git(`--since=2025-10-30 00:00:00`),
两种写法实测都稳定返回 450 条。要用相对写法请走 `--days N`。

退出码:
    0 = 正常
    1 = 参数错误 / git 不可用
    2 = 超过 --max-rate 阈值
"""
from __future__ import annotations

import argparse
import datetime
import json
import re
import subprocess
import sys

# git revert 自动生成的正文行: This reverts commit 0123abc....
REVERTS_COMMIT_RE = re.compile(r"^\s*This reverts commit\s+([0-9a-fA-F]{7,40})\.?\s*$", re.M)
# 机器信号也可能写成 "This reverts commit X." 之外的形式,保留宽松兜底
REVERTS_LOOSE_RE = re.compile(r"This reverts commit\s+([0-9a-fA-F]{7,40})")

# 手工声明回退的候选 —— 只在 --include-declared 时列出,不计入分母。
# 中文关键词必须一起收:本仓库 commit message 以中文为主,只认英文 revert
# 会把「手工写的回退」整个漏掉(候选漏报)。Python 的 \b 对中文不适用,
# 所以中文分支不加边界 —— 反正只是候选,误报的代价只是多一行人工确认。
DECLARED_RE = re.compile(
    r"\b(revert(?:s|ed|ing)?|roll\s*back|rolled\s*back|rollback)\b"
    r"|回滚|回退|撤回|撤销",
    re.IGNORECASE,
)


def _git(*args: str) -> str:
    return subprocess.check_output(["git", *args], text=True, stderr=subprocess.DEVNULL)


def collect(since: str, include_declared: bool = False) -> dict:
    raw = _git(
        "log",
        f"--since={since}",
        "--format=%H%x1f%s%x1f%b%x1e",
    )
    commits = []
    for rec in raw.split("\x1e"):
        rec = rec.strip("\n")
        if not rec.strip():
            continue
        parts = rec.split("\x1f")
        if len(parts) < 3:
            continue
        sha, subject, body = parts[0], parts[1], "\x1f".join(parts[2:])
        commits.append({"sha": sha, "subject": subject, "body": body})

    reverts = []
    declared = []
    for c in commits:
        m = REVERTS_LOOSE_RE.search(c["body"])
        if m:
            reverts.append({**c, "kind": "git-revert", "reverts_sha": m.group(1)})
        elif include_declared and DECLARED_RE.search(c["subject"]):
            declared.append({**c, "kind": "declared-candidate"})

    total = len(commits)
    return {
        "since": since,
        "total_commits": total,
        "revert_count": len(reverts),
        "revert_rate": (len(reverts) / total) if total else 0.0,
        "reverts": reverts,
        "declared_candidates": declared,
        "commits": commits,
    }


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description="回滚率指标")
    ap.add_argument(
        "--days",
        type=int,
        default=30,
        help="统计窗口天数(默认 30)。不要用 git 的 --since 相对写法,见文件头说明",
    )
    ap.add_argument(
        "--since",
        default=None,
        help="直接传给 git --since 的绝对时间(如 2025-10-30);覆盖 --days",
    )
    ap.add_argument("--json", action="store_true", help="输出 JSON")
    ap.add_argument(
        "--include-declared",
        action="store_true",
        help="额外列出 subject 里提到 revert 的候选(默认不计入分母)",
    )
    ap.add_argument(
        "--max-rate",
        type=float,
        default=None,
        help="回滚率超过该值则 exit 2(例如 --max-rate 0.05)",
    )
    args = ap.parse_args(argv)

    if args.since:
        since = args.since
    else:
        if args.days <= 0:
            print("--days 必须为正整数", file=sys.stderr)
            return 1
        since = (
            datetime.datetime.now() - datetime.timedelta(days=args.days)
        ).strftime("%Y-%m-%d %H:%M:%S")

    try:
        data = collect(since, args.include_declared)
    except (subprocess.CalledProcessError, FileNotFoundError) as e:
        print(f"git 不可用: {e}", file=sys.stderr)
        return 1

    if args.json:
        print(json.dumps(data, ensure_ascii=False, indent=2))
    else:
        print(f"窗口        : {data['since']}")
        print(f"commit 总数 : {data['total_commits']}")
        print(f"回退 commit : {data['revert_count']}")
        print(f"回滚率      : {data['revert_rate']:.2%}")
        for r in data["reverts"]:
            print(f"  [git-revert] {r['sha'][:8]} -> {r['reverts_sha'][:8]}  {r['subject']}")
        if data["declared_candidates"]:
            print("候选(未计入分母, 需人工确认):")
            for r in data["declared_candidates"]:
                print(f"  [declared]  {r['sha'][:8]}  {r['subject']}")

    if args.max_rate is not None and data["revert_rate"] > args.max_rate:
        print(
            f"回滚率 {data['revert_rate']:.2%} 超过阈值 {args.max_rate:.2%}",
            file=sys.stderr,
        )
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
