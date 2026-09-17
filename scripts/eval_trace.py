#!/usr/bin/env python3
"""
eval_trace.py — Trace-Driven Eval (本地 NDJSON baseline vs current 对比)

复刻 LangSmith Trace 思路的"用 LLM-as-a-Judge 对历史 trace 批量打分"→
我们用 4 个简单指标自评,纯 stdlib,零外部依赖。

4 指标(详见 docs/dna/trace-eval-metrics.md):
  1. success_rate        — session_close.status == "ok" 占比
  2. avg_total_ms        — 平均 session 总耗时
  3. resolver_chain_length — 平均每次 session 串行 resolver 多少次
  4. stall_count         — duration ≥ 30s 的 stall 事件总数

用法:
  python3 scripts/eval_trace.py \\
      --baseline traces/2026-09-14-baseline/ \\
      --current  traces/2026-09-17-current/

退出码:
  0 = 无退化(全部指标改善或持平)
  1 = 一般错误(目录不存在 / 文件解析失败)
  2 = 检测到退化(任一关键指标恶化)

参考: docs/dna/trace-eval-metrics.md | docs/dna/trace-schema.md
"""

import argparse
import json
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Optional


STALL_THRESHOLD_MS = 30_000  # ≥ 30s 算 stall(BUG-D / BUG-E 经验值)


@dataclass
class Metrics:
    """单个 trace 目录聚合出的指标集"""

    total_sessions: int = 0
    ok_sessions: int = 0
    success_rate: float = 0.0
    avg_total_ms: int = 0  # session_close 的输出里不带总耗时,用 fetch_video_info 近似
    resolver_chain_length: float = 0.0  # resolver_try 事件数 / session 数
    stall_count: int = 0
    errors: list[str] = field(default_factory=list)

    @classmethod
    def from_dir(cls, trace_dir: Path) -> "Metrics":
        m = cls()
        if not trace_dir.exists():
            m.errors.append(f"目录不存在: {trace_dir}")
            return m
        if not trace_dir.is_dir():
            m.errors.append(f"不是目录: {trace_dir}")
            return m

        ndjson_files = sorted(trace_dir.glob("*.ndjson"))
        if not ndjson_files:
            m.errors.append(f"目录无 .ndjson 文件: {trace_dir}")
            return m

        # 累加器
        total_ms_sum = 0
        resolver_try_count = 0

        for f in ndjson_files:
            session_close_seen = False
            session_ok = False
            session_total_ms = 0
            session_resolver_count = 0
            session_stall = 0
            valid_lines = 0

            try:
                with f.open("r", encoding="utf-8") as fp:
                    for line in fp:
                        line = line.strip()
                        if not line:
                            continue
                        try:
                            ev = json.loads(line)
                        except json.JSONDecodeError as e:
                            m.errors.append(f"{f.name}: JSON 解析失败 {e}")
                            continue
                        valid_lines += 1
                        step = ev.get("step", "")
                        duration_ms = ev.get("duration_ms", 0) or 0
                        output = ev.get("output") or {}

                        if step == "session_close":
                            session_close_seen = True
                            session_ok = output.get("status") == "ok"
                            session_total_ms = output.get("total_ms_proxy", 0) or 0
                        elif step == "resolver_try":
                            session_resolver_count += 1
                        elif step in ("resolver_try", "download_one", "fetch_video_info"):
                            if duration_ms >= STALL_THRESHOLD_MS:
                                session_stall += 1
            except OSError as e:
                m.errors.append(f"{f.name}: 读文件失败 {e}")
                continue

            if not session_close_seen:
                # 无 session_close = 未完成 session,不计入分母
                m.errors.append(f"{f.name}: 无 session_close 事件,跳过")
                continue

            m.total_sessions += 1
            if session_ok:
                m.ok_sessions += 1
            total_ms_sum += session_total_ms
            resolver_try_count += session_resolver_count
            m.stall_count += session_stall

        # 聚合
        if m.total_sessions > 0:
            m.success_rate = m.ok_sessions / m.total_sessions
            m.avg_total_ms = total_ms_sum // m.total_sessions
            m.resolver_chain_length = resolver_try_count / m.total_sessions

        return m


@dataclass
class DiffRow:
    """单指标 baseline vs current 对比"""

    name: str
    baseline_val: str
    current_val: str
    delta: str
    verdict: str  # ✅ / ⚠️ / ❌

    @classmethod
    def make(cls, name: str, b: float, c: float, fmt: str, lower_is_better: bool) -> "DiffRow":
        delta = c - b
        # 判定
        if abs(delta) < 1e-9:
            verdict = "✅ 持平"
        elif lower_is_better:
            verdict = "✅ 改善" if delta < 0 else "❌ 退化"
        else:
            verdict = "✅ 改善" if delta > 0 else "❌ 退化"
        return cls(
            name=name,
            baseline_val=format(b, fmt),
            current_val=format(c, fmt),
            delta=f"{'+' if delta >= 0 else ''}{format(delta, fmt)}",
            verdict=verdict,
        )


def compare(baseline: Metrics, current: Metrics) -> list[DiffRow]:
    """生成 4 指标 baseline vs current 对比行"""
    return [
        DiffRow.make(
            "success_rate",
            baseline.success_rate,
            current.success_rate,
            ".3f",
            lower_is_better=False,
        ),
        DiffRow.make(
            "avg_total_ms",
            baseline.avg_total_ms,
            current.avg_total_ms,
            ".0f",
            lower_is_better=True,
        ),
        DiffRow.make(
            "resolver_chain_length",
            baseline.resolver_chain_length,
            current.resolver_chain_length,
            ".2f",
            lower_is_better=True,
        ),
        DiffRow.make(
            "stall_count",
            float(baseline.stall_count),
            float(current.stall_count),
            ".0f",
            lower_is_better=True,
        ),
    ]


def format_report(baseline_dir: Path, current_dir: Path, rows: list[DiffRow]) -> str:
    """人类可读 markdown 表格"""
    lines = [
        "# Trace Eval Report",
        "",
        f"- **baseline**: `{baseline_dir}`",
        f"- **current**:  `{current_dir}`",
        "",
        "| 指标 | baseline | current | delta | 判定 |",
        "|------|----------|---------|-------|------|",
    ]
    for r in rows:
        lines.append(f"| `{r.name}` | {r.baseline_val} | {r.current_val} | {r.delta} | {r.verdict} |")
    lines.append("")
    return "\n".join(lines)


def has_regression(rows: list[DiffRow]) -> bool:
    """任一指标 ❌ 退化即视为有回归"""
    return any("❌" in r.verdict for r in rows)


def main(argv: Optional[list[str]] = None) -> int:
    parser = argparse.ArgumentParser(
        description="Trace-Driven Eval: baseline vs current NDJSON trace 对比",
        epilog="参考 docs/dna/trace-eval-metrics.md",
    )
    parser.add_argument(
        "--baseline",
        type=Path,
        required=True,
        help="baseline trace 目录(含 .ndjson 文件)",
    )
    parser.add_argument(
        "--current",
        type=Path,
        required=True,
        help="current trace 目录(含 .ndjson 文件)",
    )
    parser.add_argument(
        "--quiet",
        action="store_true",
        help="不打印收集警告(只输出最终 diff)",
    )

    args = parser.parse_args(argv)

    baseline = Metrics.from_dir(args.baseline)
    current = Metrics.from_dir(args.current)

    if not args.quiet:
        for m, label in [(baseline, "baseline"), (current, "current")]:
            for e in m.errors:
                print(f"[{label}] {e}", file=sys.stderr)

    rows = compare(baseline, current)
    print(format_report(args.baseline, args.current, rows))

    if has_regression(rows):
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
