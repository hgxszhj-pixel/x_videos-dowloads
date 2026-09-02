# X Video Downloader — 项目交接

**最新**: 2026-09-02 Resolver Skeleton 共享基类 SDD — 8 SDD commits (`c437a6b`..`ca2c45b`) + 2 housekeeping (`6f79d50` docs/handoff + `4f59255` chore/gitignore) on `gitbutler/workspace`,head `4f59255`;4 个 yt-dlp resolver (YouTube/TikTok/Bilibili/DouYin) + FxTwitter 净 -129 行,新增 `src/platform/base.rs` `YtdlpBacked` (131 行) + `src/http_client.rs` `RESOLVER_UA`/`apply_resolver_defaults`/`build_resolver_client` (46 行);242 tests pass / 0 failed / clippy clean;下次新加 yt-dlp 平台从 ~100 行降到 ~30 行。Spec `docs/superpowers/specs/2026-09-02-resolver-skeleton-design.md` + Plan `docs/superpowers/plans/2026-09-02-resolver-skeleton.md`。
**再新**: 2026-09-01 WikiSkill 三层落地 — commit `756d49d` 决策回滚日志 D-001~D-021(21 决策,4 pending)+ `.teach/` 归入 .gitignore + LR 计数 7→8;副产物 `.teach/index.md` (106 行 三视图索引) + `~/.claude/skills/zhixueyun-analyzer/PURPOSE.md` (119 行 why-exists);同日 skill 健康度盘点:mattpocock 11 skill 全部已最新(2026-08-24 batch 已覆盖,本地 fork write-a-skill hash 守门通过)+ obra/superpowers v6.3.0 已是最新(2 skill,brainstorming + systematic-debugging)。
**再新**: 2026-09-01 京峰教育/SHANHU 珊瑚在线平台逆向(无 commit) — `wx-bgfcbieh.rexxi.cn` 是白标域名,真后端 `study.wanzibook.com` + 腾讯 TRTC WebRTC;3 种 play_style(3=标直/4=tcicMiddlePage/fakeLiveJump=VOD 伪装);首课 2026-09-02 20:00 未开,83 章节 `videos[]` 全空,**当前不可下载**;3 路下载路径待用户决定。报告 `docs/shanhu-jfedu-rexxi-platform-report-2026-09-01.md`。
**再新**: 2026-08-29 智学云 `zhixueyun-analyzer` skill 创建 + 3 模式 e2e 验证(短码 → 69 视频 / 单课详情 → 3 媒体含 2 PDF / 另一单课 → 0 媒体);新发现 `sectionType=1=PDF` + `sectionType=3=国考云跳转`;无 src/ 改动。
**再新**: 2026-08-29 同日 5 个 commit — `0011caa` 小鹅通 HLS 403 真凶 + GUI 三命名兼容 · `86e497e` 课程池覆盖已下载开关 · `9ab3813` 智学云单课节 URL 解析盲区修复 · `c7b0fe4` 8-25 暂存变更集落库 · `deb54c3` 两 bench 存量断裂修复。
**再早**: 2026-08-29 `63c9f53` home-v 根因解析 + `6cd927e` ego-lite SQLite 兜底(host 链路 .zhixueyun.com/.e.189.cn/.mylearning.cn)。
**更早**: 2026-08-28 全局 Claude Code 配置机械兜底改造(5 hooks + 15 deny) · 2026-08-25 tulingvip-m3u8-download skill v3.2 + douyin-mp4-download v1(端到端 17/17 + 15/15 验证) · 2026-08-22 darwin 优化 web-protocol-recovery 93.2→95.0 + playwright 修复 · 2026-08-21 opencode failover wrapper 双前缀 bug 修复 + 默认模型切智谱 glm-5.3 · 2026-08-17 OpenCode 多 provider + 全局 git 安全防御 + wuzuapi 双域名 failover · 2026-08-16 P1 重构 + 进度条 UX + Dioxus Store 修复 · 2026-08-13 GUI 性能 + runtime 升级 · 2026-08-09 小鹅通端到端 6 节 8.5GB 验证通过。

---

## 教学 workspace 维护约定 (`.teach/`,gitignored)

`.teach/` 是项目**经验沉淀区**,不是 session 日志。写 LR / 写 HANDOFF session 的判据表见 CLAUDE.md「教学 workspace 维护」节(同文不双份)。格式与触发器: `.teach/HOWTO-WRITE-LR.md`;索引 `.teach/index.md`(主题 / 错误信号 / 反模式三视图);当前 8 条(`learning-records/0001-`~`0008-`)。**原则**: HANDOFF 写"做了什么",teach 写"学到了什么"。

---

## 最新 Session: 2026-09-02 — Resolver Skeleton 共享基类 SDD (commits `c437a6b`..`ca2c45b`)

> 7-task plan 全跑通;4 个 yt-dlp resolver + FxTwitter 共 -129 行,新增 `YtdlpBacked` (131 行) + http_client factory (46 行)。SDD = `superpowers:brainstorming → writing-plans → subagent-driven-development`,跳过 per-task reviewer(决策见 `~/memory/decisions.md` SDD 节)。

### 1. 杠杆点识别 — 4 resolver 共享 ~550 行样板

8 个 `PlatformResolver` 中 4 个 (YouTube/TikTok/Bilibili/DouYin) 重复:`tokens: HashMap<String,String>` + `cookie_manager: CookieFileManager` + `new()` / `with_tokens()` 构造器 + `get_cookie_file()` helper + `build ytdlp → 注入 cookie → 调 yt-dlp → wrap 错误` 调用链。Nnyy/FxTwitter/ZhiXueYun 散落 3+ 处 `reqwest::Client::builder()...` + UA 字符串。

### 2. 设计决策

| 候选 | 选择 | 理由 |
|---|---|---|
| Base struct vs trait | **组合式 base struct** | 编译期单态,无 dyn 开销,debug 友好 |
| Macro vs 函数 | **不要 macro** | `apply_resolver_defaults()` factory 足够 |
| 抽象范围 | **限 4 resolver** + http_client helper | YAGNI;以后有新需求再加 |

### 3. 8 commits 全清单

| Commit | 任务 | 关键改动 |
|---|---|---|
| `c437a6b` | Task 1 | `YtdlpBacked` struct + 4 tests (`base.rs` 103 行) |
| `caef4ca` | Task 2 | `RESOLVER_UA` + `apply_resolver_defaults()` + `build_resolver_client()` + 3 tests (`http_client.rs` +48 行) |
| `59f149f` | Task 3 | YouTube 87→64 行;`fetch_via_ytdlp()` 加进 base |
| `aeada25` | Task 4 | TikTok 109→79 行;surgical 移除 `#[expect(dead_code)]` on 真活 struct |
| `d307791` | Task 5 | Bilibili 97→47 行;`tokens_is_empty()` 加进 base |
| `0b07406` | Task 6 | DouYin 251→208 行;`tokens_get()` 加进 base;API 分支 verbatim 保留 |
| `f08f4cf` | Task 7a | FxTwitter `with_proxy` 改用 `apply_resolver_defaults` |
| `ca2c45b` | Task 7b | 清掉 Task 1-2 临时 `#[allow(dead_code)]` (现已被消费) |

### 4. Post-SDD 收尾(commit `6f79d50` + `4f59255`)

不在 7-task SDD 范围,只为让 HANDOFF + 工作区干净:

- `6f79d50` — 把本 session(8 commits)写进 HANDOFF.md「最新 Session」section
- `4f59255` — `.gitignore` 归入 `.omo/` + `js_reverse_cache/`(镜像既有 `.omc/` + `SkillOpt/` 模式)

### 5. 7 条踩坑 rule(写入 `~/memory/projects/x-videos-dowloads.md`)

1. **`Platform` enum 没 impl Display** — 用 `Platform::display_name() -> &str`(project-specific)
2. **crate binary-only** — `cargo test --lib` 不适用,用 `--bin x-video-downloader`
3. **clippy scope 必须 narrow** — `cargo clippy -- -D warnings` 撞 pre-existing drission_client bench error,用 `--bin x-video-downloader --tests`
4. **forward-ref 顺序** — base helper → verify build → 重写 consumer;颠倒 rustc 拒绝
5. **`#[expect(dead_code)]` vs `#[allow(dead_code)]`** — 真死代码用 expect(test 唯一 caller 用 allow,因为 test cfg 下 expect 误报 unfulfilled)
6. **brief 写错的 `#[expect(dead_code)]` 在活代码上** — Tasks 4 + 6 都触发 `unfulfilled_lint_expectations`,按 rule 删除
7. **brief 写错的 `.map_err(ResolverError::from)`** — `fetch_via_ytdlp` 已返回 `Result<_, ResolverError>`,useless_conversion

### 6. SDD workflow 决策(写入 `~/memory/decisions.md`)

- **Per-task reviewer subagent 不可靠**:3 次 dispatch (sonnet x2 / opus x1) 全返 "Acknowledged." 无 review 内容
- **改用 controller inline self-review**:读 review-package diff + 跟 brief 对照 + 写 `task-N-review.md`
- final whole-branch review 同理 controller 自审
- **implementer subagent 工作正常**(haiku 跑 7 个 task 全 success),只是 reviewer 那条评估链不触发

### 7. 验收清单

| 项 | 状态 |
|---|---|
| 8 commits 全含 `Intent:` 行 | ✅ |
| 242 tests pass / 0 failed | ✅ |
| `cargo clippy --bin x-video-downloader --tests -- -D warnings` clean | ✅ |
| 4 resolver + FxTwitter 净 -129 行 (YouTube-23 / TikTok-30 / Bilibili-21 / DouYin-43 / FxTwitter-6) | ✅ |
| DouYin API 分支 verbatim 保留 (行为零变化) | ✅ |
| `PlatformResolver` trait / `ResolverRegistry` 未改 | ✅ |
| 新依赖数 | 0 |
| workspace `.superpowers/sdd/...` scratch 已删 | ✅ |
| Memory 写入 (projects + decisions) | ✅ |

### 8. 留待下期 (spec §10 out-of-scope)

- `wrap_resolver_call!` 宏(价值低)
- `m3u8_extractor::extract_from_html` 共享
- YouTube `with_cookie_file(user_path)` 行为迁移
- Nnyy/ZhiXueYun 内部 m3u8 / CookiePool 抽取
- TulingPython Drission 抽象

---

## 更新 Session: 2026-09-01 — WikiSkill 决策日志 + skill 健康度盘点(commit `756d49d`)

> 本 session 三件事: 1) WikiSkill 论文方法论落地本仓库 2) mattpocock/superpowers skill 主动盘点 3) 0 行 src/ 代码改动,纯文档 + .gitignore + 守门验证。

### 1. WikiSkill 三层落地(why / index / decision)

Google Research **WikiSkill** 论文核心: 知识库(wiki)和技能(skill)是两个不同抽象层,wiki **不回滚**只增改,skill 才演进。映射本仓库:

| WikiSkill 层 | 本仓库落地 | 路径 |
|-------------|----------|------|
| Skills Layer(skill 文件) | `zhixueyun-analyzer/PURPOSE.md` | `~/.claude/skills/zhixueyun-analyzer/PURPOSE.md` (119 行,why-exists / 7 失败路径 / 4 拒绝方案 / 5 边界 / 6 缺口) |
| Wiki Layer(结构化索引) | `.teach/index.md` | `/Users/hgxszhj/x_videos-dowloads/.teach/index.md` (106 行,主题 / 错误信号 / 反模式 三视图 + 全 8 LR 编号倒序 + 跨主题关联图) |
| Raw Layer(原始记录) | `.teach/learning-records/0001-0008` | 已存在,本次未新增 |

### 2. HANDOFF 决策回滚日志 D-001 ~ D-021

commit `756d49d` 在 HANDOFF.md 新增「决策回滚日志」section,21 条决策记录(含本 session 追加 D-019/020/021):

- 状态分布:**16 accepted / 1 rejected-deferred(D-013 bumpalo parser 62% 快不采纳)/ 4 pending**
- pending: D-014 平台报告落 git / D-015 skill 同步镜像 / D-016 preview-new 端点实测 / D-017 SHANHU 方案选 A/B/C
- 每条决策含 6 列: 日期 / 类别 / 决策 / 状态 / 背景 / 重审触发

### 3. `.teach/` 归入 .gitignore(修复文档/实现不一致)

HANDOFF.md §教学 workspace 约定 声称 `.teach/` gitignored,**实际 .gitignore 漏写**(工作区一直显示 `.teach/` 为 untracked)。本次 `.gitignore` 加 `.teach/` 修补,文档与实现一致。

### 4. LR 计数 7 → 8

HANDOFF.md §教学 workspace 约定 称 "当前 7 条",**实际 8 条**(LR 0008-launchd macOS 启动守护卸载)。同步更新。

### 5. mattpocock/skills 健康度盘点(2026-09-01 复跑)

```bash
pnpx skills update caveman diagnose grill-me grill-with-docs \
    improve-codebase-architecture setup-matt-pocock-skills tdd \
    to-issues to-prd triage zoom-out -y -g
# → ✓ All global skills are up to date
```

- 11 个可更新 skill(全 2026-08-24 batch 已 pull,无新 commit)
- 6 个上游已删(diagnose / to-issues / to-prd / zoom-out / caveman / write-a-skill)— `update -y` 非交互模式跳过删除,本地原样保留
- **本地 fork `write-a-skill` hash 守门**:`a9011c8a...` before == `a9011c8a...` after,4KB SKILL.md 字节级未变

### 6. obra/superpowers 健康度盘点(2026-09-01 复跑)

```bash
pnpx skills update brainstorming systematic-debugging -y -g
# → ✓ All global skills are up to date
```

- 本地 updatedAt 2026-08-24 14:55 UTC,上游 v6.3.0(2026-08-12)+ 之后无新 commit
- 2 skill 已含 v6.3.0 内容: brainstorming + systematic-debugging

### 7. 验收清单

| 项 | 状态 |
|----|------|
| 决策日志 18 条(含 D-019 见 §决策日志) | ✅ |
| `.teach/index.md` 三视图完整 | ✅ |
| `zhixueyun-analyzer/PURPOSE.md` 4 章完整 | ✅ |
| `.gitignore` 修补 | ✅ |
| mattpocock 守门 hash 一致 | ✅ |
| superpowers 已最新 | ✅ |
| src/ 改动 | 0 行 |

---

## 更新 Session: 2026-08-29 — 智学云 zhixueyun-analyzer skill 创建(无 commit)

### 1. skill `~/.claude/skills/zhixueyun-analyzer/`(新)

- 3 references(api-endpoints / section-type-dict / oauth-cookie-recipe)+ 3 scripts(extract_token / dump_cookies / fetch_home_chain)
- `fetch_home_chain.py` 3 模式 e2e 通过:full chain(需 Camoufox 预缓存 hop3)/ 单课模式 / probe-only
- Pyright 静态分析 5 处全清

### 2. 端到端验证 3 URL pattern

| URL | 解析 | 下载 |
|---|---|---|
| `#/home-v?id=128558` | 17 modules → 13 cards | **69 mp4** |
| `#/study/course/detail/<UUID>` | 1 chapter → 3 sections | **3 文件**(1 mp4 + 2 PDF) |
| `#/study/subject/detail/<UUID>` | 2 chapters → 18 sections | **0**(17 国考云 + 1 空) |

**Camoufox-only 约束**: `/system/home-content-config/home-content?moduleHomeConfigId=` 在 Python urllib 必 404;改用 `/business-home-config/{homeConfigId}`。

### 3. sectionType 新发现(写入 skill)

- **type=1 = PDF/文档**(numeric attachmentId ~18 位,CDN 路径 `/document/<hash>.pdf`)
- **type=3 = 国考云跳转**(Base64 attachmentId,`url` 指 `psc.mylearning.cn/.../redirectGuoKaoYunExam`)

### 4. CDN 实测

`dianxinsafecdn.zhixueyun.com` 公开,GET 只需 User-Agent,无 Bearer 也能下。

### 5. 待用户决定

1) skill 是否同步到镜像/装机版(按 wpr 双拷贝规则)2) `docs/zhixueyun-platform-report-2026-08-29.md` 平台报告是否落 git

---

## 更近 Session 摘要

### 2026-08-29 commit `0011caa` — 小鹅通 HLS 403 真凶 + GUI 三命名兼容

- **HLS 根因**(`src/hls/parser.rs::extract_base_url`): 旧实现 `url.find("://") + 第一个斜杠位置` 只返回 `scheme://host`,丢路径前缀 → CDN 按路径验签失败 → `403 missing parameter t`。**修复**: RFC 3986 剥 query + 留目录(含尾部 `/`)。
- **GUI 根因**(`src/gui_dioxus/handlers.rs`): Python `tmp_batch_download.py` 与 Rust GUI 文件名 sanitize 不一致(Rust 空格→`_`,Python 保留空格)。**修复**: `file_is_already_downloaded_with_index(dir, title, Some(idx))` 三命名穷举:`{sanitize}.mp4` / `{idx:02}_{raw}.mp4` / `{idx:02}_{sanitize}.mp4`。
- **教训**: 403 不一定是防盗链 → `curl -I` 看 body 揭示 CDN 验签失败;m3u8 base URL 必须按 RFC 3986;跨工具文件名检测穷举所有命名。
- 测试 8/8 通过(extract_base_url 回归 + GUI 三命名兼容)。

### 2026-08-29 commit `86e497e` — 课程池覆盖已下载开关

- `CourseCollectionState.overwrite_existing: bool` 默认 false(跳过)。
- 全选/全不选旁加 toggle:关→`⊘ 跳过已下载`;开→`🔄 覆盖已下载`(橙色)。
- `delete_existing_with_index` 删三命名旧文件,无文件忽略错误。
- `CourseCollectionState` 重载时重置 `overwrite_existing=false`,防粘性误覆盖。
- 测试 6/6 通过。

### 2026-08-29 commit `9ab3813` — 智学云单课节 URL 解析盲区修复

- `extract_course_id` 三模式:`#/study/subject/detail/<id>` / `?courseId=<id>` / `#/home-v?id=<id>` + `?id=<id>`(单课节预览)。
- `fetch_video_info` probe-then-fallback:`preview-new` 成功 → `FilePreview.filename` 作 title;失败 → `get_all_documents().first().name`。
- 测试 7/7 通过(累计 227/227)。
- ⚠️ `preview-new` 端点是**推测**,真实若不同会自动 fallback(UI 无感知但首次慢一倍)。

### 2026-08-29 commit `c7b0fe4` + `deb54c3` — 8-25 暂存变更集落库 + bench 修复

- **c7b0fe4**(47 文件): 8-25 dead_code 迁移 + unwrap 收敛 + feature 门控;clippy 默认 + `--all-features` 双 0 告警;`cargo test` 217/0/8。
- **deb54c3**: `hls_concurrency` stub types 与 types 演进脱节(月度无人察觉)+ 两 bench 文件级 `#![allow(clippy::unwrap_used)]`;实跑正常(`--all-targets --all-features` 0 警告);**bumpalo 版 parser 比现行快 ~62%**,未采纳,待决定。

### 2026-08-28 — 全局 Claude Code 配置机械兜底改造

5 hooks(rtk-rewrite / rust-fmt-posttool / stop-fmt-gate / obsidian-memory-journal / obsidian-memory-prelude)+ 15 deny(.env*/key/force-push/curl|bash/rm -rf)+ statusline 修复 + 死脚本归档。**备份**: `~/.claude/settings.json.bak-20260828`。**坑**: 文件写拦截必须 `Edit(**/.env*)`,`Write()` 形式不被权限检查匹配。

### 2026-08-25 — tulingvip-m3u8-download skill v3.2 + douyin-mp4-download v1

- **小鹅通 skill v3.2**: 12 项改进 + 端到端 17/17(JS逆向 33 节)+ 15/15(爬虫基础2626 期 15 节)验证。P3.5 setTimeout 5000→8000ms 防时序 false negative。
- **关键 bug 修复**: Step 2 Vue store → HTTP API(`window.__xe_store__` 在 Vue 3 重构后死路,改用 `POST /xe.course.business_go.column.items.get/2.0.0`)。
- **douyin v1**: URL 提取前端 work,但 ffmpeg/curl/fetch 三 path 全失败(X-Bogus 签名 + CORS);**实际下载需 yt-dlp**(v1+yt-dlp = 社区标准组合)。v2 X-Bogus 逆向需 3-5 天工程。
- **产物**: `~/Downloads/xiaoe-m3u8-download.zip`(30KB,9 文件)+ `~/Downloads/douyin-mp4-download/`(13KB,6 文件)
- **关键坑**: `jingxuan?modal_id=` URL yt-dlp 不识别,需 `/video/{aweme_id}` 直链 + 登录态 cookies

### 2026-08-22 — darwin 优化 web-protocol-recovery 93.2→95.0

- R1 (`ea30625`): 非触发场景改静默降级 93.2→94.4
- R2 (`84bb0e1`): 🔴 CHECKPOINT · 🛑 STOP 显性标记 2→4 处 94.4→95.0
- 修 playwright 3 层 bug(硬编码 /Users/alchain 路径 + 野包缺 playwright-core + 缺 chromium headless shell v1169)
- **方法论**: skill 的非触发出口不应要求向用户解释 skill 机制

### 2026-08-21 — opencode 修复 ×2: wrapper 双前缀 + 默认模型切 glm-5.3

- **wrapper bug**: `exec opencode "$@"` → `exec "$@"`(双前缀 bug,8-17 巧合掩盖)
- **glm-5.3 配置**: 走智谱 anthropic 端点 + baseURL 带 `/v1` + omo.jsonc 配 `agents.sisyphus.model` + `small_model: glm-4.7`
- **4 坑**: 包月 key 走 anthropic 格式端点(paaS/v4 全 1113) / baseURL 必须带 /v1(少了流式静默空响应) / omo.jsonc 覆盖插件 agent 模型 / small_model 必改(默认 haiku 必失败)
- 备份: `opencode.json.bak.glm53.2026-08-21`

### 2026-08-17 — OpenCode 多 provider + 全局 git 安全防御

- OpenCode 1.18.18 接入 wuzuapi 多模型 + Anthropic SDK provider key 必须叫 `anthropic`(改名模型注册为空)。
- 装 gitleaks 8.30.1 + `~/.gitignore_global` +14 个密钥 glob(只匹配具体文件名,不用宽 glob `*secret*`)+ `~/.git-hooks/pre-commit-user` 顶部插 gitleaks 扫描(全仓库生效)。
- **失败快速修复**: maxRetries:0 + timeout:30000 → `opencode run` 308s → 41.6s(7.4x)。
- **wuzuapi 双域名 failover**: baseURL 改 `{env:WUZUAPI_BASE_URL}` + `~/.zshrc` 默认值 + `~/bin/opencode-failover.sh` 探测 + `alias opencode='...failover.sh opencode'`(2026-08-21 修双前缀 bug)。

### 2026-08-16 — P1 重构 + 进度条 UX + Dioxus Store 修复

- `set_status` 改 `CourseItemStatus` 7 variants(Pending/Resolving/Downloading/Completed/Failed/Skipped/Exists),编译器强制覆盖。
- **Bug 1**: 课程池新下载不进历史 → `add_completed_download` 在并发完成后补调 + `state.write().history` 立即刷新。
- **Bug 2**: 底部进度条假 56%(5×100% exists + 4×0% pending 平均)→ `aggregate_course_progress` 只统计 active。
- **Dioxus Store 根因**: `state.read().batch_state.items.iter()` 走 Vec 快照,只跟踪长度(字段变化不触发 sub-render)。修复模式: `Store<T>::index(idx)` 链式访问。
- 测试 256/0/19 通过,clippy 0 警告,rsx! 限制 `{}` 内不支持 `if`(inline 条件显示需提为 helper)。

### 2026-08-13 — GUI 性能改进 + runtime 升级

| # | 文件 | 改动 |
|---|------|------|
| P0-1 | app.rs (-60) | 删重复 polling task |
| P0-2 | app.rs + poller.rs (+35) | 拆 3 个独立 Signal,poller 写 Signal 不写 store |
| P1-1 | app.rs | theme_signal + use_context_provider 保留(components 有 3 个 use_context 消费者,删除会 panic) |
| P1-2 | app.rs + store.rs | persist_theme helper |
| P1-3 | poller.rs | speed/size 阈值节流 |
| P2-1 | main.rs | new_multi_thread().worker_threads(4) |

commit `aefd94b`(6 files, +195/-115)

---

## 端到端里程碑: 小鹅通课程 → 本地 .mp4 (2026-08-09)

**课程**: `https://vip.tulingpyton.cn/p/t_pc/course_pc_detail/column/p_6a66e8d7e4b0694c35307f38`
**结果**: 6 条课节全部下载到 `/Users/hgxszhj/Movies`,共 8.5 GB .mp4,总耗时 1125s (~19min)

**6 处根因修复**(内存详记 `tulingpyton-ego-lite-cookie-injection.md`):

1. `src/hls/parser.rs::extract_base_url` — 保留 m3u8 所在目录路径,签名 CDN 对路径敏感
2. `src/drission_client.rs::capture()` 临时 profile — `std::env::temp_dir()/xvd_diag_<pid>_<idx>`,防 stgw 400 + 隔离 stale cookies
3. `src/drission_client.rs::diag_download_all_lessons` — 每条课节 HLS 下载 .ts 成功后立即 `ffmpeg -c copy` 转 .mp4 + 删 .ts
4. `src/platform/tulingpython.rs::list_collection_impl` reqwest-fastpath — macOS + column_id → 先 `fetch_column_items_reqwest()` 直击 `column.items.get/2.0.0`,失败 fallback Drission
5. `src/platform/tulingpython.rs::get_video_info` diag-fastpath — macOS + `live_pc/pc/l_*` URL → 调 `fetch_m3u8_via_diag(player_url, idx)`(临时 profile + 12 cookies 注入 + 8 轮 JS 触发 play)
6. `src/gui_dioxus/app_course_pool.rs` 进度/状态 polling task — 500ms 读所有 slot 原子值 → 写 store

**端到端链路**:
```
decrypt_ego_lite_cookies_for_vip() (Keychain + PBKDF2 + AES-128-CBC v10)
  → reqwest POST column.items.get/2.0.0 (0.43s,绕开 headless SPA)
  → Drission headless Chromium 临时 profile + CDP set_cookies (12 cookies)
  → 改写 jump_url → /live_pc/pc/l_xxx → tab.get + 4 轮 JS 触发 play
  → 拦 m3u8 (xet.tech playlist_eof.m3u8 + sign)
  → HlsDownloader 拉 m3u8 + 8 并发 .ts + 顺序合并 → ffmpeg -c copy → .mp4
```

---

## 快速开始

```bash
cargo run -- "URL"                              # CLI
cargo run -- --gui                              # GUI (Dioxus 0.7.9)
cargo test --quiet && cargo clippy --quiet --all-targets
tail -f /tmp/x-video-downloader.log             # GUI 日志

# 小鹅通端到端 (需 ego-lite 已登录)
cargo test --bin x-video-downloader -- --ignored \
  drission_client::tests::diag_download_all_lessons --nocapture

# 单课程课节列表 smoke test (0.6s 验证 reqwest-fastpath)
cargo test --bin x-video-downloader -- --ignored \
  drission_client::tests::diag_reqwest_new_url --nocapture
```

---

## 模块路径

- 入口 `src/main.rs`
- GUI `src/gui_dioxus/` (app, app_video_card, app_batch_video, app_batch_progress, app_course_pool, app_settings, app_history, components, handlers, poller, store, types, theme, actions, bridge)
- 下载 `src/downloader/` (mod, config, parallel, merge) + `src/core/segment.rs`
- HLS `src/hls/` (mod, parser, downloader, crypto)
- 解析 `src/platform/` (ResolverRegistry singleton)
- 命令 `src/commands/download/` (mod, dispatch, history_writer, strategy_direct/native/ytdlp)
- CDP 浏览器 + 登录检测 `src/drission_client.rs`

## 解析器

| 域名 | 解析器 | 课程池 |
|------|--------|-------|
| x.com / twitter.com | FxTwitterResolver (API) | ❌ |
| tiktok.com | TikTokResolver (需 msToken) | ❌ |
| douyin.com | DouYinResolver (需 msToken) | ❌ |
| bilibili.com | BilibiliResolver (yt-dlp) | ❌ |
| **vip.tulingpyton.cn** | TulingPythonResolver (Drission CDP) | ✅ 递归 |
| nnyy.in | NnyyResolver | ❌ |
| youtube.com | YouTubeResolver | ❌ |
| 其他 | YtDlpResolver (fallback) | ❌ |
| **kc.zhixueyun.com** | ZhixueyunResolver (feature 门控,默认开) | ✅ 递归 |

**智学云**:`src/platform/zhixueyun.rs` (748 行) + `src/platform/zhixueyun_token/mod.rs` (94 行),SSO + Bearer__token 直击 API + 课程章节递归。`--no-default-features` 可整体裁剪(连带 `cookie_pool`)。

## 小鹅通课程池 — 关键路径

- GUI 入口 `course_action.call(url)` → `fetch_course_collection_impl` (`src/gui_dioxus/handlers.rs:221`)
- 登录窗口 `open_tuling_login(url)` (`src/gui_dioxus/handlers.rs:285`)
- 登录态检测 `is_logged_in(url)` (`src/drission_client.rs`)
  - macOS: SQLite + Keychain 解密 ego-lite cookies (`decrypt_ego_lite_cookies_for_vip`)
  - fallback: Drission headless (`check_login_at`)
- 3 态返回: `LoggedIn` / `SessionExpired` / `NotLoggedIn`
- **临时 profile** (每条课节独立): `std::env::temp_dir()/xvd_diag_<pid>_<idx>` —— 防 stgw 400 + 隔离 stale cookies
- 翻页 helper: `read_total_and_page_size`, `extract_pagination_params`, `build_pagination_js`, `merge_paginated_items` (`src/drission_client.rs`)

## HLS 下载关键实现 — `src/hls/`

- 并发限流 `Semaphore::new(self.config.max_concurrent)`
- 分段重试 `download_segment_with_retry` (exponential backoff + Retry-After 优先)
- 取消标志 `cancel_flag: Arc<AtomicBool>`
- 进度回调 per-segment 节流 ≤50 次,累计字节 + 起始时间算 speed
- 顺序合并 不读所有分段到内存,逐段 read + write
- 加密 (AES-128-CBC) `decrypt_segment` (大段 ≥1MB 走 `spawn_blocking`)
- **`extract_base_url`** **保留 m3u8 所在目录路径,签名 CDN 对路径敏感**

---

## 状态

| 检查 | 结果 |
|------|------|
| `cargo test` | ✅ **256 passed, 0 failed, 19 ignored** (含 4 个 drission_client 诊断测试 + 5 个 tulingpython fixture 测试 `#[ignore]`;2026-08-25 Rust 1.98.0 复验) |
| `cargo fmt --check` | ✅ 0 diff |
| `cargo clippy --all-targets` | ✅ 0 warning |

## 已知问题 / TODO

- 抖音 "Fresh cookies needed" → 导出 Netscape 格式 Cookie
- TikTok 403/HTML → 配置 msToken
- tokio shutdown panic → `2>/dev/null` 抑制 stderr
- 小鹅通未登录 → "内容暂无法观看",GUI 启动 AppleScript/Drission headed 让用户扫码
- 小鹅通登录过期 (Code:2506) → `is_logged_in` 返回 `SessionExpired`
- Chrome SingletonLock 残留 → `is_profile_dir_in_use` 拦截
- 5 个 tulingpython 单元测试 `#[ignore]` → `tests/fixtures/` 4 个 JSON 在 10 GB 清理中误删,需重新抓取真实登录响应后取消 ignore
- 2 个 [[bench]] 已注释 → `benches/` 目录已删,后续如需 benchmark 重新建 `benches/` 后恢复
- **bumpalo 版 parser 比现行快 ~62%**(2026-08-29 bench 修复时浮出),未采纳,待决定
- `preview-new` 端点是否真实存在待浏览器实测(2026-08-29 zhixueyun 单课节修复,fallback 已就绪)
- `docs/zhixueyun-platform-report-2026-08-29.md` 平台报告是否落 git(2026-08-29 待用户决定)
- skill 是否同步到镜像/装机版(2026-08-29 待用户决定)
- 抖音 v2 X-Bogus 逆向工程(3-5 天,2026-08-25 douyin v1 时排期)

## 决策回滚日志 (Decision Log)

> **目的**: WikiSkill 原则 — 「知识 ≠ Skill」「Wiki 不回滚」。HANDOFF.md 是知识库,记录**决策和理由**而非结果。Skill/LR 处理演化,本表冻结决策。新会话遇到相同选择题时,先查本表避免重新讨论。
> **规则**:
> - 状态 = `pending` / `accepted` / `rejected` / `superseded by <id>`
> - 一行一决策,3 列必填(背景 / 决策 / 状态)
> - pending 项必须有「重审触发」列 — 什么条件下应该重新打开
> - 拒绝方案写「为何否决」,即使过去式也保留作为决策证据

| ID | 日期 | 类别 | 决策 | 状态 | 背景 / 备选 | 重审触发 |
|----|------|------|------|------|------------|---------|
| D-001 | 2026-08-16 | 渲染策略 | rsx! `{}` 格式段禁用 `if`,改 `match` / 条件 div | accepted | 见 [[0006]];rsx! 解析器只认 ident + expression | 升级 Dioxus 0.7 → 0.8 时 |
| D-002 | 2026-08-16 | 类型推断 | `Store<T>` 字段链不标注类型,让编译器推断 lens | accepted | 见 [[0007]];lens 是 `MappedMutSignal`,标注必报错 | dioxus-stores 公开 lens 类型 API 时 |
| D-003 | 2026-08-16 | 性能 | 进度/状态 polling 写 Signal 不写 store | accepted | 见 2026-08-13 P0-2;原子读 + Signal 写,避免触发 sub-render 风暴 | store 信号合并出现重复触发时 |
| D-004 | 2026-08-16 | UX | `CourseItemStatus` 7 variants 替换字符串状态 | accepted | 见 2026-08-16 P1 重构;编译器强制覆盖,消除「未知状态」分支 | 状态机新增状态前必须先扩 enum |
| D-005 | 2026-08-17 | 安全 | gitleaks + `.gitignore_global` 14 个密钥 glob,pre-commit 顶部插入 | accepted | 见 2026-08-17 全局 git 安全防御 | 误报阈值 >5% 时考虑降级到 `.env*` 子集 |
| D-006 | 2026-08-17 | 性能 | OpenCode `maxRetries:0` + `timeout:30000` (vs 默认 5min) | accepted | 见 2026-08-17 失败快速修复;308s→41.6s | 上游错误率 >10% 时回退到默认重试 |
| D-007 | 2026-08-21 | 配置 | opencode 默认模型切智谱 glm-5.3 (small_model=glm-4.7) | accepted | 见 2026-08-21 §4 坑;智谱包月 key 走 anthropic 格式 | glm-5.3 涨价或下架时 |
| D-008 | 2026-08-22 | skill 设计 | web-protocol-recovery 非触发场景改静默降级,不再向用户解释机制 | accepted | 见 2026-08-22 R1;95.0 分位回归修 | skill 沉默导致用户抱怨时 |
| D-009 | 2026-08-25 | skill 边界 | douyin-mp4-download v1 实际靠 yt-dlp,不靠 X-Bogus 逆向 | accepted | 见 2026-08-25 §douyin v1;X-Bogus 逆向 3-5 天不划算 | yt-dlp 失效 / 抖音改版时重排 v2 |
| D-010 | 2026-08-29 | 解析器 | 小鹅通 HLS `extract_base_url` 按 RFC 3986 保留目录(含尾 `/`) | accepted | 见 `0011caa`;旧实现丢路径 → CDN 验签 403 | m3u8 出现无尾 `/` 的边缘格式时 |
| D-011 | 2026-08-29 | GUI 兼容 | 文件名检测穷举 3 命名:`{sanitize}.mp4` / `{idx:02}_{raw}.mp4` / `{idx:02}_{sanitize}.mp4` | accepted | 见 `0011caa`;Python 与 Rust sanitize 不一致 | 引入第 4 种命名约定时 |
| D-012 | 2026-08-29 | 平台解析 | 智学云 chapter-progress 必须带 `knowledgePaymentEnable=false` | accepted | 见 [[0001-of-zhixueyun-skill]];`=true` 返回水印噪声 | 平台 API 改版时 |
| D-013 | 2026-08-29 | bench | bumpalo 版 parser 比现行快 ~62%,**未采纳** | rejected (deferred) | 见 `deb54c3`;改动面大 + 未实测生产路径;**否决理由**: 不为「可能更快」加新依赖 | 实测现行 parser 性能瓶颈(用户感知下载慢)时 |
| D-014 | 2026-08-29 | 文档归档 | `docs/zhixueyun-platform-report-2026-08-29.md` 是否落 git | **pending** | 报告含完整 endpoint + 鉴权矩阵,落 git 暴露给所有协作者 | 用户回复落/不落 |
| D-015 | 2026-08-29 | skill 同步 | zhixueyun-analyzer skill 是否同步到镜像/装机版(wpr 双拷贝规则) | **pending** | 装机版权威;改 SKILL/router 后需跑 build_agents.py + preflight.py | 用户回复同步 / 不同步 |
| D-016 | 2026-08-29 | 端点推断 | `/tools-center/file-cloud/preview-new` 端点存在性 | **pending** | 当前靠 fallback (chapter → documents → first.name);端点若不存在,UI 无感知但首次慢一倍 | 用户在 Camoufox 实测,确认 200 + `previewUrl` 字段 |
| D-017 | 2026-09-01 | 平台策略 | SHANHU 京峰教育 (org_id=27632958) 3 路下载方案: A 手动抓 m3u8 / B 复用 tulingpython.rs 模式写 `shanhu.rs` / C 等首课后用 camoufox 嗅探 | **pending** | 见 `docs/shanhu-jfedu-rexxi-platform-report-2026-09-01.md`;首课 2026-09-02 20:00 开课 | 用户决定走哪条 / 暂不实现 |
| D-018 | 2026-09-01 | skill 文档 | zhixueyun-analyzer 加 `PURPOSE.md` (why-exists / 失败路径 / 拒绝方案 / 边界) | accepted | 遵循 WikiSkill 原则;今天新建 | 决策变更时追加,旧条目保留 |
| D-019 | 2026-09-01 | gitignore | `.teach/` 显式列入 .gitignore(修文档/实现不一致) | accepted | HANDOFF.md §教学 workspace 约定声称 gitignored,但 .gitignore 实际漏写;本次同步 | `git status` 出现 `.teach/` 时重审 |
| D-020 | 2026-09-01 | skill 安全 | mattpocock 本地 fork `write-a-skill` update 前 hash 守门 | accepted | `pnpx skills update` 不会跳过 lock 文件标记的源(源仍是 mattpocock/skills);手动 sha256 + 备份兜底 | 未来 `update -y` 行为变更或 fork 数量增加 |
| D-021 | 2026-09-01 | skill 盘点 | obra/superpowers v6.3.0 已含本地(2026-08-24 batch),无动作 | accepted | 上游 2026-08-12 release v6.3.0,本地 updatedAt 在其后;`update -y` 确认无新 commit | 上游 release 新版本时 |

### pending 项汇总(供新会话快速扫描)

```
- D-014 平台报告 zhixueyun-platform-report-2026-08-29.md 是否落 git
- D-015 zhixueyun-analyzer skill 是否同步到镜像/装机版
- D-016 preview-new 端点是否真实存在(需 Camoufox 实测)
- D-017 SHANHU 京峰教育下载方案选 A/B/C
```

---

## 文档归档 (2026-08-16)

**Single source of truth**: `/HANDOFF.md` (root)。`docs/HANDOFF.md` 281 行 stale 副本已删除。

## 记忆入口

- 跨项目决策/偏好 → `~/memory/`
- 当日流水 → `~/memory/daily/{YYYY-MM-DD}.md`
- Agent 自动记忆索引 → `/Users/hgxszhj/.claude/projects/-Users-hgxszhj-x-videos-dowloads/memory/MEMORY.md`

## commit 历史

2026-08-29 GUI 批次共 8 个 commit(`4a25dc9`~`12eca70`,skip-existing / 状态枚举 / 历史记录 / 聚合修复等);更早见 `git log --oneline -20`。全部 commit 含 `Intent:` 行,`.git/hooks/commit-msg` 强制。