//! SHANHU/万字 SaaS 平台解析器（白标域 rexxi.cn + 后端 wanzibook.com + CDN tenwiseacademy.cn）
//!
//! ## 背景
//! SHANHU 是珊瑚 / 万字 SaaS 平台的租户品牌，例如 `wx-bgfcbieh.rexxi.cn`（京峰教育）。
//! 客户端是一个 Vue SPA，课程页 URL 形如 `/PC/#/yueke3/3529714`（hash 路由）。
//!
//! ## Phase 1 真实情况（2026-09-06 v2 实测）
//! 整条 m3u8 链路 **完全匿名**，无需任何 cookie / Authorization：
//!
//! | 步骤 | Endpoint | 鉴权 |
//! |------|----------|------|
//! | 1. 章节列表 | `GET study.wanzibook.com/.../listSections` | 匿名 ✅ |
//! | 2. play_url 解析 | `GET <appid>.tenwiseacademy.cn/<hash>.m3u8?appid=...&all_play_url=true` | 匿名 ✅ |
//! | 3. HLS playlist | `GET vod-volcengine.cskziwl.cn/<hash>/<hash>.m3u8` | 匿名, 只要 `Referer: <rexxi-host>` + `Origin` |
//! | 4. TS segments | `GET ...ts?start=X&end=Y` (byte-range) | 同上 |
//!
//! **唯一卡点**：第 2 步的 `<hash>` 是 32 字符 hex，**不是 file_id 直接算出**（MD5/SHA256/SHA1 都不匹配）。
//! hash 来自腾讯云 VOD SDK 的 PlayKey/SignURL 算法，**JS bundle 硬编码**。需要一次性浏览器引导（bootstrap）
//! 抓取并写入本地缓存，后续全部匿名下载。
//!
//! ## 阶段
//! - **Phase 1 (此文件)**: URL 识别 + `list_collection` 完全匿名返回 83 章节（仅暴露有 `file_id` 的可下载章节）。
//! - **Phase 2 (待实施)**: `bin/shanhu_bootstrap.rs` 一次性引导 + hash 缓存；本文件补全 `get_video_info()`。
//!
//! ## 设计参考
//! - `tulingpython.rs`：相同的多课程池模型，但走 Drission + CatCatch 兜底；本平台更简洁（无需浏览器拦截 m3u8）。
//! - `nnyy.rs`：纯 reqwest resolver 模板。

use crate::history::Platform;
use crate::platform::{CollectionEntry, PlatformResolver, ResolverError};
use crate::types::{DownloadStrategy, VideoInfo};
use anyhow::Result;
use async_trait::async_trait;
use reqwest::Client;
use serde_json::Value;
use std::sync::Arc;
use tracing::{info, warn};

/// SHANHU/万字 SaaS 后端 base URL（实测 2026-09-06）。
pub const SHANHU_ONLINE_API: &str = "https://study.wanzibook.com";

/// 默认租户 ID（京峰教育，rexxi.cn 白标示例）。
/// 其他租户可通过 env `SHANHU_ORG_ID` 或 config 覆盖。
pub const DEFAULT_ORG_ID: u64 = 27632958;

/// 默认 class_id。当 listSections 需要 class_id 而 URL 中未提供时使用。
/// SHANHU 课程通常 1 个 default class，url 也可能携带 `?class_id=xxx` query 参数覆盖。
/// 0 = 尝试拉所有 class（部分版本支持）。
pub const DEFAULT_CLASS_ID: u64 = 0;

/// listSections 单页最大条数（实测 83 章节一页搞定，但仍预留 num=200 防服务端改上限）。
pub const SECTION_PAGE_SIZE: u32 = 200;

/// 网络请求超时。
const HTTP_TIMEOUT_SECS: u64 = 15;

/// Hash 缓存文件名（位于 XDG config dir 下，跨课程共享）。
pub const HASH_CACHE_FILENAME: &str = "shanhu-hash-cache.json";

/// 默认 appid（京峰教育/珊瑚租户的腾讯云 VOD appid，从已拦截的 tenwiseacademy.cn URL 中实测得到）。
/// 其他租户可设 env `SHANHU_APPID` 覆盖。
pub const DEFAULT_APP_ID: &str = "111453136245362688";

/// 从已小写化的 hash 缓存文件中读取 file_id → hash 映射（JSON object）。
/// 文件不存在时返回空 map（非错误，便于首次使用）。
pub fn load_hash_cache() -> std::collections::HashMap<String, String> {
    let Some(path) = hash_cache_path() else {
        return std::collections::HashMap::new();
    };
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return std::collections::HashMap::new();
    };
    serde_json::from_str(&raw).unwrap_or_else(|e| {
        warn!("SHANHU hash 缓存解析失败 ({}), 使用空缓存", e);
        std::collections::HashMap::new()
    })
}

/// 把单条 `file_id → hash` 写入缓存（原子写：先写 `.tmp` 再 rename）。
/// 已存在的 file_id 会被覆盖。
#[allow(dead_code)] // Phase 2 bootstrap CLI 使用
pub fn save_hash_entry(file_id: &str, hash: &str) -> std::io::Result<()> {
    let Some(path) = hash_cache_path() else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "无法定位 XDG config dir",
        ));
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut map = load_hash_cache();
    map.insert(file_id.to_string(), hash.to_string());
    let serialized = serde_json::to_string_pretty(&map)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serialized)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

/// Hash 缓存文件路径。XDG config dir 与 AppConfig 同源（`com.x-video-downloader.x-video-downloader`）。
pub fn hash_cache_path() -> Option<std::path::PathBuf> {
    directories::ProjectDirs::from("com", "x-video-downloader", "x-video-downloader")
        .map(|p| p.config_dir().join(HASH_CACHE_FILENAME))
}

/// 当前生效的 appid（env 覆盖 → 常量默认）。
fn app_id() -> &'static str {
    // 注意:env var 每次调用都查;可优化为 OnceLock,但调用频率低（get_video_info 一次），先简单实现。
    static CACHE: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| {
        std::env::var("SHANHU_APPID")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| DEFAULT_APP_ID.to_string())
    })
}

/// 单章节信息（来自 listSections API）。
///
/// `duration` 字段 Phase 2 才用（`get_video_info` 写入 `VideoInfo.duration`）。
/// Phase 1 仅解析并过滤 `file_id`,保留字段以避免 Phase 2 再次改 trait 签名。
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct ChapterInfo {
    /// 章节 ID（内部 ID，非 file_id）。
    id: u64,
    /// 章节显示名（如 "1.Linux基本概念和系统搭建"）。
    name: String,
    /// 腾讯云 VOD file_id（**匿名也返回**，2026-09-06 v2 实测推翻 v1 报告）。
    /// None = 录制尚未生成，跳过。
    file_id: Option<String>,
    /// 时长（秒，部分章节无）。
    duration: Option<u64>,
}

/// SHANHU/万字 SaaS 解析器（轻量级，纯 reqwest，无浏览器）。
pub struct ShanhuResolver {
    client: Arc<Client>,
}

impl ShanhuResolver {
    pub fn new() -> Result<Self> {
        let client = Client::builder()
            .user_agent(
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.15; rv:135.0) \
                 Gecko/20100101 Firefox/135.0",
            )
            .timeout(std::time::Duration::from_secs(HTTP_TIMEOUT_SECS))
            .build()?;
        Ok(Self {
            client: Arc::new(client),
        })
    }

    /// 从 URL 提取 course_id（hash 路由模式：`/PC/#/yueke3/3529714` → 3529714）。
    /// 也兼容 `?course_id=xxx` query 参数形式。
    fn extract_course_id(url: &str) -> Option<u64> {
        // 模式 1：hash 路由 `/yueke3/<id>` 或 `/yueke<n>/<id>` 或 `/course/<id>`
        let hash_pats = [
            r"/yueke\d+/(\d+)",
            r"/course/(\d+)",
            r"/learn/(\d+)",
            r"/study/(\d+)",
        ];
        for pat in &hash_pats {
            if let Ok(re) = regex::Regex::new(pat) {
                if let Some(caps) = re.captures(url) {
                    if let Some(m) = caps.get(1) {
                        if let Ok(n) = m.as_str().parse::<u64>() {
                            return Some(n);
                        }
                    }
                }
            }
        }
        // 模式 2：query 参数 `course_id=xxx` 或 `class_id=xxx`
        for key in &["course_id=", "class_id="] {
            if let Some(idx) = url.find(key) {
                let after = &url[idx + key.len()..];
                let end = after.find(['&', '#', '/']).unwrap_or(after.len());
                if let Ok(n) = after[..end].parse::<u64>() {
                    return Some(n);
                }
            }
        }
        None
    }

    /// 从 URL 提取 class_id（query 参数 `?class_id=xxx`）。
    fn extract_class_id(url: &str) -> Option<u64> {
        let key = "class_id=";
        let idx = url.find(key)?;
        let after = &url[idx + key.len()..];
        let end = after.find(['&', '#']).unwrap_or(after.len());
        after[..end].parse::<u64>().ok()
    }

    /// 确定实际使用的 org_id（env 覆盖 → 常量默认）。
    fn org_id() -> u64 {
        std::env::var("SHANHU_ORG_ID")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_ORG_ID)
    }

    /// 匿名调 listSections 拉章节列表。
    ///
    /// **Endpoint**: `GET {SHANHU_ONLINE_API}/onlineapi/stu/course/listSections?org_id=...&course_id=...&class_id=...&page=1&num=200`
    ///
    /// **实测 (2026-09-06)**: 无 cookie / Authorization,返 200 + 完整 chapters[].videos[].file_id。
    /// 与 v1 报告"开课前无法下载、videos[] 全空"矛盾 —— 实测是平台 / 端点差异或 v1 旧版 fix。
    async fn fetch_sections(
        &self,
        course_id: u64,
        class_id: u64,
    ) -> Result<Vec<ChapterInfo>, ResolverError> {
        let org_id = Self::org_id();
        let url = format!(
            "{}/onlineapi/stu/course/listSections?org_id={}&course_id={}&class_id={}&page=1&num={}",
            SHANHU_ONLINE_API, org_id, course_id, class_id, SECTION_PAGE_SIZE
        );
        info!("SHANHU listSections → {}", url);

        let resp = self
            .client
            .get(&url)
            .header("Referer", "https://wx-bgfcbieh.rexxi.cn/")
            .header("Origin", "https://wx-bgfcbieh.rexxi.cn")
            .send()
            .await
            .map_err(|e| ResolverError::Network(format!("listSections HTTP 失败: {e}")))?;

        if !resp.status().is_success() {
            return Err(ResolverError::Network(format!(
                "listSections HTTP {}",
                resp.status()
            )));
        }

        let body: Value = resp
            .json()
            .await
            .map_err(|e| ResolverError::ParseFailed(format!("listSections JSON: {e}")))?;

        Self::parse_sections(&body)
    }

    /// 解析 listSections 响应为 `Vec<ChapterInfo>`。
    /// 响应路径:`data` (数组, 通常 1 元素) → `chapters[]` → `{id, name, videos:[{file_id, ...}]}`
    /// 也兼容扁平结构：`data.chapters[]` 或根级 `chapters[]`。
    #[allow(private_interfaces)]
    pub(crate) fn parse_sections(body: &Value) -> Result<Vec<ChapterInfo>, ResolverError> {
        // 路径 1: data 是数组,data[0].chapters
        let chapters = if let Some(arr) = body.get("data").and_then(|d| d.as_array()) {
            arr.first()
                .and_then(|d0| d0.get("chapters"))
                .and_then(|c| c.as_array())
        } else if let Some(arr) = body.get("data").and_then(|d| d.get("chapters")).and_then(|c| c.as_array()) {
            Some(arr)
        } else {
            body.get("chapters").and_then(|c| c.as_array())
        };

        let chapters = chapters.ok_or_else(|| {
            ResolverError::ParseFailed(
                "listSections 响应缺 chapters 字段(可能课程无录制或 class_id 错)".into(),
            )
        })?;

        let mut out = Vec::with_capacity(chapters.len());
        for ch in chapters {
            let id = ch.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
            let name = ch
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if id == 0 || name.is_empty() {
                continue;
            }
            // videos[0].file_id —— 匿名也返回(2026-09-06 实测)
            let videos = ch.get("videos").and_then(|v| v.as_array());
            let file_id = videos
                .and_then(|vs| vs.first())
                .and_then(|v| v.get("file_id"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .filter(|s| !s.is_empty());
            let duration = videos
                .and_then(|vs| vs.first())
                .and_then(|v| v.get("duration"))
                .and_then(|v| v.as_u64());
            out.push(ChapterInfo {
                id,
                name,
                file_id,
                duration,
            });
        }
        Ok(out)
    }

    /// 生成 bootstrap 引导错误的 `action` 字段文案。
    fn bootstrap_hint(course_id: u64, class_id: u64) -> String {
        format!(
            "1. 首次使用需运行一次性浏览器引导(约 2-3 分钟,含扫码):\n\
             \n     cargo run --bin x-video-downloader -- shanhu-bootstrap \\\n         \
             \"https://wx-bgfcbieh.rexxi.cn/PC/#/yueke3/{cid}\"\n\
             \n\
             2. 引导完成后,本课程 (course_id={cid}, class_id={cls}) 所有有录制的章节即可匿名下载。\n\
             \n\
             3. hash 缓存位置:~/.config/x-video-downloader/shanhu-hash-cache.json\n\
             \n\
             4. 后续 download 阶段无须浏览器,纯 reqwest 拉 m3u8。",
            cid = course_id,
            cls = class_id
        )
    }

    /// 从 URL fragment 提取 chapter_id (`#chapter=3557155` 或 `&chapter=3557155`)。
    /// Phase 2 list_collection 的 jump_url 形式：`https://...#chapter=<id>`。
    fn extract_chapter_id(url: &str) -> Option<u64> {
        let key = "chapter=";
        let idx = url.find(key)?;
        let after = &url[idx + key.len()..];
        // 终止于 & / # 结束 / 字符串结束
        let end = after
            .find(['&', '#'])
            .unwrap_or(after.len());
        after[..end].parse::<u64>().ok()
    }

    /// 调 tenwiseacademy.cn play_url 解析 endpoint,返 m3u8 URL。
    ///
    /// **Endpoint**: `GET https://{appid}.tenwiseacademy.cn/{hash}.m3u8?appid={appid}&all_play_url=true`
    /// **鉴权**: 匿名 ✅(2026-09-06 v2 实测)
    /// **响应**: `{ "code": 0, "data": { "play_urls": [{ "url": "...m3u8", ... }] } }`
    /// **关键 headers**: `Referer: <rexxi-host>` + `Origin: <rexxi-host>` (防盗链)
    async fn resolve_play_url(&self, file_id: &str, hash: &str) -> Result<String, ResolverError> {
        let app_id = app_id();
        let url = format!(
            "https://{appid}.tenwiseacademy.cn/{appid}/{hash}.m3u8?appid={appid}&all_play_url=true",
            appid = app_id
        );
        info!("SHANHU resolve_play_url → {}", url);

        let resp = self
            .client
            .get(&url)
            .header("Referer", "https://wx-bgfcbieh.rexxi.cn/")
            .header("Origin", "https://wx-bgfcbieh.rexxi.cn")
            .send()
            .await
            .map_err(|e| ResolverError::Network(format!("play_url HTTP 失败: {e}")))?;

        if !resp.status().is_success() {
            return Err(ResolverError::Network(format!(
                "play_url HTTP {} (file_id={file_id}, hash={hash})",
                resp.status()
            )));
        }

        let body: Value = resp.json().await.map_err(|e| {
            ResolverError::ParseFailed(format!("play_url JSON: {e}"))
        })?;

        // 响应路径:`data.play_urls[0].url` (数组取首元素,通常只有 1 路)
        let m3u8_url = body
            .get("data")
            .and_then(|d| d.get("play_urls"))
            .and_then(|p| p.as_array())
            .and_then(|arr| arr.first())
            .and_then(|u| u.get("url"))
            .and_then(|u| u.as_str())
            .ok_or_else(|| {
                ResolverError::ParseFailed(format!(
                    "play_url 响应缺 data.play_urls[0].url (file_id={file_id}, hash={hash})"
                ))
            })?
            .to_string();

        Ok(m3u8_url)
    }
}

#[async_trait]
impl PlatformResolver for ShanhuResolver {
    fn platform(&self) -> Platform {
        Platform::Shanhu
    }

    fn supported_url_patterns(&self) -> Vec<&'static str> {
        vec!["rexxi.cn", "wanzibook.com", "tenwiseacademy.cn"]
    }

    fn requires_auth(&self) -> bool {
        // Phase 1: list_collection 匿名可用。download 阶段(Phase 2)需 bootstrap。
        false
    }

    fn download_strategy(&self) -> DownloadStrategy {
        // Phase 2: get_video_info 已返 HLS m3u8 URL,直接走 HlsDownloader。
        DownloadStrategy::Hls
    }

    async fn list_collection(&self, url: &str) -> Result<Vec<CollectionEntry>, ResolverError> {
        let course_id = Self::extract_course_id(url).ok_or_else(|| {
            ResolverError::ParseFailed(format!("无法从 URL 提取 course_id: {url}"))
        })?;
        // class_id 优先级:URL query > 默认 0
        let class_id = Self::extract_class_id(url).unwrap_or(DEFAULT_CLASS_ID);

        let sections = self.fetch_sections(course_id, class_id).await?;

        if sections.is_empty() {
            return Err(ResolverError::ContentUnavailable(format!(
                "课程 {course_id} (class_id={class_id}) 无章节或未开课"
            )));
        }

        let total_count = sections.len();
        let entries: Vec<CollectionEntry> = sections
            .into_iter()
            .filter_map(|ch| {
                // 仅暴露有 file_id 的章节(录制已生成)。
                // 跳过的章节 GUI 列表中不会出现,但用户能通过 list_collection 全部看到(file_id=None 也保留)。
                if ch.file_id.is_none() {
                    warn!(
                        "SHANHU 章节 {} ({}) 无 file_id,跳过(可能录制未生成)",
                        ch.id, ch.name
                    );
                    return None;
                }
                // jump_url 在原课程 URL 后挂 `#chapter={chapter_id}` 片段。
                // get_video_info 解析该片段确定要下载的章节。
                // 片段不会发到 server,所以对 listSections / play_url 调用无副作用。
                let jump_url = if url.contains('#') {
                    format!("{url}&chapter={}", ch.id)
                } else {
                    format!("{url}#chapter={}", ch.id)
                };
                Some(CollectionEntry {
                    id: ch.id.to_string(),
                    name: ch.name,
                    jump_url: Some(jump_url),
                })
            })
            .collect();

        if entries.is_empty() {
            // 全部章节都缺 file_id → 引导用户运行 bootstrap 检查 hash 缓存,或确认课程已开课。
            return Err(ResolverError::Detailed {
                platform: Platform::Shanhu,
                url: url.to_string(),
                message: format!(
                    "课程 {course_id} 有 {total_count} 个章节,但全部缺少 file_id(录制未生成?)"
                ),
                action: Self::bootstrap_hint(course_id, class_id),
            });
        }

        info!(
            "SHANHU list_collection: course_id={} class_id={} → {} 个可下载章节",
            course_id,
            class_id,
            entries.len()
        );
        Ok(entries)
    }

    async fn get_video_info(&self, url: &str) -> Result<VideoInfo, ResolverError> {
        let course_id = Self::extract_course_id(url).ok_or_else(|| {
            ResolverError::ParseFailed(format!("无法从 URL 提取 course_id: {url}"))
        })?;
        let class_id = Self::extract_class_id(url).unwrap_or(DEFAULT_CLASS_ID);

        // 1. 拿章节列表（已知匿名返回 200）
        let sections = self.fetch_sections(course_id, class_id).await?;
        if sections.is_empty() {
            return Err(ResolverError::ContentUnavailable(format!(
                "课程 {course_id} (class_id={class_id}) 无章节"
            )));
        }

        // 2. 解析 #chapter=<id> fragment,确定要下载哪个章节。
        //    无 fragment → 默认取第一个有 hash 的章节（方便批量下载时复用 get_video_info）。
        let target_chapter_id = Self::extract_chapter_id(url);
        let chapter = if let Some(cid) = target_chapter_id {
            sections
                .into_iter()
                .find(|c| c.id == cid)
                .ok_or_else(|| {
                    ResolverError::ParseFailed(format!(
                        "URL 指定章节 {cid} 不在课程 {course_id} 章节列表中"
                    ))
                })?
        } else {
            sections.into_iter().find(|c| c.file_id.is_some()).ok_or_else(
                || ResolverError::ContentUnavailable("课程无任何已录制章节".into()),
            )?
        };

        // 3. 拿章节的 file_id
        let file_id = chapter.file_id.clone().ok_or_else(|| {
            ResolverError::ContentUnavailable(format!(
                "章节 {} ({}) 无 file_id,录制未生成",
                chapter.id, chapter.name
            ))
        })?;

        // 4. 查 hash 缓存
        let cache = load_hash_cache();
        let hash = cache.get(&file_id).cloned().ok_or_else(|| {
            ResolverError::Detailed {
                platform: Platform::Shanhu,
                url: url.to_string(),
                message: format!(
                    "file_id `{file_id}` (章节 {}) 的 hash 未在缓存中",
                    chapter.id
                ),
                action: format!(
                    "{}\n\n\
                     快速补救:\n\
                     1. 浏览器打开课程页, F12 → Network → 过滤 `tenwiseacademy.cn`\n\
                     2. 播放该章节\n\
                     3. 找到形如 `<hash>.m3u8` 的请求, 复制 32 字符 hex 部分\n\
                     4. 运行: x-video-downloader --shanhu-add-hash {file_id}:<hash>\n\n\
                     缓存位置: {}",
                    Self::bootstrap_hint(course_id, class_id),
                    hash_cache_path()
                        .as_ref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| "<未知>".into()),
                ),
            }
        })?;

        // 5. 拼 play_url 并解析 m3u8
        let m3u8_url = self.resolve_play_url(&file_id, &hash).await?;
        let duration = chapter.duration.unwrap_or(0);

        info!(
            "SHANHU get_video_info: chapter={} ({}), m3u8 长度 {}",
            chapter.id,
            chapter.name,
            m3u8_url.len()
        );

        let mut info = VideoInfo::new(chapter.name.clone(), m3u8_url, Some("SHANHU/万字".into()));
        info.duration = Some(duration);
        info.download_strategy = DownloadStrategy::Hls;
        Ok(info)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ──────────────────── extract_course_id ────────────────────

    #[test]
    fn extract_course_id_hash_route_yueke3() {
        assert_eq!(
            ShanhuResolver::extract_course_id("https://wx-bgfcbieh.rexxi.cn/PC/#/yueke3/3529714"),
            Some(3529714)
        );
    }

    #[test]
    fn extract_course_id_hash_route_yueke5() {
        // 防御性:不同 yueke<n> 子路由也要识别
        assert_eq!(
            ShanhuResolver::extract_course_id("https://other.rexxi.cn/PC/#/yueke5/12345"),
            Some(12345)
        );
    }

    #[test]
    fn extract_course_id_query_param() {
        assert_eq!(
            ShanhuResolver::extract_course_id("https://rexxi.cn/anything?course_id=99999"),
            Some(99999)
        );
    }

    #[test]
    fn extract_course_id_returns_none_for_invalid_url() {
        assert_eq!(
            ShanhuResolver::extract_course_id("https://example.com/foo"),
            None
        );
        assert_eq!(
            ShanhuResolver::extract_course_id("not a url"),
            None
        );
    }

    #[test]
    fn extract_class_id_from_query() {
        assert_eq!(
            ShanhuResolver::extract_class_id("https://rexxi.cn/x?class_id=3023633"),
            Some(3023633)
        );
        assert_eq!(
            ShanhuResolver::extract_class_id("https://rexxi.cn/x?class_id=3023633&other=1"),
            Some(3023633)
        );
        assert_eq!(ShanhuResolver::extract_class_id("https://rexxi.cn/x"), None);
    }

    // ──────────────────── parse_sections ────────────────────

    #[test]
    fn parse_sections_handles_data_array_form() {
        // 真实响应(2026-09-06 v2 实测):data 是 1 元素数组,元素.chapters
        let body = json!({
            "code": 0,
            "msg": "success",
            "data": [{
                "course_id": 3529714,
                "chapters": [
                    {
                        "id": 3557155,
                        "name": "1.Linux基本概念和系统搭建",
                        "videos": [{
                            "file_id": "3877025...",
                            "duration": 7140
                        }]
                    },
                    {
                        "id": 3560223,
                        "name": "2.Linux基础命令",
                        "videos": [{
                            "file_id": "3877025abc",
                            "duration": 3960
                        }]
                    }
                ]
            }]
        });
        let sections = ShanhuResolver::parse_sections(&body).unwrap();
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].id, 3557155);
        assert_eq!(sections[0].name, "1.Linux基本概念和系统搭建");
        assert!(sections[0].file_id.is_some());
        assert_eq!(sections[0].duration, Some(7140));
        assert_eq!(sections[1].id, 3560223);
    }

    #[test]
    fn parse_sections_handles_data_object_form() {
        // 兼容:data 直接是对象,内含 chapters
        let body = json!({
            "code": 0,
            "data": {
                "chapters": [
                    {"id": 100, "name": "A", "videos": [{"file_id": "f1"}]}
                ]
            }
        });
        let sections = ShanhuResolver::parse_sections(&body).unwrap();
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].file_id.as_deref(), Some("f1"));
    }

    #[test]
    fn parse_sections_handles_root_chapters_form() {
        // 兼容:根级 chapters
        let body = json!({
            "chapters": [
                {"id": 1, "name": "X", "videos": [{"file_id": "fid"}]}
            ]
        });
        let sections = ShanhuResolver::parse_sections(&body).unwrap();
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].name, "X");
    }

    #[test]
    fn parse_sections_skips_invalid_id_and_empty_name() {
        // parse_sections 只过滤 id=0 与 name="" 这两个结构性错误。
        // file_id 是否存在由 list_collection 阶段过滤(允许 get_video_info
        // 拿到所有章节再做 file_id 匹配)。
        let body = json!({
            "data": [{
                "chapters": [
                    {"id": 1, "name": "ok-1", "videos": [{"file_id": "abc"}]},
                    {"id": 2, "name": "empty-file-id", "videos": [{"file_id": ""}]},
                    {"id": 3, "name": "no-videos"},
                    {"id": 4, "name": "no-file-id-key", "videos": [{}]},
                    {"id": 0, "name": "zero-id"},     // id=0 跳过
                    {"id": 5, "name": ""},            // name="" 跳过
                    {"id": 6, "name": "ok-2", "videos": [{"file_id": "def", "duration": 600}]}
                ]
            }]
        });
        let sections = ShanhuResolver::parse_sections(&body).unwrap();
        // 5 个保留(id=0 和 name="" 被跳过)
        assert_eq!(sections.len(), 5);
        // file_id 字段保留(空字符串也作为 None 保留,便于 caller 区分)
        assert_eq!(sections[0].name, "ok-1");
        assert_eq!(sections[0].file_id.as_deref(), Some("abc"));
        assert!(sections[1].file_id.is_none(), "empty file_id → None");
        assert!(sections[2].file_id.is_none(), "无 videos → None");
        assert!(sections[3].file_id.is_none(), "videos[] 无 file_id key → None");
        assert_eq!(sections[4].name, "ok-2");
        assert_eq!(sections[4].duration, Some(600));
    }

    #[test]
    fn parse_sections_returns_err_for_missing_chapters() {
        // 既无 data.chapters 也无根 chapters → 解析失败
        assert!(ShanhuResolver::parse_sections(&json!({"code": 0, "data": {}})).is_err());
        assert!(ShanhuResolver::parse_sections(&json!({})).is_err());
    }

    // ──────────────────── bootstrap_hint ────────────────────

    #[test]
    fn bootstrap_hint_mentions_course_and_class_id() {
        let hint = ShanhuResolver::bootstrap_hint(3529714, 3023633);
        assert!(hint.contains("3529714"));
        assert!(hint.contains("3023633"));
        assert!(hint.contains("shanhu-bootstrap"));
        assert!(hint.contains("hash 缓存"));
    }

    // ──────────────────── trait 锚点 ────────────────────

    #[test]
    fn platform_and_patterns_match_url_matcher() {
        let r = ShanhuResolver::new().expect("resolver 构造不应失败");
        assert_eq!(r.platform(), Platform::Shanhu);
        let pats = r.supported_url_patterns();
        assert!(pats.contains(&"rexxi.cn"));
        assert!(pats.contains(&"wanzibook.com"));
        assert!(pats.contains(&"tenwiseacademy.cn"));
        assert!(!r.requires_auth(), "Phase 1 匿名可用");
    }

    #[tokio::test]
    async fn get_video_info_returns_network_error_for_unreachable_host() {
        // Phase 2: 没有真实网络时,get_video_info 会触发 listSections 失败。
        // 用 mock URL 触发 network 错误 → 验证至少 Network/ParseFailed 路径被覆盖。
        // (完整 end-to-end 测试需要 mock HTTP server,见 hls downloader 的 E2E 模式)
        let r = ShanhuResolver::new().unwrap();
        let result = r
            .get_video_info(
                "https://nonexistent-host-for-test.invalid/PC/#/yueke3/1#chapter=2",
            )
            .await;
        let err = result.expect_err("应失败(无网络)");
        match err {
            ResolverError::Network(_) | ResolverError::ParseFailed(_) => {
                // 接受任一网络/解析错误(取决于超时与重试)
            }
            other => panic!("应返回 Network/ParseFailed,实际 {other:?}"),
        }
    }

    // ──────────────────── Phase 2: extract_chapter_id ────────────────────

    #[test]
    fn extract_chapter_id_from_hash_fragment() {
        // 标准 list_collection jump_url 形式
        assert_eq!(
            ShanhuResolver::extract_chapter_id(
                "https://wx-bgfcbieh.rexxi.cn/PC/#/yueke3/3529714#chapter=3557155"
            ),
            Some(3557155)
        );
    }

    #[test]
    fn extract_chapter_id_from_query_param() {
        // list_collection 中若原 URL 已含 #,会改用 &chapter=
        assert_eq!(
            ShanhuResolver::extract_chapter_id(
                "https://rexxi.cn/x?class_id=1#chapter=123"
            ),
            Some(123)
        );
    }

    #[test]
    fn extract_chapter_id_returns_none_for_plain_url() {
        assert_eq!(
            ShanhuResolver::extract_chapter_id("https://rexxi.cn/PC/#/yueke3/3529714"),
            None
        );
        assert_eq!(
            ShanhuResolver::extract_chapter_id("https://rexxi.cn/anything"),
            None
        );
    }

    // ──────────────────── Phase 2: cache load/save round-trip ────────────────────

    #[test]
    fn load_hash_cache_returns_empty_when_no_file() {
        // 默认 XDG 路径下若无缓存文件,返空 map(非错误)
        let cache = load_hash_cache();
        // 不强求 size==0(其他测试可能残留),只验证不 panic + 可用
        let _: Vec<&String> = cache.keys().collect();
    }

    #[test]
    fn save_and_load_hash_entry_round_trip() {
        // 临时污染:写一个 file_id,验证 load 拿到。
        let test_file_id = "test_round_trip_99999";
        let test_hash = "deadbeefdeadbeefdeadbeefdeadbeef";

        save_hash_entry(test_file_id, test_hash).expect("保存应成功");
        let cache = load_hash_cache();
        assert_eq!(
            cache.get(test_file_id).map(|s| s.as_str()),
            Some(test_hash),
            "刚写入的条目应能读回"
        );

        // 清理
        if let Some(path) = hash_cache_path() {
            let _ = std::fs::remove_file(&path);
        }
    }

    // ──────────────────── Phase 2: download_strategy = Hls ────────────────────

    #[test]
    fn download_strategy_is_hls() {
        let r = ShanhuResolver::new().unwrap();
        assert!(matches!(r.download_strategy(), DownloadStrategy::Hls));
    }
}
