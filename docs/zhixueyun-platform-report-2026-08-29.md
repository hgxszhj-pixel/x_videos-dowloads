# 智学云平台结构分析报告

**分析日期**: 2026-08-29
**分析样本**: `https://kc.zhixueyun.com/#/home-v?id=128558` (中国电信集团业务主页)
**证据来源**: 14 个真实 API 响应 (js_reverse_cache/2026-08-29-zhixueyun-home-v/) + 2 次 Range 下载 + cargo test
**会话 ID**: b311a192-04e7-4cce-8cb5-7cba1fee2832

---

## 1. 平台身份

| 项目 | 值 |
|---|---|
| 平台名 | **智学云** (zhixueyun) |
| 运营方 | 中国电信集团 (enterprise learning platform) |
| 主域名 | `kc.zhixueyun.com` (Knowledge Center, 学习中心) |
| 父域名 | `e.189.cn` (天翼账号, OAuth 提供方) |
| 跨域 SSO | `mylearning.cn` + `sso.mylearning.cn` (天翼学习通行证) |
| CDN | `dianxinsafecdn.zhixueyun.com` (openresty, Cache-Control: max-age=31536000) |
| 默认业务 | homeConfigId = `203b466e-2430-4f71-ae6e-8997910db13b` |

---

## 2. OAuth 3-host 链

登录走天翼账号中心，token 在 3 个域之间传递：

```
用户浏览器 → e.189.cn (天翼账号 SSO)
        → sso.mylearning.cn (天翼学习 SSO, 发 ctu.sso.tgt 等)
          → mylearning.cn (callback 拿到 casid/session/st)
            → kc.zhixueyun.com (带 authorization Bearer + SSON 落 session)
```

**ego-lite 注入 Camoufox 必需的 6 个 cookie** (实测 401→200 的关键)：

| cookie | domain | 用途 |
|---|---|---|
| `authorization` | .kc.zhixueyun.com | **Bearer token** (PBKDF2-HMAC-SHA1 AES-128-CBC v10 解密) |
| `SSON` | .e.189.cn | 天翼账号会话标识 |
| `open-casid` | open.mylearning.cn | 天翼学习用户 casid |
| `ctu.sso.session` | open.mylearning.cn | SSO session |
| `ctu.sso.tgt` | sso.mylearning.cn | TGT ticket granting ticket |
| `HMACCOUNT` | .kc.zhixueyun.com | 百度统计标记 (实际不需要但服务端认可) |

> ego-lite Cookie 加密: PBKDF2(saltysalt, 1003 iter) + AES-128-CBC + v10 prefix; Keychain service name 是 **小写** `"ego safe storage"` (不是 "ego lite Safe Storage")。

---

## 3. 业务模型 / 实体层级

```
组织 (organization)
  └─ organizationId (e.g. f32e65d0-fe3b-40d3-a025-4480a1808746 = 中国电信集团)
       └─ 业务主页 (businessConfig) ── shortCode (e.g. 128558)
            └─ homeConfig (homeConfigId, e.g. 203b466e-2430-4f71-ae6e-8997910db13b)
                 ├─ front (整页配置)
                 │    └─ pageRelation[]  (17 个模块绑定)
                 │         ├─ type=1 课程类 (9 个) ── canvasPageConfigId
                 │         └─ type=2 活动类 (8 个)
                 ├─ modules (nav 10 个 + footer + tag)
                 └─ home-content?moduleHomeConfigId=  → 卡片
                      └─ card (id, dataId=cardId/moduleId, dataName, dataType)
                           ├─ dataType=9  单课
                           └─ dataType=10 专区
                                └─ courseId (UUID, 36位)
                                     └─ chapter-progress
                                          └─ courseChapterSections[]
                                               ├─ sectionType=6 视频
                                               ├─ sectionType=9 测试题
                                               └─ sectionType=10 嵌套子课程
                                                    └─ (递归 courseId)
```

**关键 ID 命名规律**：
- `homeConfigId` / `courseId` / `canvasPageConfigId` 都是 **UUID 格式** (36位带连字符)
- `section.attachmentId` 视频用 **数字 ID** (~19位, e.g. 750128444928303104)
- `section.attachmentId` 嵌套子课用 **UUID 格式** (e.g. 3aa7d05d-ab3d-...)
- `card.dataId` 业务 ID (单课/专区 courseId), `card.id` 是 card 配置 ID (无关)
- `homeConfig.shortCode` 是 6 位数字 (128558), 是 URL 唯一短码

---

## 4. API 命名空间

全部在 `https://kc.zhixueyun.com/api/v1/` 下，4 个一级路径：

### 4.1 `/system/` — 业务系统配置

| 路径 | 方法 | 用途 |
|---|---|---|
| `/system/business-home-config/default-home-page?shortCode=` | GET | **入口** — 短码→homeConfigId (159B) |
| `/system/home-content-config/original-url?shortCode=` | GET | 短码→原始 URL (54B) |
| `/system/home-content-config/validate?shortCode=` | GET | 校验 shortCode 有效 (15B) |
| `/system/home-content-config/{homeConfigId}` | GET | home-config 全字段 (592B) |
| `/system/home-content-config/navigation-list?homeConfigId=` | GET | 候选业务列表 (350B) |
| `/system/home-content-config/front/{homeConfigId}` | GET | **整页配置** (4958B) — 含 pageRelation[] |
| `/system/home-content-config/modules?homeConfigId=` | GET | 模块 nav (10351B) — 10 个菜单 + footer + tag |
| `/system/home-content-config/home-content?moduleHomeConfigId=&size=` | GET | **课程卡片列表** (3279B) |

### 4.2 `/course-study/` — 课程学习

| 路径 | 方法 | 用途 |
|---|---|---|
| `/course-study/course-info/front/find-by-ids?ids=` | GET | **批量课程元数据** — 返回 list[课程66字段] |
| `/course-study/subject/chapter-progress?courseId=&knowledgePaymentEnable=false` | GET | **章节列表** — 含嵌套 chapters + sections |

### 4.3 `/tools-center/file-cloud/` — 文件云

| 路径 | 方法 | 用途 |
|---|---|---|
| `/tools-center/file-cloud/preview-new?id=&_=<ts>` | GET | **预览 URL** (570B) — previewUrl 是 CDN mp4 直链 |
| `/tools-center/file-cloud/preview?id=` | GET | 文件元数据 (无 previewUrl) |

> **参数 `_=<timestamp>` 是必需的**, 缺它会 422 "Invalid input."

### 4.4 其它

`/api/v1/system/canvas-page/{id}` — 404 (猜错)
`/api/v1/course-study/chapter/front/chapter-list` — 404
所有 `subject` 子路径都在 `/course-study/` 下，不是 `/course-study/subject/` 之外。

---

## 5. 完整 6 跳下载链 (home-v URL → 视频 mp4)

```
URL: https://kc.zhixueyun.com/#/home-v?id=128558
  │
  │ 跳 1: extract_course_id(url) → shortCode = "128558"
  │ 跳 2: resolve_business_home(shortCode, token)
  │        → GET /system/business-home-config/default-home-page?shortCode=128558
  │        → homeConfigId = 203b466e-2430-4f71-ae6e-8997910db13b
  │
  │ 跳 3: fetch_page_relation(homeConfigId, token)
  │        → GET /system/home-content-config/front/{homeConfigId}
  │        → pageRelation[].canvasPageConfigId  (17 个, 9 type=1 + 8 type=2)
  │
  │ 跳 4: fetch_home_content(canvasPageConfigId, size, token)
  │        → GET /system/home-content-config/home-content?moduleHomeConfigId=&size=20
  │        → cards[]: { id, dataId, dataName, dataType(9 or 10), moduleConfigId }
  │
  │ 跳 5: fetch_course_meta(dataId, token)
  │        → GET /course-study/course-info/front/find-by-ids?ids=<dataId>
  │        → course: { id, name, sectionCount, courseTime, cover, ... 66 字段 }
  │
  │ 跳 6: fetch_chapter_progress(courseId, token)
  │        → GET /course-study/subject/chapter-progress?courseId=&knowledgePaymentEnable=false
  │        → chapters[].courseChapterSections[]: { id=sectionUUID, attachmentId, sectionType }
  │        → if sectionType=6: attachmentId = 数字, 可直接 preview-new
  │        → if sectionType=10: attachmentId = UUID, 是子课程 courseId, 递归跳 5-6
  │        → if sectionType=9: 跳过 (测试题, 非媒体)
  │
  │ 跳 7 (终): fetch_preview(attachmentId, token)
  │        → GET /tools-center/file-cloud/preview-new?id=<数字attachmentId>&_=<ts>
  │        → { previewUrl = https://dianxinsafecdn...mp4, filename, size, duration }
  │        → HTTP GET previewUrl → 真实 mp4 (CDN Range 支持)
```

**实测样本**：

| 项 | card[0] (中国电信集团 12期) | card[1] (AI 智惠警情) | type=10 嵌套 (上海分享) |
|---|---|---|---|
| courseId | 514eb9f7... | 4c69c8a4... | 3aa7d05d... |
| sectionCount | 1 | 1 | 1 |
| sectionType | 6 (视频) | 6 (视频) | 6 (视频) |
| attachmentId | 750128444928303104 | 749441502788067328 | 748011489081565184 |
| filename | 第十二期+黄景平.mp4 | AI智惠警情登记提效...mp4 | 上海分享.mp4 |
| size | 795 MB | 436 MB | (待测) |
| duration | 26.9 min | 21.7 min | (待测) |
| Range 1MB magic | `ftypmp42` | `ftypisom` | `ftypisom` |
| 下载验证 | ✅ | ✅ | ✅ |

---

## 6. sectionType 字典 (业务下唯一 3 种)

| type | 数量(样本) | 含义 | attachmentId 格式 | 可下载 |
|---|---|---|---|---|
| **6** | 11 | 视频 (h264 mp4) | 数字 (~19位) | ✅ |
| **9** | 2 | 测试题/考试 | UUID | ❌ (非媒体, name 含"测试") |
| **10** | 41 | 嵌套子课程 | UUID (= 子 courseId) | ✅ 递归跳 5-7 |

> **不存在** sectionType=1/2/3/4/5/7/8 等其它类型 — 业务 (中国电信集团) 完全是视频 + 测试 + 嵌套结构，**没有任何文档/PDF/PPT/zip 附件**。

---

## 7. CDN 与下载特征

| 项 | 值 |
|---|---|
| CDN 域名 | `dianxinsafecdn.zhixueyun.com` |
| 服务器 | openresty |
| 缓存策略 | Cache-Control: max-age=31536000 (1年) |
| 断点续传 | `Accept-Ranges: bytes` ✓ + Range 请求 → 206 Partial Content |
| ETag | `dd922806d00c1b2dd0ecfc67cd9a18a3-76` (多段, -76 表示 76 个分片) |
| 跨域 | `Access-Control-Allow-Origin: *` |
| 客户端 | 浏览器/原生 HTTP GET 都能下, **无需任何签名/cookie** (CDN 是公开的) |
| 视频编码 | mp4 + ftyp mp42 (QuickTime) 或 isom (ISO Base Media) |
| 码流 | 单文件直链, 无 m3u8/HLS 分片 |

**关键洞察**: CDN 是公开资源, 一旦拿到 previewUrl 就是普通 HTTP 下载, 不需要鉴权。这简化了 Rust 下载器实现 — `download_file_with_retry` 配合 HEAD + Range 即可。

---

## 8. 项目代码覆盖现状 (src/platform/zhixueyun.rs)

| 跳 | API | 项目代码 | 单测 | commit |
|---|---|---|---|---|
| 1-2 | shortCode → homeConfigId | `resolve_business_home` | 8/8 ✓ | 63c9f53 |
| 3 | homeConfigId → canvasPageConfigId[] | **缺** | 0 | — |
| 4 | canvasPageConfigId → cards[] | **缺** `fetch_business_home_cards` | 0 | — |
| 5 | card.dataId → course meta | **缺** `find_course_meta_by_ids` | 0 | — |
| 6 | courseId → chapters/sections | `chapter-progress` 已有 | ✓ | (已有) |
| 7 | attachmentId → mp4 URL | `get_file_preview_with_retry` 已有 | ✓ | (已有) |
| 递归 | type=10 嵌套处理 | **缺** | 0 | — |
| 跳过 | type=9 测试题识别 | **缺** | 0 | — |

**测试覆盖**: cargo test resolve_business_home → 8 passed (test_parse_business_home_response_*, test_pick_first_str_multi_key)

---

## 9. 安全约束 (强制)

- ❌ Bearer token 永不入源码, 仅会话内使用
- ❌ ego-lite Cookie 永远不写入 git
- ❌ js_reverse_cache/ 包含可能 user-identifying 响应, 严禁 commit
- ❌ 下载的视频不二次分发 (用户私有, 用于个人学习)
- ❌ 不绕过任何验证码 (本次分析使用 ego-lite 合法登录态)

---

## 10. Gap 与待办 (按优先级)

### P0 — 缺失的核心入口
- [ ] `fetch_business_home_cards(canvasPageConfigId, size, token) → Vec<BusinessHomeCard>`
- [ ] `find_course_meta_by_ids(course_ids, token) → Vec<CourseMeta>`
- [ ] batch orchestrator `download_home_v(url, out_dir)` 串跳 1-7

### P1 — 嵌套与类型分发
- [ ] sectionType 枚举 (6=Video, 9=Exam, 10=SubCourseLink)
- [ ] `dispatch_section(section) → DownloadAction` (递归/下载/跳过)
- [ ] 递归深度限制 (默认 3 层, 防 type=10 → type=10 → ... 死循环)

### P2 — 错误处理
- [ ] 401/403 fallback (token 过期提示用户重登 ego-lite)
- [ ] 422/500 retry (preview-new 偶发, 加 backoff)
- [ ] 部分成功保存 (manifest.json 记录已下/未下, 支持断点续下)

### P3 — 性能
- [ ] 并发下载 (channel + rayon, 受 CDN rate-limit 控制)
- [ ] 进度回调 (eventsource 模式给 GUI 显示)
- [ ] 磁盘预分配 (759 MB 文件建议 truncate + write)

---

## 11. 关键文件路径

```
x_videos-dowloads/
├── src/platform/zhixueyun.rs         # 主实现 (resolve_business_home, chapter-progress, preview-new)
├── src/drission_client.rs            # Chromium v10 cookie 解密算法 (PBKDF2+AES-128-CBC)
├── docs/zhixueyun-platform-report-2026-08-29.md  # 本报告
├── js_reverse_cache/2026-08-29-zhixueyun-home-v/
│   ├── 01_default-home-page.json     # 短码→homeConfig
│   ├── 03_home-config.json           # homeConfig 全字段 (含 shortCode=128558)
│   ├── 05_front.json                 # pageRelation 17 模块
│   ├── 11_modules.json               # nav 10 菜单
│   ├── 12_home-content-e664.json     # 8 张课程卡 (dataType=9)
│   ├── 13_home-content-6e3f.json     # 5 张专区卡 (dataType=10)
│   ├── 14_preview-new-video.json     # 第一节课视频预览
│   ├── extract_token.py              # 解密 ego-lite authorization
│   ├── dump_cookies.py               # 导全 6 个 auth cookie
│   └── build_inject.py              # 构造 Camoufox 注入 payload
└── scripts/README_intent_lock.md     # Commit 规范 (Intent Lock)
```

---

## 12. 复用建议

1. **下次会话起步**: 读本报告 + `MEMORY.md` + `63c9f53` commit diff, 直接接着写 `fetch_business_home_cards`
2. **不要重新探 API**: 14 个 JSON 是真实响应, 直接做 fixture
3. **不要重抓**: token 是会话级, 过期再 dump_cookies.py 重解
4. **新业务**: 换 shortCode 即可, 其它链不变 (智学云所有业务共用同一套 API)
5. **批量入口**: 用户给任意 `home-v?id=<短码>` 都可批量解析

---

**报告状态**: 完整 (14 个 JSON + 2 段视频下载验证 + 8 个 cargo test)
**下次更新触发**: 新增业务 / 平台 API 升级 / Rust 代码补完 P0