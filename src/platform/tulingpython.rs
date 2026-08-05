//! TulingPython (小鹅通) 平台解析器
//!
//! 支持 vip.tulingpyton.cn 课程视频
//! - 通过 Drission (纯 Rust CDP) 拦截播放器 m3u8 URL，持久 profile 复用登录态
//! - 课程列表页提取课程信息，拦截失败时引导用户使用 CatCatch 扩展

use crate::drission_client::DrissionClient;
use crate::history::Platform;
use crate::platform::{CollectionEntry, PlatformResolver, ResolverError};
use crate::types::VideoInfo;
use anyhow::Result;
use async_trait::async_trait;
use tracing::{info, warn};

/// TulingPython 小鹅通解析器
pub struct TulingPythonResolver {
    drission: DrissionClient,
}

impl TulingPythonResolver {
    pub fn new() -> Result<Self> {
        Ok(Self {
            drission: DrissionClient::new()?,
        })
    }

    /// 从 URL 提取 column/product ID
    fn extract_id(url: &str) -> Option<String> {
        let patterns = [
            r"/column/(p_\w+)",
            r"/member/(p_\w+)",
            r"/big_column/(p_\w+)",
            r"/live_pc/pc/(l_\w+)",
            r"product_id=(p_\w+)",
        ];
        for pat in &patterns {
            if let Ok(re) = regex::Regex::new(pat) {
                if let Some(caps) = re.captures(url) {
                    return Some(caps.get(1).unwrap().as_str().to_string());
                }
            }
        }
        None
    }

    /// 从课程列表 API 响应中提取所有视频项。
    /// 真实接口字段命名至少 3 种:`id`/`name`、`resource_id`/`resource_title`、
    /// `spu_id`/`title`(合集页 course_nav.get 用此套);此处对三者都做识别。
    pub fn extract_video_list(course_list: &serde_json::Value) -> Vec<(String, String)> {
        let list = match course_list
            .get("data")
            .and_then(|d| d.get("list"))
            .and_then(|l| l.as_array())
        {
            Some(arr) => arr,
            None => return vec![],
        };

        let mut videos = Vec::new();
        for item in list {
            let id = item
                .get("id")
                .and_then(|v| v.as_str())
                .or_else(|| item.get("resource_id").and_then(|v| v.as_str()))
                .or_else(|| item.get("spu_id").and_then(|v| v.as_str()));
            let name = item
                .get("name")
                .and_then(|v| v.as_str())
                .or_else(|| item.get("resource_title").and_then(|v| v.as_str()))
                .or_else(|| item.get("title").and_then(|v| v.as_str()));
            if let (Some(id), Some(name)) = (id, name) {
                if !id.is_empty() && !name.is_empty() && name != "null" {
                    videos.push((id.to_string(), name.to_string()));
                }
            }
        }
        videos
    }

    /// 是否为“独立直播”URL（不是某课程下的某节课）。
    /// 规则：URL 必须形如 `live_pc/pc/l_xxx`，且没有 `product_id`/`column`/`member`/`big_column` 等课程上下文。
    pub fn is_standalone_live(url: &str) -> bool {
        let has_live = url.contains("/live_pc/pc/");
        if !has_live {
            return false;
        }
        let course_markers = ["/column/", "/member/", "/big_column/", "product_id="];
        !course_markers.iter().any(|m| url.contains(m))
    }

    /// 返回 CatCatch 扩展引导（helper 方法）
    fn guidance(
        &self,
        url: &str,
        course_name: Option<&str>,
        course_id: Option<&str>,
        video_list: &[(String, String)],
    ) -> Result<VideoInfo, ResolverError> {
        let (name, _id, course_url) = match (course_name, course_id) {
            (Some(n), Some(i)) => (
                n.to_string(),
                i.to_string(),
                format!("https://vip.tulingpyton.cn/p/course/member/{}", i),
            ),
            _ => {
                let id = Self::extract_id(url).unwrap_or_else(|| "课程ID".to_string());
                (
                    url.to_string(),
                    id.clone(),
                    format!("https://vip.tulingpyton.cn/p/course/member/{}", id),
                )
            }
        };

        // 构建视频列表字符串
        let video_list_str = if !video_list.is_empty() {
            let items: Vec<String> = video_list
                .iter()
                .take(20)
                .map(|(_, name)| format!("  - {}", name))
                .collect();
            let more = if video_list.len() > 20 {
                format!("\n  ... 还有 {} 个视频", video_list.len() - 20)
            } else {
                String::new()
            };
            format!(
                "\n\n课程视频列表 (共 {} 个):\n{}{}",
                video_list.len(),
                items.join("\n"),
                more
            )
        } else {
            String::new()
        };

        Err(ResolverError::Detailed {
            platform: Platform::TulingPython,
            url: url.to_string(),
            message: "小鹅通课程需要用 CatCatch 扩展拦截视频 URL".to_string(),
            action: format!(
                "1. 安装 CatCatch 扩展: chrome://extensions/ > 开发者模式 > 加载已解压\n\
                 2. 选择 scripts/cat-catch 目录\n\
                 3. 浏览器打开 {} 并登录账号\n\
                 4. 播放视频，扩展自动拦截 m3u8 URL\n\
                 5. 将 m3u8 URL 粘贴到下载器{}\n\n\
                 当前课程: {}",
                course_url, video_list_str, name
            ),
        })
    }
}

#[async_trait]
impl PlatformResolver for TulingPythonResolver {
    fn platform(&self) -> Platform {
        Platform::TulingPython
    }

    fn supported_url_patterns(&self) -> Vec<&'static str> {
        vec!["vip.tulingpyton.cn", "tulingpyton.cn"]
    }

    async fn get_video_info(&self, url: &str) -> Result<VideoInfo, ResolverError> {
        info!("使用 Drission CDP 拦截 m3u8 URL: {}", url);

        let response = match self.drission.fetch(url).await {
            Ok(r) => r,
            Err(e) => {
                warn!("Drission 请求失败: {}", e);
                return self.guidance(url, None, None, &[]);
            }
        };

        // m3u8 直接拦截成功
        if !response.m3u8_urls.is_empty() {
            let m3u8_url = &response.m3u8_urls[0];
            info!("Drission 拦截到 m3u8: {}", m3u8_url);
            let title = crate::scrapling_extractor::extract_title(&response.html)
                .unwrap_or_else(|| "tulingpython_video".to_string());
            let mut info = VideoInfo::new(title, m3u8_url.clone(), None);
            info.download_strategy = crate::types::DownloadStrategy::Hls;
            return Ok(info);
        }

        warn!("Drission 未拦截到 m3u8");

        // 优先从 HTML 提取标题
        let html_title = crate::scrapling_extractor::extract_title(&response.html);

        // 尝试从课程列表 API 获取课程信息
        let course_name = response
            .course_list
            .as_ref()
            .and_then(|cl| cl.get("data")?.get("list")?.as_array()?.first())
            .and_then(|item| item.get("name")?.as_str());

        let course_id = response
            .course_list
            .as_ref()
            .and_then(|cl| cl.get("data")?.get("list")?.as_array()?.first())
            .and_then(|item| item.get("id")?.as_str());

        // 如果 HTML 标题存在但课程列表 API 失败，仍可使用 HTML 标题
        let effective_course_name = course_name.or(html_title.as_deref());

        // 提取所有视频列表
        let video_list = response
            .course_list
            .as_ref()
            .map(Self::extract_video_list)
            .unwrap_or_default();

        self.guidance(url, effective_course_name, course_id, &video_list)
    }

    fn download_strategy(&self) -> crate::types::DownloadStrategy {
        crate::types::DownloadStrategy::Hls
    }

    async fn list_collection(&self, url: &str) -> Result<Vec<CollectionEntry>, ResolverError> {
        let mut visited = std::collections::HashSet::new();
        list_collection_impl(self, url, &mut visited).await
    }
}

/// 频道/合集页的单层解析：l_xxx 立即可用（jump_url 改写到 live_pc 形式），
/// p_xxx 返回待递归 URL。s_xxx / 其它（套餐、单独付费）跳过。
///
/// 返回 `None` 表示响应不是 channel 页（data.list 缺失或不是数组），调用方应走 leaf 路径。
/// 抽出纯函数便于离线测试（无需 Drission + 真实浏览器）。
fn parse_channel_items(
    course_list: &serde_json::Value,
    parent_url: &str,
) -> Option<(Vec<CollectionEntry>, Vec<String>)> {
    let top = course_list
        .get("data")
        .and_then(|d| d.get("list"))
        .and_then(|l| l.as_array())?;
    let mut concrete: Vec<CollectionEntry> = Vec::new();
    let mut recurse: Vec<String> = Vec::new();

    for item in top {
        let id = item
            .get("id")
            .and_then(|v| v.as_str())
            .or_else(|| item.get("resource_id").and_then(|v| v.as_str()))
            .or_else(|| item.get("spu_id").and_then(|v| v.as_str()));
        let name = item
            .get("name")
            .and_then(|v| v.as_str())
            .or_else(|| item.get("resource_title").and_then(|v| v.as_str()))
            .or_else(|| item.get("title").and_then(|v| v.as_str()));
        let jump = item.get("jump_url").and_then(|v| v.as_str());
        let (id, name) = match (id, name) {
            (Some(i), Some(n)) if !i.is_empty() && !n.is_empty() && n != "null" => {
                (i.to_string(), n.to_string())
            }
            _ => continue,
        };
        if id.starts_with("l_") {
            // 直播回放：API 返回的 jump_url = `/v4/course/alive/l_xxx?type=2&...`
            //   单独访问该 URL 返"暂无内容"(2026-07-31 ego-browser 实测),
            //   需改写到真实可用的 player URL: `/p/t_pc/live_pc/pc/l_xxx?product_id=...&app_id=...`
            //   否则下载阶段 get_video_info 拉不到 m3u8 → 走 CatCatch 兜底。
            let app_id = item
                .get("app_id")
                .and_then(|v| v.as_str())
                .unwrap_or("appssry6rs71641");
            let pro_id = jump
                .and_then(extract_product_id_from_url)
                .or_else(|| extract_product_id_from_url(parent_url))
                .unwrap_or_default();
            let jump_url = if !pro_id.is_empty() {
                Some(format!(
                    "https://vip.tulingpyton.cn/p/t_pc/live_pc/pc/{}?product_id={}&app_id={}",
                    id, pro_id, app_id
                ))
            } else {
                warn!("l_xxx {} 缺少 product_id,fallback 到 API jump_url", id);
                jump.map(|j| {
                    if j.starts_with("http") {
                        j.to_string()
                    } else {
                        format!("https://vip.tulingpyton.cn{j}")
                    }
                })
            };
            concrete.push(CollectionEntry { id, name, jump_url });
        } else if id.starts_with("p_") {
            // 子课程/合集：递归
            if let Some(j) = jump {
                let sub_url = if j.starts_with("http") {
                    j.to_string()
                } else {
                    format!("https://vip.tulingpyton.cn{j}")
                };
                recurse.push(sub_url);
            }
        }
        // s_xxx / 其它：跳过（套餐、单独付费）
    }

    Some((concrete, recurse))
}

/// 递归展开,带 visited 防环。同 URL 第二次访问直接返回空,避免系列课互相跳转时无限递归。
fn list_collection_impl<'a>(
    resolver: &'a TulingPythonResolver,
    url: &'a str,
    visited: &'a mut std::collections::HashSet<String>,
) -> std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<Vec<CollectionEntry>, ResolverError>> + Send + 'a>,
> {
    let canonical = canonicalize_url(url);
    Box::pin(async move {
        if !visited.insert(canonical) {
            return Ok(vec![]);
        }
        if TulingPythonResolver::is_standalone_live(url) {
            return Ok(vec![]);
        }
        let response = resolver
            .drission
            .fetch(url)
            .await
            .map_err(|e| ResolverError::Network(format!("Drission 请求失败: {e}")))?;
        // 频道/合集页：data.list 中 p_xxx 递归 → l_xxx 改写后直出
        if let Some(cl) = response.course_list.as_ref() {
            if let Some((mut concrete, recurse)) = parse_channel_items(cl, url) {
                for sub_url in recurse {
                    match list_collection_impl(resolver, &sub_url, visited).await {
                        Ok(mut subs) => concrete.append(&mut subs),
                        Err(e) => {
                            warn!("子课程 {sub_url} 展开失败：{e}");
                        }
                    }
                }
                return Ok(concrete);
            }
        }
        // 课程目录直拉：data.list 本身就是课节
        let list = match response.course_list.as_ref() {
            Some(v) => TulingPythonResolver::extract_video_list(v),
            None => vec![],
        };
        Ok(list
            .into_iter()
            .map(|(id, name)| CollectionEntry {
                id,
                name,
                jump_url: None,
            })
            .collect())
    })
}

/// URL 规范化(去 query/fragment),用于 list_collection 防环的 visited 集合。
/// 系列课的 course_nav 响应里章节之间互相跳转,query 串会带 product_id 等差异;
/// 同一资源用规范化 key 去重。
fn canonicalize_url(url: &str) -> String {
    let end = url.find(['?', '#']).unwrap_or(url.len());
    url[..end].to_string()
}

/// 从 URL 提取 product_id (`p_xxx`)。优先 `product_id=`,其次 `pro_id=`(API jump_url 用后者)。
/// 用于把 l_xxx 条目的 jump_url 改写到真实可用的 `/p/t_pc/live_pc/pc/l_xxx?product_id=...&app_id=...`,
/// 避免回落到 CatCatch 兜底(API jump_url `/v4/course/alive/...` 单独访问返"暂无内容")。
fn extract_product_id_from_url(url: &str) -> Option<String> {
    for key in &["product_id=", "pro_id="] {
        if let Some(idx) = url.find(key) {
            let after = &url[idx + key.len()..];
            let end = after.find(['&', '#']).unwrap_or(after.len());
            let val = &after[..end];
            if val.starts_with("p_") {
                return Some(val.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn real_fixture_path() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("tuling_course_real.json")
    }

    fn column_items_fixture_path() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("tuling_column_items_real.json")
    }

    fn load_real_fixture() -> serde_json::Value {
        let path = real_fixture_path();
        let raw = fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!(
                "real course fixture should exist at {}: {e}",
                path.display()
            )
        });
        serde_json::from_str(&raw).expect("real course fixture must be valid JSON")
    }

    fn build_wrapper(list: serde_json::Value) -> serde_json::Value {
        serde_json::json!({ "data": { "list": list } })
    }

    #[test]
    fn extract_video_list_returns_all_22_unique_items_from_real_fixture() {
        let fixture = load_real_fixture();
        let items = TulingPythonResolver::extract_video_list(&fixture);
        assert_eq!(items.len(), 22, "must parse all 22 unique course items");
        let unique: std::collections::HashSet<&str> =
            items.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(unique.len(), 22, "ids must be unique");
        // 真实字段映射：resource_id / resource_title 应当被识别为 id / name
        assert!(items.iter().all(|(id, _)| id.starts_with("l_")));
        assert!(items[0].1.contains("核心"));
    }

    #[test]
    fn extract_video_list_ignores_missing_or_null_fields() {
        let wrapper = build_wrapper(serde_json::json!([
            { "id": "l_a", "name": "ok-1" },
            { "id": "l_b", "name": "" },
            { "id": "",    "name": "no-id" },
            { "id": "l_c", "name": "null" },
            { "id": "l_d", "name": "ok-2" },
        ]));
        let items = TulingPythonResolver::extract_video_list(&wrapper);
        assert_eq!(
            items,
            vec![("l_a".into(), "ok-1".into()), ("l_d".into(), "ok-2".into())]
        );
    }

    #[test]
    fn extract_video_list_returns_empty_when_data_list_missing() {
        let empty = serde_json::json!({ "data": {} });
        assert!(TulingPythonResolver::extract_video_list(&empty).is_empty());
        let no_data = serde_json::json!({});
        assert!(TulingPythonResolver::extract_video_list(&no_data).is_empty());
    }

    #[test]
    fn extract_id_distinguishes_standalone_live_from_course_url() {
        // 课程页（column/p_xxx 或 product_id=p_xxx）→ 非独立直播
        assert!(!TulingPythonResolver::is_standalone_live(
            "https://vip.tulingpyton.cn/p/t_pc/course_pc_detail/column/p_695b9de1e4b0694c5b6f52c4?product_id=p_695b9de1e4b0694c5b6f52c4"
        ));
        // 单条直播（live_pc/pc/l_xxx，无 product_id/column/member/big_column）→ 独立直播
        assert!(TulingPythonResolver::is_standalone_live(
            "https://vip.tulingpyton.cn/p/t_pc/live_pc/pc/l_69eb507ae4b0694c34ff3b91"
        ));
        // 课节页（live_pc/pc/l_xxx，但带 product_id）→ 不算独立直播，应返回课程池
        assert!(!TulingPythonResolver::is_standalone_live(
            "https://vip.tulingpyton.cn/p/t_pc/live_pc/pc/l_xxx?product_id=p_yyy"
        ));
    }

    #[test]
    fn extract_video_list_recognises_resource_id_and_resource_title_aliases() {
        // 真实接口有些条目用 resource_id/resource_title 而不是 id/name
        let wrapper = build_wrapper(serde_json::json!([
            { "resource_id": "l_a", "resource_title": "直播回放A" },
            { "id": "l_b", "name": "直播回放B" },
            { "id": "l_c" }
        ]));
        let items = TulingPythonResolver::extract_video_list(&wrapper);
        let ids: Vec<&str> = items.iter().map(|(id, _)| id.as_str()).collect();
        assert!(ids.contains(&"l_a"));
        assert!(ids.contains(&"l_b"));
        // 缺 name/title 的条目应被忽略
        assert!(!ids.contains(&"l_c"));
    }

    /// 回归测试:课程合集页(course_pc_detail/middle_page + resource_type=6)触发的
    /// `xe.micro_page.pc.course_nav.get` 响应使用 `spu_id` / `title` 而非 `id` / `name`,
    /// extract_video_list 必须能识别,否则 list_collection 收到 0 项。
    ///
    /// Fixture 是真实登录响应脱敏(2026-07-28,用户 URL p_69e86659e4b0694c5bb4b923)。
    #[test]
    fn extract_video_list_handles_course_nav_spuid_and_title() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("tuling_course_nav_real.json");
        let raw = fs::read_to_string(&path).expect("course_nav fixture should exist");
        let fixture: serde_json::Value =
            serde_json::from_str(&raw).expect("course_nav fixture must be valid JSON");

        let items = TulingPythonResolver::extract_video_list(&fixture);
        assert_eq!(items.len(), 5, "5 个 chapter 都能从 spu_id+title 抽出");
        let unique: std::collections::HashSet<&str> =
            items.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(unique.len(), 5, "ids 唯一");
        // 每个 id 都以 p_ 开头(spu_id 都是子课程)
        assert!(items.iter().all(|(id, _)| id.starts_with("p_")));
        // 真实章节名包含
        let names: Vec<&str> = items.iter().map(|(_, n)| n.as_str()).collect();
        assert!(names.iter().any(|n| n.contains("核心编程2630期")));
        assert!(names.iter().any(|n| n.contains("核心编程2629期")));
    }

    /// 回归测试:canonicalize_url 必须去 query/fragment,确保 visited 集合对同一资源
    /// (不同 product_id 参数)能正确去重。
    #[test]
    fn canonicalize_url_strips_query_and_fragment() {
        assert_eq!(
            canonicalize_url("https://vip.tulingpyton.cn/detail/p_xxx/6?product_id=p_xxx"),
            "https://vip.tulingpyton.cn/detail/p_xxx/6"
        );
        assert_eq!(
            canonicalize_url("https://vip.tulingpyton.cn/detail/p_xxx/6"),
            "https://vip.tulingpyton.cn/detail/p_xxx/6"
        );
        assert_eq!(
            canonicalize_url("https://example.com/path#anchor?q=1"),
            "https://example.com/path"
        );
        // 不同 product_id 但同一路径 → 同 canonical → visited 命中
        let a = canonicalize_url("https://vip.tulingpyton.cn/detail/p_xxx/6?product_id=p_xxx");
        let b = canonicalize_url("https://vip.tulingpyton.cn/detail/p_xxx/6?product_id=p_yyy");
        assert_eq!(a, b, "同 path 不同 query 应去重,防止 visited 漏检");
    }

    /// 回归测试:l_xxx 条目的 jump_url 必须改写到 `/p/t_pc/live_pc/pc/l_xxx?product_id=...&app_id=...`,
    /// 否则下载阶段会拿到 API 的 `/v4/course/alive/...`(该 URL 单独访问返"暂无内容"),
    /// 拦不到 m3u8 → 走 CatCatch 兜底。
    ///
    /// 2026-07-31 通过 ego-browser 实测:
    ///   - /v4/course/alive/l_xxx?type=2&...  → "暂无内容"
    ///   - /p/t_pc/live_pc/pc/l_xxx?product_id=p_xxx&app_id=appssry6rs71641 → 真实播放器,拦到 m3u8
    #[test]
    fn extract_product_id_from_url_recognises_both_keys() {
        // 优先 product_id=
        assert_eq!(
            extract_product_id_from_url(
                "https://vip.tulingpyton.cn/p/t_pc/course_pc_detail/column/p_xxx?product_id=p_zzz"
            ),
            Some("p_zzz".to_string())
        );
        // fallback pro_id=(API jump_url 用此 key)
        assert_eq!(
            extract_product_id_from_url(
                "/v4/course/alive/l_xxx?type=2&pro_id=p_yyy&conduit_id=p_yyy"
            ),
            Some("p_yyy".to_string())
        );
        // product_id 优先于 pro_id
        assert_eq!(
            extract_product_id_from_url("https://x.com/?pro_id=p_a&product_id=p_b"),
            Some("p_b".to_string())
        );
        // 解析到 # fragment 终止
        assert_eq!(
            extract_product_id_from_url("https://x.com/?product_id=p_xxx#section"),
            Some("p_xxx".to_string())
        );
        // 非 p_ 开头 → 忽略
        assert_eq!(
            extract_product_id_from_url("https://x.com/?product_id=l_xxx"),
            None
        );
        // 无 key → None
        assert_eq!(extract_product_id_from_url("https://x.com/foo"), None);
    }

    /// 真实课节列表(`xe.course.business_go.column.items.get`)使用 `resource_id` + `resource_title`
    /// 而非 `id` + `name`,extract_video_list 必须正确识别。fixture 来自用户 URL
    /// `p_69e079e9e4b0694c5bb0b781`(爬虫基础 27 课,2026-07-28 抓)。
    #[test]
    fn extract_video_list_handles_column_items_resource_id_and_title() {
        let path = column_items_fixture_path();
        let raw = fs::read_to_string(&path).expect("column_items fixture should exist");
        let fixture: serde_json::Value =
            serde_json::from_str(&raw).expect("column_items fixture must be valid JSON");

        let items = TulingPythonResolver::extract_video_list(&fixture);
        // API 分页:服务端 total=27 但单次返回 20(后续需分页拉取,目前不做)
        assert!(items.len() >= 20, "至少 20 项,实际 {}", items.len());
        // 每项 id 都是 l_ 前缀(直播回放)
        assert!(
            items.iter().all(|(id, _)| id.starts_with("l_")),
            "所有 id 必须以 l_ 开头"
        );
        // 每项 title 都是非空、含中文的课节标题
        assert!(items.iter().all(|(_, n)| !n.is_empty() && n != "null"));
        // 真实课节标题片段验证
        let titles: Vec<&str> = items.iter().map(|(_, n)| n.as_str()).collect();
        assert!(
            titles.iter().any(|t| t.contains("爬虫基础27")),
            "应包含 爬虫基础27 课节"
        );
        assert!(
            titles.iter().any(|t| t.contains("feapder")),
            "应包含 feapder 关键词"
        );
    }

    fn channel_fixture_path() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("tuling_channel_real.json")
    }

    fn load_channel_fixture() -> serde_json::Value {
        let raw = fs::read_to_string(channel_fixture_path()).expect("channel fixture should exist");
        serde_json::from_str(&raw).expect("channel fixture must be valid JSON")
    }

    /// 离线 E2E (1/2):频道页只含 p_xxx 子课程(无 l_xxx)时,
    /// parse_channel_items 必须把所有 p_ 项识别为待递归,跳过 s_ 套餐项。
    /// 这条路径原先只能在 src/drission_client.rs 的 #[ignore] E2E 里覆盖。
    #[test]
    fn parse_channel_items_p_subcourses_are_marked_for_recursion_only() {
        let fixture = load_channel_fixture();
        let parent_url =
            "https://vip.tulingpyton.cn/p/t_pc/course_pc_detail/column/p_root?product_id=p_root";
        let (concrete, recurse) =
            parse_channel_items(&fixture, parent_url).expect("data.list 存在 → channel 页");
        assert!(
            concrete.is_empty(),
            "channel 页全 p_/s_,concrete 应为空,实有 {}",
            concrete.len()
        );
        assert_eq!(recurse.len(), 4, "4 个 p_ 子课程,recurse 应有 4 条");
        for url in &recurse {
            assert!(url.starts_with("https://vip.tulingpyton.cn/p/course/big_column/p_"));
        }
    }

    /// 离线 E2E (2/2):课节目录页全 l_xxx 时,jump_url 必须改写到
    /// `/p/t_pc/live_pc/pc/l_xxx?product_id=p_xxx&app_id=appssry6rs71641`,
    /// 否则下载阶段会拿到 API 原始 `/v4/course/alive/...`(单独访问返"暂无内容")。
    /// 这条路径原先也只能在 #[ignore] E2E 里覆盖。
    #[test]
    fn parse_channel_items_live_replays_get_rewritten_jump_url() {
        let path = column_items_fixture_path();
        let raw = fs::read_to_string(&path).expect("column_items fixture should exist");
        let fixture: serde_json::Value =
            serde_json::from_str(&raw).expect("column_items fixture must be valid JSON");
        let parent_url =
            "https://vip.tulingpyton.cn/p/t_pc/course_pc_detail/column/p_root?product_id=p_root";
        let (concrete, recurse) =
            parse_channel_items(&fixture, parent_url).expect("data.list 存在 → channel 页");
        assert!(
            concrete.len() >= 20,
            "至少 20 个 l_ 项,实有 {}",
            concrete.len()
        );
        assert!(recurse.is_empty(), "全 l_ → recurse 应为空");
        assert!(concrete.iter().all(|e| e.id.starts_with("l_")));
        for entry in &concrete {
            let url = entry.jump_url.as_ref().expect("l_ 必须有改写后的 jump_url");
            assert!(
                url.starts_with("https://vip.tulingpyton.cn/p/t_pc/live_pc/pc/l_"),
                "l_ jump_url 应改写到 live_pc 形式,实际: {url}"
            );
            assert!(
                url.contains("product_id=p_"),
                "改写后 jump_url 必须含 product_id=p_xxx,实际: {url}"
            );
            assert!(
                url.contains("app_id=appssry6rs71641"),
                "改写后 jump_url 必须含默认 app_id,实际: {url}"
            );
        }
    }

    /// 非 channel 页(无 data 或 data.list 不是数组)→ parse_channel_items 返回 None,
    /// 调用方应走 leaf 路径(extract_video_list)。
    #[test]
    fn parse_channel_items_returns_none_for_non_channel_page() {
        let no_data = serde_json::json!({});
        assert!(parse_channel_items(&no_data, "https://x.com").is_none());
        let data_str = serde_json::json!({ "data": "string" });
        assert!(parse_channel_items(&data_str, "https://x.com").is_none());
        let list_obj = serde_json::json!({ "data": { "list": {} } });
        assert!(parse_channel_items(&list_obj, "https://x.com").is_none());
    }
}
