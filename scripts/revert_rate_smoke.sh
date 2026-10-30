#!/usr/bin/env bash
# revert_rate_smoke.sh — 证明 revert_rate.py 的检测器**能红**,且不乱叫。
#
# Why: 这个指标是 Anthropic 的北极星（PR 翻倍但回滚率降 40%+），本地一旦
# 接进发版流程，它就成了「敢不敢发版」的依据。一个永远返回 0.00% 的检测器
# 比没有检测器更危险 —— 它看起来在守门。
#
# 首版用 subject 关键字判定，实测 3/3 全是误报：
#   a426d392  fix(docs): SKILL.md 回滚策略描述准确化      ← 说的是「回滚策略」这个概念
#   7f6dc15d  feat(download): X 下载速度自适应 (空标题回退…) ← 「回退」= fallback
# 所以现在只认 git 自己写的机器信号 `This reverts commit <sha>.`。
# 本脚本把这个口径钉死：真 revert 必须被抓，纯关键字必须不被抓。
#
# Usage: bash scripts/revert_rate_smoke.sh [-v]
# Exit: 0 = 全过, 1 = 有用例失败。
set -uo pipefail

REPO_ROOT="${REPO_ROOT:-$(git rev-parse --show-toplevel)}"
SCRIPT="$REPO_ROOT/scripts/revert_rate.py"
VERBOSE=0
[ "${1:-}" = "-v" ] && VERBOSE=1

TMP="$(mktemp -d -t revert-rate.XXXXXX)"
trap 'rm -rf "$TMP"' EXIT

PASS=0; FAIL=0; FAILURES=()
ok()  { PASS=$((PASS+1)); printf '  ok    %-30s %s\n' "$1" "$2"; }
bad() { FAIL=$((FAIL+1)); FAILURES+=("$1: $2"); printf '  FAIL  %-30s %s\n' "$1" "$2"; }
# check <名字> <实际> <期望>
check() {
    if [ "$2" = "$3" ]; then ok "$1" "$2"; else bad "$1" "got=$2 want=$3"; fi
}
# check_ge <名字> <实际> <下界>  —— 数值下界
check_ge() {
    if [ "${2:-0}" -ge "$3" ]; then ok "$1" "$2"; else bad "$1" "got=${2:-none} want>=$3"; fi
}
section() { printf '\n%s\n' "$1"; }
jget() { python3 -c "import json,sys; d=json.load(sys.stdin); print($1)" 2>/dev/null; }

if [ ! -f "$SCRIPT" ]; then
    echo "找不到 $SCRIPT" >&2
    exit 1
fi

# ---------------------------------------------------------------------------
# 夹具:一个 6-commit 的临时 repo,其中 1 个被真 git revert 掉,3 个纯关键字干扰项
# ---------------------------------------------------------------------------
FIX="$TMP/fixture"
mkdir -p "$FIX"
(
    cd "$FIX" || exit 1
    git init -q
    git config user.email smoke@local
    git config user.name smoke
    echo one > a.txt; git add -A; git commit -q -m "feat: 第一次改动"
    echo two > b.txt; git add -A; git commit -q -m "feat: 第二次改动"
    echo three > c.txt; git add -A; git commit -q -m "feat: 第三次改动"
    # 真回滚 —— git 会自动写 "This reverts commit <sha>."
    git revert --no-edit HEAD~1 >/dev/null 2>&1
    # 三个纯关键字干扰项:都不该被当成回滚
    echo x > d.txt; git add -A; git commit -q -m "fix(docs): 回滚策略描述准确化"
    echo y > e.txt; git add -A; git commit -q -m "feat: 速度自适应 (空标题回退)"
    echo z > f.txt; git add -A; git commit -q -m "chore: revert the staging branch pointer"
) >/dev/null 2>&1

section "1. 真 git revert 必须被抓:"
OUT="$(cd "$FIX" && python3 "$SCRIPT" --since "2000-01-01 00:00:00" --json 2>&1)"
RC=$?
check "json-mode-rc" "$RC" "0"
COUNT="$(printf '%s' "$OUT" | jget 'd["revert_count"]')"
TOTAL="$(printf '%s' "$OUT" | jget 'd["total_commits"]')"
RATE="$(printf '%s' "$OUT" | jget 'round(d["revert_rate"],4)')"
check "revert-count" "${COUNT:-none}" "1"
check "total-commits" "${TOTAL:-none}" "7"
check "revert-rate" "${RATE:-none}" "0.1429"

section "2. 纯关键字的 subject 必须不被当成回滚:"
DECL="$(cd "$FIX" && python3 "$SCRIPT" --since "2000-01-01 00:00:00" --include-declared --json 2>&1 \
    | jget 'len(d["declared_candidates"])')"
# --include-declared 只是**候选**清单:数量 >= 3 说明噪声确实被识别出来了,
# 但它们没有进入 revert_count(上一节已断言 = 1)
check_ge "declared-candidates-listed" "${DECL:-0}" "3"

section "3. --max-rate 阈值必须真的能判红:"
(cd "$FIX" && python3 "$SCRIPT" --since "2000-01-01 00:00:00" --max-rate 0.05 >/dev/null 2>&1)
check "over-threshold-rc" "$?" "2"
(cd "$FIX" && python3 "$SCRIPT" --since "2000-01-01 00:00:00" --max-rate 0.90 >/dev/null 2>&1)
check "under-threshold-rc" "$?" "0"

section "4. 本仓库真实历史:机器信号下应为 0 次回滚（防回归到关键字口径）:"
OUT2="$(cd "$REPO_ROOT" && python3 "$SCRIPT" --days 365 --json 2>&1)"
RC2=$?
check "real-repo-rc" "$RC2" "0"
C2="$(printf '%s' "$OUT2" | jget 'd["revert_count"]')"
T2="$(printf '%s' "$OUT2" | jget 'd["total_commits"]')"
check "real-repo-revert-count" "${C2:-none}" "0"
check_ge "real-repo-window-populated" "${T2:-0}" "100"

# ---------------------------------------------------------------------------
echo
echo "passed: $PASS   failed: $FAIL"
if [ "$FAIL" -gt 0 ]; then
    echo; echo "failures:"
    for f in "${FAILURES[@]}"; do echo "  - $f"; done
    exit 1
fi
exit 0
