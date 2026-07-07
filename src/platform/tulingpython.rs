//! TulingPython (小鹅通) 平台解析器
//!
//! 支持 vip.tulingpyton.cn 课程视频
//! - 通过 Scrapling 拦截播放器 m3u8 URL
//! - 课程列表页提取课程信息，引导用户使用 CatCatch 扩展

use crate::platform::{PlatformResolver, ResolverError};
use crate::types::VideoInfo;
use crate::history::Platform;
use crate::scrapling_client::ScraplingClient;
use async_trait::async_trait;
use anyhow::Result;
use tracing::{info, warn};

/// TulingPython 小鹅通解析器
pub struct TulingPythonResolver {
    scrapling: ScraplingClient,
}

impl TulingPythonResolver {
    pub fn new() -> Result<Self> {
        Ok(Self {
            scrapling: ScraplingClient::new()?,
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

    /// 从课程列表 API 响应中提取所有视频项
    fn extract_video_list(course_list: &serde_json::Value) -> Vec<(String, String)> {
        let list = match course_list.get("data").and_then(|d| d.get("list")).and_then(|l| l.as_array()) {
            Some(arr) => arr,
            None => return vec![],
        };

        let mut videos = Vec::new();
        for item in list {
            if let (Some(id), Some(name)) = (
                item.get("id").and_then(|v| v.as_str()),
                item.get("name").and_then(|v| v.as_str()),
            ) {
                if !id.is_empty() && !name.is_empty() && name != "null" {
                    videos.push((id.to_string(), name.to_string()));
                }
            }
        }
        videos
    }

    /// 返回 CatCatch 扩展引导（helper 方法）
    fn guidance(&self, url: &str, course_name: Option<&str>, course_id: Option<&str>, video_list: &[(String, String)]) -> Result<VideoInfo, ResolverError> {
        let (name, id, course_url) = match (course_name, course_id) {
            (Some(n), Some(i)) => (
                n.to_string(),
                i.to_string(),
                format!("https://vip.tulingpyton.cn/p/course/member/{}", i),
            ),
            _ => {
                let id = Self::extract_id(url).unwrap_or_else(|| "课程ID".to_string());
                (url.to_string(), id.clone(), format!("https://vip.tulingpyton.cn/p/course/member/{}", id))
            }
        };

        // 构建视频列表字符串
        let video_list_str = if !video_list.is_empty() {
            let items: Vec<String> = video_list.iter().take(20).map(|(_, name)| {
                format!("  - {}", name)
            }).collect();
            let more = if video_list.len() > 20 {
                format!("\n  ... 还有 {} 个视频", video_list.len() - 20)
            } else {
                String::new()
            };
            format!("\n\n课程视频列表 (共 {} 个):\n{}{}", video_list.len(), items.join("\n"), more)
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
        info!("使用 Scrapling 拦截 m3u8 URL: {}", url);

        let response = match self.scrapling.fetch(url).await {
            Ok(r) => r,
            Err(e) => {
                warn!("Scrapling 请求失败: {}", e);
                return self.guidance(url, None, None, &[]);
            }
        };

        // m3u8 直接拦截成功
        if !response.m3u8_urls.is_empty() {
            let m3u8_url = &response.m3u8_urls[0];
            info!("Scrapling 拦截到 m3u8: {}", m3u8_url);
            let title = crate::scrapling_extractor::extract_title(&response.html)
                .unwrap_or_else(|| "tulingpython_video".to_string());
            let mut info = VideoInfo::new(title, m3u8_url.clone(), None);
            info.download_strategy = crate::types::DownloadStrategy::Hls;
            return Ok(info);
        }

        warn!("Scrapling 未拦截到 m3u8");

        // 优先从 HTML 提取标题
        let html_title = crate::scrapling_extractor::extract_title(&response.html);

        // 尝试从课程列表 API 获取课程信息
        let course_name = response.course_list
            .as_ref()
            .and_then(|cl| cl.get("data")?.get("list")?.as_array()?.first())
            .and_then(|item| item.get("name")?.as_str());

        let course_id = response.course_list
            .as_ref()
            .and_then(|cl| cl.get("data")?.get("list")?.as_array()?.first())
            .and_then(|item| item.get("id")?.as_str());

        // 如果 HTML 标题存在但课程列表 API 失败，仍可使用 HTML 标题
        let effective_course_name = course_name.or(html_title.as_deref());

        // 提取所有视频列表
        let video_list = response.course_list
            .as_ref()
            .map(|cl| Self::extract_video_list(cl))
            .unwrap_or_default();

        self.guidance(url, effective_course_name, course_id, &video_list)
    }

    fn download_strategy(&self) -> crate::types::DownloadStrategy {
        crate::types::DownloadStrategy::Hls
    }
}
