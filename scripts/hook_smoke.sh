#!/usr/bin/env bash
# Hook smoke test — proves each Stop hook CAN fire, not just that it is quiet.
#
# Why this exists (2026-10-22): stop-subagent-rate.sh had been wired since before
# 2026-10-12 and never once printed a warning across 32 sessions. It parsed
# role/content at the TOP level of each JSONL record while Claude Code nests both
# under "message", so n_user_turns was always 0, the `< 4` guard always fired,
# and it printed OK forever. A detector that cannot fire is worse than no
# detector, because it looks like a safety net. This script is the same
# discipline as scripts/mutation_smoke.sh, pointed at hooks instead of Rust:
# every case asserts a hook DOES warn (and that it stays quiet when it should).
#
# Usage:
#   bash scripts/hook_smoke.sh          # run all cases
#   bash scripts/hook_smoke.sh -v       # show each hook's stderr
#
# Exit: 0 = all cases pass, 1 = at least one case failed.
#
# A failing case is information, not noise: it means a hook is broken right now.
set -uo pipefail

HOOKS_DIR="${HOOKS_DIR:-$HOME/.claude/hooks}"
VERBOSE=0
[ "${1:-}" = "-v" ] && VERBOSE=1

TMP="$(mktemp -d -t hook-smoke.XXXXXX)"
trap 'rm -rf "$TMP"' EXIT

PASS=0
FAIL=0
declare -a FAILURES=()

# ---------------------------------------------------------------- fixtures ---
# Synthetic transcripts only — this never reads the user's real session data.
python3 - "$TMP" <<'PYEOF'
import json, sys, pathlib

tmp = pathlib.Path(sys.argv[1])
tx = tmp / "tx"
tx.mkdir(parents=True, exist_ok=True)


def rec(role, content):
    return json.dumps({"type": role, "message": {"role": role, "content": content}},
                      ensure_ascii=False)


def turn(text):
    return rec("user", text)


def call(name, i):
    return rec("assistant", [{"type": "tool_use", "id": f"t{i}", "name": name,
                              "input": {"command": "true", "pattern": "x"}}])


def write(name, lines):
    (tx / name).write_text("\n".join(lines) + "\n", encoding="utf-8")


# 1. zero subagent dispatch + lots of eligible work -> subagent-rate MUST warn
lines = []
for n in range(6):
    lines.append(turn(f"do task {n}"))
    for i in range(15):
        lines.append(call("Read", f"{n}-{i}"))
write("no-agent.jsonl", lines)

# 2. healthy delegation -> subagent-rate MUST stay quiet
lines = []
for n in range(6):
    lines.append(turn(f"do task {n}"))
    for i in range(3):
        lines.append(call("Read", f"{n}-{i}"))
    lines.append(call("Agent", f"{n}-agent"))
write("with-agent.jsonl", lines)

# 3. raw file reads, no structured/sandboxed retrieval -> retrieval-mix MUST warn
lines = []
for n in range(6):
    lines.append(turn(f"do task {n}"))
    for i in range(12):
        lines.append(call("Read", f"{n}-{i}"))
write("read-heavy.jsonl", lines)

# 4. same volume, but routed through ctx_* -> retrieval-mix MUST stay quiet
lines = []
for n in range(6):
    lines.append(turn(f"do task {n}"))
    for i in range(12):
        lines.append(call("mcp__plugin_context-mode_context-mode__ctx_search", f"{n}-{i}"))
write("ctx-heavy.jsonl", lines)

def say(text):
    # Real transcripts always carry assistant content as a list of blocks, and
    # anti-slop-stop.sh walks it with `.message.content[]? | select(.type=="text")`.
    # A bare string fixture silently yields nothing — which is how the first run
    # of this script produced a false FAIL on a hook that was actually healthy.
    return rec("assistant", [{"type": "text", "text": text}])


# 5. final assistant text carries a slop phrase -> anti-slop MUST block (exit 2)
write("slop.jsonl", [
    turn("go"), call("Read", "s0"),
    say("In this analysis we must delve into the topic."),
])

# 6. clean final assistant text -> anti-slop MUST stay quiet
write("clean.jsonl", [
    turn("go"), call("Read", "c0"),
    say("The parser nests role under message; that was the bug."),
])
print("fixtures written")
PYEOF

# ------------------------------------------------------------------ runner ---
# run <hook> <transcript> <expect: fire|quiet> <case-name>
run() {
    local hook="$1" tfile="$2" expect="$3" name="$4"
    local path="$TMP/tx/$tfile"
    local out rc
    out="$(printf '{"transcript_path":"%s","cwd":"%s","session_id":"hook-smoke","stop_hook_active":false}' \
        "$path" "$PWD" | "$HOOKS_DIR/$hook" 2>&1)"
    rc=$?
    local got="quiet"
    [ -n "$out" ] && got="fire"
    [ "$rc" -ne 0 ] && got="fire"

    if [ "$got" = "$expect" ]; then
        PASS=$((PASS + 1))
        printf '  ok    %-22s %s\n' "$name" "$hook"
    else
        FAIL=$((FAIL + 1))
        FAILURES+=("$name ($hook): expected $expect, got $got [rc=$rc]")
        printf '  FAIL  %-22s %s (expected %s, got %s)\n' "$name" "$hook" "$expect" "$got"
    fi
    [ "$VERBOSE" = 1 ] && [ -n "$out" ] && printf '        %s\n' "$(printf '%s' "$out" | head -3)"
    return 0
}

echo "hook smoke test (hooks in $HOOKS_DIR)"
echo
echo "Stop hooks must FIRE when they have something to report:"
run stop-subagent-rate.sh  no-agent.jsonl   fire  "no-subagent-dispatch"
run stop-retrieval-mix.sh  read-heavy.jsonl fire  "read-heavy-session"
run anti-slop-stop.sh      slop.jsonl       fire  "slop-in-final-text"

echo
echo "Stop hooks must stay QUIET when there is nothing to report:"
run stop-subagent-rate.sh  with-agent.jsonl quiet "healthy-delegation"
run stop-retrieval-mix.sh  ctx-heavy.jsonl  quiet "ctx-routed-session"
run anti-slop-stop.sh      clean.jsonl      quiet "clean-final-text"

# ------------------------------------------------- stop-fmt-gate: 唯一无测试的 ---
# stop-fmt-gate.sh 是接线的 Stop hook 里唯一没有被"能不能响"测试覆盖的一个。
# 它读的是环境变量 CLAUDE_PROJECT_DIR(不是 stdin JSON 里的字段),所以不能复用 run()。
# 静默失守的代价:未格式化的 Rust 直接提交,门形同虚设还看起来像安全网。
runfmt() {
    local name="$1" proj="$2" expect="$3"
    local out rc got="quiet"
    out="$(printf '{"stop_hook_active":false}' \
        | env CLAUDE_PROJECT_DIR="$proj" "$HOOKS_DIR/stop-fmt-gate.sh" 2>&1)"
    rc=$?
    [ -n "$out" ] && got="fire"
    [ "$rc" -ne 0 ] && got="fire"

    if [ "$got" = "$expect" ]; then
        PASS=$((PASS + 1))
        printf '  ok    %-22s %s\n' "$name" "stop-fmt-gate.sh"
    else
        FAIL=$((FAIL + 1))
        FAILURES+=("$name (stop-fmt-gate.sh): expected $expect, got $got [rc=$rc]")
        printf '  FAIL  %-22s %s (expected %s, got %s)\n' "$name" "stop-fmt-gate.sh" "$expect" "$got"
    fi
    [ "$VERBOSE" = 1 ] && [ -n "$out" ] && printf '        %s\n' "$(printf '%s' "$out" | head -3)"
    return 0
}

# rustfmt 缺失时整组跳过,而不是误报 —— hook 自己对 cargo 不可用是 fail-open 的,
# 测试不该比被测对象更严。
if export PATH="$HOME/.cargo/bin:$PATH" && command -v rustfmt >/dev/null 2>&1; then
    RDIR_BAD="$TMP/rust-bad"; RDIR_OK="$TMP/rust-ok"
    for d in "$RDIR_BAD" "$RDIR_OK"; do
        mkdir -p "$d/src"
        printf '[package]\nname="fmtprobe"\nversion="0.1.0"\nedition="2021"\n' > "$d/Cargo.toml"
    done
    # rustfmt 会跳过没有 main.rs 的 crate,这里两个 crate 同名会互相干扰 —— 用不同名字
    printf 'fn  main( ) {\nlet x=1;\nprintln!("{}",x);\n}\n' > "$RDIR_BAD/src/main.rs"
    printf 'fn main() {\n    let x = 1;\n    println!("{}", x);\n}\n' > "$RDIR_OK/src/main.rs"

    echo
    echo "stop-fmt-gate.sh must FIRE on unformatted Rust and stay quiet otherwise:"
    runfmt "unformatted-rust" "$RDIR_BAD" fire
    runfmt "formatted-rust"   "$RDIR_OK"  quiet
    runfmt "non-rust-project" "$TMP"      quiet
else
    echo
    echo "skip: rustfmt not found — stop-fmt-gate.sh cases skipped (hook itself is fail-open here)"
fi

# --------------------------------------------------- trace-eval blind spot ----
# stop-trace-eval.sh only ever looked at $HOME/.local/share/x_videos-dowloads-trace/<date>/
# while src/core/trace.rs:262 writes no-session events to .../no-session/<date>.ndjson.
# Probe it without touching any real data: point HOME at a sandbox holding ONLY
# the no-session file. If the hook is blind, it stays silent; if it sees the data,
# it proceeds far enough to complain about the missing baseline.
echo
echo "stop-trace-eval.sh must not be blind to no-session/ traces:"
FAKE_HOME="$TMP/home"
TODAY="$(date +%Y-%m-%d)"
TRACE_ROOT="$FAKE_HOME/.local/share/x_videos-dowloads-trace"
mkdir -p "$TRACE_ROOT/no-session"
printf '{"step":"start_download_impl","duration_ms":10}\n' > "$TRACE_ROOT/no-session/$TODAY.ndjson"
OUT="$(printf '{}' | env HOME="$FAKE_HOME" "$HOOKS_DIR/stop-trace-eval.sh" 2>&1)"
if printf '%s' "$OUT" | grep -q 'baseline'; then
    PASS=$((PASS + 1)); printf '  ok    %-22s %s\n' "sees-no-session-trace" "stop-trace-eval.sh"
else
    FAIL=$((FAIL + 1))
    FAILURES+=("sees-no-session-trace (stop-trace-eval.sh): hook stayed silent, so it never evaluates no-session/ traces")
    printf '  FAIL  %-22s %s (hook is blind to no-session/ traces)\n' "sees-no-session-trace" "stop-trace-eval.sh"
fi
[ "$VERBOSE" = 1 ] && [ -n "$OUT" ] && printf '        %s\n' "$(printf '%s' "$OUT" | head -3)"

# ------------------------------------------------- pretool-web-tier 直跳拦截 ----
# pretool-web-tier.sh 能 DENY 工具调用 —— 方向错了就是「白名单脚本一坏，全站抓取
# 被拦死」。所以它必须同时证明两件事：该拦的拦住，不该拦的（以及它自己坏了）放行。
#
# 先守依赖：webtier.sh 是判定依据。它语法坏掉时 `bash webtier.sh` 的退出码也是 1，
# 而 pretool hook 只认「rc==1 = 直跳」，于是损坏的依赖会被静默读成「全部拦截」。
# 这里用一条断言把这个失效模式钉死 —— 语法坏了要在测试里红，不能等到线上。
WEBTIER="${WEBTIER:-$HOME/.claude/scripts/webtier.sh}"
if bash -n "$WEBTIER" 2>/dev/null; then
    PASS=$((PASS + 1)); printf '  ok    %-22s %s\n' "webtier-syntax-valid" "$WEBTIER"
else
    FAIL=$((FAIL + 1))
    FAILURES+=("webtier-syntax-valid: $WEBTIER 不是合法 bash 脚本，pretool-web-tier.sh 会把它的退出码 1 误读成「直跳」而拦截一切")
    printf '  FAIL  %-22s %s (损坏的依赖会让 hook fail-closed)\n' "webtier-syntax-valid" "$WEBTIER"
fi

# PreToolUse 的 payload 形状与 Stop 不同（没有 transcript_path，是 tool_name/tool_input），
# 所以不能复用上面的 run()。
runpre() {
    local hook="$1" tool="$2" input="$3" expect="$4" name="$5"
    local payload out
    payload="$(python3 -c '
import json, sys
print(json.dumps({"session_id": "hook-smoke", "cwd": "/tmp",
                  "hook_event_name": "PreToolUse",
                  "tool_name": sys.argv[1],
                  "tool_input": json.loads(sys.argv[2])}))' "$tool" "$input")"
    out="$(printf '%s' "$payload" | "$HOOKS_DIR/$hook" 2>&1)"
    local got="allow"
    printf '%s' "$out" | grep -q '"permissionDecision": *"deny"' && got="deny"
    if [ "$got" = "$expect" ]; then
        PASS=$((PASS + 1)); printf '  ok    %-22s %s\n' "$name" "$hook"
    else
        FAIL=$((FAIL + 1))
        FAILURES+=("$name ($hook): expected $expect, got $got")
        printf '  FAIL  %-22s %s (expected %s, got %s)\n' "$name" "$hook" "$expect" "$got"
    fi
    [ "$VERBOSE" = 1 ] && [ -n "$out" ] && printf '        %s\n' "$(printf '%s' "$out" | head -3)"
    return 0
}

TT_URL='{"url":"https://www.toutiao.com/article/7698920405368635944/"}'
echo
echo "pretool-web-tier.sh must DENY cheap fetches the whitelist says to skip:"
runpre pretool-web-tier.sh mcp__plugin_context-mode_context-mode__ctx_fetch_and_index "$TT_URL" deny "deny-ctx-fetch-whitelisted"
runpre pretool-web-tier.sh WebFetch "$TT_URL" deny "deny-webfetch-whitelisted"
runpre pretool-web-tier.sh Bash '{"command":"moli fetch https://www.toutiao.com/article/7698920405368635944/ -d markdown"}' deny "deny-moli-whitelisted"

echo
echo "pretool-web-tier.sh must stay quiet for everything else:"
runpre pretool-web-tier.sh mcp__plugin_context-mode_context-mode__ctx_fetch_and_index '{"url":"https://example.com/a"}' allow "allow-unknown-domain"
runpre pretool-web-tier.sh Bash '{"command":"ls -la /tmp"}' allow "allow-non-fetch-bash"
runpre pretool-web-tier.sh Read '{"file_path":"/tmp/x"}' allow "allow-unrelated-tool"

echo
echo "pretool-web-tier.sh must FAIL OPEN when its dependency is gone:"
OUT="$(printf '{"hook_event_name":"PreToolUse","tool_name":"WebFetch","tool_input":%s}' "$TT_URL" \
    | env WEBTIER=/nonexistent/webtier.sh "$HOOKS_DIR/pretool-web-tier.sh" 2>&1)"
if printf '%s' "$OUT" | grep -q '"deny"'; then
    FAIL=$((FAIL + 1))
    FAILURES+=("fail-open-missing-webtier: webtier.sh 不存在时 hook 仍在拦截 —— 白名单会把抓取整个搞瘫")
    printf '  FAIL  %-22s %s (missing dependency still denies)\n' "fail-open-missing-webtier" "pretool-web-tier.sh"
else
    PASS=$((PASS + 1)); printf '  ok    %-22s %s\n' "fail-open-missing-webtier" "pretool-web-tier.sh"
fi

# ------------------------------------------------------------------ summary ---
echo
echo "passed: $PASS   failed: $FAIL"
if [ "$FAIL" -gt 0 ]; then
    echo
    echo "failures:"
    for f in "${FAILURES[@]}"; do echo "  - $f"; done
    exit 1
fi
exit 0