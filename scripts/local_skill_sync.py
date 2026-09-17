#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""local_skill_sync.py — 同步 gitee.com/tuling-python 的 skill / MCP 到 Claude Code 本地。

用法:
    python3 local_skill_sync.py doctor                    # 环境体检
    python3 local_skill_sync.py list                      # 列已安装 skill + symlink 状态
    python3 local_skill_sync.py list --json                # JSON 输出
    python3 local_skill_sync.py verify                     # 检查 symlink 断链 + SKILL.md 完整性
    python3 local_skill_sync.py check                      # 检测上游更新(git ls-remote)
    python3 local_skill_sync.py apply --dry-run            # 试运行(默认)
    python3 local_skill_sync.py apply --force              # 实际应用
    python3 local_skill_sync.py repair                     # 修复断链 symlink
    python3 local_skill_sync.py diff <name>                # 对比本地 vs 上游 SKILL.md
    python3 local_skill_sync.py sweep                      # check + apply + verify 一条龙

退出码:
    0   正常
    1   一般失败(网络 / git)
    2   有断链 / 目标缺失
    10  check 检测到上游更新
    11  apply 后出现新 skill

设计要点(参考 docs/dna/三档地盘.md 🟡 审查档):
    * 33 个 ~/.claude/skills symlink 进 preserve_symlinks(身份保留,绝不 git rm)
    * ~/.agents/skills 是真源(skill), ~/.claude/skills 是镜像层
    * ~/.local/share/skill-mirror 是 git clone 长期持有
    * .md/.py/.sh/.json/.yaml/.toml 强制 CRLF → LF(macOS 装 Windows-origin skill 血泪)
    * 不触碰 write-a-skill / ego-browser / understand(本地 fork / 跨边界)
    * 参考 tuling_upstream_sync.py 主脚本(987 行,纯 stdlib)
"""

from __future__ import annotations

import argparse
import difflib
import fnmatch
import json
import logging
import os
import shutil
import subprocess
import sys
import tarfile
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Callable, Dict, Iterable, List, Optional

# ---- 退出码 ----

EXIT_OK = 0
EXIT_FAIL = 1
EXIT_BROKEN = 2
EXIT_UPDATES = 10
EXIT_NEW = 11

# ---- 默认值 ----

DEFAULT_PATHS = {
    "mirror_root": "~/.local/share/skill-mirror",
    "agents_root": "~/.agents/skills",
    "claude_root": "~/.claude/skills",
    "backup_root": "~/.local/share/skill-mirror.bak",
    "mcp_root": "~/.local/share/mcp-tools",
}

DEFAULT_EXCLUDE = ["__pycache__", "*.egg-info", ".DS_Store", ".git"]
DEFAULT_VERIFY = {
    "check_crlf": True,
    "check_skill_md": True,
    "check_frontmatter": ["name", "description"],
    "file_extensions_lf": [".md", ".py", ".sh", ".json", ".yaml", ".toml"],
}

# tuling-python 公开的三个仓库(skill + MCP + 浏览器运行时)
DEFAULT_REPOS = [
    {
        "name": "web-reverse-engineering-skills",
        "url": "https://gitee.com/tuling-python/web-reverse-engineering-skills.git",
        "kind": "skill",
        "branch": "main",
        "subdir_layout": "top-level",
    },
    {
        "name": "web-reverse-mcps-tool",
        "url": "https://gitee.com/tuling-python/web-reverse-mcps-tool.git",
        "kind": "mcp",
        "branch": "main",
        "subdir_layout": "top-level",
    },
    {
        "name": "NV8",
        "url": "https://gitee.com/tuling-python/nv8.git",
        "kind": "git-repo",
        "branch": "master",
        "subdir_layout": "whole-repo",
        "target": "~/.local/share/nv8",
    },
]


# ---- 工具函数 ----

def expand(p: str) -> Path:
    """展开 ~ 与 ${home} 占位符。"""
    return Path(os.path.expandvars(os.path.expanduser(p)))


def run(cmd: List[str], *, check: bool = True, cwd: Optional[Path] = None,
        timeout: int = 60) -> subprocess.CompletedProcess:
    """subprocess wrapper,stderr 截断 300 字符(参考 tuling 行 220-237)。"""
    try:
        return subprocess.run(cmd, check=check, cwd=cwd,
                              capture_output=True, text=True, timeout=timeout)
    except subprocess.CalledProcessError as e:
        stderr = (e.stderr or "").strip()[:300]
        raise RuntimeError(f"Command failed: {' '.join(cmd)}\nstderr: {stderr}") from e
    except subprocess.TimeoutExpired as e:
        raise RuntimeError(f"Command timeout ({timeout}s): {' '.join(cmd)}") from e


def should_exclude(name: str, patterns: Iterable[str]) -> bool:
    """简单通配符匹配(* / ?),只引 fnmatch。"""
    return any(fnmatch.fnmatch(name, p) for p in patterns)


def setup_logging(log_dir: Path) -> logging.Logger:
    """stderr 流式日志 + 文件日志(每次启动一份)。"""
    log_dir.mkdir(parents=True, exist_ok=True)
    ts = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    log_file = log_dir / f"sync-{ts}.log"
    log = logging.getLogger("local_skill_sync")
    log.setLevel(logging.INFO)
    for h in list(log.handlers):
        log.removeHandler(h)
    fmt = logging.Formatter("[%(asctime)s] %(levelname)s %(message)s",
                            datefmt="%Y-%m-%dT%H:%M:%SZ")
    sh = logging.StreamHandler(sys.stderr)
    sh.setFormatter(fmt)
    log.addHandler(sh)
    fh = logging.FileHandler(log_file, encoding="utf-8")
    fh.setFormatter(fmt)
    log.addHandler(fh)
    log.info("log_file=%s", log_file)
    return log


def _default_config() -> Dict[str, Any]:
    return {
        "repos": DEFAULT_REPOS,
        "paths": dict(DEFAULT_PATHS),
        "exclude": list(DEFAULT_EXCLUDE),
        "verify": dict(DEFAULT_VERIFY),
        "preserve_symlinks": [],  # 首次跑 list --json 后由用户补全
    }


def load_config(path: Path) -> Dict[str, Any]:
    """读 JSON 配置;不存在则写默认模板。"""
    path = expand(str(path))
    if not path.exists():
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(_default_config(), indent=2, ensure_ascii=False))
        print(f"[info] created default config at {path}", file=sys.stderr)
    with path.open() as f:
        cfg = json.load(f)
    cfg.setdefault("paths", {})
    for k, v in DEFAULT_PATHS.items():
        cfg["paths"].setdefault(k, v)
    cfg.setdefault("exclude", DEFAULT_EXCLUDE)
    cfg.setdefault("verify", DEFAULT_VERIFY)
    cfg.setdefault("repos", DEFAULT_REPOS)
    cfg.setdefault("preserve_symlinks", [])
    return cfg


# ---- git / mirror ----

def git_remote_head(url: str, branch: str = "main") -> Optional[str]:
    """git ls-remote 取远端 HEAD commit hash。失败返回 None。"""
    try:
        result = run(["git", "ls-remote", "--heads", url, branch],
                     check=True, timeout=30)
        for line in result.stdout.splitlines():
            sha, ref = line.split("\t", 1)
            if ref == f"refs/heads/{branch}":
                return sha
    except (RuntimeError, ValueError):
        return None
    return None


def git_local_head(mirror_dir: Path) -> Optional[str]:
    """取本地 mirror HEAD commit hash(无 .git 返回 None)。"""
    if not (mirror_dir / ".git").exists():
        return None
    try:
        result = run(["git", "rev-parse", "HEAD"], cwd=mirror_dir,
                     check=True, timeout=10)
        return result.stdout.strip()
    except RuntimeError:
        return None


def ensure_mirror(url: str, mirror_dir: Path, branch: str,
                  log: logging.Logger) -> Path:
    """git clone(首次)或 fetch + reset --hard(后续)。"""
    if not (mirror_dir / ".git").exists():
        log.info("git clone %s -> %s", url, mirror_dir)
        mirror_dir.parent.mkdir(parents=True, exist_ok=True)
        run(["git", "clone", "--branch", branch, "--depth", "1", url, str(mirror_dir)],
            check=True, timeout=180)
    else:
        log.info("git fetch + reset --hard in %s", mirror_dir)
        run(["git", "fetch", "--depth", "1", "origin", branch],
            cwd=mirror_dir, check=True, timeout=60)
        run(["git", "reset", "--hard", f"origin/{branch}"],
            cwd=mirror_dir, check=True, timeout=30)
    return mirror_dir


def backup(local: Path, backup_root: Path, asset_id: str,
           log: logging.Logger) -> Optional[Path]:
    """改前 tar.gz 备份整个目录(参考 tuling 行 272-294)。"""
    if not local.exists():
        return None
    backup_root.mkdir(parents=True, exist_ok=True)
    ts = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    out = backup_root / f"{asset_id}_{ts}.tar.gz"
    log.info("backup %s -> %s", local, out)
    with tarfile.open(out, "w:gz") as tar:
        tar.add(local, arcname=local.name)
    return out


def mirror_skill(src: Path, dst: Path, exclude: Iterable[str],
                 log: logging.Logger) -> None:
    """mirror src 到 dst。**symlinks=True** 避免穿透(memory 血泪)。"""
    if dst.exists():
        shutil.rmtree(dst)
    dst.mkdir(parents=True)
    for child in src.iterdir():
        if should_exclude(child.name, exclude) or child.name.startswith("."):
            continue
        s = src / child.name
        d = dst / child.name
        if s.is_symlink():
            os.symlink(os.readlink(s), d)
        elif s.is_dir():
            shutil.copytree(s, d, symlinks=True)
        else:
            shutil.copy2(s, d)
    log.info("mirrored %s -> %s", src, dst)


# ---- verify / check / 完整性 ----

def verify_symlinks(claude_root: Path, preserve: Iterable[str],
                    log: logging.Logger) -> List[Dict[str, Any]]:
    """33 个 symlink 健康检查(os.path.realpath + exists)。"""
    broken: List[Dict[str, Any]] = []
    preserve_set = set(preserve)
    for entry in sorted(claude_root.iterdir()):
        if not entry.is_symlink():
            continue
        target_str = os.readlink(entry)
        target = Path(target_str)
        if not target.is_absolute():
            target = (entry.parent / target).resolve()
        info = {
            "name": entry.name,
            "link": str(entry),
            "target": str(target),
            "exists": target.exists(),
            "preserve": entry.name in preserve_set,
        }
        if not info["exists"]:
            log.warning("BROKEN symlink: %s -> %s", entry.name, info["target"])
            broken.append(info)
    return broken


def check_skill_md(skill_dir: Path, frontmatter_keys: Iterable[str]
                   ) -> List[str]:
    """SKILL.md 存在 + frontmatter 含必要字段(参考 skills/*.md 格式)。"""
    skill_md = skill_dir / "SKILL.md"
    issues: List[str] = []
    if not skill_md.exists():
        issues.append(f"missing SKILL.md at {skill_dir}")
        return issues
    text = skill_md.read_text(encoding="utf-8", errors="replace")
    if not text.startswith("---"):
        issues.append(f"no frontmatter in {skill_md}")
        return issues
    try:
        end = text.index("\n---", 3)
    except ValueError:
        issues.append(f"unclosed frontmatter in {skill_md}")
        return issues
    fm_block = text[3:end]
    for key in frontmatter_keys:
        if f"{key}:" not in fm_block:
            issues.append(f"frontmatter missing key '{key}' in {skill_md}")
    return issues


def fix_line_endings(root: Path, extensions: Iterable[str],
                     log: logging.Logger) -> int:
    """递归 CRLF → LF(macOS 装 Windows-origin skill 血泪)。"""
    fixed = 0
    ext_set = set(extensions)
    for p in root.rglob("*"):
        if not p.is_file() or p.suffix not in ext_set:
            continue
        try:
            data = p.read_bytes()
        except OSError as e:
            log.warning("read fail %s: %s", p, e)
            continue
        if b"\r\n" in data:
            p.write_bytes(data.replace(b"\r\n", b"\n"))
            log.info("CRLF→LF %s", p)
            fixed += 1
    return fixed


# ---- 子命令实现 ----

def cmd_doctor(cfg: Dict[str, Any], args: argparse.Namespace,
               log: logging.Logger) -> int:
    """环境体检:git / python3 / 关键路径 / gitee 网络。"""
    log.info("=== doctor ===")
    issues = 0
    # git
    try:
        r = run(["git", "--version"], check=True, timeout=10)
        log.info("git: %s", r.stdout.strip())
    except RuntimeError as e:
        log.error("git missing: %s", e)
        issues += 1
    log.info("python3: %s", sys.version.split()[0])
    # 路径
    claude_root = expand(cfg["paths"]["claude_root"])
    agents_root = expand(cfg["paths"]["agents_root"])
    mcp_root = expand(cfg["paths"]["mcp_root"])
    for label, root in [("claude_root", claude_root), ("agents_root", agents_root),
                        ("mcp_root", mcp_root)]:
        if not root.exists():
            log.warning("missing %s=%s(首次 apply 会创建)", label, root)
        else:
            n = sum(1 for _ in root.iterdir())
            log.info("%s=%s entries=%d", label, root, n)
    # gitee 网络探测(可选)
    for repo in cfg["repos"]:
        head = git_remote_head(repo["url"], repo.get("branch", "main"))
        if head:
            log.info("gitee OK %s @%s", repo["name"], head[:8])
        else:
            log.warning("gitee unreachable/empty: %s", repo["url"])
    log.info("=== doctor done issues=%d ===", issues)
    return EXIT_OK if issues == 0 else EXIT_FAIL


def cmd_list(cfg: Dict[str, Any], args: argparse.Namespace,
             log: logging.Logger) -> int:
    """列 ~/.claude/skills/* 类型与状态。"""
    claude_root = expand(cfg["paths"]["claude_root"])
    preserve = set(cfg["preserve_symlinks"])
    items: List[Dict[str, Any]] = []
    if not claude_root.exists():
        log.warning("claude_root missing: %s", claude_root)
        if args.json:
            print(json.dumps(items, indent=2, ensure_ascii=False))
        return EXIT_FAIL
    for entry in sorted(claude_root.iterdir()):
        info: Dict[str, Any] = {"name": entry.name}
        if entry.is_symlink():
            target = os.readlink(entry)
            if not os.path.isabs(target):
                target = str((entry.parent / target).resolve())
            info.update({"type": "symlink", "target": target,
                         "exists": Path(target).exists(),
                         "preserve": entry.name in preserve})
        elif entry.is_dir():
            info.update({"type": "copy", "path": str(entry)})
        else:
            continue  # 跳过文件
        items.append(info)
    if args.json:
        print(json.dumps(items, indent=2, ensure_ascii=False))
    else:
        for it in items:
            mark = "L" if it["type"] == "symlink" else "D"
            extra = ""
            if it["type"] == "symlink":
                extra = f" -> {it['target']}"
                if not it["exists"]:
                    extra += " [MISSING]"
                if it.get("preserve"):
                    extra += " [PRESERVE]"
            log.info("  %s %s%s", mark, it["name"], extra)
        log.info("total=%d", len(items))
        # 提示:33 个 symlink 名单生成
        symlinks = [it["name"] for it in items if it["type"] == "symlink"]
        log.info("symlinks=%d → 加入 preserve_symlinks: %s",
                 len(symlinks), json.dumps(symlinks, ensure_ascii=False))
    return EXIT_OK


def cmd_verify(cfg: Dict[str, Any], args: argparse.Namespace,
               log: logging.Logger) -> int:
    """symlink 断链 + SKILL.md frontmatter 完整性。"""
    log.info("=== verify ===")
    claude_root = expand(cfg["paths"]["claude_root"])
    agents_root = expand(cfg["paths"]["agents_root"])
    fm_keys = cfg["verify"]["check_frontmatter"]
    broken = verify_symlinks(claude_root, cfg["preserve_symlinks"], log) \
        if claude_root.exists() else []
    skill_issues: List[Dict[str, str]] = []
    if agents_root.exists():
        for entry in agents_root.iterdir():
            if not entry.is_dir() or entry.is_symlink():
                continue
            for issue in check_skill_md(entry, fm_keys):
                skill_issues.append({"skill": entry.name, "issue": issue})
    args_json = args.json
    report = {"broken_symlinks": broken, "skill_md_issues": skill_issues}
    if args_json:
        print(json.dumps(report, indent=2, ensure_ascii=False))
    log.info("=== verify done: %d broken, %d SKILL.md issues ===",
              len(broken), len(skill_issues))
    return EXIT_OK if not broken and not skill_issues else EXIT_BROKEN


def cmd_check(cfg: Dict[str, Any], args: argparse.Namespace,
              log: logging.Logger) -> int:
    """git ls-remote 检测上游更新。"""
    log.info("=== check upstream ===")
    mirror_root = expand(cfg["paths"]["mirror_root"])
    updates: List[Dict[str, Any]] = []
    for repo in cfg["repos"]:
        url = repo["url"]
        branch = repo.get("branch", "main")
        remote = git_remote_head(url, branch)
        local_mirror = mirror_root / repo["name"]
        local = git_local_head(local_mirror)
        if not local:
            status = "new"
        elif not remote:
            status = "unreachable"
        elif local == remote:
            status = "up-to-date"
        else:
            status = "behind"
        log.info("  %-40s remote=%s local=%-8s status=%s",
                 repo["name"], (remote or "?")[:8], (local or "absent")[:8], status)
        if status not in ("up-to-date",):
            updates.append({"repo": repo["name"], "status": status,
                            "remote": remote, "local": local})
    if args.json:
        print(json.dumps(updates, indent=2, ensure_ascii=False))
    return EXIT_UPDATES if updates else EXIT_OK


def cmd_apply(cfg: Dict[str, Any], args: argparse.Namespace,
              log: logging.Logger) -> int:
    """fetch + mirror + CRLF 修复 + SKILL.md 体检。--force 真跑,否则 dry-run。"""
    dry_run = not args.force
    log.info("=== apply (dry_run=%s, force=%s) ===", dry_run, args.force)
    mirror_root = expand(cfg["paths"]["mirror_root"])
    agents_root = expand(cfg["paths"]["agents_root"])
    mcp_root = expand(cfg["paths"]["mcp_root"])
    backup_root = expand(cfg["paths"]["backup_root"])
    exclude = cfg["exclude"]
    exts = cfg["verify"]["file_extensions_lf"]
    check_crlf = cfg["verify"]["check_crlf"]
    check_skill = cfg["verify"]["check_skill_md"]
    fm_keys = cfg["verify"]["check_frontmatter"]
    new_skills: List[Dict[str, str]] = []
    for repo in cfg["repos"]:
        name = repo["name"]
        url = repo["url"]
        kind = repo.get("kind", "skill")
        branch = repo.get("branch", "main")
        mirror_dir = mirror_root / name
        try:
            ensure_mirror(url, mirror_dir, branch, log)
        except RuntimeError as e:
            log.error("mirror failed %s: %s", name, e)
            return EXIT_FAIL
        # 目标根(按 kind 路由)
        if kind == "skill":
            target_root = agents_root
        elif kind == "mcp":
            target_root = mcp_root
        elif kind == "git-repo":
            # NV8 类:整仓镜像到 repo.target(不是顶层子目录布局)
            target_str = repo.get("target")
            if not target_str:
                log.warning("git-repo %s missing 'target', skip", name)
                continue
            target_root = expand(target_str)
            if not dry_run:
                target_root.parent.mkdir(parents=True, exist_ok=True)
                if target_root.exists():
                    backup(target_root, backup_root, name, log)
                mirror_skill(mirror_dir, target_root, exclude, log)
            log.info("git-repo %s -> %s (whole repo, dry_run=%s)",
                     name, target_root, dry_run)
            continue  # 跳过顶层子目录循环
        else:
            log.warning("unknown kind=%s for %s, skip", kind, name)
            continue
        if not dry_run:
            target_root.mkdir(parents=True, exist_ok=True)
        # 顶层子目录布局(plan 已确认)
        if repo.get("subdir_layout", "top-level") != "top-level":
            log.warning("non-top-level layout not supported: %s",
                        repo.get("subdir_layout"))
            continue
        for entry in mirror_dir.iterdir():
            if not entry.is_dir() or entry.name in exclude \
                    or entry.name.startswith("."):
                continue
            dst = target_root / entry.name
            is_new = not dst.exists()
            if is_new:
                new_skills.append({"repo": name, "name": entry.name, "kind": kind})
                log.info("NEW %s/%s -> %s (kind=%s)", name, entry.name, dst, kind)
            else:
                log.info("update %s/%s -> %s", name, entry.name, dst)
            if not dry_run and dst.exists():
                backup(dst, backup_root, entry.name, log)
            if not dry_run:
                mirror_skill(entry, dst, exclude, log)
                if check_crlf and kind == "skill":
                    fix_line_endings(dst, exts, log)
                if check_skill and kind == "skill":
                    for issue in check_skill_md(dst, fm_keys):
                        log.warning("SKILL.md: %s", issue)
    log.info("=== apply done new=%d dry_run=%s ===", len(new_skills), dry_run)
    if args.json:
        print(json.dumps({"new": new_skills, "dry_run": dry_run}, indent=2))
    if not dry_run:
        log.info("⚠️  重启 Claude Code 后 MCP 配置才会生效")
    if new_skills and not dry_run:
        return EXIT_NEW
    return EXIT_OK


def cmd_repair(cfg: Dict[str, Any], args: argparse.Namespace,
               log: logging.Logger) -> int:
    """修复断链 symlink:在 ~/.agents/skills 找同名的真源重建。"""
    log.info("=== repair (dry_run=%s) ===", not args.force)
    dry_run = not args.force
    claude_root = expand(cfg["paths"]["claude_root"])
    agents_root = expand(cfg["paths"]["agents_root"])
    if not claude_root.exists():
        log.error("claude_root missing: %s", claude_root)
        return EXIT_FAIL
    fixed: List[str] = []
    failed: List[Dict[str, str]] = []
    for entry in sorted(claude_root.iterdir()):
        if not entry.is_symlink():
            continue
        target_str = os.readlink(entry)
        target = Path(target_str)
        if not target.is_absolute():
            target = (entry.parent / target).resolve()
        if target.exists():
            continue
        candidate = agents_root / entry.name
        if candidate.exists():
            if dry_run:
                log.info("WOULD repair %s -> %s", entry.name, candidate)
            else:
                entry.unlink()
                os.symlink(str(candidate), str(entry))
                log.info("repaired %s -> %s", entry.name, candidate)
            fixed.append(entry.name)
        else:
            failed.append({"name": entry.name, "broken_link": str(target)})
            log.warning("cannot repair %s (no candidate at %s)", entry.name, candidate)
    if args.json:
        print(json.dumps({"fixed": fixed, "failed": failed}, indent=2))
    return EXIT_OK if not failed else EXIT_BROKEN


def cmd_diff(cfg: Dict[str, Any], args: argparse.Namespace,
             log: logging.Logger) -> int:
    """对比本地 ~/.agents/skills/<name> vs mirror gitee SKILL.md。"""
    name = args.name
    agents_root = expand(cfg["paths"]["agents_root"])
    mirror_root = expand(cfg["paths"]["mirror_root"])
    skill_repo = next((r for r in cfg["repos"]
                       if r.get("kind", "skill") == "skill"), None)
    if not skill_repo:
        log.error("no skill repo in config")
        return EXIT_FAIL
    local = agents_root / name / "SKILL.md"
    remote_mirror = mirror_root / skill_repo["name"] / name / "SKILL.md"
    if not local.exists():
        log.error("local missing: %s", local)
        return EXIT_FAIL
    if not remote_mirror.exists():
        log.error("remote missing: %s", remote_mirror)
        return EXIT_FAIL
    local_text = local.read_text(encoding="utf-8", errors="replace")
    remote_text = remote_mirror.read_text(encoding="utf-8", errors="replace")
    if local_text == remote_text:
        log.info("%s: identical", name)
        return EXIT_OK
    diff = list(difflib.unified_diff(
        local_text.splitlines(), remote_text.splitlines(),
        fromfile=f"local/{name}", tofile=f"remote/{name}", lineterm=""))
    log.info("=== diff %s (%d lines) ===", name, len(diff))
    for line in diff[:80]:
        print(line)
    return EXIT_OK


def cmd_sweep(cfg: Dict[str, Any], args: argparse.Namespace,
              log: logging.Logger) -> int:
    """check + apply + verify 串联。"""
    log.info("=== sweep start ===")
    crc = cmd_check(cfg, args, log)
    if crc == EXIT_UPDATES:
        log.info("updates detected, running apply")
        cmd_apply(cfg, args, log)
    vrc = cmd_verify(cfg, args, log)
    log.info("=== sweep done check=%d verify=%d ===", crc, vrc)
    return vrc if vrc != EXIT_OK else crc


# ---- argparse 路由 ----

HANDLERS: Dict[str, Callable[..., int]] = {
    "doctor": cmd_doctor,
    "list": cmd_list,
    "verify": cmd_verify,
    "check": cmd_check,
    "apply": cmd_apply,
    "repair": cmd_repair,
    "diff": cmd_diff,
    "sweep": cmd_sweep,
}


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="local_skill_sync",
        description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument("--config", type=Path,
                        default=Path(__file__).parent / "local_skill_sync.config.json",
                        help="配置文件路径(默认:脚本同目录 local_skill_sync.config.json)")
    parser.add_argument("--json", action="store_true", help="JSON 输出")
    sub = parser.add_subparsers(dest="command", required=True, metavar="<subcommand>")
    # (name, help, kind): None=无 flag, "force"=有 dry-run/force, "diff"=有 name 位置参数
    subcommands = [
        ("doctor", "环境体检(git/python3/路径/gitee 网络)", None),
        ("list", "列已安装 skill + symlink 状态", None),
        ("verify", "symlink 断链 + SKILL.md 完整性", None),
        ("check", "检测上游更新(git ls-remote)", None),
        ("apply", "fetch + mirror + CRLF 修复(--force 真跑)", "force"),
        ("repair", "修复断链 symlink(--force 真跑)", "force"),
        ("diff", "对比本地 vs gitee SKILL.md(需 name 参数)", "diff"),
        ("sweep", "check + apply + verify 一条龙", None),
    ]
    for name, help_text, kind in subcommands:
        sp = sub.add_parser(name, help=help_text)
        if kind == "force":
            sp.add_argument("--dry-run", dest="dry_run", action="store_true",
                            default=True, help="试运行(默认)")
            sp.add_argument("--force", action="store_true",
                            help="实际应用(覆盖 dry-run)")
        elif kind == "diff":
            sp.add_argument("name", help="skill 名字")
    return parser


def main() -> int:
    parser = build_parser()
    args = parser.parse_args()
    cfg = load_config(args.config)
    log_dir = expand(cfg["paths"]["mirror_root"]) / ".log"
    log = setup_logging(log_dir)
    log.info("command=%s config=%s", args.command, args.config)
    try:
        return HANDLERS[args.command](cfg, args, log)
    except RuntimeError as e:
        log.error("FATAL: %s", e)
        return EXIT_FAIL


if __name__ == "__main__":
    sys.exit(main())