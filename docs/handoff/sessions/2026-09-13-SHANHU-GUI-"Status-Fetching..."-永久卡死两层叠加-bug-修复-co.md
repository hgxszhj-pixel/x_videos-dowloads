# 2026-09-13 — SHANHU GUI "Status: Fetching..." 永久卡死两层叠加 bug 修复 (commits `4f2128b` → `d939cab`)


### 1. 症状
GUI 粘贴 `https://wx-bgfcbieh.rexxi.cn/PC/#/yueke3/3529714`,点"获取信息",右上角状态卡 `Status: Fetching...` 永远不消失。批量列表不出 entry。Session 已导入 (token 有,org_id=27632958),list_collection 返 3 entries 成功,但每条 entry 的 `get_video_info` 失败。

### 2. 根因链（**两层叠加**,第一层修了第二层才暴露）

**第一层 (`0664fbb`)**:`list_collection` jump_url 拼接吃进 Vue hash 路由
- 旧逻辑:`if url.contains('#') { format!("{url}&chapter={id}") } else { format!("{url}#chapter={id}") }`
- SHANHU URL 本身是 Vue 2 SPA:`https://wx-bgfcbieh.rexxi.cn/PC/#/yueke3/3529714`,`#` 是 router 前缀不是 fragment marker
- 拼接后变成 `#/yueke3/3529714&chapter=3557155` — `&chapter=` 被吃进 hash fragment 内部
- `extract_chapter_id` 仍能识别 (子串匹配),但 YtDlpResolver fallback 报 `Unsupported URL`

**第二层 (`6691737`)**:`videoAuth` cfileid 字段格式假设错误
- 旧代码链路:`videoAuth → 取 cfileid → 32 hex 校验 → resolve_play_url (二次 HTTP) → m3u8 URL`
- 2026-09-13 实测响应:`cfileid = "220903970091175937"` (**19 字符十进制**,不是 32 hex)
- 校验失败 → `ResolverError::ParseFailed` → `try_resolve_all` 走 YtDlp fallback → 报 Unsupported URL
- batch_video_state 永远 `is_loading=true` → GUI 永久卡 "Status: Fetching..."

**根因之外的真凶**:第三天补上的鉴权机制 (前置,4f2128b / ad4aae1)
- 旧实现:`listClass/listSections` 用 GET/JSON,`videoAuth` 用 `.json(&body)` 配 `noteCourseId` 字段
- 实测全部要求 **POST form-urlencoded** + **token in body** (`wx_<hash>_<org_id>_<fans_id>` 格式,从浏览器 localStorage 取)
- `requires_auth()` 旧返 `false` (v2 匿名结论),新返 `true` 反映现实

### 3. 修复架构（**三层都改**）

| 层 | 改动 | commit |
|----|------|--------|
| 鉴权 | 3 个 API 函数全部改 POST form-urlencoded + token in body + 字段重命名 (`noteCourseId`→`course_id`) | `4f2128b` |
| 鉴权 metadata | `requires_auth()` → `true`,test 同步改 assert | `ad4aae1` |
| jump_url | 改 query string (`?chapter=`/`&chapter=`),避开 Vue hash 路由 | `0664fbb` |
| videoAuth | 改返 `data.video.mp4Url` (完整 m3u8 URL),跳过 `resolve_play_url` 二次 HTTP | `6691737` |
| hash 缓存 | 语义改存完整 m3u8 URL (不再只是 32hex hash) | `6691737` |
| CLI | `--shanhu-add-hash <file_id>:<mp4_url>` 文档+校验同步 | `6691737` |
| token 导入 | 新 `--shanhu-login-token <TOKEN>` CLI + `extract_token_from_state(&Value)` 3 路径 | 配合 `4f2128b` |
| 兜底 | `resolve_play_url` 保留 `#[allow(dead_code)]` (离线 / 老格式 cache 兜底) | `6691737` |
| 测试 | 新增 `smoke_get_video_info_rexxi_3529714_chapter_3557155` 端到端 smoke | `d939cab` |

### 4. 关键设计要点

**A. mp4Url vs cfileid 选择**:实测 `mp4Url` 是完整可下 m3u8 URL,`cfileid` 是 19 字符十进制 ID (腾讯云 VOD FileID,**不可算法反推**)。`mp4Url` 一次拿到就够,无需再发 HTTP。

**B. query string vs hash fragment**:Vue 2 SPA hash 路由下 `#` 是 router 前缀,**不能再叠 `#chapter=`**。query string 对 Vue router 无副作用 (router 不读 query),且 fragment 整体对 server 透明。

**C. `extract_token_from_state` 3 路径**:top-level `{"token": "..."}` (最简手工) / `origins[].localStorage[].name == "token"` (完整 camoufox) / 防御性数组元素。Session 导入兼容所有用户场景。

**D. fail-fast vs graceful**:token 字段为空字符串时继续找下一路径,不直接返 None — 老 state.json 格式兼容。

### 5. 验证状态

| 测试 | 结果 |
|------|------|
| `smoke_list_collection_rexxi_3529714` | ✅ 3 entries + 83 raw chapters |
| `smoke_get_video_info_rexxi_3529714_chapter_3557155` (新) | ✅ m3u8=`https://11145xxx.tenwiseacademy.cn/.../2586671d83683fda9fba38f4ab74225e.m3u8`, strategy=Hls |
| 全量 unit tests | ✅ 287 passed / 0 failed / 10 ignored |
| clippy `-D warnings` | ✅ clean (bin + tests) |
| GUI 实测 | ✅ Status="Video info fetched", 3 entry 全显示 (Linux的基本概念和系统搭建 / 基础命令讲解 / 文件权限) |

### 6. SHANHU API 鉴权机制实测总表（2026-09-13 推翻多次假设）

| API | 鉴权 | 请求格式 | 关键字段 |
|-----|------|---------|---------|
| `listClass` | session token | POST form-urlencoded | `token` + `org_id` + `course_id` |
| `listSections` | session token | POST form-urlencoded | `token` + `org_id` + `course_id` + `class_id` + `page` + `num` |
| `videoAuth` | session (token OR cookies) | POST form-urlencoded | `token` + `course_id` + `class_id` + `chapter_id` + `video_id` |
| `mp4Url` 解析 (tenwiseacademy.cn) | 匿名 | GET | URL 路径含 `<32hex>.m3u8`,Referer + Origin 防盗链 |

字段校正:
- `cfileid` = **19 字符十进制** (例 `220903970091175937`),**不是 32 hex**
- `mp4Url` = 完整 m3u8 URL (直接可用)
- `file_id` (listSections `videos[].file_id`) = 19 字符十进制,与 cfileid 不同 ID 体系
- token 格式 = `wx_<hash>_<org_id>_<fans_id>`,从 localStorage `token` 键取

### 7. SHANHU session 导入 4 入口 fallback 链路

| 命令 | 何时用 |
|------|--------|
| `--shanhu-login <state.json>` | camoufox MCP 完整 state.json (含 cookies + origins.localStorage) |
| `--shanhu-login-cookie "<Cookie header>"` | Chrome DevTools Network → Cookie header 直接拷 |
| `--shanhu-login-token <token>` | 只拿到 localStorage token 时 (最简,推荐) |
| `python3 scripts/chrome_cookie_extract.py --auto-detect --output-cookie-string` | macOS Chrome 本地 SQLite + Keychain 解密 |

任一入口成功即写入 `~/.config/x-video-downloader/shanhu-session.json`。

### 8. 教训 (写入 `~/.memory/projects/x-videos-dowloads.md` 关键教训 #13–#17)

| # | 教训 |
|---|------|
| 13 | **jump_url 拼接必须先看 URL 自身结构**:Vue hash 路由 SPA 的 `#` 是路由前缀,不能再叠 `#xxx=` |
| 14 | **第三方 API 响应字段格式永远不能硬编码假设**(长度/字符集/进制),失败时打印 raw body |
| 15 | **错误根因排查必须看到错误链路终点**:GUI 永远 loading 是症状,日志全链路追踪定位 |
| 16 | **`try_resolve_all` fallback 链会掩盖真 bug**:SHANHU 自身报错被吞,YtDlp fallback 报错用户看到的也是"失败" |
| 17 | **续接 compaction 摘要时,先 git status + log 看真实状态再下手** |

### 9. 关联引用 (留待下期)

- `docs/dna/review-checklist.md` → "环境"小节加 1 行 SHANHU session 导入提示
- `docs/dna/新功能前必查.md` → 血泪教训 grep 关键词加 #13–#17
- `~/.claude/skills/rexxi-course-m3u8/SKILL.md` → 鉴权 API 表已过期 (写的是 GET/JSON),需同步更新为 POST form-urlencoded/token

---

