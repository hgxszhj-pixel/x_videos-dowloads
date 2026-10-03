# 2026-09-06 — `shanhu-analyzer` skill 创建(无 commit)


> 用户在 `--shanhu-login` CLI (`bd2dfeb`) 完成后选 "建 SHANHU analyzer skill (Recommended)"。本 Session 把 Phase 1-5 全部发现沉淀为可复用 skill,类似 `zhixueyun-analyzer` / `tulingvip-m3u8-download`。

### 1. 设计目标

| 候选 | 否决原因 | 选定 |
|------|---------|------|
| 写在 Rust doc 注释 | 模型不主动读注释,且散落在 6 个文件 | ❌ |
| 只追加到 memory 文件 | 缺触发词,下次查询不会自动跳到对应位置 | ❌ |
| **~/.claude/skills/<name>/** (SKILL.md + PURPOSE.md + references + scripts) | WikiSkill 三层架构之一,frontmatter description = 触发词,自动 discoverable | ✅ |

### 2. skill 结构 (8 文件, ~76K)

```
~/.claude/skills/shanhu-analyzer/
├── SKILL.md (8.0K)              # 触发词 + When/When NOT + Quick Start + 4 endpoint 链路图 + 反模式
├── PURPOSE.md (16K)             # 9 节:为什么存在 / 失败路径 / 边界 / 缺口 / 拒绝方案 / 反模式 / 验证 / 与其他 skill / 维护规则
├── references/
│   ├── api-endpoints.md         # 4 端点 (listClass / listSections / videoAuth / resolve_play_url) + ERR codes
│   ├── cookie-recipe.md         # camoufox 6 步流程 + cookie 过滤 + session 文件格式 + P1 失效检测
│   └── hash-chain.md            # 链路 + 6 种 hash 反推失败 + COS 假阳性 + 双层 hash + CDN 防盗链 + 性能数据
└── scripts/
    ├── shanhu_bootstrap.py      # camoufox 一次性扫码 + 拦截 m3u8 + 写 session + 写 hash cache
    ├── shanhu_video_auth.py     # state.json → videoAuth → cfileid → (可选) 写 hash cache
    └── shanhu_resolve_m3u8.py   # 匿名 listSections → m3u8 URL (管道友好)
```

### 3. 关键设计原则

- **SKILL.md 极简** — 只 Quick Start + 决策表 + 反模式 (5.7KB),详细知识下沉到 references
- **PURPOSE.md 持久化失败路径** — `v1→v2` 反转、6 种 hash 反推失败、COS creds 假阳性、双层 hash 容易被遗漏的第二层 (避免下次重蹈覆辙)
- **scripts 独立可执行** — 3 个 Python 脚本各自独立完成一件事,可被 shell 管道调用 (`--print-m3u8`)
- **frontmatter description = 触发词** — 含 8 个关键词 (rexxi.cn / wanzibook.com / tenwiseacademy / cskziwl / videoAuth / cfileid / 珊瑚 / 万字),确保模型主动 recall

### 4. 验证

| 项 | 状态 |
|---|---|
| 8 文件全部创建 | ✅ |
| `python3 -m py_compile` 3 scripts | ✅ |
| `python3 -c "import ast; ast.parse(...)"` 3 scripts | ✅ |
| Pyright 0 ✘ (3 ★ info-level: optional dep import + unused param) | ✅ |
| frontmatter `name: shanhu-analyzer` 自动注册 skill | ✅ |
| `chmod +x` 3 scripts | ✅ |

### 5. 与已有 skill 的关系

- **`zhixueyun-analyzer`** (2026-08-29) — 同样 SaaS 教育平台,但全程登录 + 6 跳 API;SHANHU 是 4 endpoint + 大部分匿名
- **`tulingvip-m3u8-download`** (2026-08-25) — 小鹅通 1 端点 JS 触发,SHANHU 是双层 hash 服务端分配
- **`camoufox-reverse-mcp`** — 提供 anti-detect browser + JS reverse,本 skill 复用 `export_state()` 不直接调 camoufox

### 6. Memory 同步

`~/.claude/projects/-Users-hgxszhj-x-videos-dowloads/memory/jfedu-rexxi-shanhu-m3u8-download.md` 追加 Phase 6 (skill 结构 + 设计决策 + 为什么用 skill),MEMORY.md 已有条目无需重复添加。

### 7. SHANHU 闭环全景 (Phase 1-6)

```
┌──────────────────────────────────────────────────────────────────┐
│ Phase 1: 54bbe4d   ShanhuResolver 骨架 + 匿名 listSections      │
│ Phase 2: 90b574e   get_video_info + hash cache + CLI 缓存管理    │
│ Phase 3: 130f3b2   class_id 自动发现 (listClass + URL 路由)      │
│ Phase 4: 1164286   fix CLI global_registry 漏注册                │
│ Phase 5: 2a68896   session 模块 + videoAuth 自动 cfileid 发现    │
│ Phase 6: bd2dfeb   --shanhu-login CLI (camoufox export_state)    │
│ Phase 7: 无 commit  沉淀为 shanhu-analyzer skill (本 Session)    │
└──────────────────────────────────────────────────────────────────┘
```

---

## Session: 2026-09-06 — SHANHU session cookie 复用 (commit `2a68896`)

> 用户在 Phase 1-3 (`54bbe4d`..`1164286`) 后选 Path "扩展 SHANHU: 抓 hash 算法反推",JS 逆向证明 hash **不可算法反推**,改为 Path A "一次性登录 + session cookie 复用"。

### 1. JS 逆向发现(3 件否决"算法反推"路径)

| 发现 | 证据 | 否决什么 |
|------|------|---------|
| **`cfileid` 是 VOD 服务端分配** | MD5/SHA1/SHA256 of `(file_id / hex(file_id) / file_id+appid / appid+file_id / chapter_id / video_id / course_id)` 全不匹配 `2586671d83683fda9fba38f4ab74225e` | 算法反推(JS bundle 的 hash 计算器是 VOD SDK 黑盒) |
| **JS bundle 硬编码 COS creds** | `<REDACTED: SecretID + SecretKey>` 在 app.js:4823885(`cos.uploadFile` → `huaxia-1396267596` COS bucket) | 客户端 signing;测 `/verify/sign` → 拒签 `SecretID 不存在或已禁用`(COS ≠ VOD) |
| **唯一返 `cfileid` 端点 100% 需登录** | `POST /onlineapi/stu/course/videoAuth` 匿名返 `code=100067` 非法请求 | 匿名 videoAuth |

### 2. 双层 hash 链(被发现不止一层)

```
file_id (listSections 匿名)
  → cfileid (videoAuth 登录态,32 字符 hex)
    → tenwiseacademy.cn/<cfileid>.m3u8 (匿名 redirect)
      → cskziwl.cn/<hash1>/<hash2>.m3u8 (VOD 切片)
```
**关键**:`<hash1>/<hash2>` 与 `<cfileid>` 完全无关;我们要缓存的是 **`file_id → cfileid`** 映射(VOD 切入点),不是 VOD URL 末段 hash。

### 3. session 模块设计(净 270 行)

| 元素 | 内容 |
|------|------|
| 文件 | `~/.config/com.x-video-downloader.x-video-downloader/shanhu-session.json`(同 hash cache 目录) |
| 格式 | `{saved_at, token?, cookies: [{name, value, domain: ".wanzibook.com" 默认, path: "/" 默认}]}` |
| API | `load_session()` / `save_session()` (dead_code allow,待 `--shanhu-login`) / `cookies_to_header()` / `session_summary()` / `session_from_export()` 接 camoufox `export_state` 格式 |
| 测试 | 4 个: `cookies_to_header_format` / `session_round_trip` / `session_from_export_skips_malformed` / `session_summary_default_no_file` |

### 4. `fetch_cfileid_via_video_auth` 集成

```rust
// src/platform/shanhu.rs (新增 ~100 行)
async fn fetch_cfileid_via_video_auth(
    &self, file_id: &str, course_id: u64, class_id: u64,
    chapter_id: u64, video_id: Option<u64>,
) -> Result<String, ResolverError> {
    // POST /onlineapi/stu/course/videoAuth
    // Body: {noteCourseId, class_id, chapter_id, video_id}
    // Headers: Cookie (from session), Referer, Origin
    // Returns: data.video.cfileid (校验 32 字符 hex)
    // 错误: code=100067 → session expired
}
```

`get_video_info` Step 4 缓存未命中分支:
```
if hash NOT in cache → load_session()? → fetch_cfileid_via_video_auth() → save_hash_entry(fid, cfileid)
```
**后续该章节完全匿名**(cache hit 路径,无需 session)。

### 5. CLI `--shanhu-session-info`

```
SHANHU login session
  路径: ~/.config/com.x-video-downloader.x-video-downloader/shanhu-session.json
  cookie 数: 12
  含 token:  否
  保存时间:  epoch:1757185200
  videoAuth 自动发现可用,缺失章节会通过 session cookie 自动获取 cfileid
```

未配置时输出友好"配置方式 (二选一)"指引(浏览器 DevTools / camoufox 自动化)。

### 6. 验证

| 项 | 状态 |
|---|---|
| `cargo test --bin x-video-downloader` 277 passed (+4) | ✅ |
| `cargo clippy --bin x-video-downloader -- -D warnings` clean | ✅ |
| `cargo run --quiet -- --shanhu-session-info` 正确输出 | ✅ |
| src/ 改动:`src/platform/shanhu_session.rs` (新,270 行) + `src/platform/shanhu.rs` (+198/-40) + `src/platform/mod.rs` (+4) + `src/cli.rs` (+5) + `src/main.rs` (+29) = 净 +296 行 | ✅ |
| stashed 无关 gui_dioxus batch-video checkbox 改动(待用户后续处理) | ✅ |

### 7. 待办

- `--shanhu-login` CLI 子命令(从浏览器交互/import 落地 cookie)— 当前靠用户手工或 camoufox 自动化
- session 过期检测 token timestamp + 自动重试(当前靠 `code=100067` 错误信号)

---

## 教学 workspace 维护约定 (`.teach/`,gitignored)

`.teach/` 是项目**经验沉淀区**,不是 session 日志。写 LR / 写 HANDOFF session 的判据表见 CLAUDE.md「教学 workspace 维护」节(同文不双份)。格式与触发器: `.teach/HOWTO-WRITE-LR.md`;索引 `.teach/index.md`(主题 / 错误信号 / 反模式三视图);当前 8 条(`learning-records/0001-`~`0008-`)。**原则**: HANDOFF 写"做了什么",teach 写"学到了什么"。

---

