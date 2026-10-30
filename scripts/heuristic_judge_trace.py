#!/usr/bin/env python3
"""
heuristic_judge_trace.py — 可选的启发式 trace 质性打分(纯规则,零 LLM 依赖)

为什么是"可选":eval_trace.py 的 4 个结构化指标(success_rate / avg_total_ms /
resolver_chain_length / stall_count)对下载器场景已经足够;本脚本加一层 1-5 分
质性视角,供人工对比两个版本的 session 行为差异。**不**接入自动化评估。

4 维度启发式打分(与 LLM-as-a-Judge 1-5 标度对齐,便于将来切回 LLM 时直接映射):
  1. efficiency  — resolver 越少越好(0→5, >6→1)
  2. robust       — stall 越少越好(0→5, ≥4→1)
  3. readable     — 结构完整度(open + close + 核心事件全在→5)
  4. suspicious   — 反向:resolver/stall/缺失事件越多越可疑(基线 1,可疑到 5)

用法:
  python3 scripts/heuristic_judge_trace.py traces/2026-09-17-current/

输出:
  stdout — Markdown 评分表(session_id / efficiency/robust/readable/suspicious / 理由)
  exit 0 = 成功,1 = 一般错误,2 = 无 NDJSON

不接入主 pipeline 的契约:
  - 不读 eval_trace.py
  - 不写 PR 门槛 / CI 门禁
  - 不写 docs/dna/(避免污染项目主规范)
"""

import argparse
import json
import sys
from collections import defaultdict
from pathlib import Path
from typing import Optional


STALL_THRESHOLD_MS = 30_000  # 与 eval_trace.py 对齐

# 参与 stall 判定的叶子步骤 —— **必须与 eval_trace.py 的 STALL_ELIGIBLE_STEPS 一致**。
#
# 这里刻意不 import eval_trace(本脚本的契约是「不读 eval_trace.py」,保持独立可跑),
# 代价是两份定义需要同步。改一边就要改另一边。
#
# 为什么必须对齐:2026-10-30 新增了聚合事件 `cli_download_run_done`
# (整次 CLI 运行的耗时)和 `cli_resolve_done`(整个解析阶段的耗时)。
# 本脚本原来的 stall 判定是「任何 duration ≥ 阈值的事件都算」——
# 那样任何一次真实下载(必然 > 30s)都会算成 stall,robust 分数被凭空压低,
# 制造出「版本变慢」的假信号。对齐到叶子白名单后,聚合事件不再计入。
STALL_ELIGIBLE_STEPS = frozenset(
    {
        "resolver_try",  # 兼容旧 trace(新埋点里是 marker,duration 恒 0)
        "resolver_try_done",  # GUI:单次解析尝试
        "cli_resolve_attempt_done",  # CLI:单个后端尝试
        "download_one",  # GUI:单个课节下载
        "fetch_video_info",  # GUI:单个课节信息获取
        "hls_manifest_done",  # HLS:取播放清单
        "hls_keys_done",  # HLS:预取 AES key
        "hls_merge_done",  # HLS:合并分段
    }
)

# 「核心事件」判定用:命中任意一个即认为结构里有实质内容。
# 2026-10-30 补齐 CLI 侧的步骤 —— 否则 CLI session(只有 cli_resolve / cli_download_run)
# 会被判成「结构缺 1 项」,readable 分数白丢一档。
CORE_STEPS = frozenset(
    {
        "resolver_try",
        "resolver_try_done",
        "fetch_video_info",
        "download_one",
        "cli_resolve",
        "cli_resolve_done",
        "cli_resolve_attempt",
        "cli_resolve_attempt_done",
        "cli_download_run",
        "cli_download_run_done",
    }
)


def heuristic_judge(events: list[dict]) -> dict:
    """基于纯规则的 1-5 分质性评分(无网络、无 key、无外部依赖)"""
    steps = [ev.get("step", "") for ev in events]
    has_open = "session_open" in steps
    has_close = "session_close" in steps
    has_core = any(s in CORE_STEPS for s in steps)

    resolver_tries = sum(1 for s in steps if s == "resolver_try")
    # stall 只看叶子步骤(与 eval_trace.py 对齐,理由见 STALL_ELIGIBLE_STEPS 注释)
    stall_count = sum(
        1
        for ev in events
        if ev.get("step", "") in STALL_ELIGIBLE_STEPS
        and (ev.get("duration_ms", 0) or 0) >= STALL_THRESHOLD_MS
    )

    # efficiency: resolver 链长度
    if resolver_tries == 0:
        efficiency = 5
    elif resolver_tries <= 2:
        efficiency = 4
    elif resolver_tries <= 4:
        efficiency = 3
    elif resolver_tries <= 6:
        efficiency = 2
    else:
        efficiency = 1

    # robust: stall 次数
    if stall_count == 0:
        robust = 5
    elif stall_count == 1:
        robust = 4
    elif stall_count == 2:
        robust = 3
    elif stall_count == 3:
        robust = 2
    else:
        robust = 1

    # readable: 结构完整度(0/1/2+ 缺失 → 5/3/1)
    missing = (not has_open) + (not has_close) + (not has_core)
    readable = 5 if missing == 0 else (3 if missing == 1 else 1)

    # suspicious: 异常信号累加(基线 1,每个信号 +1,封顶 5)
    abnormal = 0
    if resolver_tries > 6:
        abnormal += 2
    elif resolver_tries > 4:
        abnormal += 1
    if stall_count >= 4:
        abnormal += 2
    elif stall_count >= 2:
        abnormal += 1
    abnormal += missing  # 缺 1 个核心事件 +1;2 个 + 也 +2;3 个 +3
    suspicious = min(5, 1 + abnormal)

    # 一句话理由(<=30 字,中文)
    parts = []
    if resolver_tries > 4:
        parts.append(f"{resolver_tries}次resolver")
    if stall_count >= 2:
        parts.append(f"{stall_count}次stall")
    if missing == 1:
        parts.append("结构缺1项")
    elif missing >= 2:
        parts.append("结构残缺")
    reason = ";".join(parts) if parts else "正常"

    return {
        "efficiency": efficiency,
        "robust": robust,
        "readable": readable,
        "suspicious": suspicious,
        "reason": reason[:30],
    }


def main(argv: Optional[list[str]] = None) -> int:
    p = argparse.ArgumentParser(description="启发式 trace 质性打分(纯规则,无 LLM)")
    p.add_argument("trace_dir", type=Path, help="含 .ndjson 的目录")
    args = p.parse_args(argv)

    ndjsons = sorted(args.trace_dir.glob("*.ndjson"))
    if not ndjsons:
        print(f"[err] 无 .ndjson: {args.trace_dir}", file=sys.stderr)
        return 2

    sessions: dict[str, list[dict]] = defaultdict(list)
    for f in ndjsons:
        for line in f.read_text(encoding="utf-8").splitlines():
            line = line.strip()
            if line:
                try:
                    ev = json.loads(line)
                    sessions[ev.get("session_id", "?")].append(ev)
                except json.JSONDecodeError:
                    pass

    rows = []
    for sid, evs in sessions.items():
        score = heuristic_judge(evs)
        rows.append((sid, score))

    print("| session | eff | rob | rd | sus | 理由 |")
    print("|---------|-----|-----|----|----|------|")
    for sid, s in rows:
        print(f"| `{sid[:8]}` | {s['efficiency']} | {s['robust']} | "
              f"{s['readable']} | {s['suspicious']} | {s['reason']} |")
    return 0


if __name__ == "__main__":
    sys.exit(main())