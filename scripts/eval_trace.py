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

stall 判定只看**叶子步骤**(STALL_ELIGIBLE_STEPS):聚合步骤不参与,
否则同一次慢解析会被叶子 + 聚合各数一遍,stall_count 虚高。

用法:
  python3 scripts/eval_trace.py \\
      --baseline traces/2026-09-14-baseline/ \\
      --current  traces/2026-09-17-current/

退出码:
  0 = 无退化(全部指标改善或持平)
  1 = 一般错误(目录不存在 / 文件解析失败 / 基线无可用 session)
  2 = 检测到退化(任一关键指标恶化)
  3 = 埋点版本不兼容(baseline 与 current 的 schema_version 不一致)⇒ **拒绝出判决**

  退出码 3 是 2026-10-30 新增的:本轮改了埋点语义(`resolver_try` 的 duration
  从「写死 0」变成真实耗时,新增 `cli_*` 步骤),旧基线与新 trace 的 stall_count
  口径不同 —— delta 既可能是「性能变差」也可能是「测得更准」,光看数字分不出来。
  这条规则之前只写在文档里,**没有任何东西检查它** ⇒ hook 会报一个假的「退化」。
  现在版本号写进每个 session 的事件,版本对不上就拒绝出判决。

参考: docs/dna/trace-eval-metrics.md | docs/dna/trace-schema.md
"""

import argparse
import json
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Optional


STALL_THRESHOLD_MS = 30_000  # ≥ 30s 算 stall(BUG-D / BUG-E 经验值)

# 参与 stall 判定的 step 白名单 —— **只放叶子步骤**。
#
# 为什么要白名单而不是「任何 duration ≥ 阈值的事件都算」:
# 聚合步骤(`cli_resolve_done` = 整个解析阶段、`cli_download_run_done` = 整次 CLI
# 下载)覆盖了叶子步骤的时间区间。同一次慢解析会同时命中
# `cli_resolve_attempt_done`(叶子,60s)和 `cli_resolve_done`(聚合,60s),
# 计两次 stall = 同一次墙钟耗时被数了两遍 —— stall_count 会虚高,
# 发版对比时把「测得更准」误判成「性能退化」。
#
# 2026-10-30 新增的叶子步骤:
#   · resolver_try_done        —— GUI 侧单次解析尝试(API / Drission),真实耗时
#   · cli_resolve_attempt_done —— CLI 侧单个后端尝试(fxtwitter/ytdlp/registry/…),真实耗时
# 旧步骤保留在白名单里是为了兼容**历史 trace 文件**(旧文件的 `resolver_try`
# 可能带真实耗时);新埋点的 `resolver_try` 是开始标记(duration 恒 0),永远命不中。
STALL_ELIGIBLE_STEPS = frozenset(
    {
        "resolver_try",  # 兼容旧 trace(新埋点里是 marker,duration 恒 0)
        "resolver_try_done",  # GUI:单次解析尝试(新)
        "cli_resolve_attempt_done",  # CLI:单个后端尝试(新)
        "download_one",  # GUI:单个课节下载
        "fetch_video_info",  # GUI:单个课节信息获取
    }
)

# 聚合步骤 —— 明确**不**参与 stall 判定(理由见 STALL_ELIGIBLE_STEPS 注释)。
# 列出它们是为了让「为什么这个 step 不算 stall」在代码里可查,而不是靠读者猜。
#
# marker 半边(`cli_resolve` / `cli_download_run`)也列进来:它们的 duration 恒 0,
# 本来就命不中阈值,列出来是为了让「一个聚合 stage = marker + done」的对应关系可查。
AGGREGATE_STEPS = frozenset(
    {
        "cli_resolve",  # CLI 解析阶段开始(marker,duration 恒 0)
        "cli_resolve_done",  # CLI 解析阶段总计
        "cli_download_run",  # CLI 整次运行开始(marker,duration 恒 0)
        "cli_download_run_done",  # CLI 整次运行总计
    }
)


@dataclass
class Metrics:
    """单个 trace 目录聚合出的指标集"""

    total_sessions: int = 0
    ok_sessions: int = 0
    success_rate: float = 0.0
    # avg_total_ms 读的是 session_close.output["total_ms_proxy"]。
    # 2026-10-22 实测:这个 key 在整个 src/ 里**一次都没被写过**(只在 eval_trace.py
    # 自己这里出现),session_close 的 output 只有 status ⇒ 这一项对任何 trace 恒为 0。
    # 2026-10-30 修复:src/core/trace.rs 现在真的写了它 —— 口径 = session_open 到
    # session_close 的**墙钟经过时间**(含全部等待;不用叶子步骤 duration 之和,
    # 那会把没埋点的等待漏掉,而「转圈但没速度」恰恰就是那部分)。
    # 旧基线仍然没有这个 key ⇒ 对旧基线这项指标仍然是「⚪ 不可测」,不会假装有值。
    avg_total_ms: int = 0
    # 有多少个 session_close 真的带了 total_ms_proxy。0 = 这项指标**结构上测不了**,
    # 和「测出来是 0」是两回事 —— 见 main() 里的披露逻辑。
    # 它同时是 avg_total_ms 的**分母**(见 from_dir 聚合处):只有带了 key 的 session
    # 才进分子,拿 total_sessions 当分母会把「没测到」当成「耗时 0」摊进去。
    sessions_with_total_ms: int = 0
    # 本目录里出现过的埋点 schema 版本(来自 session_open/close 的 extra.schema_version)。
    schema_versions: set[str] = field(default_factory=set)
    # 有多少个 session 没能拿到 schema_version —— 埋点版本未知 ⇒ 与任何一侧都不可比。
    sessions_without_version: int = 0
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
            session_total_ms_seen = False  # key 存在 ≠ 值为 0,必须分开记
            session_resolver_count = 0
            session_stall = 0
            valid_lines = 0
            session_version: str | None = None  # 埋点 schema 版本(open 取不到就退到 close)

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
                            v = (ev.get("extra") or {}).get("schema_version")
                            if v is not None:
                                session_version = str(v)
                        if step == "session_close":
                            session_close_seen = True
                            session_ok = output.get("status") == "ok"
                            session_total_ms = output.get("total_ms_proxy", 0) or 0
                            # `in` 而不是取值判断:埋点压根不写这个 key 时,值自然是 0,
                            # 但「没测到」和「测出来是 0」在报告里必须长得不一样。
                            session_total_ms_seen = "total_ms_proxy" in output
                            if session_version is None:
                                v = (ev.get("extra") or {}).get("schema_version")
                                if v is not None:
                                    session_version = str(v)
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
                        #
                        # 2026-10-30: 旧 `resolver_try` 的 duration 是**写死的 0**
                        # (它只是个开始标记),所以哪怕把它加进白名单也永远不会命中。
                        # 真正的耗时由新埋点 `resolver_try_done` / `cli_resolve_attempt_done`
                        # 携带,这两个叶子步骤已加入 STALL_ELIGIBLE_STEPS。
                        if (
                            step in STALL_ELIGIBLE_STEPS
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
            if session_total_ms_seen:
                m.sessions_with_total_ms += 1
            if session_ok:
                m.ok_sessions += 1
            total_ms_sum += session_total_ms
            resolver_try_count += session_resolver_count
            m.stall_count += session_stall
            # 埋点版本:只有**计入分母的 session** 才参与版本判定 ——
            # 被丢弃的 fallback 文件没有 session_open,版本未知是正常的,
            # 不该让整次比较变成「不可比」。
            if session_version is None:
                m.sessions_without_version += 1
            else:
                m.schema_versions.add(session_version)

        # 聚合
        if m.total_sessions > 0:
            m.success_rate = m.ok_sessions / m.total_sessions
            # 分母用 sessions_with_total_ms 而不是 total_sessions:
            # 只有带了 total_ms_proxy 的 session 才进了分子,拿全部 session 当分母
            # 会把「没测到」摊成「耗时 0」,系统性低报(H3:分母必须可复算)。
            if m.sessions_with_total_ms > 0:
                m.avg_total_ms = total_ms_sum // m.sessions_with_total_ms
            m.resolver_chain_length = resolver_try_count / m.total_sessions

        return m


@dataclass
class DiffRow:
    """单指标 baseline vs current 对比"""

    name: str
    baseline_val: str
    current_val: str
    delta: str
    verdict: str  # ✅ / ⚠️ / ❌ / ⚪
    measurable: bool = True

    @classmethod
    def unmeasurable(cls, name: str, why: str) -> "DiffRow":
        """指标**结构上测不了**时用它,不要拿 0 冒充测量结果。

        Why: 一行 `avg_total_ms | 0 | 0 | 0 | ✅持平` 看起来像「耗时指标很健康」,
        实际是埋点从没写过那个 key。这和 stop-subagent-rate.sh 恒绿、eval_trace 空基线
        恒绿是同一款病:**看起来像安全网的死指标,比没有更危险。**
        """
        return cls(
            name=name,
            baseline_val="n/a",
            current_val="n/a",
            delta="n/a",
            verdict=f"⚪ 不可测 — {why}",
            measurable=False,
        )

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


def check_version_compatible(baseline: Metrics, current: Metrics) -> tuple[bool, str]:
    """埋点 schema 版本必须一致,否则**拒绝出判决**。

    Why(2026-10-30):本轮改了埋点语义 —— `resolver_try` 的 duration 从「写死 0」
    变成真实耗时(由 `resolver_try_done` 携带),并新增了一批 `cli_*` 步骤。
    于是**旧 baseline 与新 current 的 stall_count 口径不同**:旧 baseline 里
    resolver 永远不可能被判成 stall(阈值 30s 而 duration 恒 0),新 current 里会。
    同一个 delta 既可能是「性能变差」也可能是「测得更准」,光看数字分不出来。

    这条规则之前只写在 `docs/dna/trace-eval-metrics.md` 里,**没有任何东西检查它**
    ⇒ 下一次拿旧基线比新 trace,stop-trace-eval hook 会报一个假的「退化」。
    现在版本号写进每个 session 的事件里,由本函数机器强制。

    判定:
      · current 侧没有 session → **不做判定**(见下)
      · 任一侧有 session 缺版本 → 埋点版本未知 → 不可比
      · 两侧版本集合不同 → 不可比

    **current 侧没有 session 时必须放过**(2026-10-30 差点踩的坑):
    只用了 CLI(没开 GUI session)时,今天的 trace 只落在 `no-session/` fallback 里
    ⇒ `current.total_sessions == 0` 是**常态**,不是异常。此时 current 的版本集合
    自然是空的,若照常比对就会判「版本不一致」⇒ **永久误报**。
    那正是 2026-10-22 把「静默漏报」换成「永久误报」的老错误:数据确实不完整,
    但那属于设计缺口,不该用一道会永远红的门来暴露。
    """
    if current.total_sessions == 0:
        # 没有可比对象 → 版本无从比对。main() 会另外披露「current 无可用 session」。
        return True, ""
    if baseline.sessions_without_version or current.sessions_without_version:
        return False, (
            f"有 session 缺 schema_version"
            f"(baseline {baseline.sessions_without_version} 个,"
            f"current {current.sessions_without_version} 个)—— 埋点版本未知,不可比"
        )
    if baseline.schema_versions != current.schema_versions:
        return False, (
            f"埋点版本不一致: baseline={sorted(baseline.schema_versions) or ['(无)']}"
            f" vs current={sorted(current.schema_versions) or ['(无)']}"
        )
    return True, ""


def compare(baseline: Metrics, current: Metrics) -> list[DiffRow]:
    """生成 4 指标 baseline vs current 对比行"""
    # avg_total_ms 只有**两侧都真的测到了**才可比。
    # 任一侧为 0 ⇒ 那一侧压根没埋点 ⇒ 显示「⚪ 不可测」,而不是拿 0 冒充测量结果。
    total_ms_unmeasurable = (
        baseline.sessions_with_total_ms == 0 or current.sessions_with_total_ms == 0
    )
    total_ms_reason = (
        "session_close 不带 total_ms_proxy"
        f"(baseline {baseline.sessions_with_total_ms}/{baseline.total_sessions} 个 session 有,"
        f" current {current.sessions_with_total_ms}/{current.total_sessions} 个)"
    )
    return [
        DiffRow.make(
            "success_rate",
            baseline.success_rate,
            current.success_rate,
            ".3f",
            lower_is_better=False,
        ),
        DiffRow.unmeasurable("avg_total_ms", total_ms_reason)
        if total_ms_unmeasurable
        else DiffRow.make(
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

    # 埋点版本必须一致(2026-10-30 新增)。
    # 拒绝出判决而不是「照比不误」:旧埋点的 baseline 与新埋点的 current 放一起,
    # stall_count 的 delta 分不清是「性能变差」还是「测得更准」。
    # 退出码用 3(与 1=基线不可用 / 2=检测到退化 区分开),让调用方能分别处理。
    version_ok, version_why = check_version_compatible(baseline, current)
    if not version_ok:
        print(
            f"⛔ [schema-mismatch] 拒绝出判决: {version_why}\n"
            "   修: 用**同一套埋点版本**的数据重锁基线 —— "
            "bash scripts/install_perf_baseline.sh <日期> <标签>\n"
            "   (说明:本次换了埋点语义,旧基线的 stall 口径与新 trace 不同,delta 无意义)",
            file=sys.stderr,
        )
        return 3

    # current 侧没有可用 session 时必须披露,但**不因此拒绝出判决**。
    # 2026-10-22 教训:只用了 CLI(没有 GUI session)时,今天的 trace 只落在
    # no-session fallback 里 ⇒ current.total_sessions == 0。这是常态不是异常,
    # 做成 exit=1 的硬守卫就是「把静默漏报换成永久误报」,比原来更糟。
    # 所以只如实说明「这批指标没有分母」,让人别把它读成「没有退化」。
    if current.total_sessions == 0:
        print(
            "⚠️  [current-no-sessions] current 侧没有可用 session(只有 fallback 文件或空)—— "
            "下面的指标没有分母,**不可读作「没有退化」**"
        )

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

    # 死指标必须在**判决之前**说明白,不能只让表格里出现一个看起来正常的 0。
    for r in rows:
        if not r.measurable:
            print(
                f"⚠️  [{r.name}-dead] 该指标结构上测不了:{r.verdict.split('—', 1)[1].strip()}"
                f" —— 表格里的 n/a 不是「持平」,是**没埋点**。别把它读成耗时正常。",
                file=sys.stderr,
            )

    print(format_report(args.baseline, args.current, rows))

    if has_regression(rows):
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
