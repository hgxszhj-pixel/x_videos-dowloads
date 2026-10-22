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
    # 被丢掉的 trace 文件分两类,严重程度完全不同:
    #   dropped_interrupted   —— 有 session_open 无 session_close = 用户 Ctrl-C,排除合理
    #   dropped_no_lifecycle  —— 两个都没有 = 生产端没写生命周期事件,丢它等于掩盖 bug
    # 只有后者会让 baseline 变得不可比(见 main 里的守卫)。
    dropped_interrupted: int = 0
    dropped_no_lifecycle: int = 0
    fallback_stalls: int = 0  # 从 fallback 文件并入 stall_count 的那部分,单独记以便披露
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
            session_open_seen = False
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

                        if step == "session_open":
                            session_open_seen = True
                        if step == "session_close":
                            session_close_seen = True
                            session_ok = output.get("status") == "ok"
                            session_total_ms = output.get("total_ms_proxy", 0) or 0
                        elif step == "resolver_try":
                            session_resolver_count += 1

                        # stall 判定**独立于上面的分类**,不能挂在同一条 elif 链上。
                        # 2026-10-22 FIX: 原来 resolver_try 那一支同时出现在
                        # `elif step == "resolver_try"` 和下一行 `elif step in (...)` 里,
                        # 前者先匹配就把它吃掉了,后者永远轮不到 —— 实测两个 99s 的
                        # resolver_try(阈值的 3 倍)报 stall_count=0,而 download_one /
                        # fetch_video_info 同样的 stall 能被抓到。
                        # 讽刺的是 resolver_try 恰恰是最容易 stall 的那一步
                        # (BUG-D 就是串行 resolver 链把会话挂住),它却是唯一被漏掉的。
                        if (
                            step in ("resolver_try", "download_one", "fetch_video_info")
                            and duration_ms >= STALL_THRESHOLD_MS
                        ):
                            session_stall += 1
            except OSError as e:
                m.errors.append(f"{f.name}: 读文件失败 {e}")
                continue

            if not session_close_seen:
                # 无 session_close = 该文件不计入分母。但**分两类**,不能都当"正常跳过":
                #   · 有 session_open 无 session_close = 用户 Ctrl-C / 进程被杀,排除合理
                #   · 连 session_open 都没有 = 生产端压根没写生命周期事件(实测
                #     baseline-2026-10-12/n-2026-10-12.ndjson 就是这样:29 次 resolver_try
                #     + 3 次 download_one 全在这个文件里,session_id 还是空字符串)。
                #     把它连同几十个事件一起丢掉,却让剩下 4 个只含 open/close 的空跑
                #     GUI session 撑起 success_rate=1.000 —— 指标看着漂亮,分母是残的。
                if session_open_seen:
                    m.dropped_interrupted += 1
                    m.errors.append(f"{f.name}: 有 session_open 无 session_close(中断),跳过")
                else:
                    m.dropped_no_lifecycle += 1
                    # fallback 文件的 stall **仍然计入** —— stall_count 是绝对计数、
                    # 没有分母,收进来不改变任何现有指标的算法,已锁基线继续可比。
                    # 但 success_rate / resolver_chain_length / avg_total_ms 一律不算:
                    # 没有 session_close 就没有 status,任何"成功率"都是编出来的。
                    # (trace.rs 里 session_close 是唯一带 status 的终止事件)
                    m.stall_count += session_stall
                    m.fallback_stalls += session_stall
                    m.errors.append(
                        f"{f.name}: 无生命周期事件(fallback 路径),"
                        f"已并入 stall_count(+{session_stall}),其余指标不计"
                    )
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

    # 2026-10-22 FIX: 基线不可用时必须拒绝出判决。
    # 之前空基线 (目录存在但没有 .ndjson) 会让所有指标算成 0.000,而 0.000 对任何
    # current 都是"改善" —— 判定永远是 ✅,检测器在结构上不可能报警。真实发生过:
    # ~/.local/share/x_videos-dowloads-trace/2026-09-17 是空目录,而 --quiet 又把
    # "目录无 .ndjson 文件" 吞掉,于是 stop-trace-eval.sh 每天静默放行。
    # 这里把"无数据"和"数据好"分开: 基线没有可用 session 就不给判决。
    if baseline.total_sessions == 0:
        detail = "; ".join(baseline.errors[:3]) or "无可用 session(session_close)"
        print(
            f"❌ [baseline-unusable] 基线无可用数据,拒绝出判决: {args.baseline} ({detail})\n"
            "   修: 用 scripts/install_perf_baseline.sh 重新锁定基线",
            file=sys.stderr,
        )
        return 1

    # 分母残缺要如实披露,但**不能因此拒绝出判决**。
    # 踩过的坑: 2026-10-22 一度把"有文件连 session_open 都没有"做成 exit=1 的硬守卫,
    # 结果 install_perf_baseline.sh 按设计就会把 no-session fallback(n-*.ndjson)拷进
    # 基线 —— 于是任何有 CLI 下载的基线都天天误报,把"静默漏报"换成了"永久误报",
    # 比原来更糟。数据确实不完整,但那属于设计缺口,不该用一道会永远红的门来暴露。
    for label, m in (("baseline", baseline), ("current", current)):
        if m.dropped_no_lifecycle:
            print(
                f"⚠️  [{label}-partial] {m.dropped_no_lifecycle} 个 fallback trace 文件无生命周期"
                f"事件 —— 其 stall 已并入 stall_count(+{m.fallback_stalls}),但 success_rate /"
                f" resolver_chain_length / avg_total_ms 不含它们(无 session_close 即无 status)"
                + (f"　→ {label} 这三项指标的分母不完整" if label == "baseline" else "")
            )

    rows = compare(baseline, current)
    print(format_report(args.baseline, args.current, rows))

    if has_regression(rows):
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
