#!/usr/bin/env python3
"""
test_eval_trace.py — eval_trace.py 单测

≥5 case,覆盖核心计算 + diff + stall 检测 + 错误处理:
  1. 空目录 → errors + 零指标
  2. 单文件 100% 成功
  3. 多文件平均(resolver_chain_length)
  4. baseline vs current diff:改善 + 退化两种方向
  5. stall 检测(≥30s)
  6. has_regression 判定

NDJSON fixture 格式与 src/core/trace.rs 实际产出保持一致:
  session_open → fetch_video_info → fetch_video_info_ok → resolver_try×N → download_one → session_close

参考 docs/dna/trace-eval-metrics.md
"""

import sys
import tempfile
import unittest
from pathlib import Path

# 让脚本能找到 eval_trace.py
SCRIPTS_DIR = Path(__file__).resolve().parent.parent / "scripts"
sys.path.insert(0, str(SCRIPTS_DIR))

from eval_trace import (  # noqa: E402
    AGGREGATE_STEPS,
    Metrics,
    STALL_ELIGIBLE_STEPS,
    STALL_THRESHOLD_MS,
    check_version_compatible,
    compare,
    format_report,
    has_regression,
    main,
)


def write_ndjson(path: Path, events: list[dict]) -> None:
    """fixture 辅助:写 NDJSON 文件"""
    with path.open("w", encoding="utf-8") as f:
        for ev in events:
            f.write(__import__("json").dumps(ev) + "\n")


def make_session(
    session_id: str,
    status: str = "ok",
    total_ms_proxy: int = 5000,
    resolver_count: int = 2,
    stall_steps: tuple[tuple[str, int], ...] = (),
    schema_version: str | None = "2",
) -> list[dict]:
    """fixture 辅助:构造一个完整 session 的 NDJSON 事件列表

    `schema_version` 对应 src/core/trace.rs 的 `TRACE_SCHEMA_VERSION`(写进
    session_open / session_close 的 `extra.schema_version`)。传 None 模拟
    **旧埋点**——那时还没有这个字段,版本未知 ⇒ 与任何一侧都不可比。
    """
    version_extra = {} if schema_version is None else {"schema_version": schema_version}
    events = [
        {
            "step": "session_open",
            "session_id": session_id,
            "duration_ms": 0,
            "output": {},
            "extra": dict(version_extra),
        },
        {
            "step": "fetch_video_info",
            "session_id": session_id,
            "duration_ms": 100,
            "input": {"url": "https://example.com"},
            "output": {},
            "extra": {},
        },
        {
            "step": "fetch_video_info_ok",
            "session_id": session_id,
            "duration_ms": 100,
            "input": {"url": "https://example.com"},
            "output": {"status": "ok", "video_count": 1},
            "extra": {},
        },
    ]
    for i in range(resolver_count):
        events.append(
            {
                "step": "resolver_try",
                "session_id": session_id,
                "duration_ms": 0,
                "input": {"idx": i, "title": f"章节{i + 1}"},
                "output": {"selected": True},
                "extra": {"resolver_chain_length": resolver_count},
            }
        )
    for step, ms in stall_steps:
        events.append(
            {
                "step": step,
                "session_id": session_id,
                "duration_ms": ms,
                "input": {"title": "stall"},
                "output": {},
                "extra": {},
            }
        )
    events.append(
        {
            "step": "session_close",
            "session_id": session_id,
            "duration_ms": 0,
            "input": {},
            "output": {
                "status": status,
                "total_ms_proxy": total_ms_proxy,
            },
            "extra": dict(version_extra),
        }
    )
    return events


class TestEmptyDir(unittest.TestCase):
    """Case 1: 空目录 → errors + 零指标,不崩溃"""

    def test_empty_dir_returns_zero_metrics_with_errors(self):
        with tempfile.TemporaryDirectory() as tmp:
            m = Metrics.from_dir(Path(tmp))
            self.assertEqual(m.total_sessions, 0)
            self.assertEqual(m.success_rate, 0.0)
            self.assertEqual(m.resolver_chain_length, 0.0)
            self.assertEqual(m.stall_count, 0)
            self.assertTrue(any("无 .ndjson" in e for e in m.errors))


class TestSingleFileSuccess(unittest.TestCase):
    """Case 2: 单文件 100% 成功"""

    def test_single_session_ok_metrics(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            write_ndjson(tmp_path / "sess1.ndjson", make_session("s1", status="ok"))
            m = Metrics.from_dir(tmp_path)
            self.assertEqual(m.total_sessions, 1)
            self.assertEqual(m.ok_sessions, 1)
            self.assertEqual(m.success_rate, 1.0)
            self.assertEqual(m.resolver_chain_length, 2.0)
            self.assertEqual(m.avg_total_ms, 5000)
            self.assertEqual(m.stall_count, 0)


class TestMultiFileAverage(unittest.TestCase):
    """Case 3: 多文件平均(resolver_chain_length + success_rate 部分失败)"""

    def test_two_sessions_one_ok_one_fail(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            write_ndjson(tmp_path / "s1.ndjson", make_session("s1", status="ok", resolver_count=2))
            write_ndjson(tmp_path / "s2.ndjson", make_session("s2", status="err", resolver_count=4))
            m = Metrics.from_dir(tmp_path)
            self.assertEqual(m.total_sessions, 2)
            self.assertEqual(m.ok_sessions, 1)
            self.assertEqual(m.success_rate, 0.5)
            # 平均 resolver 次数 = (2 + 4) / 2 = 3.0
            self.assertEqual(m.resolver_chain_length, 3.0)


class TestBaselineVsCurrentDiff(unittest.TestCase):
    """Case 4: baseline vs current diff — 改善 + 退化两种方向"""

    def test_improvement_all_metrics(self):
        with tempfile.TemporaryDirectory() as tmp:
            b_dir = Path(tmp) / "baseline"
            c_dir = Path(tmp) / "current"
            b_dir.mkdir()
            c_dir.mkdir()

            # baseline: success_rate 0.5, chain_length 4, stall 2
            write_ndjson(
                b_dir / "b1.ndjson",
                make_session("b1", status="ok", resolver_count=2),
            )
            write_ndjson(
                b_dir / "b2.ndjson",
                make_session("b2", status="err", resolver_count=6, stall_steps=(("download_one", 35000),)),
            )
            # current: success_rate 1.0, chain_length 1, stall 0
            write_ndjson(
                c_dir / "c1.ndjson",
                make_session("c1", status="ok", resolver_count=1),
            )
            write_ndjson(
                c_dir / "c2.ndjson",
                make_session("c2", status="ok", resolver_count=1),
            )

            b = Metrics.from_dir(b_dir)
            c = Metrics.from_dir(c_dir)
            rows = compare(b, c)

            row_by_name = {r.name: r for r in rows}
            self.assertIn("✅", row_by_name["success_rate"].verdict)
            self.assertIn("✅", row_by_name["resolver_chain_length"].verdict)
            self.assertIn("✅", row_by_name["stall_count"].verdict)
            self.assertFalse(has_regression(rows))

    def test_regression_success_rate_drops(self):
        with tempfile.TemporaryDirectory() as tmp:
            b_dir = Path(tmp) / "baseline"
            c_dir = Path(tmp) / "current"
            b_dir.mkdir()
            c_dir.mkdir()

            # baseline: 全成功
            for i in range(3):
                write_ndjson(b_dir / f"b{i}.ndjson", make_session(f"b{i}", status="ok"))
            # current: 全失败
            for i in range(3):
                write_ndjson(c_dir / f"c{i}.ndjson", make_session(f"c{i}", status="err"))

            b = Metrics.from_dir(b_dir)
            c = Metrics.from_dir(c_dir)
            rows = compare(b, c)
            row_by_name = {r.name: r for r in rows}
            self.assertIn("❌", row_by_name["success_rate"].verdict)
            self.assertTrue(has_regression(rows))


class TestStallDetection(unittest.TestCase):
    """Case 5: stall ≥ 30s 检测"""

    def test_one_download_stall_counted(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            write_ndjson(
                tmp_path / "s1.ndjson",
                make_session(
                    "s1",
                    status="ok",
                    stall_steps=(("download_one", 35000),),
                ),
            )
            m = Metrics.from_dir(tmp_path)
            self.assertEqual(m.stall_count, 1)

    def test_under_threshold_not_counted(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            write_ndjson(
                tmp_path / "s1.ndjson",
                make_session(
                    "s1",
                    status="ok",
                    stall_steps=(("download_one", STALL_THRESHOLD_MS - 1),),
                ),
            )
            m = Metrics.from_dir(tmp_path)
            self.assertEqual(m.stall_count, 0)


class TestNewStepStallEligibility(unittest.TestCase):
    """Case 7(2026-10-30): 新增叶子步骤必须能计成 stall,聚合步骤必须**不**计。

    这组 case 是「eval 侧改动是否真的生效」的牙齿证明:
    旧 eval_trace.py 的白名单是硬编码三元组
    `("resolver_try", "download_one", "fetch_video_info")`,
    所以下面前两条在旧代码上必然 stall_count == 0 → 必红。
    """

    def test_resolver_try_done_stall_counted(self):
        """GUI 单次解析尝试卡住 ≥30s → 必须被计成 stall(BUG-D 那一步)。"""
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            write_ndjson(
                tmp_path / "s1.ndjson",
                make_session(
                    "s1",
                    status="ok",
                    stall_steps=(("resolver_try_done", 99_000),),
                ),
            )
            m = Metrics.from_dir(tmp_path)
            self.assertEqual(m.stall_count, 1)

    def test_cli_resolve_attempt_done_stall_counted(self):
        """CLI 单个后端尝试卡住 ≥30s → 必须被计成 stall。"""
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            write_ndjson(
                tmp_path / "s1.ndjson",
                make_session(
                    "s1",
                    status="ok",
                    stall_steps=(("cli_resolve_attempt_done", 45_000),),
                ),
            )
            m = Metrics.from_dir(tmp_path)
            self.assertEqual(m.stall_count, 1)

    def test_legacy_resolver_try_with_real_duration_still_counted(self):
        """旧 trace 文件里 resolver_try 可能带真实耗时 → 必须仍然命中(向后兼容)。"""
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            write_ndjson(
                tmp_path / "s1.ndjson",
                make_session(
                    "s1",
                    status="ok",
                    stall_steps=(("resolver_try", 99_000),),
                ),
            )
            m = Metrics.from_dir(tmp_path)
            self.assertEqual(m.stall_count, 1)

    def test_aggregate_steps_not_counted_as_stall(self):
        """聚合步骤(cli_resolve_done / cli_download_run_done)不算 stall —— 防重复计数。

        同一次慢解析会同时命中叶子(`cli_resolve_attempt_done`)和聚合
        (`cli_resolve_done`);两者都计 = 同一段墙钟耗时被数两遍。
        """
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            write_ndjson(
                tmp_path / "s1.ndjson",
                make_session(
                    "s1",
                    status="ok",
                    stall_steps=(
                        ("cli_resolve_done", 60_000),
                        ("cli_download_run_done", 600_000),
                    ),
                ),
            )
            m = Metrics.from_dir(tmp_path)
            self.assertEqual(m.stall_count, 0)

    def test_leaf_and_aggregate_not_double_counted(self):
        """一次慢解析 = 1 次 stall(叶子),不是 2 次(叶子+聚合)。"""
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            write_ndjson(
                tmp_path / "s1.ndjson",
                make_session(
                    "s1",
                    status="ok",
                    stall_steps=(
                        ("cli_resolve_attempt_done", 60_000),
                        ("cli_resolve_done", 60_000),
                    ),
                ),
            )
            m = Metrics.from_dir(tmp_path)
            self.assertEqual(m.stall_count, 1)

    def test_new_steps_do_not_change_resolver_chain_length(self):
        """指标口径不变:`resolver_chain_length` 仍只按 `resolver_try` 事件数算。

        新埋点多了 `resolver_try_done` 事件,但**不能**让它进入链长指标 ——
        否则同一个指标名会同时表示「课节数」和「尝试数」,基线直接失效。
        """
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            write_ndjson(
                tmp_path / "s1.ndjson",
                make_session(
                    "s1",
                    status="ok",
                    resolver_count=3,
                    stall_steps=(("resolver_try_done", 99_000),),
                ),
            )
            m = Metrics.from_dir(tmp_path)
            # 3 个 resolver_try marker / 1 个 session = 3.0(done 事件不参与)
            self.assertEqual(m.resolver_chain_length, 3.0)

    def test_step_sets_are_disjoint(self):
        """白名单与聚合名单不得重叠 —— 重叠就意味着某个 step 又被算了两次。"""
        self.assertEqual(STALL_ELIGIBLE_STEPS & AGGREGATE_STEPS, frozenset())

    def test_new_leaf_steps_registered(self):
        """两个新叶子步骤必须在白名单里(防止以后有人误删)。"""
        self.assertIn("resolver_try_done", STALL_ELIGIBLE_STEPS)
        self.assertIn("cli_resolve_attempt_done", STALL_ELIGIBLE_STEPS)


class TestWhitelistSync(unittest.TestCase):
    """Case 8(2026-10-30):两个脚本的 stall 白名单必须一致。

    `heuristic_judge_trace.py` 的契约是「不读 eval_trace.py」(保持独立可跑),
    代价是白名单有**两份定义**。忘了同步 ⇒ 同一份 trace 在两个工具里结论不同
    (eval 说 stall=1,heuristic 说 stall=0),而且没人会发现。

    变异:从 heuristic_judge_trace.py 的白名单里删掉 `resolver_try_done`
         → 本 case 必红。
    """

    def test_heuristic_judge_stall_whitelist_matches_eval(self):
        import heuristic_judge_trace

        self.assertEqual(
            heuristic_judge_trace.STALL_ELIGIBLE_STEPS,
            STALL_ELIGIBLE_STEPS,
            "heuristic_judge_trace.py 与 eval_trace.py 的 stall 白名单必须一致",
        )

    def test_aggregate_markers_not_stall_eligible(self):
        """marker(step = `<stage>`)永远不该进 stall 白名单。

        聚合 stage 的 marker 半边(`cli_resolve` / `cli_download_run`)列在
        AGGREGATE_STEPS 里;叶子 stage 的 marker 半边(`cli_resolve_attempt`)
        两边都不列 —— 它既不是聚合(duration 恒 0),也不该计 stall。
        """
        for marker in ("cli_resolve", "cli_download_run", "cli_resolve_attempt"):
            self.assertNotIn(marker, STALL_ELIGIBLE_STEPS)
        for aggregate_marker in ("cli_resolve", "cli_download_run"):
            self.assertIn(aggregate_marker, AGGREGATE_STEPS)


class TestReportFormat(unittest.TestCase):
    """Case 6: format_report 输出 markdown 表格"""

    def test_report_contains_table(self):
        b = Metrics(total_sessions=2, ok_sessions=1, success_rate=0.5, avg_total_ms=5000, resolver_chain_length=3.0, stall_count=1)
        c = Metrics(total_sessions=2, ok_sessions=2, success_rate=1.0, avg_total_ms=3000, resolver_chain_length=1.0, stall_count=0)
        rows = compare(b, c)
        report = format_report(Path("/b"), Path("/c"), rows)
        self.assertIn("| 指标 |", report)
        self.assertIn("`success_rate`", report)
        self.assertIn("`stall_count`", report)
        # baseline=0.5 current=1.0 → 改善 ✅
        self.assertIn("✅", report)
        # baseline=3.0 current=1.0 → 改善 ✅
        # baseline=1 current=0 → 改善 ✅


class TestSchemaVersionGuard(unittest.TestCase):
    """埋点版本必须一致,否则拒绝出判决(2026-10-30 新增)。

    Why: 本轮改了埋点语义(`resolver_try` 的 duration 从写死 0 变成真实耗时,
    新增 `cli_*` 步骤)。旧 baseline 与新 current 的 stall_count 口径不同 ——
    delta 既可能是「性能变差」也可能是「测得更准」。这条规则之前只写在文档里,
    **没有任何东西检查它** ⇒ hook 会报一个假的「退化」。

    变异点:把 `check_version_compatible` 改成恒返回 True(或 main() 里删掉那个
    `if not version_ok` 分支)→ 本 class 的 mismatch 用例必红。
    """

    def _write_dir(self, tmp: str, name: str, version: str | None) -> Path:
        d = Path(tmp) / name
        d.mkdir()
        write_ndjson(d / "s.ndjson", make_session("s1", schema_version=version))
        return d

    def test_same_version_is_compatible(self):
        with tempfile.TemporaryDirectory() as tmp:
            b = self._write_dir(tmp, "b", "2")
            c = self._write_dir(tmp, "c", "2")
            mb, mc = Metrics.from_dir(b), Metrics.from_dir(c)
            ok, why = check_version_compatible(mb, mc)
            self.assertTrue(ok, f"同版本应当可比,实得 {why}")
            self.assertEqual(mb.schema_versions, {"2"})
            self.assertEqual(mc.schema_versions, {"2"})

    def test_different_version_is_incompatible(self):
        with tempfile.TemporaryDirectory() as tmp:
            b = self._write_dir(tmp, "b", "2")
            c = self._write_dir(tmp, "c", "1")
            ok, why = check_version_compatible(Metrics.from_dir(b), Metrics.from_dir(c))
            self.assertFalse(ok, "版本不一致必须判为不可比")
            self.assertIn("版本不一致", why)

    def test_missing_version_is_incompatible(self):
        """旧埋点(没有 schema_version)与任何一侧都不可比。"""
        with tempfile.TemporaryDirectory() as tmp:
            b = self._write_dir(tmp, "b", None)
            c = self._write_dir(tmp, "c", "2")
            ok, why = check_version_compatible(Metrics.from_dir(b), Metrics.from_dir(c))
            self.assertFalse(ok, "版本未知必须判为不可比")
            self.assertIn("缺 schema_version", why)

    def test_main_returns_3_on_version_mismatch(self):
        """main() 必须用**独立的退出码 3** 表示「版本不兼容」。

        为什么要独立于 1/2:1 = 基线坏(要重锁),2 = 真的检测到退化(要处理),
        3 = 埋点换了(要重锁)。混在一起调用方就没法分别处置。
        """
        with tempfile.TemporaryDirectory() as tmp:
            b = self._write_dir(tmp, "b", "2")
            c = self._write_dir(tmp, "c", "1")
            rc = main(["--baseline", str(b), "--current", str(c), "--quiet"])
            self.assertEqual(rc, 3, "版本不兼容必须 exit 3")

    def test_main_returns_0_when_versions_match(self):
        with tempfile.TemporaryDirectory() as tmp:
            b = self._write_dir(tmp, "b", "2")
            c = self._write_dir(tmp, "c", "2")
            rc = main(["--baseline", str(b), "--current", str(c), "--quiet"])
            self.assertEqual(rc, 0, "同版本应正常出判决")

    def test_current_without_sessions_is_not_a_false_alarm(self):
        """current 侧没有 session 时**不得**判成「版本不兼容」,也**不得**判成「退化」。

        Why: 只用了 CLI(没开 GUI session)时,今天的 trace 只落在 no-session/
        fallback 里 ⇒ current.total_sessions == 0 是**常态**。此时 current 的
        版本集合自然是空的,照常比对就会得到「版本不一致」⇒ **永久误报** ——
        那正是 2026-10-22 把「静默漏报」换成「永久误报」的老错误。

        变异点 1:把 `check_version_compatible` 里的 `if current.total_sessions == 0:
        return True, ""` 删掉 → 本 case 必红(rc 变成 3)。
        变异点 2:把 `compare()` 里的 `if current.total_sessions == 0:` 提前返回
        整段删掉 → 本 case 必红(rc 变成 2,报告里出现 ❌ 退化)。
        """
        with tempfile.TemporaryDirectory() as tmp:
            b = self._write_dir(tmp, "b", "2")
            # current 目录只有一个没有生命周期事件的 fallback 文件
            c = Path(tmp) / "c"
            c.mkdir()
            write_ndjson(c / "n.ndjson", [{"step": "resolver_try", "duration_ms": 10}])
            rc = main(["--baseline", str(b), "--current", str(c), "--quiet"])
            self.assertNotEqual(
                rc, 3, "current 无 session 是常态,不能判成版本不兼容(会永久误报)"
            )
            self.assertNotEqual(
                rc, 2, "current 无 session 时不得判成「退化」—— 没有分母,不是「变差了」"
            )
            self.assertEqual(rc, 4, "没有分母 ⇒ 未出判决,应返回独立的 4(不是 0/2)")


class TestNoVerdictWhenCurrentHasNoSessions(unittest.TestCase):
    """current 侧没有可用 session ⇒ 4 个指标都没有分母 ⇒ **不出判决**。

    Why(2026-10-30 实测):旧实现照常比较,得到
    `success_rate | 1.000 | 0.000 | -1.000 | ❌ 退化` ⇒ rc=2 ⇒ stop-trace-eval
    每天报「trace regression detected」—— 而实际只是「今天没开 GUI session」。
    这就是 2026-10-22 踩过的「把静默漏报换成永久误报」,只是这次更隐蔽:
    版本检查(退出码 3)恰好把它挡住了,一旦用户重锁基线就会复现。

    H2(不可测 ≠ 零值):没有分母的指标必须显示「⚪ 不可测」,
    绝不能显示「❌ 退化」或「✅ 持平」。

    变异点:删掉 `compare()` 开头的 `if current.total_sessions == 0:` 提前返回
    ⇒ 本 class 全红。
    """

    def _baseline(self, tmp: str) -> Path:
        d = Path(tmp) / "base"
        d.mkdir()
        write_ndjson(d / "s.ndjson", make_session("s1"))
        return d

    def _current_empty(self, tmp: str) -> Path:
        d = Path(tmp) / "cur"
        d.mkdir()
        return d

    def test_all_metrics_unmeasurable(self):
        with tempfile.TemporaryDirectory() as tmp:
            rows = compare(Metrics.from_dir(self._baseline(tmp)), Metrics.from_dir(self._current_empty(tmp)))
            self.assertEqual(len(rows), 4)
            for r in rows:
                self.assertFalse(r.measurable, f"{r.name} 必须标为不可测")
                self.assertIn("⚪", r.verdict)
                self.assertNotIn("❌", r.verdict, f"{r.name} 不得判成退化")
                self.assertNotIn("✅", r.verdict, f"{r.name} 不得判成持平/改善")

    def test_no_regression_flag(self):
        with tempfile.TemporaryDirectory() as tmp:
            rows = compare(Metrics.from_dir(self._baseline(tmp)), Metrics.from_dir(self._current_empty(tmp)))
            self.assertFalse(has_regression(rows), "没有分母时不得报回归")

    def test_report_has_no_regression_verdict(self):
        """整份报告里不能出现 ❌ —— hook 只 grep 这个字符。"""
        with tempfile.TemporaryDirectory() as tmp:
            b, c = self._baseline(tmp), self._current_empty(tmp)
            report = format_report(b, c, compare(Metrics.from_dir(b), Metrics.from_dir(c)))
            self.assertNotIn("❌", report, f"报告里不能有 ❌:\n{report}")

    def test_main_exit_code_4(self):
        with tempfile.TemporaryDirectory() as tmp:
            b, c = self._baseline(tmp), self._current_empty(tmp)
            rc = main(["--baseline", str(b), "--current", str(c), "--quiet"])
            self.assertEqual(rc, 4, "没有分母 ⇒ 未出判决,必须用独立的 4")

    def test_realistic_cli_only_scenario(self):
        """真实场景:current 只有 no-session fallback 文件(无生命周期事件)。"""
        with tempfile.TemporaryDirectory() as tmp:
            b = self._baseline(tmp)
            c = Path(tmp) / "cur"
            c.mkdir()
            write_ndjson(c / "n.ndjson", [{"step": "resolver_try", "duration_ms": 10}])
            rc = main(["--baseline", str(b), "--current", str(c), "--quiet"])
            self.assertEqual(rc, 4, "只用 CLI 的常态路径必须是 4,不能是 2")


class TestAvgTotalMsDenominator(unittest.TestCase):
    """avg_total_ms 的分母必须是「真测到 total_ms_proxy 的 session 数」。

    Why:分子只累加了带 key 的 session,分母却用全部 session ⇒ 把「没测到」
    摊成「耗时 0」,系统性低报(H3:分母必须可复算)。
    2026-10-30 起 total_ms_proxy 真的被埋点了,这个偏差会第一次显形。

    变异点:把 from_dir 里的 `total_ms_sum // m.sessions_with_total_ms`
    改回 `// m.total_sessions` → 本 case 必红。
    """

    def test_denominator_counts_only_sessions_with_total_ms(self):
        with tempfile.TemporaryDirectory() as tmp:
            d = Path(tmp)
            # session A 测到了 8000ms
            write_ndjson(d / "a.ndjson", make_session("a", total_ms_proxy=8000))
            # session B 没测到(旧埋点:close 不带这个 key)
            events_b = make_session("b")
            for ev in events_b:
                if ev["step"] == "session_close":
                    ev["output"].pop("total_ms_proxy", None)
            write_ndjson(d / "b.ndjson", events_b)

            m = Metrics.from_dir(d)
            self.assertEqual(m.total_sessions, 2)
            self.assertEqual(m.sessions_with_total_ms, 1)
            self.assertEqual(
                m.avg_total_ms,
                8000,
                "分母必须是 1(只有测到的那个),不是 2 —— 否则 8000 被摊成 4000",
            )


if __name__ == "__main__":
    unittest.main()
