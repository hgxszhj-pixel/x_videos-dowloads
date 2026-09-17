---
name: local-skill-sync
description: 本地 Claude Code skill / MCP 同步工具 — 从 gitee.com/tuling-python 单仓同步 skill + MCP + NV8,带 33 symlink 健康维护。Use when user mentions "同步本地 skill" / "tuling-python skill 更新" / "Claude Code MCP 更新" / "skill 断链修复"。
---

# local-skill-sync

把 `gitee.com/tuling-python` 的 3 个仓库同步到 Claude Code 本机:
- **web-reverse-engineering-skills** → `~/.agents/skills/<name>/`(顶层子目录布局)
- **web-reverse-mcps-tool** → `~/.local/share/mcp-tools/<name>/`
- **NV8**(Node.js 浏览器运行时,master 分支)→ `~/.local/share/nv8/`

**核心特性**: 33 个 `~/.claude/skills/` symlink 全部进 `preserve_symlinks` 硬名单,身份保留绝不覆写(参考 memory `claude-skills-symlink-pattern.md`)。

## 快速开始

```bash
# 1. 环境体检
python3 scripts/local_skill_sync.py doctor
# 输出 git / python3 / 关键目录 / gitee 网络探测
# 期望: 3 个仓库全部 gitee OK

# 2. 列出已安装 skill + 状态
python3 scripts/local_skill_sync.py list
# 输出 L = symlink, D = copy, [MISSING] = 断链, [PRESERVE] = 硬名单
# 同时显示 symlinks=N → 加入 preserve_symlinks: [...]

# 3. 检查 symlink 健康 + SKILL.md 完整性
python3 scripts/local_skill_sync.py verify
# EXIT=0 健康, EXIT=2 有断链 / frontmatter 缺失

# 4. 检测上游更新
python3 scripts/local_skill_sync.py check
# EXIT=0 无更新, EXIT=10 有更新

# 5. 试运行(默认 dry-run,不会真改)
python3 scripts/local_skill_sync.py apply
python3 scripts/local_skill_sync.py apply --dry-run    # 等价

# 6. 实际应用(必须显式 --force)
python3 scripts/local_skill_sync.py apply --force
# 改前自动备份到 ~/.local/share/skill-mirror.bak/<name>_<ts>.tar.gz

# 7. 修复断链 symlink
python3 scripts/local_skill_sync.py repair             # dry-run
python3 scripts/local_skill_sync.py repair --force    # 真跑

# 8. 对比单个 skill 与上游
python3 scripts/local_skill_sync.py diff <skill-name>

# 9. 一条龙:check + apply + verify
python3 scripts/local_skill_sync.sh sweep
```

## 子命令速查

| 子命令 | 副作用 | 退出码 | 说明 |
|--------|--------|--------|------|
| `doctor` | 无 | 0/1 | 环境体检 + gitee 网络探测 |
| `list`   | 无 | 0 | 列 `~/.claude/skills/*` 状态 |
| `verify` | 无 | 0/2 | symlink 断链 + SKILL.md frontmatter 完整性 |
| `check`  | 无(只 git ls-remote) | 0/10 | 检测上游更新 |
| `apply`  | ⚠️ 默认 dry-run | 0/1/11 | fetch + 备份 + CRLF 修复 + SKILL.md 体检 |
| `repair` | ⚠️ 默认 dry-run | 0/2 | 重建断链 symlink 指向 `~/.agents/skills/<name>` |
| `diff`   | 无 | 0/1 | 本地 vs gitee SKILL.md unified diff |
| `sweep`  | ⚠️ 默认 dry-run | 0/2/10/11 | check + apply + verify 串联 |

## 退出码语义化

- `0` 正常
- `1` 一般失败(网络 / git 命令)
- `2` 有断链 / 目标缺失 / SKILL.md 缺失
- `10` `check` 检测到上游更新(可触发 apply)
- `11` `apply` 后出现新 skill(可考虑在 `~/.claude/skills/` 手动创建 symlink)

## 配置要点(`scripts/local_skill_sync.config.json`)

| 字段 | 说明 |
|------|------|
| `repos[].kind` | `skill` / `mcp` / `git-repo`,决定同步到哪里 |
| `repos[].subdir_layout` | `top-level` = 顶层每子目录 1 skill;`whole-repo` = 整仓镜像(NV8) |
| `repos[].target` | 仅 `git-repo` 需要,整仓镜像的目标路径 |
| `repos[].branch` | 默认 `main`,**NV8 必须用 `master`** |
| `paths.mirror_root` | git clone 长期持有目录(`~/.local/share/skill-mirror`) |
| `paths.agents_root` | skill 真源(`~/.agents/skills`) |
| `paths.claude_root` | skill 镜像层(`~/.claude/skills`),**只读** — 不写 |
| `paths.mcp_root` | MCP 镜像根(`~/.local/share/mcp-tools`) |
| `paths.backup_root` | 改前备份根(`~/.local/share/skill-mirror.bak`) |
| `preserve_symlinks` | **33 个硬名单**,身份保留绝不覆写 |
| `verify.file_extensions_lf` | CRLF → LF 的扩展名列表 |

## 安全与回滚

### 已知血泪(必避)

1. **rsync/cp 穿透 symlink**(memory `claude-skills-symlink-pattern.md`)
   - 规避: `shutil.copytree(..., symlinks=True)`,脚本所有 mirror 调用都传
2. **git rm `~/.claude/skills/*`**(git 跟踪 symlink 对象本身)
   - 规避: 本工具**绝不** `git rm` `~/.claude/skills/*`,只读 `ls -l`
3. **`write-a-skill` 已 fork**(`~/.agents/skills/write-a-skill.bak.20260824`)
   - 规避: 本工具**不**触碰该 skill,`list/verify` 标"local fork" 警告
4. **CRLF(macOS 装 Windows-origin skill)**
   - 规避: `fix_line_endings` 对 6 种扩展名强制 LF(md/py/sh/json/yaml/toml)
5. **`~/.claude/skills` git 跟踪 symlink 对象本身**
   - 规避: 本工具对 `~/.claude/skills/*` 只读 `ls -l`,不写

### 回滚策略

每次 `apply --force` **覆盖现有资产前**自动 tar.gz 备份(NEW 全新资产无备份,因为没有旧内容可打包):
```
~/.local/share/skill-mirror.bak/<skill_name>_<UTC-ts>.tar.gz
```
恢复:
```bash
mkdir -p ~/.agents/skills/<skill_name>
tar -xzf ~/.local/share/skill-mirror.bak/<skill_name>_*.tar.gz -C ~/.agents/skills/
```

`~/.claude/skills` 是镜像层 symlink,git checkout 即可回滚:
```bash
cd ~/.claude && git checkout skills/<name>
```

## 它不会做的事

- ❌ 调度(`schedule`):项目 .git/hooks/post-commit 已存在,无需 cron/LaunchAgent
- ❌ Windows / Linux 适配(本机 macOS only,首版)
- ❌ 跨 agent 副本(单仓 single agent)
- ❌ 删除 `~/.claude/skills/*`(只读 `ls -l`)
- ❌ 触碰 `write-a-skill` / `ego-browser` / `understand` 内部内容
- ❌ 网络代理配置(`gitee` 直连)

## 已验证(2026-09-17)

- ✅ `py_compile` 语法门禁
- ✅ `--help` argparse 8 子命令齐全
- ✅ `doctor`: git 2.55 / python 3.11.7 / 107 claude_root entries / 111 agents_root entries / 3 gitee 仓库全部 OK
- ✅ `verify`: 0 broken symlink / 10 SKILL.md frontmatter issues(已识别为非 sync 问题)
- ✅ `check`: 3 仓库 commit hash 探测 `local=absent` → `status=new`
- ✅ `list`: 输出 33 symlink + 78 copy 分类正确

## 相关文档

- **Plan**: `~/.claude/plans/lexical-painting-pumpkin.md`
- **参考**: `/Users/hgxszhj/Downloads/tuling-upstream-sync-v1.0.0/`
- **Memory 血泪**: `~/.claude/projects/-Users-hgxszhj-x-videos-dowloads/memory/MEMORY.md:claude-skills-symlink-pattern.md`
- **dna 三份**: `docs/dna/三档地盘.md` 🟡 审查档 / `docs/dna/review-checklist.md` 5 阶段 / `docs/dna/新功能前必查.md`