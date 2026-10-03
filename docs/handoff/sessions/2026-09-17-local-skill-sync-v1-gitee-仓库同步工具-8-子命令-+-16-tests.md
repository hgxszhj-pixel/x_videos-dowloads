# 2026-09-17 — local-skill-sync v1 (gitee 仓库同步工具, 8 子命令 + 16 tests)


### 1. 工具交付

`scripts/local_skill_sync.py` (~570 行, 纯 stdlib, 参考 tuling_upstream_sync.py 设计):
- 8 子命令: doctor / list / verify / check / apply / repair / diff / sweep
- 退出码语义化: 0/1/2/10/11
- 3 仓库默认: web-reverse-engineering-skills (main) + web-reverse-mcps-tool (main) + NV8 (**master**)
- 33 个 `~/.claude/skills` symlink 进 `preserve_symlinks` 硬名单(身份保留绝不覆写)
- 改前自动备份 `~/.local/share/skill-mirror.bak/<name>_<ts>.tar.gz`
- CRLF→LF 对 .md/.py/.sh/.json/.yaml/.toml 强制(macOS 装 Windows-origin skill 血泪)
- kind 路由: `skill` → `~/.agents/skills` / `mcp` → `~/.local/share/mcp-tools` / `git-repo` → 整仓镜像(NV8)

### 2. 验证结果 (2026-09-17 实测)

| 子命令 | 结果 |
|--------|------|
| `py_compile` | ✅ 语法门禁 |
| `--help` | ✅ 8 子命令齐全 |
| `doctor` | ✅ git 2.55 / py 3.11.7 / 107 claude_root / 111 agents_root / **3 仓库 gitee OK** |
| `verify` | ✅ **0 broken symlink** / 10 SKILL.md frontmatter issues (已识别为非 sync 问题) |
| `check`  | ✅ 3 仓库 commit hash 探测(local=absent → status=new) |
| `unittest` | ✅ **16 tests / 0.011s**(CRLF / 断链 / 不穿透 symlink 三个核心 case) |

### 3. 交付物清单

- `scripts/local_skill_sync.py` — 主脚本 (~570 行)
- `scripts/local_skill_sync.config.example.json` — 配置模板 (33 symlink 预填)
- `scripts/local_skill_sync.config.json` — 本机配置(auto-created, **gitignore**)
- `scripts/run_local_skill_sync.sh` — bash 包装
- `tests/test_local_skill_sync.py` — 单元测试 (16 cases)
- `skills/local-skill-sync/SKILL.md` — 方法论 skill
- Plan: `~/.claude/plans/lexical-painting-pumpkin.md`

### 4. 已知血泪规避 (dna 🟡 审查档)

- rsync/cp 穿透 symlink → `shutil.copytree(symlinks=True)`
- `git rm ~/.claude/skills/*` → 本工具只读 `ls -l`
- `write-a-skill` 已 fork → 本工具不触碰
- CRLF macOS 血泪 → `fix_line_endings` 6 扩展名强制 LF

### 5. 待办 follow-up

- ⏳ 用户首次跑 `apply --force` 真同步(默认 dry-run 机制已验证)
- ⏳ 33 个 symlink 名单可考虑 `list --json` 自动生成(目前手工预填 config.example.json)
- ⏳ `sweep` 调 `apply` 时未透传 `--force`(设计默认 sweep 是 dry-run)
- ⏳ `~/.local/share/skill-mirror` 首次 git clone ~120KB+ 较慢,考虑 `--depth 1` 已 OK

