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

# 消费内嵌 python 打印的 `状态<TAB>名字<TAB>细节`。
# 必须**只认首字段是 ok/FAIL 的行**: 各门会往 stderr 吐自己的提示（如
# record_event 的「未知事件类型」），而内层是 `2>&1` 捕获的，那些行没有 TAB，
# 直接进循环会被当成一条空的 FAIL —— 一个纯噪声的假失败，看起来像真问题。
report() {
    local st name detail
    while IFS=$'\t' read -r st name detail; do
        case "$st" in ok|FAIL) ;; *) continue ;; esac
        if [ "$st" = "ok" ]; then ok "$name" "$detail"; else bad "$name" "$detail"; fi
    done <<< "$1"
}

# section <标题> <输出变量名> —— 跑完一段必须**至少产出一个用例**。
#
# Why (2026-10-22): 这个守卫是被真事故逼出来的。我批量把 5 个内联读循环抽成
# report() 时，把 `report "$VAR"` 写成了 `report "VAR"`（漏了 $）—— 传给函数的是
# 字面量 "VAR"，那一行没有 TAB，被上面那个 ok|FAIL 守卫静默跳过。结果:五个整段
# 用例全空，脚本照样 `passed: 10 failed: 0` **退出 0**。
# 也就是说护栏本身挡得住噪声，却挡得住自己这类错，于是把一个明显的事故
# 包装成了 100% 绿灯。**测试脚本报绿不等于它测了东西 —— 必须是它测过之后
# 才报绿。** 空段显式判红，不给「没跑」和「全过」留同一种表示。
section() {
    local before=$PASS
    printf '\n%s\n' "$1"
    # 取变量值用 eval 而非 `${!2}` —— 后者对位置参数是 bad substitution（bash 5.3 也一样）。
    # 传进来的永远是本文件里的字面量变量名，不含外部输入。
    eval "report \"\$$2\""
    if [ "$PASS" -eq "$before" ]; then
        bad "section:$2" "本段一个用例都没产出 —— 静默跳过不等于通过"
    fi
}

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
section "memory_prompt.py 在条件满足时必须提示（patch 掉 mtime 依赖才能测）：" MP_OUT
[ "$VERBOSE" = 1 ] && printf '        %s\n' "$MP_OUT"

# ---------------------------------------------------------------------------
# 门 4: trace_link —— 注入 `Ref-trace:` 行。不是门（永不阻塞），但同样有
# 「接线了却什么都不做」的风险，所以也要证明它在该注入时能注入。
#
# 隔离方式: 它从 `X_VIDEOS_TRACE_ROOT` 环境变量取根目录（trace_link.py:32），
# 找 <root>/YYYY-MM-DD/ 下按 mtime 最新的 .ndjson。用临时目录 + 今天的日期，
# 完全不碰真实 trace 数据 —— 走的是真实参数化路径，不是打补丁绕过。
# ---------------------------------------------------------------------------
TL_OUT="$(python3 - "$SCRIPTS" "$TMP" <<'PYEOF' 2>&1
import sys, os, pathlib
from datetime import datetime
sys.path.insert(0, sys.argv[1])
tmp = pathlib.Path(sys.argv[2])
DAY = datetime.now().strftime("%Y-%m-%d")

root = tmp / "trace"
day = root / DAY
day.mkdir(parents=True, exist_ok=True)
old = day / "older.ndjson";  old.write_text("{}\n"); os.utime(old, (1000, 1000))
new = day / "newest.ndjson"; new.write_text("{}\n"); os.utime(new, (2000, 2000))
empty = root / "empty-day"          # 另一个根，今天的目录存在但没有 .ndjson
(empty / DAY).mkdir(parents=True, exist_ok=True)

import trace_link as tl

def run(name, root_dir, msg, expect_injected, expect_pick=None):
    os.environ["X_VIDEOS_TRACE_ROOT"] = str(root_dir)
    out, injected = tl.inject_ref_trace(msg)
    ok = (injected == expect_injected)
    detail = f"injected={injected} expected={expect_injected}"
    if expect_injected:
        # 断言它挑的是 mtime 最新的那个，而不只是「挑了某个」
        line = [l for l in out.splitlines() if l.startswith("Ref-trace:")]
        picked = pathlib.Path(line[0].split("Ref-trace: ")[1]) if line else None
        ok = ok and picked is not None and picked.name == expect_pick
        detail += f" picked={picked.name if picked else None} expected={expect_pick}"
    else:
        ok = ok and out == msg
        detail += f" unchanged={out == msg}"
    print(f"{'ok' if ok else 'FAIL'}\t{name}\t{detail}")

run("picks-newest-by-mtime", root, "fix: something", True, "newest.ndjson")
run("quiet-already-has-ref", root, "fix: x\nRef-trace: /x.ndjson", False)
run("quiet-empty-day-dir",   empty, "fix: something", False)
run("quiet-root-missing",    tmp / "no-such-root", "fix: something", False)
PYEOF
)"
section "trace_link.py 必须在该注入时注入 Ref-trace，该安静时安静：" TL_OUT
[ "$VERBOSE" = 1 ] && printf '        %s\n' "$TL_OUT"

# ---------------------------------------------------------------------------
# 门 2 + 门 3: handoff_size_guard / openloops_schema —— 都读 git 的**暂存区**
# （`git diff --cached`，不是 --show-toplevel），所以隔离方式是 os.chdir 到一个
# 临时 git 仓库，不碰本仓库的暂存区。
#
# 这两个门的返回值是**三态**，不是布尔: (False, msg) 拦 commit /
# (True, msg非空) 警告但放行 / (True, "") 静默。第三态和第二态在调用方
# 眼里都是「放行」，只有把 msg 也断言掉才分得开 —— 警告退化成静默通过，
# 和门坏了在输出上完全一样。
# ---------------------------------------------------------------------------
HO_OUT="$(python3 - "$SCRIPTS" "$TMP" <<'PYEOF' 2>&1
import sys, os, subprocess, pathlib
sys.path.insert(0, sys.argv[1])
tmp = pathlib.Path(sys.argv[2])
ORIG = os.getcwd()

import handoff_size_guard as hsg
import openloops_schema as ols

def mkrepo(name):
    r = tmp / name
    r.mkdir(parents=True, exist_ok=True)
    subprocess.run(["git", "init", "-q", str(r)], check=True, capture_output=True)
    return r

def stage(repo, content):
    subprocess.run(["git", "add", "-A"], cwd=repo, check=True, capture_output=True)
    (repo / "HANDOFF.md").write_text(content, encoding="utf-8")
    subprocess.run(["git", "add", "HANDOFF.md"], cwd=repo, check=True, capture_output=True)

def three_state(name, fn, expect):
    ok, msg = fn()
    got = "block" if not ok else ("warn" if msg else "silent")
    print(f"{'ok' if got == expect else 'FAIL'}\t{name}\tstate={got} expected={expect}")

def lines(n):
    return "# HANDOFF\n" + "".join(f"line {i}\n" for i in range(n))

VALID_LOOPS = """# HANDOFF

## Open Loops — Active

- **收口 trace-eval 管线** — owner: 用户 · blocker: 无 · next: 跑 eval_gate · touched: 2026-10-22

## Open Loops — Blocked / Waiting

- **等 schema 改动批准** — waiting-on: 用户在权限提示里点确认 · eta: 明天 · touched: 2026-10-22
"""

r = mkrepo("ho")
os.chdir(r)

# --- handoff_size_guard 三态 --------------------------------------------------
stage(r, lines(250)); three_state("size-block-over-200",  hsg.check_handoff_size, "block")
stage(r, lines(150)); three_state("size-warn-over-100",   hsg.check_handoff_size, "warn")
stage(r, lines(50));  three_state("size-silent-under-100", hsg.check_handoff_size, "silent")

# 暂存区里没有 HANDOFF.md → 必须完全跳过（None），而不是拿工作区版本凑合判
(r / "HANDOFF.md").write_text(lines(250), encoding="utf-8")
subprocess.run(["git", "reset", "-q"], cwd=r, check=True, capture_output=True)
three_state("size-silent-not-staged", hsg.check_handoff_size, "silent")

# fail-open 方向: 非 git 目录里 git 会报错，门必须**放弃检查**而不是崩/拦
bare = tmp / "not-a-repo"; bare.mkdir(parents=True, exist_ok=True)
os.chdir(bare)
assert hsg.get_staged_handoff_lines() is None, "非 git 目录里 get_staged_handoff_lines 应返回 None"
three_state("size-silent-non-git-dir", hsg.check_handoff_size, "silent")

# --- openloops_schema 三态 ----------------------------------------------------
os.chdir(r)
stage(r, VALID_LOOPS)
three_state("loops-valid-schema", ols.check_openloops_schema, "silent")

# 抽掉 Active slot 的 touched 字段 → 必须拦
broken = VALID_LOOPS.replace(" · touched: 2026-10-22\n", "\n", 1)
stage(r, broken); three_state("loops-block-missing-touched", ols.check_openloops_schema, "block")

# 完全没有 Open Loops 段 → 按设计静默（check_openloops_schema:257）
stage(r, "# HANDOFF\n\n就一句话。\n")
three_state("loops-silent-no-section", ols.check_openloops_schema, "silent")

os.chdir(ORIG)
PYEOF
)"
section "handoff_size_guard / openloops_schema 必须三态分明（block / warn / silent）：" HO_OUT
[ "$VERBOSE" = 1 ] && printf '        %s\n' "$HO_OUT"

# ---------------------------------------------------------------------------
# 门 5: eval_gate —— 7 个门里最复杂的一个，**4 条不触发的路径**（[skip-eval] /
# DID/VC 证据 / 无 cargo / 无 staged *.rs / 无完成声明），任何一条判宽了，
# 门就静默退化成装饰品，而且没人会发现。
#
# 隔离方式: os.chdir 到临时 git 仓库 + 把 `_run` 换掉。`_run` 是唯一会碰
# cargo 的入口，替换后**整棵判定树都能测，cargo 一次都不会跑**
# （真跑一次 fmt + clippy 要 30~180s，冒烟测试不该付这个钱）。
# 顺带断言它真的走到第 4 步并调了 2 条 cargo 命令 —— 只断言「没报错」
# 的话，「因为判宽而没触发」和「跑过了」长得一模一样。
# ---------------------------------------------------------------------------
EG_OUT="$(python3 - "$SCRIPTS" "$TMP" <<'PYEOF' 2>&1
import sys, os, subprocess, pathlib, shutil
sys.path.insert(0, sys.argv[1])
tmp = pathlib.Path(sys.argv[2])
ORIG = os.getcwd()
import eval_gate as eg

r = tmp / "eg"; r.mkdir(parents=True, exist_ok=True)
subprocess.run(["git", "init", "-q", str(r)], check=True, capture_output=True)
os.chdir(r)

calls = []
def fake_run(cmd, timeout=120, cwd=None):
    calls.append(cmd)
    return 0, "", ""
eg._run = fake_run

real_which = shutil.which
def has_cargo(_):
    calls.clear()
    return "/usr/bin/cargo"

def expect(name, msg, want):
    """want: (ok, msg_substr, n_cargo_calls)"""
    eg._run = fake_run
    calls.clear()
    try:
        ok, out = eg.check_eval_gate(msg)
    except Exception as e:
        print(f"FAIL\t{name}\traised {type(e).__name__}: {e}")
        return
    good = (ok == want[0]) and (want[1] in out) and (len(calls) == want[2])
    print(f"{'ok' if good else 'FAIL'}\t{name}\tok={ok} cargo_calls={len(calls)} "
          f"msg={out[:46]!r}")

# --- 豁免路径（都应放行，0 次 cargo 调用）---
shutil.which = lambda _: "/usr/bin/cargo"     # 越过第 1 步，专注测 0/2/3
expect("skip-marker",   "fix: x\n\n[skip-eval]",                 (True, "skipped via", 0))
expect("did-evidence",  "fix: x\n\n[DID: abc]",                  (True, "DID/VC evidence", 0))
expect("vc-evidence",   "fix: x\n\n[VC: def-1]",                 (True, "DID/VC evidence", 0))

# 暂存区空 -> 第 2 步豁免
subprocess.run(["git", "rm", "-r", "-q", "--cached", "."], cwd=r,
               capture_output=True)
expect("no-staged-rust", "feat: 完成全部改造",                   (True, "no staged *.rs", 0))

# 暂存了 .rs 但**没有完成声明** -> 第 3 步豁免。这一条是防误判的关键:
# 一个只测「拦没拦」的用例看不出 claim 检测是否过宽。
(r / "lib.rs").write_text("pub fn a() {}\n", encoding="utf-8")
subprocess.run(["git", "add", "lib.rs"], cwd=r, check=True, capture_output=True)
expect("no-completion-claim", "docs: 调整措辞",                   (True, "no completion-claim", 0))

# 完成声明 + staged .rs -> 必须真的跑 fmt + clippy 两条。
# 关键词是**语义动词**（fix/ship/complete/修复/完成…），不是 conventional-commit
# 类型 —— 下面紧跟着一条反向用例锁住这个设计。
for claim in ("fix: 修复了 xxx", "perf: shipped the lazy sampler"):
    expect(f"engages-on-claim:{claim[:4]}", f"{claim}\n\nRef-trace: /t.ndjson", (True, "eval-gate passed", 2))

# 反向用例: **光有 commit 类型不算完成声明**。
# 这条是设计意图不是缺陷: CLAIM_KEYWORDS 是语义动词表(eval_gate.py:37-50)，
# 不含 feat/perf/refactor —— 那些是几乎每个 commit 都有的类型，计进去
# 这个门就会**逢 commit 必触发**，那正是本 session 认定的更致命失效方向
# （fail-closed，一次 --no-verify 就让门永久变装饰品）。
# 有人日后「顺手补全」关键词表时，这条会红。
expect("commit-type-is-not-a-claim", "feat: add the sampler\n\nRef-trace: /t.ndjson",
       (True, "no completion-claim", 0))
expect("perf-type-is-not-a-claim", "perf: 优化了 yyy\n\nRef-trace: /t.ndjson",
       (True, "no completion-claim", 0))

# cargo 不在 -> 第 1 步豁免（patch which，因为本机确实装了 cargo）
shutil.which = lambda _: None
expect("no-cargo-on-path", "fix: 完成全部改造",                   (True, "cargo 未找到", 0))
shutil.which = real_which

os.chdir(ORIG)
PYEOF
)"
section "eval_gate 的 4 条豁免路径必须精确，最后一条必须真的触发：" EG_OUT
[ "$VERBOSE" = 1 ] && printf '        %s\n' "$EG_OUT"

# ---------------------------------------------------------------------------
# 门 7: agent_did —— 往 `~/.claude/` 写 DID/信誉分，不是安全网但**会写真实状态**，
# 坏了就是损坏用户文件。`DID_STORE`/`REPUTATION_STORE` 是模块级 Path 全局
# (agent_did.py:34-35)，指向临时目录即可完全隔离，不碰真数据。
#
# 最值钱的一条是 corrupt-json: 那个文件一旦损坏，**每一次 commit 都会读它**，
# `load_*` 必须降级成默认值而不是抛异常 —— 否则一次坏写入就把整条链永久卡死。
# ---------------------------------------------------------------------------
AD_OUT="$(python3 - "$SCRIPTS" "$TMP" <<'PYEOF' 2>&1
import sys, pathlib, json
sys.path.insert(0, sys.argv[1])
tmp = pathlib.Path(sys.argv[2])
import agent_did as ad

store = tmp / "claude-home"
store.mkdir(parents=True, exist_ok=True)
ad.DID_STORE = store / "agent_did.json"
ad.REPUTATION_STORE = store / "agent_reputation.json"

def check(name, got, expect):
    ok = got == expect
    print(f"{'ok' if ok else 'FAIL'}\t{name}\tgot={got} expected={expect}")

# 1. 下夹到 0 —— test_fail 是 -3.0,初始 50,20 次就该见底
for _ in range(20): ad.record_event("test_fail")
check("score-clamps-at-zero", ad.load_reputation()["score"], 0.0)

# 2. 上夹到 100 —— 从 0 起 40 次 test_pass(+2.0) 会到 80,再来 20 次到 120
for _ in range(60): ad.record_event("test_pass")
check("score-clamps-at-100", ad.load_reputation()["score"], 100.0)

# 3. 事件环形封顶 500 —— 上面已记 20+60=80 条,再灌 425 条凑够 505
for _ in range(425): ad.record_event("clippy_pass")
check("events-capped-at-500", len(ad.load_reputation()["events"]), 500)

# 4. 未知事件必须退出非 0(否则调用方会把垃圾事件记进信誉分)
try:
    ad.record_event("not-a-real-event"); rc = 0
except SystemExit as e:
    rc = e.code
check("unknown-event-exits-nonzero", rc, 1)

# 5. **坏 JSON 必须降级**: 那个文件每个 commit 都被读一次,坏了就会永久卡死整条链
ad.REPUTATION_STORE.write_text("{ this is not json", encoding="utf-8")
rep = ad.load_reputation()
check("corrupt-rep-degrades", (rep["score"], rep["events"]), (50.0, []))
ad.DID_STORE.write_text("}}not json{{", encoding="utf-8")
check("corrupt-did-degrades", ad.load_did(), None)

# 6. 降级后仍可继续写入(恢复路径通不通)
ad.record_event("clippy_pass")
check("writable-after-corrupt", ad.load_reputation()["score"], 51.0)

# 7. DID 签发后落盘,再取不应换 id
a, b = ad.get_did(), ad.get_did()
check("did-stable-across-calls", a == b and a.startswith("did:"), True)
PYEOF
)"
section "agent_did 的信誉分必须夹在 [0,100]、事件封顶 500、坏 JSON 必须降级：" AD_OUT
[ "$VERBOSE" = 1 ] && printf '        %s\n' "$AD_OUT"

# ---------------------------------------------------------------------------
echo
echo "passed: $PASS   failed: $FAIL"
if [ "$FAIL" -gt 0 ]; then
    echo; echo "failures:"
    for f in "${FAILURES[@]}"; do echo "  - $f"; done
    exit 1
fi
exit 0