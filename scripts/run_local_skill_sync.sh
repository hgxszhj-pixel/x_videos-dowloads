#!/usr/bin/env bash
# run_local_skill_sync.sh — bash 包装,自动找 python3 + 同目录脚本 + config
#
# 用法:
#   ./run_local_skill_sync.sh doctor                    # 环境体检
#   ./run_local_skill_sync.sh list                      # 列已安装 skill
#   ./run_local_skill_sync.sh verify                     # 33 symlink 健康检查
#   ./run_local_skill_sync.sh check                      # 上游更新检测
#   ./run_local_skill_sync.sh apply --dry-run            # 试运行(默认)
#   ./run_local_skill_sync.sh apply --force              # 实际应用
#   ./run_local_skill_sync.sh repair                     # 修复断链 symlink
#   ./run_local_skill_sync.sh diff <name>                # 对比本地 vs gitee
#   ./run_local_skill_sync.sh sweep                      # check + apply + verify

set -euo pipefail

# 自动定位脚本同目录
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# 优先 PYTHON3 环境变量,否则探测
if [[ -n "${PYTHON3:-}" ]]; then
    PY="$PYTHON3"
elif command -v python3 >/dev/null 2>&1; then
    PY="$(command -v python3)"
elif command -v python >/dev/null 2>&1; then
    PY="$(command -v python)"
else
    echo "[error] no python3/python found in PATH" >&2
    exit 1
fi

# 让 user 强制指定 config 时也生效
if [[ -n "${LOCAL_SKILL_SYNC_CONFIG:-}" ]]; then
    CONFIG_FLAG=(--config "$LOCAL_SKILL_SYNC_CONFIG")
else
    # 默认 config 就在脚本同目录
    CONFIG_FLAG=()
fi

exec "$PY" "$SCRIPT_DIR/local_skill_sync.py" "${CONFIG_FLAG[@]}" "$@"