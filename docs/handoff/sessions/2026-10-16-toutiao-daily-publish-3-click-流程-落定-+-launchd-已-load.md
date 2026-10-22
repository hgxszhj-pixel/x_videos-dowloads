# Toutiaohao Daily Auto-Publish — 3-click 流程落定 + launchd 已 load (2026-10-16)

## why
Horizon AI 日报 (horizon-YYYY-MM-DD-zh.md) → 已落 `toutiao-YYYY-MM-DD-zh.txt` (12 汉字标题 + 1200 字正文 + hashtag 行) → 用户要求 9:00 AM 自动发到 `https://mp.toutiao.com/profile_v4/graphic/publish`。初版 (1-click 直发) 反复失败 8 次,smoke 通过后才发现 Toutiaohao 是 **3-click 流程** (底部 "发布" → schedule modal → preview modal → 实际提交)。

## what changed
- `~/Horizon/scripts/toutiao_publish.py` — 3-click 流程 + `:has-text` selector 替代 `get_by_role(name=)` + SUCCESS_URL_FRAG 修正为 `articles`
- `~/Horizon/scripts/patch_playwright_pageerror.sh` — playwright 1.60.0 pageerror crash patch (idempotent,`uv sync` 后必跑)
- `~/Horizon/pyproject.toml` — `toutiao` optional dep `camoufox>=0.4.5`
- `~/Horizon/scripts/toutiao_login.py` — headful login shim (623B)
- `~/Library/LaunchAgents/com.horizon.toutiao-daily.plist` — `StartCalendarInterval {Hour: 9, Minute: 0}` (已 `launchctl load` ✓)
- `~/.claude/projects/.../memory/toutiao-daily-publish-2026-10-16.md` — 8 教训落档

## 3-click 流程 (smoke 2026-10-16 15:23:53 通过,exit 0)
1. 底部 action bar "定时发布" (substring "发布" `.first` 命中) → 打开 schedule 选时 modal
2. Schedule modal: `button.byte-btn-primary:has-text("预览并定时发布")` → 打开手机预览 modal
3. Preview modal: `button.byte-btn-primary.publish-btn:has-text("定时发布")` → 实际提交 → 跳 `/profile_v4/graphic/articles`

## 8 关键 selector / 行为教训

1. **`get_by_role("button", name=...)` 在 byte-btn 模态框中失效** — modal 被检测,按钮 substring 在 role-tree 中不匹配。**替代**: `button.byte-btn-primary:has-text("...")` 1 击命中
2. **`:has-text` substring 匹配比 `name=` 强** — 同一按钮 1 击 vs 30+ 次 miss
3. **Cover radio 切换**: radio.click / React-aware setter 都不更新 React 状态;**唯一可靠** `label.click(force=True)` (label 真实事件 bubbles 通过 React 合成事件)
4. **ProseMirror body fill**: `keyboard.type` 丢字 / `innerHTML + dispatchEvent` 被 schema 压到 ~852 / **`keyboard.insertText` per line + 每段 Enter** 1235 字完整 round-trip
5. **`get_by_role("button", name="发布")` substring 多匹配陷阱** — DOM 顺序里 "定时发布" (schedule modal 路径) > "预览并发布" (直发路径)
6. **底部按钮实际是**: `添加至合集` / `返回编辑` / `定时发布` (primary,publish-btn class)
7. **playwright 1.60.0 `pageError.location.url` crash** (Tencent SDK 触发后整 Node driver 死) — patch 在 `scripts/patch_playwright_pageerror.sh`
8. **session 文件**: Playwright `storage_state` JSON 即可 rehydrate (cookies+localStorage+IndexedDB),`chmod 600`

## tradeoffs
- **直接定时发布而非保存草稿** — 当前 3-click 走定时发布,留草稿是另一个 button "保存草稿" (用户后续要直发的话改 click 顺序)
- **无图 / 无 rich text / 无话题自动添加** — 头条编辑器 contenteditable 不是 markdown,v1 纯文本 + 留空 hashtags
- **session TTL 未实测** — 假设 30 天;`status` 失败时 launchd exit 1,需重跑 `login` 流程

## risks
- **sk-cp- API key 本 session 泄露 3x** (出现在 planning / memory 写入 / log 路径),备份 `~/Horizon/.env.bak-20261016-leaked`,**session 后必 rotate**
- launchd 自跑明天 9 AM 第一次,无人值守;`status` 失败 → exit 1,需重 login,无桌面通知 (后续可加)
- 头条"图文"审核有时延,定时发布提交后到时间才上;非实时发布

## team understanding
- 这个 session 跟 x-video-downloader 主线无关,但用了同一套 camoufox 浏览器栈 + 与 `horizon-news-radar-install-2026-10-16` 上游对接 (radar 出 briefing → toutiao_publish 发头条)
- 跟 `mcp-agent-trim-2026-10-15` 的精简思路一致:用 camoufox Python 包而非 MCP server (launchd 没法驱动 MCP)
- 跟 `shanhu-smoke-test-2026-09-13` 同样是"smart dispatch 数据层 + camoufox GUI 层"分层思路

## verify
- [x] `bash scripts/patch_playwright_pageerror.sh` → ✓ already patched
- [x] `.venv/bin/python scripts/toutiao_publish.py publish` → exit 0,redirect `/graphic/articles`
- [x] `launchctl load ~/Library/LaunchAgents/com.horizon.toutiao-daily.plist` → `- 0 com.horizon.toutiao-daily` in `launchctl list`
- [ ] 明天 9 AM 第一次自跑 → `tail -F ~/Horizon/logs/toutiao-cron.log`
- [ ] sk-cp- key rotate (本 session 泄露 3x)

## refs
- memory: `~/.claude/projects/-Users-hgxszhj-x-videos-dowloads/memory/toutiao-daily-publish-2026-10-16.md`
- probes: `/tmp/probe_modal3.py` / `/tmp/probe_preview.py` (验证 selectors,使用后清)
- 已知限制 + 5 步上手: memory file 末段
