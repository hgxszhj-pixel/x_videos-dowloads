# 2026-09-06 — GUI 单 URL 智能识别 + 批量视频面板 checkbox UI (commits `a9ca186` → `6492013`)


> 用户报:SHANHU 课程 URL `https://wx-bgfcbieh.rexxi.cn/PC/#/yueke3/3529714` 点"获取信息"只显示 1 个 video card(`Linux基本概念和系统搭建 119:52`),期望显示完整 83 章节批量面板。根因:`use_fetch_action` 单 URL 分支只调 `fetch_video_info_impl` → SHANHU 走 `get_video_info` → 只返回首章。本 Session **完成 smart dispatch 闭环 + checkbox UI 落地 + 错误 docstring 修正**。

### 1. 设计权衡

| 候选 | 否决原因 | 选定 |
|------|---------|------|
| 重写 `use_fetch_action` 把课程池逻辑塞进主流程 | 与既有 `course_collection_state`/"分析课程"按钮路径耦合 | ❌ |
| 在主流程加智能派发,显式按钮路径不变 | smart dispatch 只在"获取信息"按钮生效,显式课程入口保留 | ✅(per plan `curious-stirring-dusk.md`) |
| eager 解析全部章节 | 83 章节 × 2s = 3 分钟 | ✅(用户接受,与现有 batch 路径一致;lazy 留作未来优化) |
| TulingPython 也走 smart dispatch | 该平台需要登录预检,与 SHANHU 匿名 `list_collection` 行为不对称 | ❌(只 SHANHU 走批量,其它平台保持单视频) |

### 2. 4 commits 落地 (HEAD `6492013`)

| commit | 类型 | 内容 |
|--------|------|------|
| `a9ca186` | feat(gui) | 新 `fetch_smart_impl(url)` 优先 `list_collection` → 逐章节 `get_video_info` → `FetchResult::Batch`;`use_fetch_action` 单 URL 分支改调 |
| `caf4da3` | docs(history) | 收录 2 份历史 platform 报告(SHANHU v1 + 智学云) |
| `cbf412d` / `609a451` | docs(handoff) | 同步 a9ca186 + caf4da3,修正"HANDOFF line 11 v1 报告已推翻" |
| `86d9298` | docs(gui) | **修正 `fetch_smart_impl` docstring 错误信息**("已知限制:N 条相同首章" → "章节级跳转"准确描述) |
| `f4bf425` | feat(gui) | 批量视频面板加 per-item checkbox + 全选/全不选 + "▶ 下载选中 (N)" + 0 选中守卫(独立 commit,与 smart dispatch 解耦) |
| `6492013` | docs(handoff) | 同步 f4bf425 + 86d9298;修正原 line 3 worktree 残留 |

### 3. 关键设计要点

- **章节级跳转**:`shanhu.rs:739-742` `list_collection` 已给每条 chapter 生成 `jump_url = "<原URL>#chapter=<id>"`;`shanhu.rs:790-791` `get_video_info` 用 `extract_chapter_id` 解析 fragment → 返回该章节 m3u8。所以批量面板实际显示 N 个**不同**章节项,不会下载首章 N 次。
- **fallback 容忍**:`fetch_smart_impl` 任一环节失败(list_collection 空/错、单章节 `get_video_info` 失败)→ graceful 降级到单视频 `FetchResult::Single`;用户始终拿到结果。
- **SHANHU 范围内智能派发**:其它平台(X.com / B 站 / TikTok)直接走 `fetch_video_info_impl` 单视频,行为不变。
- **checkbox UI 解耦**:`VideoInfoDisplay.selected: bool` 是新增字段(默认 true),但 `BatchVideoPanel` 与 `fetch_smart_impl` 无代码耦合 — 两层独立 commit 独立验证。

### 4. 验证状态

- `cargo check --bin x-video-downloader` clean
- `cargo clippy --bin x-video-downloader -- -D warnings` clean
- `cargo test --bin x-video-downloader` → **283 passed / 0 failed / 8 ignored**
- **未跑 GUI smoke test**(必须用户操作窗口):4 个 case 详见 plan `curious-stirring-dusk.md` Verification 段
  - **Case A(重点)**:SHANHU 课程 URL → 批量面板 2 个不同章节 + checkbox
  - Case B/C/D:X.com / B 站 / TulingPython 各 1 case

### 5. 与 SHANHU 闭环全景的衔接

Phase 7 (smart dispatch) + Phase 8 (checkbox UI + docstring 修正) 填补了 SHANHU 全链路最后一块 GUI 缺口:`Phase 1-3 接入 → Phase 4-5 session + 过期检测 → Phase 6 skill 复用 → Phase 7-8 GUI 闭环`。完整能力矩阵见前 Session (line 71)。



> 用户在前一 commit (`2a68896`) 完成后选 "实现 --shanhu-login CLI (Recommended)"。本 commit **完成 SHANHU 全自动闭环**。

### 1. 设计权衡

| 候选 | 否决原因 | 选定 |
|------|---------|------|
| Python camoufox SDK subprocess | 用户机器不一定装 (`python3 -c 'import camoufox'` 报 No module);新增 pip 依赖 | ❌ |
| **camoufox MCP `export_state()` JSON 文件导入** | 用户用 MCP 登录 + 导出文件 → CLI 导入,零额外依赖 | ✅ |
| (后续) ego-lite Keychain 解密路径 | mirrors `tulingpyton` 模式 (见 `chromium-macos-cookie-decryption.md`),工作量大 | follow-up |

### 2. 实现要点 (4 文件,净 +148 行)

| 文件 | 改动 |
|------|------|
| `src/cli.rs` | `--shanhu-login <STATE_JSON>` flag |
| `src/main.rs` | `handle_shanhu_login(path)` (~75 行) — 6 步流程 + dispatch 条件扩展 |
| `src/platform/shanhu_session.rs` | `is_shanhu_cookie_domain()` + 2 unit tests + 移除 `save_session` / `session_from_export` 的 `#[allow(dead_code)]` |
| `src/platform/mod.rs` | re-export `is_shanhu_cookie_domain` + `save_session` + `session_from_export` |

### 3. handler 6 步流程

1. 读 state 文件 (`std::fs::read_to_string`)
2. 解析 JSON (`serde_json::from_str`)
3. 提取 cookies 数组 (兼容 `{"cookies":[…]}` 与顶层 `[…]`)
4. 过滤 SHANHU 域 (`.wanzibook.com` / `.rexxi.cn` / `.tenwiseacademy.cn` 子串匹配)
5. `session_from_export()` + `save_session()` (原子写 `.tmp` + rename)
6. 输出 session_summary + "videoAuth 已可用" 提示

### 4. 错误路径 (4 种,各自友好提示)

| 错误 | 提示 |
|------|------|
| 文件不存在 | `读取 <path> 失败: No such file or directory` |
| JSON 损坏 | `JSON 解析失败: ...` + 提示 camoufox 导出格式 |
| 无 cookies 字段 | 提示两种格式 (顶层 / nested) + 当前文件路径 |
| 全是非 SHANHU 域 | 提示 "请在 camoufox 中登录 wx-bgfcbieh.rexxi.cn 后再 export_state" |

### 5. 验证

| 项 | 状态 |
|---|---|
| `cargo test --bin x-video-downloader` 279 passed (+2) | ✅ |
| `cargo clippy --bin x-video-downloader -- -D warnings` clean | ✅ |
| 烟雾测试三路径 (3 cookies 跳过 1 / missing file / only-google) | ✅ |
| Intent: 行通过 commit-msg hook | ✅ |

### 6. SHANHU 完整能力矩阵 (4 commits `54bbe4d` → `bd2dfeb`)

| 步骤 | 方式 | 是否需浏览器 |
|------|------|--------------|
| 1. 解析 URL → 课程/章节列表 | `list_collection` 匿名 | ❌ |
| 2. 已知 file_id 下载 (已缓存 hash) | `get_video_info` 匿名 | ❌ |
| 3. **缺失 file_id 自动发现** | `videoAuth` + session cookie | **仅首次** |
| 4. **session 一次性导入** | `--shanhu-login <state.json>` | **仅首次** |

**全自动 + 仅一次性浏览器** 闭环完成。

---

