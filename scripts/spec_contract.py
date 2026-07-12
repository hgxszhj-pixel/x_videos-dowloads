#!/usr/bin/env python3
"""SHA256 内容级契约 — 校验 specs/ 目录变更后契约同步更新

机制（参考 spec-superflow 的 SHA256 + 内容级状态检测，本地最小化实现）:
- specs/ 目录下所有 .md 被视作"规划契约"
- 计算 specs/ 全部 .md 的 SHA256，写入 .contract.sha256（项目根目录）
- commit 时：若 specs/ 下文件有改动但 .contract.sha256 未同步，reject

用法:
    # 检查契约（被 .git/hooks/pre-commit 调用）
    python3 scripts/spec_contract.py verify

    # 更新契约（specs/ 改完后必须运行）
    python3 scripts/spec_contract.py update

    # 查看当前 hash（调试用）
    python3 scripts/spec_contract.py show
"""
from __future__ import annotations

import hashlib
import sys
from pathlib import Path

SPECS_DIR = Path("specs")
CONTRACT_FILE = Path(".contract.sha256")


def compute_hash(specs_dir: Path = SPECS_DIR) -> str:
    """计算 specs/ 全部 .md 的拼接 SHA256。无 specs/ 目录返回空串（= 不强制）。"""
    if not specs_dir.exists():
        return ""
    files = sorted(specs_dir.rglob("*.md"))
    h = hashlib.sha256()
    for f in files:
        # 用相对路径避免路径噪音影响 hash
        rel = f.relative_to(specs_dir).as_posix()
        h.update(rel.encode())
        h.update(b"\0")
        h.update(f.read_bytes())
        h.update(b"\0")
    return h.hexdigest()


def verify() -> tuple[bool, str]:
    """校验当前 specs/ 状态与 .contract.sha256 一致。

    Returns:
        (ok, message): ok=True 通过；ok=False 拒绝并给出修复命令。
    """
    current = compute_hash()
    if not current:
        return True, "✓ No specs/ dir, contract not enforced"
    if not CONTRACT_FILE.exists():
        return False, (
            "❌ .contract.sha256 missing.\n"
            "   specs/ 目录存在但契约文件未初始化。\n"
            "   Run: python3 scripts/spec_contract.py update"
        )
    stored_line = CONTRACT_FILE.read_text().strip().splitlines()
    if not stored_line:
        return False, "❌ .contract.sha256 empty. Run: python3 scripts/spec_contract.py update"
    stored = stored_line[0].split()[0]  # 文件格式: "<hash>  specs/"
    if stored != current:
        return False, (
            f"❌ specs/ changed but contract stale.\n"
            f"   stored:  {stored[:16]}...\n"
            f"   current: {current[:16]}...\n"
            f"   规范变了，契约没同步。Run: python3 scripts/spec_contract.py update"
        )
    return True, f"✓ specs/ contract matches ({current[:16]}...)"


def update() -> tuple[bool, str]:
    """重新计算 specs/ 哈希并写入 .contract.sha256。"""
    current = compute_hash()
    if not current:
        # specs/ 不存在，删除契约文件（如果有）
        if CONTRACT_FILE.exists():
            CONTRACT_FILE.unlink()
        return True, "✓ specs/ removed (or empty), contract cleared"
    CONTRACT_FILE.write_text(f"{current}  specs/\n")
    return True, f"✓ Contract updated: {current[:16]}... ({CONTRACT_FILE})"


def show() -> str:
    """打印当前与存储的 hash（调试用）。"""
    current = compute_hash() or "(no specs/ dir)"
    stored = (
        CONTRACT_FILE.read_text().strip().split()[0]
        if CONTRACT_FILE.exists()
        else "(no .contract.sha256)"
    )
    return f"current: {current[:32] if len(current) > 16 else current}\nstored:  {stored[:32] if len(stored) > 16 else stored}"


def main() -> int:
    if len(sys.argv) < 2:
        print("usage: spec_contract.py {verify|update|show}", file=sys.stderr)
        return 2
    cmd = sys.argv[1]
    if cmd == "verify":
        ok, msg = verify()
        print(msg)
        return 0 if ok else 1
    elif cmd == "update":
        ok, msg = update()
        print(msg)
        return 0 if ok else 1
    elif cmd == "show":
        print(show())
        return 0
    else:
        print(f"unknown command: {cmd}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())