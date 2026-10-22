---
name: 2026-10-15-OMC-HUD-install-+-marketplace-SHA-pin-lag
description: oh-my-claudecode HUD 首次安装 (omc-hud.mjs + statusLine 切换) + mattpocock-skills 升级失败根因诊断 (marketplace.json SHA pin 落后 upstream)
type: session-log
---

# 2026-10-15 — OMC HUD 安装 + mattpocock-skills marketplace SHA pin 滞后诊断

## 触发

启动 system reminder: `[OMC] HUD not configured (HUD script missing). Run /hud setup then restart Claude Code.`

## 做什么

### 1. OMC HUD 安装

- 复核已有自定 shell statusLine (branch + dir + time + rtk saved tokens),用户授权覆盖
- 装包: `~/.claude/hud/omc-hud.mjs` ← `plugins/cache/omc/oh-my-claudecode/5.3.0/scripts/lib/hud-wrapper-template.txt`,`~/.claude/hud/lib/config-dir.mjs` ← canonical template,`chmod 755`
- 写 settings.json statusLine: `node ${CLAUDE_CONFIG_DIR:-$HOME/.claude}/hud/omc-hud.mjs`
- 删 legacy `omc-hud.js` (无)
- 自测: stdin sample → `[OMC#5.3.0L] | [API err] | session:0m | ctx:---------- 0%`(API err 是 probe 命中 minimaxi 代理,正常)
- **重启 CC 后生效**

### 2. mattpocock-skills 升级失败

- `claude plugin update mattpocock-skills@claude-plugins-official` → "already at latest (1.2.3)"
- 但 upstream `https://github.com/mattpocock/skills/releases` 有 v1.3.0 (sha 984a2c0) + v1.3.1 (sha 24fe0ef),均 2026-10-04
- 根因: Anthropic 维护的 `~/.claude/plugins/marketplaces/claude-plugins-official/.claude-plugin/marketplace.json` 把 mattpocock-skills 钉到 SHA `c55ee46073ed923f86ce59a5eb3b6d895095d1b7`,该 commit 的 plugin.json 仍是 1.2.3
- 附加证据: `plugin-catalog-cache.json` 中该插件的 `last_updated` = 2026-09-18 (v1.3 发布前)
- 用户拍板: **等 marketplace bump**(三个选项中选等),不手动 pin(SHA pin 仍归 Anthropic,会回滚)

## 验证

- HUD 自测输出含 `[OMC#5.3.0L]` 标识 + 各元素渲染正确(API err 是 healthy noise)
- settings.json 经 jq 验证仍是有效 JSON,statusLine 字段类型正确
- mattpocock-skills 11 个 skill 名仍可在系统 skill list 看到 (1.2.3 已可发现,无需等 1.3)

## 决策

- **决策 1 (D-2026-10-15-1)**: OMC HUD 启用覆盖自定 statusLine — 用户授权在先
- **决策 2 (D-2026-10-15-2)**: mattpocock-skills 1.2.3 → 1.3.x 升级延后 — marketplace SHA pin 未 bump,我们这侧无法绕过(手动 pin 会被下次 update 回滚)

## 落地件

- `~/.claude/hud/omc-hud.mjs` (9.6K, +x)
- `~/.claude/hud/lib/config-dir.mjs` (902B)
- `~/.claude/settings.json` statusLine 字段已换
- `memory/claude-plugins-official-marketplace-lag-2026-10-15.md` (新增 memory note)
- MEMORY.md 加指针

## 下一步

- **重启 CC** 生效 HUD
- 监控: 下次 `claude plugin update mattpocock-skills@claude-plugins-official` 报非 "already at latest" 时再装 1.3.x
- Reason: project 不动,无需 commit
- Ref-trace: 无 (本 session 0 commits)
