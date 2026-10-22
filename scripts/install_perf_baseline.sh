#!/usr/bin/env bash
# Lock a perf baseline for scripts/eval_trace.py from an existing trace date.
#
# Why (2026-10-22): stop-trace-eval.sh told you to run this script when the
# baseline was missing, but no such script existed — and the baseline dir it
# pointed at (2026-09-17) was empty. An empty baseline silently turns every
# metric into "improved", so the detector could never fire. Re-locking is the
# other half of the fix; eval_trace.py now refuses to emit a verdict at all
# when the baseline has no usable sessions.
#
# Usage:
#   bash scripts/install_perf_baseline.sh                 # newest date with trace data
#   bash scripts/install_perf_baseline.sh 2026-10-12      # a specific date
#   bash scripts/install_perf_baseline.sh 2026-10-12 my-baseline
#   bash scripts/install_perf_baseline.sh 2026-10-12 baseline-x --force
#
# The default label is "baseline-<date>", never the bare date: a baseline must
# be distinguishable from the live date dir it was copied from, otherwise the
# no-clobber guard below would refuse to replace it.
#
# Layout it understands (see src/core/trace.rs):
#   $TRACE_ROOT/<YYYY-MM-DD>/*.ndjson      ← sessions with an active session id
#   $TRACE_ROOT/no-session/<date>.ndjson   ← fallback when no session is active
# Both are copied into the baseline, prefixed so they cannot collide.
set -euo pipefail

TRACE_ROOT="${TRACE_ROOT:-${HOME}/.local/share/x_videos-dowloads-trace}"
FORCE=0
POSITIONAL=()
for arg in "$@"; do
    case "$arg" in
        --force) FORCE=1 ;;
        *) POSITIONAL+=("$arg") ;;
    esac
done

SOURCE_DATE="${POSITIONAL[0]:-}"
BASELINE_LABEL="${POSITIONAL[1]:-}"

die() { printf 'error: %s\n' "$1" >&2; exit 1; }

[ -d "$TRACE_ROOT" ] || die "trace root not found: $TRACE_ROOT (set TRACE_ROOT)"

# Pick the newest date dir that actually holds .ndjson files.
if [ -z "$SOURCE_DATE" ]; then
    for d in $(ls -1 "$TRACE_ROOT" 2>/dev/null | sort -r); do
        if [ -d "$TRACE_ROOT/$d" ] && ls "$TRACE_ROOT/$d"/*.ndjson >/dev/null 2>&1; then
            SOURCE_DATE="$d"
            break
        fi
    done
    [ -n "$SOURCE_DATE" ] || die "no date dir under $TRACE_ROOT contains .ndjson files"
    printf 'auto-selected source date: %s\n' "$SOURCE_DATE"
fi

BASELINE_LABEL="${BASELINE_LABEL:-baseline-$SOURCE_DATE}"

SRC_DIR="$TRACE_ROOT/$SOURCE_DATE"
NO_SESSION="$TRACE_ROOT/no-session/$SOURCE_DATE.ndjson"
DEST="$TRACE_ROOT/$BASELINE_LABEL"

[ -n "$BASELINE_LABEL" ] || die "baseline label missing (pass a second arg)"
[ -d "$SRC_DIR" ] || ls "$NO_SESSION" >/dev/null 2>&1 || \
    die "no trace data for $SOURCE_DATE (looked for $SRC_DIR and $NO_SESSION)"

# Refuse to clobber an existing baseline unless forced — this is the artifact
# every regression check is measured against.
if [ -d "$DEST" ] && ls "$DEST"/*.ndjson >/dev/null 2>&1; then
    if [ "$FORCE" -ne 1 ]; then
        die "$DEST already holds $(ls -1 "$DEST"/*.ndjson | wc -l | tr -d ' ') .ndjson file(s). Re-run with --force to replace."
    fi
    printf 'replacing existing baseline: %s\n' "$DEST"
    rm -f "$DEST"/*.ndjson
fi

mkdir -p "$DEST"
N=0
if [ -d "$SRC_DIR" ]; then
    for f in "$SRC_DIR"/*.ndjson; do
        [ -e "$f" ] || continue
        cp "$f" "$DEST/s-$(basename "$f")"
        N=$((N + 1))
    done
fi
if [ -f "$NO_SESSION" ]; then
    cp "$NO_SESSION" "$DEST/n-$(basename "$NO_SESSION")"
    N=$((N + 1))
fi

[ "$N" -gt 0 ] || die "copied 0 files — refusing to leave an empty baseline at $DEST"

printf 'locked baseline %s from %s (%s file%s)\n' "$DEST" "$SOURCE_DATE" "$N" \
    "$([ "$N" -eq 1 ] && echo '' || echo 's')"
printf 'verify with:\n  python3 scripts/eval_trace.py --baseline %s --current %s\n' \
    "$DEST" "$DEST"