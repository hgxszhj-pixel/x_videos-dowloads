#!/usr/bin/env bash
# mutation_smoke.sh - 防 AI 写"假测试"
#
# 起源: 2026-09-13 采纳「五派」中的 机器审派 + 消灭派
# 目的: 变异测试 (mutation testing) 抓 "测试在说谎" 的情况
#   - 经典场景: AI 照着实现写测试 → bug 实现 + 配套测试 → 全绿 → 上线炸
#   - 变异测试故意改 1-2 行 expected behavior,如果测试还全绿 = 测试在说谎
#
# 用法:
#   bash scripts/mutation_smoke.sh           # 跑 cargo mutants 全量 (慢,~5-15 分钟)
#   bash scripts/mutation_smoke.sh smoke     # 单文件 smoke (快,~30 秒,适合 pre-commit)
#   bash scripts/mutation_smoke.sh manual <file> <line> <old> <new>  # 单点手动变异
#
# 退出码:
#   0 = mutations killed (测试有效)
#   1 = mutations survived (测试在说谎,需要修)
#   2 = 工具未安装 / 配置错误 (warn,不是 fail)

set -e

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

MODE="${1:-smoke}"

warn_no_cargo_mutants() {
    if ! command -v cargo-mutants >/dev/null 2>&1; then
        echo "⚠️  cargo-mutants 未安装"
        echo "   安装: cargo install cargo-mutants"
        echo "   文档: https://mutants.rs/"
        echo "   跳过变异测试,继续常规 quality check..."
        return 1
    fi
    return 0
}

run_full() {
    echo "🧬 Running cargo mutants (全量,预计 5-15 分钟)..."
    if warn_no_cargo_mutants; then
        cargo mutants --bin x-video-downloader --output ./target/mutants
    fi
}

run_smoke() {
    echo "🧬 Running cargo mutants smoke (单文件快速验证)..."
    if warn_no_cargo_mutants; then
        # 优先变异审查档的核心模块,这些是 "测试在说谎" 影响最大的地方
        cargo mutants --bin x-video-downloader \
            --file src/core/segment.rs \
            --output ./target/mutants-smoke \
            --no-shuffle \
            --jobs 2
    fi
}

run_manual() {
    # 参数: <file> <line> <old> <new>
    local file="$1"
    local line="$2"
    local old="$3"
    local new="$4"

    if [ -z "$file" ] || [ -z "$line" ] || [ -z "$old" ] || [ -z "$new" ]; then
        echo "用法: bash scripts/mutation_smoke.sh manual <file> <line> <old> <new>"
        echo "示例: bash scripts/mutation_smoke.sh manual src/core/segment.rs 42 '==' '!='"
        exit 2
    fi

    echo "🧬 Manual mutation: $file:$line  '$old' → '$new'"
    local backup="${file}.bak-mutation"
    cp "$file" "$backup"
    trap "mv '$backup' '$file' && echo '✅ Reverted mutation'" EXIT

    # 用 sed 替换指定行
    sed -i.bak "${line}s|${old}|${new}|" "$file"
    rm -f "${file}.bak"

    echo "🧪 Running tests..."
    if bash scripts/track_quality.sh test >/dev/null 2>&1; then
        echo "❌ MUTATION SURVIVED — 测试在说谎!"
        echo "   $file:$line 改为 '$new' 后测试还是全绿"
        echo "   需要补测试,或原有测试只是测了实现路径没测契约"
        exit 1
    else
        echo "✅ MUTATION KILLED — 测试有效"
        exit 0
    fi
}

case "$MODE" in
    full)
        run_full
        ;;
    smoke)
        run_smoke
        ;;
    manual)
        # shift MODE,剩下的是 <file> <line> <old> <new>
        run_manual "$2" "$3" "$4" "$5"
        ;;
    *)
        echo "Usage: $0 [smoke|full|manual <file> <line> <old> <new>]"
        exit 2
        ;;
esac
