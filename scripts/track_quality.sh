#!/usr/bin/env bash
# quality_tracker.sh - 统一质量验证脚本
# 运行 cargo test + clippy,结果自动记录到信誉系统
#
# 用法: bash scripts/track_quality.sh [test|clippy|all]
#  - 默认 all: cargo test --bin x-video-downloader + cargo clippy -- -D warnings
#  - 自动 record_event 到 ~/.claude/agent_reputation.json

set -e

MODE="${1:-all}"
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

PYTHON="python3"
AGENT_DID="$REPO_ROOT/scripts/agent_did.py"

record_test() {
    local status="$1"  # pass / fail
    local detail="$2"
    if [ "$status" = "pass" ]; then
        "$PYTHON" "$AGENT_DID" record --event test_pass --detail "$detail" >/dev/null
    else
        "$PYTHON" "$AGENT_DID" record --event test_fail --detail "$detail" >/dev/null
    fi
}

record_clippy() {
    local status="$1"
    local detail="$2"
    if [ "$status" = "pass" ]; then
        "$PYTHON" "$AGENT_DID" record --event clippy_pass --detail "$detail" >/dev/null
    else
        "$PYTHON" "$AGENT_DID" record --event clippy_fail --detail "$detail" >/dev/null
    fi
}

run_test() {
    echo "🧪 Running cargo test..."
    if cargo test --bin x-video-downloader --quiet 2>&1 | tail -5; then
        local count=$(cargo test --bin x-video-downloader 2>&1 | grep -oE "[0-9]+ passed" | tail -1)
        record_test "pass" "$count"
        echo "✅ Test passed: $count"
    else
        record_test "fail" "cargo test failed"
        echo "❌ Test failed"
        return 1
    fi
}

run_clippy() {
    echo "📋 Running cargo clippy..."
    if cargo clippy --bin x-video-downloader -- -D warnings 2>&1 | tail -10; then
        record_clippy "pass" "0 warnings"
        echo "✅ Clippy clean"
    else
        record_clippy "fail" "warnings or errors"
        echo "❌ Clippy failed"
        return 1
    fi
}

case "$MODE" in
    test)
        run_test
        ;;
    clippy)
        run_clippy
        ;;
    all)
        run_test && run_clippy
        ;;
    *)
        echo "Usage: $0 [test|clippy|all]"
        exit 1
        ;;
esac

echo ""
echo "📊 当前信誉:"
"$PYTHON" "$AGENT_DID" reputation | head -20