# x_videos-dowloads 升级计划 v1.0

基于 RVD 项目分析后的功能增强计划

**生成时间**: 2026-06-02
**参考项目**: [SpenserCai/rust-video-downloader](https://github.com/SpenserCai/rust-video-downloader) (29 stars)

---

## 背景

当前项目 x_videos-dowloads 功能完整，GUI 已实现，但相比 RVD 项目存在以下差距：
1. 架构分层不够清晰
2. 缺少字幕/弹幕支持
3. 缺少 FFmpeg 混流
4. 认证系统简单
5. 批量下载功能有限

---

## 升级阶段

### Phase 1: 架构重构 (基础优化)

**目标**: 提升代码可维护性和扩展性

| 迭代 | 内容 | 优先级 | 工作量 |
|------|------|--------|--------|
| 1.1 | Platform Trait 重构 - 统一解析器接口 | P0 | 高 |
| 1.2 | 核心层分离 - downloader 与解析器解耦 | P0 | 高 |
| 1.3 | 配置系统重构 - 支持 rvd.toml 格式 | P1 | 中 |
| 1.4 | 错误类型统一 - 完善 error.rs | P1 | 中 |

#### 1.1 Platform Trait 重构

```rust
// 目标: 新增 src/platform/trait.rs
#[async_trait]
pub trait Platform: Send + Sync {
    fn can_handle(&self, url: &str) -> bool;
    async fn parse_video(&self, url: &str, auth: Option<&Auth>) -> Result<VideoInfo>;
    async fn get_streams(&self, video_id: &str, auth: Option<&Auth>) -> Result<Vec<Stream>>;
    fn name(&self) -> &str;
}

// 迁移现有 ResolverRegistry 到 Platform trait
```

#### 1.2 核心层分离

```
src/
├── cli/           # 新增: 命令行参数解析
├── app/           # 新增: 应用协调逻辑
├── platform/      # 重命名: resolvers/ → platform/
│   ├── trait.rs   # 新增: Platform trait
│   ├── bilibili.rs
│   ├── tiktok.rs
│   └── ...
├── core/          # 新增: 核心功能
│   ├── downloader.rs
│   ├── muxer.rs   # 新增: FFmpeg 混流
│   ├── subtitle.rs # 新增: 字幕处理
│   └── progress.rs
├── auth/          # 新增: 认证模块
│   ├── login.rs
│   ├── cookie.rs
│   └── token.rs
└── utils/         # 现有工具模块
```

---

### Phase 2: 功能增强

**目标**: 对齐 RVD 核心功能

| 迭代 | 内容 | 优先级 | 工作量 |
|------|------|--------|--------|
| 2.1 | 字幕下载支持 | P1 | 中 |
| 2.2 | 弹幕下载支持 (ASS) | P1 | 中 |
| 2.3 | FFmpeg 混流集成 | P1 | 中 |
| 2.4 | 画质/编码选择 | P2 | 低 |
| 2.5 | 下载队列管理 | P2 | 中 |

#### 2.1 字幕支持

```rust
// src/core/subtitle.rs
pub struct Subtitle {
    lang: String,
    url: String,
    format: SubtitleFormat, // SRT, VTT, ASS
}

pub async fn download_subtitle(client: &Client, subtitle: &Subtitle) -> Result<String>;
pub async fn convert_subtitle(content: &str, from: SubtitleFormat, to: SubtitleFormat) -> Result<String>;
```

#### 2.2 弹幕支持

```rust
// src/core/danmaku.rs
pub struct Danmaku {
    text: String,
    time: f64,
    color: u32,
}

pub async fn download_danmaku(url: &str, format: DanmakuFormat) -> Result<Vec<Danmaku>>;
pub fn danmaku_to_ass(danmaku: Vec<Danmaku>, width: u32, height: u32) -> String;
```

#### 2.3 FFmpeg 混流

```rust
// src/core/muxer.rs
pub async fn mux_video_audio(
    video_path: &Path,
    audio_path: &Path,
    output_path: &Path,
    ffmpeg_path: &str,
) -> Result<()> {
    // 使用 FFmpeg 合并视频和音频流
    // 支持杜比视界/杜比全景声
}
```

---

### Phase 3: 认证系统增强

**目标**: 完善登录和认证机制

| 迭代 | 内容 | 优先级 | 工作量 |
|------|------|--------|--------|
| 3.1 | Cookie Pool 增强 | P0 | 中 |
| 3.2 | Token 自动刷新 | P1 | 中 |
| 3.3 | 二维码登录 (Bilibili) | P2 | 高 |
| 3.4 | 认证状态持久化 | P1 | 低 |

#### 3.1 Cookie Pool 增强

```rust
// src/auth/cookie_manager.rs
pub struct CookieManager {
    pool: HashMap<Platform, Vec<Cookie>>,
    current_index: AtomicUsize,
}

impl CookieManager {
    pub fn get_cookie(&self, platform: &str) -> Option<String>;
    pub fn refresh_if_needed(&mut self, platform: &str) -> Result<()>;
}
```

---

### Phase 4: 批量下载增强

**目标**: 支持更多批量下载场景

| 迭代 | 内容 | 优先级 | 工作量 |
|------|------|--------|--------|
| 4.1 | 收藏夹批量下载 | P2 | 中 |
| 4.2 | UP主空间批量下载 | P2 | 中 |
| 4.3 | 合集/系列批量下载 | P2 | 中 |
| 4.4 | 下载队列管理 | P1 | 中 |

#### 4.1 批量下载场景

```rust
// 支持的批量下载 URL 类型
// - https://space.bilibili.com/{mid}/favlist?fid={fav_id}  收藏夹
// - https://space.bilibili.com/{mid}                        UP主空间
// - https://www.bilibili.com/medialist/play/ml{media_id}   合集
// - https://space.bilibili.com/{mid}/channel/seriesdetail?sid={series_id}  系列
```

---

### Phase 5: 用户体验优化

**目标**: 提升用户体验

| 迭代 | 内容 | 优先级 | 工作量 |
|------|------|--------|--------|
| 5.1 | 进度显示优化 | P0 | 低 |
| 5.2 | 详细的错误提示 | P1 | 低 |
| 5.3 | 下载完成通知 | P2 | 低 |
| 5.4 | 支持配置文件热重载 | P2 | 中 |

---

## 实施顺序

```
Phase 1 (架构重构)
  ├── 1.1 Platform Trait 重构
  └── 1.2 核心层分离
        └── 1.3 配置系统重构
              └── 1.4 错误类型统一

Phase 2 (功能增强)
  ├── 2.1 字幕支持
  └── 2.2 弹幕支持
        └── 2.3 FFmpeg 混流

Phase 3 (认证系统)
  └── 3.1 Cookie Pool 增强
        └── 3.4 认证状态持久化

Phase 4 (批量下载)
  └── 4.4 下载队列管理

Phase 5 (用户体验)
  └── 5.1 进度显示优化
```

---

## 风险评估

| 风险 | 影响 | 缓解措施 |
|------|------|----------|
| 架构重构影响现有功能 | 高 | 分阶段进行，每阶段测试 |
| FFmpeg 依赖增加 | 中 | 提供可选功能 |
| 认证系统复杂度 | 中 | 保持 Cookie 方式兼容 |

---

## 预期收益

1. **代码可维护性**: Platform trait 使添加新平台更容易
2. **功能完整性**: 字幕/弹幕支持提升 B 站用户体验
3. **扩展性**: 清晰的层次结构便于后续功能添加
4. **竞争性**: 对标 RVD 29 stars 项目功能

---

## 后续规划

- Phase 6: WebUI (可选)
- Phase 7: 插件系统
- Phase 8: 云端同步配置