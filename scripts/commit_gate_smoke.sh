#!/usr/bin/env bash
# commit_gate_smoke.sh — 证明 commit 期的每一道门都能响，且不该响时保持沉默。
#
# Why (2026-10-22): 与 scripts/hook_smoke.sh 同一纪律，指向 commit-msg 链而不是 Stop hook。
# 这条链上曾经有一个**接线了但从不触发**的门:
#   commit-msg 算 `_memory_prompt_path = <repo>/scripts/memory_prompt.py`，
#   那个文件实际在 scripts/git_hooks/ 下 —— `.exists()` 恒假，`if` 体从未进入，
#   `check_memory_prompt` 一次都没被调用过。跟 stop-subagent-rate.sh 恒绿同款病:
#   **看起来像安全网的死门，比没有门更危险。**
#
# 与 hook_smoke.sh 的关键差异: commit 链的门失效有两个方向，且第二个更致命 ——
#   1. 永远不响 → 没人提醒（Stop hook 的失效方向）
#   2. 永远拦   → 一个 `--no-verify` 就让门变装饰品，而且用户不会说
# 所以每个门都要**两个方向**都有用例。
#
# Usage: bash scripts/commit_gate_smoke.sh [-v]
# Exit: 0 = 全过, 1 = 有用例失败。
set -uo pipefail

REPO_ROOT="${REPO_ROOT:-$(git rev-parse --show-toplevel)}"
DISPATCHER="${DISPATCHER:-$REPO_ROOT/scripts/git_hooks/commit-msg}"
SCRIPTS="$REPO_ROOT/scripts"
VERBOSE=0
[ "${1:-}" = "-v" ] && VERBOSE=1

TMP="$(mktemp -d -t commit-gate.XXXXXX)"
trap 'rm -rf "$TMP"' EXIT

PASS=0; FAIL=0; FAILURES=()
ok()   { PASS=$((PASS+1)); printf '  ok    %-26s %s\n' "$1" "$2"; }
bad()  { FAIL=$((FAIL+1)); FAILURES+=("$1: $2"); printf '  FAIL  %-26s %s\n' "$1" "$2"; }

# ---------------------------------------------------------------------------
# 门 1（静态）: 调度器 import 的每个模块都必须能在 scripts/ 下解析到。
#
# 这是最值钱的一条，因为它覆盖的是**整个 bug 类**而不是单个实例:
# 只要某个门被移动/改名/忘记跟上，import 就解析不到，而调度器里那些
# `if _xxx_path.exists():` 的守卫会**静默跳过**它 —— 门看起来还在，实际已死。
# 解析方式是扫调度器源码里的 `from <name> import`，不硬编码门清单，
# 新增门时这条断言自动跟上。
# ---------------------------------------------------------------------------
echo "commit-msg 的每个门都必须解析得到（静态，不靠跑）："
missing=0
mods="$(grep -oE '^\s*from ([a-z_]+) import' "$DISPATCHER" 2>/dev/null | awk '{print $2}' | sort -u)"
for m in $mods; do
    case "$m" in
        pathlib|subprocess|sys|os|re|datetime|__future__) continue ;;  # 标准库
    esac
    if [ -f "$SCRIPTS/$m.py" ]; then
        ok "resolves:$m" "$SCRIPTS/$m.py"
    else
        bad "resolves:$m" "调度器 import 了 $m，但 $SCRIPTS/$m.py 不存在 —— 那个门永远不会被调用"
        missing=$((missing+1))
    fi
done
[ "$missing" -eq 0 ] || echo "        ↑ $missing 个门结构上不可达（守卫会静默跳过它们）"

# ---------------------------------------------------------------------------
# 门 2: intent_lock —— 纯函数，读 stdin，无 git 依赖，最容易也最该先证明能红。
# ---------------------------------------------------------------------------
echo
echo "intent_lock.py 必须拦下不合格的 Intent，并放行合格的："
runlock() { # <expect: 0|1> <name> <message>
    local expect="$1" name="$2" msg="$3" out rc
    out="$(printf '%s\n' "$msg" | python3 "$SCRIPTS/intent_lock.py" 2>&1)"; rc=$?
    if [ "$rc" = "$expect" ]; then ok "$name" "rc=$rc"
    else bad "$name" "expected rc=$expect, got rc=$rc — $(printf '%s' "$out" | head -1)"; fi
    [ "$VERBOSE" = 1 ] && [ -n "$out" ] && printf '        %s\n' "$(printf '%s' "$out" | head -2)"
    return 0
}
runlock 1 "intent-missing"   "fix: 修个东西"
runlock 1 "intent-too-vague" "fix: 修个东西
Intent: 修"
runlock 0 "intent-valid"     "fix: 修个东西
Intent: 移除 adaptive sampler 死代码，不改变下载 API"

# ---------------------------------------------------------------------------
# 门 3: memory_prompt —— 软提示，靠 stderr 有无判断（它永不阻塞，rc 恒 0），
# 所以断言方式与前两个不同：用「有没有输出」而不是退出码。
#
# 必须 patch 掉 PROJECT_MEMORY_GLOBS 才能测：它硬编码了本机绝对路径，
# 而 `_memory_touched_today()` 只要今天动过 memory 就返回 True → 门按设计静默。
# 不 patch 的话，一条"今天碰过 memory 所以静默"的记录和一个坏掉的门
# 在输出上完全无法区分 —— 这正是本脚本要消灭的歧义。
# ---------------------------------------------------------------------------
echo
echo "memory_prompt.py 在条件满足时必须提示（patch 掉 mtime 依赖才能测）："
MP_OUT="$(python3 - "$SCRIPTS" <<'PYEOF' 2>&1
import sys, pathlib
sys.path.insert(0, sys.argv[1])
import memory_prompt as mp
# 指向一个不存在的 glob → _memory_touched_today() 恒 False → 门的其余条件决定输出
mp.PROJECT_MEMORY_GLOBS = ["/nonexistent-dir-for-smoke-test/*.md"]

cases = [
    ("fix-signal-no-memory", "fix: 修复 sampler 抖动，发现阈值不对", True),
    ("docs-type",            "docs: 更新 readme",                    False),
    # 注意: 不能用 "feat: add flag" 当反例 —— `feat` 本身就在 INSIGHT_SIGNALS 里
    # (memory_prompt.py:23),它**应该**触发。第一版就写错在这里,红了才发现自己
    # 在拿一个真阳性当阴性。选 `revert:` —— 既不在 DOC_ONLY_TYPES 也不在 INSIGHT_SIGNALS。
    ("no-insight-signal",    "revert: undo thing",                    False),
]
for name, msg, expect in cases:
    out = mp.check_memory_prompt(msg)
    got = bool(out)
    print(f"{'ok' if got == expect else 'FAIL'}\t{name}\tfired={got} expected={expect}")
PYEOF
)"
while IFS=$'\t' read -r st name detail; do
    [ -z "$st" ] && continue
    if [ "$st" = "ok" ]; then ok "$name" "$detail"; else bad "$name" "$detail"; fi
done <<< "$MP_OUT"
[ "$VERBOSE" = 1 ] && printf '        %s\n' "$MP_OUT"

# ---------------------------------------------------------------------------
echo
echo "passed: $PASS   failed: $FAIL"
if [ "$FAIL" -gt 0 ]; then
    echo; echo "failures:"
    for f in "${FAILURES[@]}"; do echo "  - $f"; done
    exit 1
fi
exit 0