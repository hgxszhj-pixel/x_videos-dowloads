# 小鹅通课程批量解析与可选下载池设计

日期：2026-07-27（设计） / 2026-08-04（已实施）
状态：✅ 已实施（MVP 验收通过：22 项 fixture 可解析 + GUI 默认全选已落盘）

## 实施落点（2026-08-04）

| 模块 | 路径 |
|------|------|
| Collection 解析 | `src/platform/tulingpython.rs` (line 197 `list_collection` + line 207 `list_collection_impl` 递归去环) |
| CourseInfo / CourseItem 类型 | `src/platform/tulingpython.rs` (与 `PlatformResolver` trait 一致) |
| 课程池 GUI 面板 | `src/gui_dioxus/app_course_pool.rs` (154 行：标题 + 已选 N/共 M + 全选/全不选 + 每行复选框 + 进度槽) |
| 解析器注册 | `src/gui_dioxus/handlers.rs:185-186` (`registry.register(tp)`) |
| 解析→展示桥接 | `src/gui_dioxus/handlers.rs:219-262` (`fetch_course_collection` + `CourseItemDisplay` 转换) |
| 进度槽（跨 task 原子） | `src/gui_dioxus/handlers.rs:301-` (`CourseItemSlot`) |
| 真实 fixture | `tests/fixtures/tuling_course_real.json` + `tuling_column_items_real.json` + `tuling_course_nav_real.json` + `tuling_channel_real.json` |
| 单元测试 | `src/platform/tulingpython.rs:367-525` (9 个 #[test]，含 fixture 解析、l_/p_ 区分、重复 ID 去重) |

## 已知未完成（下一阶段）

- `course_pool_e2e` (`src/drission_client.rs:1101`) 仍 `#[ignore]`，依赖真实浏览器。下一步：抽离纯解析函数，使其基于 fixture 在 CI 中可跑。
- 直播回放 (`l_*`) 走单视频流程，不进课程池 — 设计文档已规定，无需改动。

## 背景

现有下载器已经能解析并下载单个视频。用户希望输入一个小鹅通课程页 URL，批量分析其中的视频，形成可勾选的下载池，再只下载选中的视频。

验收样本：

- 课程：核心编程2627期
- URL：`https://vip.tulingpyton.cn/p/t_pc/course_pc_detail/column/p_695b9de1e4b0694c5b6f52c4?product_id=p_695b9de1e4b0694c5b6f52c4`
- 预期：解析出 22 个唯一视频条目

匿名抓取只返回“该内容暂无法观看”。现有专用 Drission profile 的真实探测也显示未登录，未触发课程目录接口。因此实现必须先建立已授权登录态，并用登录后的真实响应确认目录 API 和字段，不能假定现有 `xe.course.business.middle.page.data.get` 的 `data.list` 就是课节列表。

## 目标

1. 输入小鹅通课程页 URL 后，展示课程标题和完整视频目录。
2. 默认全选，支持全选、全不选和逐项勾选。
3. 只下载用户选中的条目。
4. 每项独立显示解析、下载、完成、失败、取消状态，并支持失败项重试。
5. 复用现有 HLS 下载器、进度模型和下载并发配置。
6. 不读取或保存账号密码，不绕过课程权限。

## 非目标

- 不直接读取日常 Chrome/Edge profile。
- 不绕过登录、付费或内容授权。
- 不缓存长期可复用的 m3u8 地址。
- MVP 不增加 CLI 交互式课程选择。
- 不重构其他平台解析器或通用下载链路。

## 架构

### Collection 解析契约

保留现有 `PlatformResolver::get_video_info()` 的单视频契约，新增默认可选的 collection 方法：

```text
list_collection(url) -> Result<Option<CourseInfo>, ResolverError>
```

其他平台默认返回 `None`，仅 `TulingPythonResolver` 首期实现。

核心类型：

```text
CourseInfo
- id
- title
- items: Vec<CourseItem>

CourseItem
- id
- index
- title
- page_url
```

目录条目只保存稳定元数据，不保存有时效的 m3u8。

### 登录态

使用应用专用的持久浏览器 profile：

1. 检测未登录时，GUI 提供“打开登录窗口”。
2. 用户在弹出的浏览器中自行登录。
3. 登录态写入应用专用 profile，后续无头复用。
4. 应用不接触账号密码，也不把 Cookie 或响应体写入日志。

直接附加普通 Chrome/Edge 需要远程调试端口，安全和兼容成本更高，不进入 MVP。

### 数据流

```text
课程 URL
  -> 登录态检查
  -> 捕获并解析真实课程目录接口
  -> CourseInfo（预期 22 项）
  -> GUI 可选下载池（默认全选）
  -> 用户开始下载
  -> 逐项通过 CDP 实时解析 m3u8
  -> 现有 HLS 下载器
  -> 单项进度、结果和历史记录
```

CDP 解析使用同一 profile，必须串行。获得可下载 URL 后，下载可按现有配置并发；若签名寿命不足，则退化为“解析一个、立即下载一个”。

## GUI 设计

基于现有 Dioxus 批量面板增量扩展：

- 课程标题
- `已选择 N / 共 M`
- 全选、全不选、开始下载
- 每行包含：复选框、序号、标题、状态、进度、速度、取消/重试
- 默认全选
- 未选中项不创建任务，不新增 `skipped` 状态

状态流：

```text
pending -> resolving -> downloading -> completed
                                |-> failed
                                |-> cancelled
```

错误展示遵守 `src/gui_dioxus/CLAUDE.md`：图标和错误类型标签、红色背景、明确解决建议。

## 错误处理

- 未登录：提示打开登录窗口，不把匿名页误判为课程目录。
- 权限不足：明确提示，无绕过或匿名降级。
- 接口变化：显示“课程结构暂不支持”，仅记录非敏感字段名和诊断信息。
- 空列表、重复条目：拒绝静默成功；展示实际数量和原因。
- 单项解析失败：仅该项失败，其他任务继续。
- m3u8 签名过期：重新打开对应课节获取新地址，最多自动重试一次。
- profile 被占用：提示关闭应用专用浏览器窗口后重试。
- `l_*` 直播 URL：继续走单视频流程，不显示课程池。

## 测试与验收

采用 TDD：

1. 登录后捕获真实目录响应，制作脱敏 fixture。
2. 先写失败测试，验证 fixture 能解析出 22 个有序、唯一条目。
3. 覆盖缺字段、空列表、重复 ID、非视频项和 `p_*`/`l_*` 区分。
4. 覆盖默认全选、全选/全不选和只提交勾选项。
5. 验证 CDP 解析串行、下载并发不超过配置。
6. 验证单项失败与重试不影响其他项。
7. 验证现有单视频行为不回归。
8. 运行：
   - `cargo fmt --check`
   - `cargo check`
   - `cargo test`
   - `cargo clippy --all-targets -- -D warnings`
9. 真实 E2E：课程页显示 22 项，选择部分项目后只下载所选项。

## 预计修改范围

- `src/platform/mod.rs`
- `src/platform/tulingpython.rs`
- `src/drission_client.rs`
- `src/gui_dioxus/types.rs`
- `src/gui_dioxus/actions.rs`
- `src/gui_dioxus/handlers.rs`
- `src/gui_dioxus/app.rs`
- 对应单元测试、脱敏 fixture 和必要的项目文档

GUI 当前的解析器注册链还未注册 `TulingPythonResolver`，实施时一并补齐。

## 工作区保护

开始设计时工作区已有未提交改动：`Cargo.toml`、`src/main.rs`、`src/platform/tulingpython.rs`、`src/drission_client.rs`、`.claude/` 和 `SkillOpt` 等。实施必须在现有改动上做增量，不覆盖、不回退、不混入无关清理。未经用户明确要求，不创建 commit 或 push。
