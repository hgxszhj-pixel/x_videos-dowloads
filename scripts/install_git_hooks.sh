#!/usr/bin/env bash
# 安装本地 Git Hooks (commit-msg + post-commit)
# 使用: bash scripts/install_git_hooks.sh

set -e

REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null)"
if [ -z "$REPO_ROOT" ]; then
    echo "❌ 未在 git 仓库中"
    exit 1
fi

HOOKS_DIR="$REPO_ROOT/.git/hooks"
SOURCE_DIR="$REPO_ROOT/scripts/git_hooks"

for hook in commit-msg post-commit; do
    src="$SOURCE_DIR/$hook"
    dst="$HOOKS_DIR/$hook"

    if [ ! -f "$src" ]; then
        echo "⚠️  源文件不存在: $src"
        continue
    fi

    if [ -f "$dst" ] && [ ! -L "$dst" ]; then
        echo "📦 备份现有 $hook 到 $dst.backup"
        mv "$dst" "$dst.backup"
    fi

    cp "$src" "$dst"
    chmod +x "$dst"
    echo "✅ 已安装: $dst"
done

echo ""
echo "🎉 Git hooks 安装完成"
echo "   - commit-msg: 自动注入 Agent DID + 信誉分"
echo "   - post-commit: 记录 commit 到信誉系统"