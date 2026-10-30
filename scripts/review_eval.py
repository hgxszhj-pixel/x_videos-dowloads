#!/usr/bin/env python3
"""review_eval.py — 审查 prompt 的回归评测（Anthropic「incidents as evals」本地化）

## 为什么要这个

Anthropic《How the Claude Code team does code review》里最狠的一条不是流程,是**评测**:
> 历史事故 PR 变成每日 eval 集;任何对 reviewer 的 prompt / 工具 / 模型改动,
> 必须先在分数上证明不退化,才能合入。

本仓库之前只有**被测对象**的评测(`mutation_smoke.sh` 管测试有效性、
`eval_trace.py` 管运行时指标),**没有给「审查这件事本身」建评测**。
于是「改 review prompt 让它更啰嗦/更宽松」这类改动从来无法被度量 ——
只能靠人感觉。

本脚本补上这一层:
  - `fixtures/` 里是本仓库**真实历史缺陷**的最小复现(base → head 的改动)
  - 每个 fixture 声明「一个合格的 reviewer 必须抓到的问题」
  - 用**真实的 reviewer prompt**（`~/.claude/skills/code-review-canonical/code-reviewer.md`）
    去审这些 diff,看它有没有抓到
  - 抓到率(recall)低于阈值 = 分数退化 = 不许合入

## 三条设计取舍

1. **diff 内联,不让 reviewer 自己跑 git。**
   `claude -p` 在非交互下跑 Bash 需要额外授权,会让评测变得不稳定(有时能跑,
   有时被权限打断)。而且评测要测的是「**prompt 能不能发现缺陷**」,
   不是「agent 会不会用 git」—— 所以直接把 `git diff` 贴进 prompt。
2. **只认机器可判定的信号。** 每个期望问题用 `any_of` 子串列表表达,
   大小写不敏感;命中即算抓到。没有 LLM-judge,结果可复算、可 diff、可进 CI。
3. **控制变量只有一个:prompt 文件本身。** fixture / 阈值 / 模型都在脚本里,
   变的只有 prompt。改了 prompt 分数掉了 → 就是 prompt 改坏了。

## 用法

    python3 scripts/review_eval.py run                # 跑评测,写 baseline
    python3 scripts/review_eval.py run --min-recall 1.0
    python3 scripts/review_eval.py gate               # 检查 baseline 是否与当前 prompt 匹配
    python3 scripts/review_eval.py list               # 列出 fixture

`gate` 被 commit-msg 链调用:prompt 文件变了但没重跑评测 → 拒绝 commit。
逃生舱:commit message 含 `[skip-review-eval]`。

退出码:
    0 = 通过 / baseline 最新
    1 = 失败(召回率不达标 / baseline 过期 / fixture 缺失)
    2 = 用法错误
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
FIXTURE_DIR = REPO_ROOT / "scripts" / "review_eval_fixtures"
# baseline 路径可被 REVIEW_EVAL_BASELINE 覆盖 —— 冒烟测试要用临时文件,
# 不能污染仓库里那份真的 baseline。
BASELINE_PATH = Path(
    os.environ.get(
        "REVIEW_EVAL_BASELINE", str(REPO_ROOT / "scripts" / "review_eval_baseline.json")
    )
).expanduser()

# 真实 reviewer prompt 的位置。装在 ~/.claude/skills 下(独立 git repo),
# 所以仓库 commit 看不到它 —— 这正是要用 baseline 记 prompt 哈希的原因。
PROMPT_PATH = Path(
    os.environ.get(
        "REVIEW_PROMPT_PATH",
        "~/.claude/skills/code-review-canonical/code-reviewer.md",
    )
).expanduser()

# 评测时给 reviewer 用的模型。可用 REVIEW_EVAL_MODEL 覆盖。
MODEL = os.environ.get("REVIEW_EVAL_MODEL", "sonnet")

# 子 reviewer 的最大轮数。评测要的是「一次给出结论」,不是让它自我审查到天荒地老。
# 12 轮实测不够:reviewer 推理对了但轮数耗尽前没写报告(见 ask_reviewer 注释)。
REVIEWER_MAX_TURNS = int(os.environ.get("REVIEW_EVAL_MAX_TURNS", "20"))

# 单个 fixture 的超时(秒)。
REVIEWER_TIMEOUT = int(os.environ.get("REVIEW_EVAL_TIMEOUT", "600"))

SKIP_MARKER = "[skip-review-eval]"

# 默认召回率门槛。三个 fixture 各只有一条「必须抓到的问题」——
# reviewer 只要漏掉一条就是能力退化,不允许「差不多」。
MIN_RECALL = 1.0


# --------------------------------------------------------------------------
# prompt 处理
# --------------------------------------------------------------------------
def prompt_sha256() -> str | None:
    if not PROMPT_PATH.exists():
        return None
    return hashlib.sha256(PROMPT_PATH.read_bytes()).hexdigest()


def load_prompt_template() -> str:
    """从 code-reviewer.md 里抽出 ``` 包裹的 prompt 模板。"""
    if not PROMPT_PATH.exists():
        raise SystemExit(
            f"找不到 reviewer prompt: {PROMPT_PATH}\n"
            f"设置 REVIEW_PROMPT_PATH 指向 code-reviewer.md"
        )
    text = PROMPT_PATH.read_text(encoding="utf-8")
    blocks = re.findall(r"```[a-z]*\n(.*?)```", text, re.DOTALL)
    # 取最长的那个块 —— 模板是文件里最长的 fenced block
    if not blocks:
        raise SystemExit(f"{PROMPT_PATH} 里没有 ``` 包裹的 prompt 模板")
    return max(blocks, key=len)


def render_prompt(template: str, fixture: dict, diff_text: str, base_sha: str, head_sha: str) -> str:
    repo_ctx = fixture.get("repo_context", "")
    if repo_ctx:
        repo_ctx = "\n".join(f"- {line}" for line in repo_ctx)
    return (
        template.replace("[DESCRIPTION]", fixture.get("description", ""))
        .replace("[PLAN_OR_REQUIREMENTS]", fixture.get("plan", ""))
        .replace("[REPO_CONTEXT]", repo_ctx or "- (none provided)")
        .replace("[BASE_SHA]", base_sha)
        .replace("[HEAD_SHA]", head_sha)
        + "\n\n## Diff under review (BASE..HEAD)\n\n```diff\n"
        + diff_text
        + "\n```\n"
    )


# --------------------------------------------------------------------------
# fixture 执行
# --------------------------------------------------------------------------
def _run(cmd: list[str], cwd: Path) -> str:
    return subprocess.check_output(cmd, cwd=cwd, text=True, stderr=subprocess.DEVNULL)


def build_diff(fixture: dict, workdir: Path) -> tuple[str, str, str, Path]:
    """把 fixture 的 base/head 两套文件落成一个临时 git repo,取 diff。

    返回 (diff 文本, base_sha, head_sha, repo 目录)。
    """
    repo = workdir / "repo"
    repo.mkdir(parents=True, exist_ok=True)
    _run(["git", "init", "-q"], repo)
    _run(["git", "config", "user.email", "eval@local"], repo)
    _run(["git", "config", "user.name", "eval"], repo)

    for name, content in (fixture.get("base_files") or {}).items():
        p = repo / name
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(content, encoding="utf-8")
    _run(["git", "add", "-A"], repo)
    _run(["git", "commit", "-q", "-m", "base"], repo)
    base_sha = _run(["git", "rev-parse", "HEAD"], repo).strip()

    for name, content in (fixture.get("head_files") or {}).items():
        p = repo / name
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(content, encoding="utf-8")
    _run(["git", "add", "-A"], repo)
    _run(["git", "commit", "-q", "-m", "head"], repo)
    head_sha = _run(["git", "rev-parse", "HEAD"], repo).strip()

    return (
        _run(["git", "diff", f"{base_sha}..{head_sha}"], repo),
        base_sha,
        head_sha,
        repo,
    )


def ask_reviewer(
    prompt: str, workdir: Path, timeout: int | None = None
) -> tuple[bool, str]:
    """跑一次 `claude -p`。返回 (是否拿到输出, 输出文本)。

    只放行只读工具（Read/Grep/Glob）:diff 已内联进 prompt,评测不需要 reviewer
    自己跑 git。这样做有两个实测理由:
      - 给了 Bash,reviewer 会花好几轮做「考古 + 写 /tmp 复现」——
        实测 13 轮里有一半花在探索和被拒绝的文件写入上,轮数耗尽时报告还没写。
      - 非交互模式下写文件要审批,被拒后它会换招再来,又是几轮。
    评测要测的是「**这份 prompt 能不能从 diff 里发现缺陷**」,不是「agent 会不会用 shell」。

    `--setting-sources ""` 与 `--max-turns` 也是**必需的**,不是可选优化:
      - 空 setting sources = 隔离全局 settings(尤其是本机的 anti-slop Stop hook)。
        不隔离时,子 reviewer 的措辞会被全局 hook 判成「AI 味」,然后它花好几轮
        解释自己为什么改措辞 —— 实测卡到 max turns 超时(300s / 600s 各挂一次)。
      - `--max-turns`:评测要的是「一次性给出结论」,不是让它反复自我审查。
        20 轮是实测出来的下限:12 轮时 reviewer 已经**推理出正确结论**
        (它在第 13 轮用 bash 演示了 `"VAR"` vs `"$VAR"` 的区别),
        但轮数耗尽前没来得及写出报告 —— 结论对、评测却记 0 分。
    """
    timeout = timeout or REVIEWER_TIMEOUT
    try:
        proc = subprocess.run(
            [
                "claude",
                "-p",
                "--model",
                MODEL,
                "--tools",
                "Read,Grep,Glob",
                "--setting-sources",
                "",
                "--max-turns",
                str(REVIEWER_MAX_TURNS),
            ],
            input=prompt,
            capture_output=True,
            text=True,
            timeout=timeout,
            cwd=workdir,
        )
    except FileNotFoundError:
        raise SystemExit("找不到 `claude` CLI —— review eval 依赖它")
    except subprocess.TimeoutExpired:
        return False, ""
    if proc.returncode != 0 and not proc.stdout.strip():
        return False, proc.stderr[-500:]
    return True, proc.stdout


def score_fixture(fixture: dict, output: str) -> dict:
    low = output.lower()
    found, missed = [], []
    for exp in fixture.get("expected") or []:
        needles = [n.lower() for n in exp.get("any_of", [])]
        hit = any(n in low for n in needles)
        (found if hit else missed).append(exp["id"])
    false_pos = []
    for needle in fixture.get("must_not_contain") or []:
        if needle.lower() in low:
            false_pos.append(needle)
    total = len(found) + len(missed)
    recall = (len(found) / total) if total else 1.0
    return {
        "id": fixture["id"],
        "name": fixture.get("name", fixture["id"]),
        "expected_total": total,
        "found": found,
        "missed": missed,
        "recall": recall,
        "false_positives": false_pos,
    }


# --------------------------------------------------------------------------
# 子命令
# --------------------------------------------------------------------------
def cmd_list() -> int:
    """列出所有 fixture 及其期望问题数。"""
    for f in sorted(FIXTURE_DIR.glob("*.json")):
        data = json.loads(f.read_text(encoding="utf-8"))
        exp = data.get("expected") or []
        print(f"{data['id']:34} expected={len(exp):2}  {data.get('name','')}")
    return 0


def cmd_run(args) -> int:
    fixtures = []
    for f in sorted(FIXTURE_DIR.glob("*.json")):
        fixtures.append(json.loads(f.read_text(encoding="utf-8")))
    if not fixtures:
        print(f"没有 fixture: {FIXTURE_DIR}", file=sys.stderr)
        return 1

    template = load_prompt_template()
    results = []
    with tempfile.TemporaryDirectory(prefix="review-eval.") as td:
        workdir = Path(td)
        for fx in fixtures:
            print(f"\n=== {fx['id']} ===", flush=True)
            diff_text, base_sha, head_sha, repo = build_diff(
                fx, workdir / fx["id"].replace("/", "_")
            )
            prompt = render_prompt(template, fx, diff_text, base_sha, head_sha)
            # cwd 落在 fixture 的临时 repo 里 —— reviewer prompt 里有 git 命令,
            # 在真 repo 里跑它才不会因为「不是 git 仓库」而无从下手。
            ok, output = ask_reviewer(prompt, repo)
            if not ok:
                print(f"  reviewer 没返回结果(model={MODEL})", file=sys.stderr)
                results.append(
                    {
                        "id": fx["id"],
                        "name": fx.get("name", fx["id"]),
                        "expected_total": len(fx.get("expected") or []),
                        "found": [],
                        "missed": [e["id"] for e in fx.get("expected") or []],
                        "recall": 0.0,
                        "false_positives": [],
                        "error": "reviewer 未返回输出",
                    }
                )
                continue
            r = score_fixture(fx, output)
            results.append(r)
            status = "OK " if r["recall"] == 1.0 and not r["false_positives"] else "MISS"
            print(
                f"  {status} recall={r['recall']:.2f} "
                f"found={r['found']} missed={r['missed']} "
                f"false_pos={r['false_positives']}",
                flush=True,
            )
            (workdir / fx["id"].replace("/", "_")).mkdir(parents=True, exist_ok=True)
            out_dir = Path(
                os.environ.get(
                    "REVIEW_EVAL_OUT", str(Path(tempfile.gettempdir()) / "review_eval_out")
                )
            )
            out_dir.mkdir(parents=True, exist_ok=True)
            (out_dir / f"{fx['id']}.md").write_text(output, encoding="utf-8")

    total_exp = sum(r["expected_total"] for r in results)
    total_found = sum(len(r["found"]) for r in results)
    recall = (total_found / total_exp) if total_exp else 0.0
    fp = sum(len(r["false_positives"]) for r in results)

    report = {
        "prompt_sha256": prompt_sha256(),
        "prompt_path": str(PROMPT_PATH),
        "model": MODEL,
        "generated_at": subprocess.check_output(["date", "-u", "+%Y-%m-%dT%H:%M:%SZ"], text=True).strip(),
        "recall": recall,
        "expected_total": total_exp,
        "found_total": total_found,
        "false_positive_count": fp,
        "results": results,
    }

    # **失败的结果不许覆盖 baseline** —— 否则一次退化会被固化成新标准,
    # 门禁从此再也拦不住东西。失败的报告另存 *.failed.json 供诊断。
    passed = report["recall"] >= args.min_recall and report["false_positive_count"] == 0
    target = BASELINE_PATH if passed else BASELINE_PATH.with_suffix(".failed.json")
    target.write_text(
        json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    print(f"\nrecall={recall:.2f} ({total_found}/{total_exp})  false_positives={fp}")

    if recall < args.min_recall:
        print(
            f"\nFAIL: recall {recall:.2f} < 阈值 {args.min_recall:.2f} —— "
            f"prompt 改动让审查能力退化了\n"
            f"      失败详情 → {target}\n"
            f"      旧 baseline 未被覆盖（失败的结果不许成为新标准）",
            file=sys.stderr,
        )
        return 1
    if fp:
        print(
            f"\nFAIL: {fp} 个误报 —— prompt 改动引入了不该报的噪音\n"
            f"      失败详情 → {target}",
            file=sys.stderr,
        )
        return 1
    print(f"\nPASS\nbaseline → {BASELINE_PATH}")
    return 0


def has_skip_marker(commit_msg: str) -> bool:
    """逃生舱标记是否**独占一行**。

    旧实现用 `SKIP_MARKER in commit_msg` —— 只要正文里**提到**这个字符串
    就会静默关掉整个门禁。实测踩中:本仓自己的 commit message 写着
    「逃生舱 [skip-review-eval]」来解释这个机制,结果门禁被这条说明
    自己豁免掉了(commit 8009666)。

    豁免必须是**显式动作**,不能是顺口一提。所以要求标记独占一行。
    """
    return any(line.strip() == SKIP_MARKER for line in commit_msg.splitlines())


def check_review_eval(commit_msg: str) -> tuple[bool, str]:
    """commit-msg 链调用:prompt 改了但没重跑评测 → 拒绝。

    返回 (是否通过, 给用户看的提示)。**不打印** —— 由调用方决定怎么呈现,
    与本仓库其它 check_xxx() 的契约一致。

    逃生舱:commit message 里有**独占一行**的 `[skip-review-eval]`。
    """
    if has_skip_marker(commit_msg):
        return True, f"[review-eval] 已跳过 (msg 有独占一行的 {SKIP_MARKER})"

    if not PROMPT_PATH.exists():
        return True, f"[review-eval] 跳过: 没有 {PROMPT_PATH}"
    if not BASELINE_PATH.exists():
        return True, f"[review-eval] 跳过: 还没有 baseline({BASELINE_PATH})"
    try:
        base = json.loads(BASELINE_PATH.read_text(encoding="utf-8"))
    except json.JSONDecodeError as e:
        return False, f"[review-eval] baseline 坏了({e}),先跑 run"

    cur = prompt_sha256()
    if base.get("prompt_sha256") != cur:
        return False, (
            "[review-eval] 拒绝: reviewer prompt 已改动但没重跑评测。\n"
            "            先跑: python3 scripts/review_eval.py run\n"
            "            (分数不退化才允许把新 prompt 用起来)\n"
            f"            逃生舱: commit message 加 {SKIP_MARKER}"
        )

    # baseline 本身也必须是「合格」的 —— 光哈希对得上不够:
    # 一份 recall 0.67 的 baseline 照样能通过哈希检查。
    recall = float(base.get("recall", 0.0))
    fp = int(base.get("false_positive_count", 0))
    if recall < MIN_RECALL:
        return False, (
            f"[review-eval] 拒绝: baseline recall={recall:.2f} < {MIN_RECALL:.2f} "
            f"—— 这份 baseline 本身就不合格,先跑 run 重新评测。\n"
            f"            逃生舱: commit message 加 {SKIP_MARKER}"
        )
    if fp:
        return False, (
            f"[review-eval] 拒绝: baseline 带 {fp} 个误报 —— 重新评测。\n"
            f"            逃生舱: commit message 加 {SKIP_MARKER}"
        )

    return True, (
        f"[review-eval] OK (recall={recall:.2f}, prompt={cur[:8] if cur else '?'})"
    )


def cmd_gate(args) -> int:
    """commit-msg 链调用:prompt 改了但没重跑评测 → 拒绝。

    `--message` 让冒烟测试能验证逃生舱(`[skip-review-eval]`)确实生效。
    """
    ok, msg = check_review_eval(getattr(args, "message", "") or "")
    print(msg, file=sys.stderr)
    return 0 if ok else 1


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description="审查 prompt 回归评测")
    sub = ap.add_subparsers(dest="cmd", required=True)

    r = sub.add_parser("run", help="跑评测并写 baseline")
    r.add_argument("--min-recall", type=float, default=1.0)

    g = sub.add_parser("gate", help="检查 baseline 是否与当前 prompt 匹配")
    g.add_argument(
        "--message",
        default="",
        help="被检查的 commit message(用于验证逃生舱 %s)" % SKIP_MARKER,
    )
    sub.add_parser("list", help="列出 fixture")

    args = ap.parse_args(argv)
    if args.cmd == "run":
        return cmd_run(args)
    if args.cmd == "gate":
        return cmd_gate(args)
    if args.cmd == "list":
        return cmd_list()
    return 2


if __name__ == "__main__":
    sys.exit(main())
