#!/bin/sh
# 双击这个文件就能开查题页（不用敲命令）。关掉这个终端窗口 = 关掉查题页。
cd "$(dirname "$0")" || exit 1
exec python3 webui.py
