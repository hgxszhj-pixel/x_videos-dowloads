#!/usr/bin/env bash
# review_eval_smoke.sh — 证明 review_eval 的 gate **能红**,且不乱叫。
#
# Why: 「改审查 prompt 必须先跑分」这条规则的**全部价值**在 gate 会拒绝。
# 一个永远 rc=0 的 gate 比没有 gate 更危险 —— 它看起来在守门。
# 所以本脚本覆盖两个方向:
#   A. 该红的场景必须红（prompt 改了 / baseline 不合格 / baseline 坏了）
#   B. 不该红的场景必须绿（baseline 匹配 / 没有 baseline / 逃生舱）
#
# 不跑真实评测（要调 claude CLI、几分钟）。门禁逻辑只依赖
# REVIEW_PROMPT_PATH + REVIEW_EVAL_BASELINE 两个 env,夹具直接造。
#
# Usage: bash scripts/review_eval_smoke.sh [-v]
# Exit: 0 = 全过, 1 = 有用例失败。
set -uo pipefail

REPO_ROOT="${REPO_ROOT:-$(git rev-parse --show-toplevel)}"
export REPO_ROOT
SCRIPT="$REPO_ROOT/scripts/review_eval.py"
VERBOSE=0
[ "${1:-}" = "-v" ] && VERBOSE=1

TMP="$(mktemp -d -t review-eval-smoke.XXXXXX)"
trap 'rm -rf "$TMP"' EXIT

PASS=0; FAIL=0; FAILURES=()
ok()  { PASS=$((PASS+1)); printf '  ok    %-32s %s\n' "$1" "$2"; }
bad() { FAIL=$((FAIL+1)); FAILURES+=("$1: $2"); printf '  FAIL  %-32s %s\n' "$1" "$2"; }
section() { printf '\n%s\n' "$1"; }
# check <名字> <实际> <期望>
check() {
    if [ "$2" = "$3" ]; then ok "$1" "$2"; else bad "$1" "got=$2 want=$3"; fi
}
# check_rc <名字> <期望rc> <实际rc>
check_rc() {
    if [ "$3" = "$2" ]; then ok "$1" "rc=$3"; else bad "$1" "got rc=$3 want rc=$2"; fi
}

if [ ! -f "$SCRIPT" ]; then
    echo "找不到 $SCRIPT" >&2
    exit 1
fi

# ---------------------------------------------------------------------------
# 夹具:一份假 prompt + 几份不同状态的 baseline
# ---------------------------------------------------------------------------
PROMPT="$TMP/code-reviewer.md"
printf '# fake reviewer prompt\n```\nreview this diff\n```\n' > "$PROMPT"

PSHA="$(python3 -c "import hashlib,sys; print(hashlib.sha256(open(sys.argv[1],'rb').read()).hexdigest())" "$PROMPT")"

mk_baseline() {  # mk_baseline <file> <sha> <recall> <fp>
    python3 - "$1" "$2" "$3" "$4" <<'PY'
import json, sys
path, sha, recall, fp = sys.argv[1:5]
json.dump({
    "prompt_sha256": sha,
    "prompt_path": "(fixture)",
    "model": "sonnet",
    "recall": float(recall),
    "expected_total": 3,
    "found_total": int(float(recall) * 3),
    "false_positive_count": int(fp),
    "results": [],
}, open(path, "w"), ensure_ascii=False, indent=2)
PY
}

mk_baseline "$TMP/base_good.json"   "$PSHA" 1.0 0   # 一切正常
mk_baseline "$TMP/base_stale.json"  "deadbeef" 1.0 0   # prompt 改了没重跑
mk_baseline "$TMP/base_lowrec.json" "$PSHA" 0.67 0  # baseline 本身不合格
mk_baseline "$TMP/base_fp.json"     "$PSHA" 1.0 2   # baseline 带误报
printf '{ this is not json' > "$TMP/base_broken.json"

# gate_rc <baseline> [message]  → 打印 rc
gate_rc() {
    local base="$1" msg="${2:-}"
    REVIEW_PROMPT_PATH="$PROMPT" REVIEW_EVAL_BASELINE="$base" \
        python3 "$SCRIPT" gate --message "$msg" >/dev/null 2>&1
    echo $?
}

section "1. 该红的场景必须红:"
check_rc "prompt-changed-no-rerun" 1 "$(gate_rc "$TMP/base_stale.json")"
check_rc "baseline-recall-too-low" 1 "$(gate_rc "$TMP/base_lowrec.json")"
check_rc "baseline-has-false-pos"  1 "$(gate_rc "$TMP/base_fp.json")"
check_rc "baseline-broken-json"    1 "$(gate_rc "$TMP/base_broken.json")"

section "2. 不该红的场景必须绿:"
check_rc "baseline-fresh"          0 "$(gate_rc "$TMP/base_good.json")"
check_rc "no-baseline-skip"        0 "$(gate_rc "$TMP/does_not_exist.json")"
check_rc "escape-hatch-skip-marker" 0 "$(gate_rc "$TMP/base_stale.json" "[skip-review-eval] not sure")"

section "3. 缺 prompt 文件时跳过(而不是崩):"
MISSING_PROMPT_RC="$(REVIEW_PROMPT_PATH="$TMP/nope.md" REVIEW_EVAL_BASELINE="$TMP/base_good.json" \
    python3 "$SCRIPT" gate >/dev/null 2>&1; echo $?)"
check_rc "missing-prompt-skip"     0 "$MISSING_PROMPT_RC"

section "4. check_review_eval() 契约(与 CLI 同源,供 commit-msg hook 调用):"
CONTRACT="$(python3 - "$PROMPT" "$TMP/base_good.json" "$TMP/base_stale.json" <<'PY'
import sys, os
sys.path.insert(0, os.path.join(os.environ["REPO_ROOT"], "scripts"))
import review_eval as r
r.PROMPT_PATH = r.Path(sys.argv[1])
r.BASELINE_PATH = r.Path(sys.argv[2])
ok_good, _ = r.check_review_eval("")
r.BASELINE_PATH = r.Path(sys.argv[3])
ok_stale, _ = r.check_review_eval("")
r.BASELINE_PATH = r.Path(sys.argv[2])
ok_skip, _ = r.check_review_eval("please [skip-review-eval] now")
print(f"{int(ok_good)} {int(ok_stale)} {int(ok_skip)}")
PY
)"
check "contract-ok/ok/ok" "$(printf '%s' "$CONTRACT" | cut -d' ' -f1)" "1"
check "contract-stale-red" "$(printf '%s' "$CONTRACT" | cut -d' ' -f2)" "0"
check "contract-marker-bypasses" "$(printf '%s' "$CONTRACT" | cut -d' ' -f3)" "1"

# ---------------------------------------------------------------------------
echo
echo "passed: $PASS   failed: $FAIL"
if [ "$FAIL" -gt 0 ]; then
    echo; echo "failures:"
    for f in "${FAILURES[@]}"; do echo "  - $f"; done
    exit 1
fi
exit 0
