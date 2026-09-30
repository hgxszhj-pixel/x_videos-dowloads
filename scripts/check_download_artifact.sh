#!/usr/bin/env bash
# check_download_artifact.sh - 改下载/落盘路径后的验收闸门
#
# 为什么需要它：改动下载链（策略分发、分段、合并、落盘）后，日志"看起来对了"不等于产物是对的。
# 唯一能证明没改坏的证据是「产物字节数等于期望 + sha256 与对照版本逐字节相同」。
# 顺带校验两个已知会骗人的地方：日志里「最终文件 X / 期望 Y」是否一致、进度行「已下载 X」是否大于文件本身。
#
# 用法:
#   bash scripts/check_download_artifact.sh <url>                    # 跑一次并打印 size/sha256（建立基准）
#   bash scripts/check_download_artifact.sh <url> --record           # 额外打印可直接粘贴的断言行
#   bash scripts/check_download_artifact.sh <url> \
#        --expect-bytes 38718248 --expect-sha256 bc70965a8bf7dc3d…   # 断言模式，不符 exit 1
#   --keep 保留临时目录（失败时自动保留）
#
# 退出码: 0 = 通过；1 = 断言不符或产物异常；2 = 用法/前置条件错

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="$REPO_ROOT/target/release/x-video-downloader"

URL="${1:-}"
[ -n "$URL" ] || { echo "用法: $0 <url> [--expect-bytes N] [--expect-sha256 HEX] [--record] [--keep]" >&2; exit 2; }
shift

EXPECT_BYTES=""; EXPECT_SHA=""; RECORD=0; KEEP=0
while [ $# -gt 0 ]; do
    case "$1" in
        --expect-bytes)  EXPECT_BYTES="${2:-}"; shift 2 ;;
        --expect-sha256) EXPECT_SHA="${2:-}";   shift 2 ;;
        --record)        RECORD=1; shift ;;
        --keep)          KEEP=1; shift ;;
        *) echo "未知参数: $1" >&2; exit 2 ;;
    esac
done

[ -x "$BIN" ] || { echo "✗ 找不到 $BIN —— 先 cargo build --release" >&2; exit 2; }

OUT="$(mktemp -d "${TMPDIR:-/tmp}/dlcheck.XXXXXX")"
LOG="$OUT/download.log"
cleanup() { if [ "$KEEP" = 0 ] && [ "${FAILED:-0}" = 0 ]; then rm -rf "$OUT"; else echo "   保留现场: $OUT"; fi; }
trap cleanup EXIT

echo "▶ 下载: $URL"
set +e
RUST_LOG=info "$BIN" "$URL" -o "$OUT" >"$LOG" 2>&1
STATUS=$?
set -e
[ "$STATUS" = 0 ] || { FAILED=1; echo "✗ 下载进程退出码 $STATUS（日志尾部）:"; tail -12 "$LOG"; exit 1; }

FILES=$(find "$OUT" -type f ! -name 'download.log' | sort)
COUNT=$(printf '%s\n' "$FILES" | grep -c . || true)
[ "$COUNT" = 1 ] || { FAILED=1; echo "✗ 期望恰好 1 个产物，实得 $COUNT 个:"; printf '%s\n' "$FILES"; exit 1; }
ART="$FILES"

SIZE=$(stat -f%z "$ART")
SHA=$(shasum -a 256 "$ART" | awk '{print $1}')
echo "  产物: $(basename "$ART")"
echo "  字节: $SIZE"
echo "  sha256: $SHA"

# 内部自检：日志声明的最终大小 vs 期望大小
INNER=$(grep -o '最终文件: [0-9]* bytes (期望: [0-9]* bytes)' "$LOG" | tail -1 || true)
if [ -n "$INNER" ]; then
    INNER_ACTUAL=$(echo "$INNER" | grep -o '最终文件: [0-9]*' | grep -o '[0-9]*')
    if [ "$INNER_ACTUAL" != "$SIZE" ]; then
        FAILED=1; echo "✗ 日志声明 $INNER_ACTUAL bytes，实际落盘 $SIZE bytes —— 报告值与产物不符"; exit 1
    fi
    echo "  ✓ 日志自检: $INNER"
else
    echo "  ⚠ 日志里没有「最终文件」行（策略可能是 yt-dlp 子进程或 HLS 分支）"
fi

# 进度行不得大于文件本身（字节重复计数类 bug 的护栏）
LAST_PROGRESS=$(tr '\r' '\n' <"$LOG" | grep -o '已下载: [0-9.]* [KMGT]*B' | tail -1 || true)
if [ -n "$LAST_PROGRESS" ]; then
    NUM=$(echo "$LAST_PROGRESS" | grep -o '[0-9.]*' | head -1)
    UNIT=$(echo "$LAST_PROGRESS" | grep -o '[KMGT]*B$')
    case "$UNIT" in
        B)   PROG=$(printf '%.0f' "$NUM") ;;
        KB)  PROG=$(printf '%.0f' "$(echo "$NUM * 1024" | bc -l)") ;;
        MB)  PROG=$(printf '%.0f' "$(echo "$NUM * 1048576" | bc -l)") ;;
        GB)  PROG=$(printf '%.0f' "$(echo "$NUM * 1073741824" | bc -l)") ;;
        *)   PROG=0 ;;
    esac
    if [ "$PROG" -gt "$SIZE" ]; then
        FAILED=1; echo "✗ 进度行末值 $LAST_PROGRESS ≈ $PROG bytes > 文件 $SIZE bytes —— 进度字节疑似重复计数"; exit 1
    fi
    echo "  ✓ 进度末值 $LAST_PROGRESS ≤ 文件大小"
fi

if [ "$RECORD" = 1 ]; then
    echo
    echo "记录基准（下次用这行断言）:"
    echo "  bash scripts/check_download_artifact.sh \"$URL\" --expect-bytes $SIZE --expect-sha256 $SHA"
fi

[ -z "$EXPECT_BYTES" ] || if [ "$SIZE" != "$EXPECT_BYTES" ]; then
    FAILED=1; echo "✗ 字节数不符: 实际 $SIZE ≠ 期望 $EXPECT_BYTES"; exit 1
else
    echo "  ✓ 字节数 = 期望 $EXPECT_BYTES"
fi

[ -z "$EXPECT_SHA" ] || if [ "$SHA" != "$EXPECT_SHA" ]; then
    FAILED=1; echo "✗ sha256 不符: 实际 $SHA ≠ 期望 $EXPECT_SHA"; exit 1
else
    echo "  ✓ sha256 与对照版本逐字节相同"
fi

echo "✓ PASS"
