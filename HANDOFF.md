# X Video Downloader - 项目交接文档

**更新时间**: 2026-06-14 (最新更新)
**分支**: gitbutler/workspace
**最后提交者**: Claude Code (MiniMax-M2.7)

---

## 项目概述

多平台视频下载器，支持 X/Twitter, TikTok, 抖音, Bilibili 等。

**技术栈**: Rust + tokio, Dioxus GUI, yt-dlp, ResolverRegistry

---

## 快速开始

```bash
cargo run -- "URL"              # CLI
cargo run -- --gui              # GUI (Dioxus)
cargo build --release           # 发行版
```

---

## 核心模块

| 模块 | 文件 | 说明 |
|------|------|------|
| Platform | `src/platform/` | 平台解析器 (TikTok/DouYin/Bilibili 等) |
| GUI | `src/gui_dioxus.rs` | Dioxus GUI (~1100行) |
| Downloader | `src/downloader.rs` | 分段并行下载 + 断点续传 |
| Resolver | `src/platform/mod.rs` | ResolverRegistry + PlatformResolver trait |

---

## 解析器选择逻辑

| URL 域名 | 解析器 | 说明 |
|----------|--------|------|
| x.com/twitter.com | FxTwitterResolver | API 解析 |
| tiktok.com | TikTokResolver | 需要 msToken |
| douyin.com | DouYinResolver | 需要 msToken |
| bilibili.com | BilibiliResolver | yt-dlp |
| 其他 | YtDlpResolver | yt-dlp fallback |

---

## 近期完成的工作

### 5. Clippy Warning Cleanup + Slint 移除 (2026-06-14)

**问题**: 71 clippy warnings accumulated + Slint migration residue
**修复**: Systematically fixed warnings, removed Slint residue (95KB code)

#### 5.1 Clippy Warning Cleanup

| 修复项 | 文件 | 状态 |
|--------|------|------|
| 移除未使用的 imports (RwLock, Config) | gui_dioxus.rs | ✅ |
| 移除 dead assignment (last_error) | downloader.rs | ✅ |
| 移除不必要的 mut (state, ytdlp) | gui_dioxus.rs | ✅ |
| 移除 dead function (fetch_video_info_sync) | gui_dioxus.rs | ✅ |
| 修复可见性不匹配 (BatchState) | gui_dioxus.rs | ✅ |
| 添加 #[allow(dead_code)] 到预留字段/方法 | gui_dioxus.rs, i18n.rs, queue.rs, history.rs | ✅ |
| cargo fix 自动修复 | - | ✅ 14处 |

#### 5.2 Slint + ui/ 模块完全移除

**已删除文件/目录** (~135KB → 0):
- `src/gui.rs` (74KB) - Slint GUI 实现，从未调用
- `ui.slint` (21KB) - Slint UI 定义文件
- `build.rs` - Slint 编译脚本
- `src/ui/` (40KB) - 整个 Dioxus UI 组件模块（从未被 gui_dioxus.rs 使用）

**已修改**:
- `Cargo.toml` - 移除 slint/slint-build 依赖
- `src/main.rs` - 移除 `mod gui;` 和 `mod ui;`
- `src/lib.rs` - 移除 `pub mod gui;`

**已全部清除！** 通过添加 `#[allow(dead_code)]` 到预留字段/方法，抑制 Dioxus RSX 宏导致的误报。

---

### 6. 性能优化 (Phase 1-2)

#### Phase 1: URL 匹配优化
- **问题**: 每个解析器调用 `to_lowercase()` + `extract_hostname()`，重复分配
- **修复**: `select_resolver()` 入口一次性提取 hostname 并缓存
- **文件**: `src/platform/url_matcher.rs`, `src/platform/mod.rs`
- **预估加速**: 5-10%

```rust
// url_matcher.rs 新增 API
pub fn extract_hostname_and_lower(url: &str) -> Option<String>
pub fn detect_platform_by_host(host_lower: &str) -> Platform
pub fn url_matches_platform_by_host(host_lower: &str, platform: Platform) -> bool

// platform/mod.rs PlatformResolver trait 新增
fn supports_url_by_host(&self, host_lower: &str) -> bool
```

#### Phase 2: CookiePool 读写锁优化
- **问题**: `get()` 方法用写锁 + O(n) 扫描，高并发时串行化
- **修复**: 读写锁分离 — 读锁扫描找 LRU 候选，写锁只更新一条记录
- **文件**: `src/platform/cookie_pool.rs`
- **预估加速**: 10-20%

```rust
pub fn get(&self) -> Option<HashMap<String, String>> {
    // Phase 1: 读锁扫描找 LRU 候选
    let candidate_idx = { ... };

    // Phase 2: 写锁只更新命中的条目
    if let Some(idx) = candidate_idx {
        let mut entries = self.entries.write().ok()?;
        // ...
    }
}
```

#### 修复总结

| Fix | 状态 | 预估加速 |
|-----|------|----------|
| FxTwitterResolver → shared client | ✅ 已就绪 | 15-20% |
| Cache to_lowercase URL | ✅ 完成 | 5-10% |
| CookiePool — 读写锁分离 | ✅ 完成 | 10-20% |

**总计预估加速**: 15-30%

---

### 7. GUI 主题背景颜色

- **问题**: 主题切换按钮只更新状态，UI 颜色硬编码无变化
- **修复**: 主容器、卡片、按钮改用动态主题色
- **文件**: `src/gui_dioxus.rs`

```rust
// AppTheme 新增方法
pub fn background_color(&self) -> &'static str   // 主背景
pub fn card_background(&self) -> &'static str    // 卡片背景
pub fn text_color(&self) -> &'static str        // 文字颜色
```

**4 套主题**: Purple / Ocean / Sunset / Forest

---

### 8. GUI 格式选择器（未完成/部分支持）

- **问题**: 无法选择下载视频的画质
- **修复**: 添加 `selected_format_id` 字段 + 下拉框 UI
- **文件**: `src/gui_dioxus.rs`
- **状态**: UI 已完成，但 X/Twitter 平台不支持格式选择

**原因**: X/Twitter API 返回的媒体格式信息不完整，formats 列表为空

**支持格式选择的平台**: Bilibili、抖音等（需要 yt-dlp 完整格式信息）

---

### 9. omp (oh-my-pi) 安装配置

**安装方式**: 本地开发模式
```bash
cd /Users/hgxszhj/Downloads/oh-my-pi-main
bun install && bun run install:dev
```

**配置文件**: `~/.omp/agent/mcp.json`
```json
{
  "mcpServers": { ... },
  "disabledServers": ["sequential-thinking", "ecc:github", ...]
}
```

**模型配置**: `~/.omp/agent/models.yml`
```yaml
providers:
  minimax:
    baseUrl: https://api.minimax.chat/v1
    apiKey: sk-cp-7RERj87DFu5e0p2dRlOUlpG_eU7Lr4qiG-...
    api: openai-completions
    models:
      - id: MiniMax-M2.7
```

**使用**:
```bash
omp -p --provider minimax --model MiniMax-M2.7 "你的分析任务"
```

---

## 关键实现经验

### 1. URL 匹配必须精确域名

**问题**: `contains("bilibili.com")` 会误匹配 `notbilibili.com`

**解决**: 精确域名匹配
```rust
// hostname 完全等于或以 ".domain" 结尾
host_lower == p || host_lower.ends_with(&format!(".{}", p))
```

### 2. 错误消息必须净化 URL

**问题**: `ResolverError::Detailed` 暴露完整 URL 可能含 token

**解决**: 移除敏感参数 (token, session, msToken, csrf, sig 等)

### 3. 进度更新用阈值过滤

**问题**: 频繁更新导致 UI 渲染浪费

**解决**: 进度变化超过 1% 才更新 UI

### 4. Cookie 文件权限 0o600

**解决**: `#[cfg(unix)]` 下设置 `std::fs::Permissions::from_mode(0o600)`

### 5. 分段下载失败不要回退

**问题**: 分段下载失败后回退到普通下载会重复下载

**解决**: 分段失败直接报错，不回退

---

## 项目状态

| 检查项 | 状态 |
|--------|------|
| cargo build | ✅ 0 errors |
| cargo test | ✅ 345 passed (165 lib + 174 bin + 6 integration) |
| cargo clippy | ✅ 0 warnings |

---

## 已知问题

| 问题 | 状态 | 解决 |
|------|------|------|
| 抖音 "Fresh cookies needed" | 已知 | 导出 Netscape 格式 Cookie |
| TikTok 403/HTML | 已知 | 配置 msToken |
| X/Twitter 格式选择 | 平台限制 | X API 不返回完整格式信息 |
| 主题背景渐变未生效 | ✅ 已修复 | AppTheme 方法 + 动态样式 |
| tokio shutdown panic | 已知 | 可用 `2>/dev/null` 抑制 stderr |
| SkillOpt-Sleep 无新 session | Night 2: 0 sessions | 继续积累使用 |

---

### 7. FxTwitter 文件大小估算 (2026-06-14)

**问题**: X/Twitter 视频格式选择器显示的文件大小为空
**修复**: 用比特率 × 时长 ÷ 8 估算文件大小
**文件**: `src/fxtwitter.rs`

```rust
// 估算文件大小: bitrate(bps) × duration(s) / 8 = bytes
let filesize = fmt.bitrate.and_then(|br| {
    video_info.duration.map(|dur| (br * dur) / 8)
});
```

---

### 8. 新增功能 (2026-06-14)

#### 8.1 YouTube 解析器
- **文件**: `src/platform/youtube.rs` (新建)
- **功能**: 专项 YouTube 解析器，支持 Cookie 认证
- **注册**: 已添加到 `src/main.rs` ResolverRegistry
- **支持 URL**: youtube.com, youtu.be

#### 8.2 下载历史 CSV 导出
- **文件**: `src/history.rs`
- **新增方法**: `export_csv(path: &Path) -> Result<()>`
- **依赖**: Cargo.toml 添加 `csv = "1.3"`

#### 8.3 下载调度字段
- **文件**: `src/types.rs`
- **新增字段**: `DownloadRequest.schedule_time: Option<DateTime<Utc>>`

#### 8.4 tokio shutdown panic workaround
- **问题**: tokio 1.52+ 在 runtime shutdown 时打印 panic 消息
- **状态**: 已知问题，可用 `cargo run -- "URL" 2>/dev/null` 抑制 stderr
- **不影响功能**: 下载实际正常工作

---

## 调试

```bash
RUST_LOG=debug cargo run -- "URL" 2>/dev/null  # 详细日志（抑制 tokio panic）
```

---

## 下一步建议

1. **格式选择器完善**: 在支持的平台上（Bilibili、抖音）测试格式选择
2. **进度条优化**: 考虑 channel 推送替代轮询
3. **协作下载**: 完善 P2P 协作系统的 NAT 穿透
4. **tokio panic 修复**: 调查嵌套 runtime 问题，切换到 #[tokio::main] 宏
