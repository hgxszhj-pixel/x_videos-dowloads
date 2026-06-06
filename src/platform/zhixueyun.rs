//! ZhiXueYun 平台解析器
//!
//! 支持 kc.zhixueyun.com 智学云平台的视频和文档下载
//! sectionType: 6 = 视频 (mp4), 10 = 文档 (pdf)

use crate::platform::{CookiePool, PlatformResolver, ResolverError, url_matcher};
use crate::types::VideoInfo;
use crate::history::Platform;
use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderName};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::channel;
use std::thread;
use tracing::{info, debug};

/// 下载进度回调
pub type ProgressCallback = Arc<dyn Fn(u64, u64, &str) + Send + Sync>;

/// 下载结果
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct DownloadResult {
    pub filename: String,
    pub success: bool,
    pub error: Option<String>,
    pub size: u64,
}

/// 课程信息
#[derive(Debug, Clone)]
pub struct CourseInfo {
    pub id: String,
    pub name: String,
}

/// 文档信息
#[derive(Debug, Clone)]
pub struct DocumentInfo {
    pub id: String,
    pub name: String,
    pub chapter_name: String,
}

/// 文件预览信息
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct FilePreview {
    pub secure_url: String,
    pub filename: String,
    pub file_size: u64,
}

/// 下载上下文 (封装进度相关参数，减少函数参数)
struct DownloadContext {
    progress: Option<ProgressCallback>,
    completed: Arc<AtomicU64>,
    total_bytes: Arc<AtomicU64>,
}

/// ZhiXueYun 解析器
pub struct ZhiXueYunResolver {
    cookie_pool: Arc<CookiePool>,
    token: std::sync::RwLock<Option<String>>,
    client: Arc<reqwest::blocking::Client>,
    /// 缓存的认证头（避免每次克隆 BASE_HEADERS）
    cached_headers: std::sync::RwLock<Option<HeaderMap>>,
}

impl ZhiXueYunResolver {
    /// 创建新的 ZhiXueYunResolver
    #[allow(dead_code)]
    pub fn new(cookie_pool: Arc<CookiePool>) -> Self {
        let client = reqwest::blocking::Client::builder()
            .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/148.0.0.0 Safari/537.36")
            .tcp_keepalive(std::time::Duration::from_secs(120))
            .connect_timeout(std::time::Duration::from_secs(30))
            .timeout(std::time::Duration::from_secs(600))
            .build()
            .unwrap_or_else(|e| panic!("Failed to build ZhiXueYun HTTP client: {}", e));
        Self {
            cookie_pool,
            token: std::sync::RwLock::new(None),
            client: Arc::new(client),
            cached_headers: std::sync::RwLock::new(None),
        }
    }

    /// 设置认证 token（支持运行时动态更新）
    #[allow(dead_code)]
    pub fn set_token(&self, token: String) {
        let mut write_lock = self.token.write().unwrap();
        *write_lock = Some(token);
        // Invalidate cached headers when token changes
        if let Ok(mut cached) = self.cached_headers.write() {
            *cached = None;
        }
    }

    /// 获取当前 token
    fn get_token(&self) -> Result<String, ResolverError> {
        // 1. 先尝试运行时设置的 token
        if let Ok(read_lock) = self.token.read() {
            if let Some(t) = read_lock.as_ref() {
                if !t.is_empty() {
                    return Ok(t.clone());
                }
            }
        }
        // 2. 环境变量
        if let Ok(t) = std::env::var("ZHIXUEYUN_TOKEN") {
            if !t.is_empty() {
                return Ok(t);
            }
        }
        // 3. 自动从 cookies 文件加载
        if let Some(token) = crate::platform::zhixueyun_token::load_zhixueyun_cookies() {
            return Ok(token);
        }
        // 4. CookiePool fallback
        if let Some(cookies) = self.cookie_pool.get() {
            if let Some(t) = cookies.get("authorization") {
                // Return clone since cookies HashMap may not live long enough
                return Ok(t.clone());
            }
        }
        // 没有可用的 token，返回错误
        Err(ResolverError::AuthenticationRequired)
    }

    /// 从 URL 中提取 course/category ID (支持两种格式)
    fn extract_course_id(url: &str) -> Option<String> {
        if let Some(fragment) = url.split('#').nth(1) {
            let parts: Vec<&str> = fragment.split('/').collect();
            if parts.len() >= 5 && parts[1] == "study" && parts[2] == "subject" && parts[3] == "detail" {
                return Some(parts[4].to_string());
            }
        }
        url.split("courseId=")
            .nth(1)
            .and_then(|s| s.split('&').next())
            .map(|s| s.to_string())
    }
}

/// 智学云基础请求头（静态缓存避免每次重新构建）
static BASE_HEADERS: once_cell::sync::Lazy<HeaderMap> = once_cell::sync::Lazy::new(|| {
    let mut headers = HeaderMap::new();
    headers.insert(reqwest::header::USER_AGENT, "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/148.0.0.0 Safari/537.36".parse().unwrap());
    headers.insert(reqwest::header::ACCEPT, "*/*".parse().unwrap());
    headers.insert(reqwest::header::ACCEPT_LANGUAGE, "zh-CN,zh;q=0.9".parse().unwrap());
    headers.insert(reqwest::header::DNT, "1".parse().unwrap());
    headers.insert(HeaderName::from_static("x-requested-with"), "XMLHttpRequest".parse().unwrap());
    headers.insert(HeaderName::from_static("referer"), "https://kc.zhixueyun.com/".parse().unwrap());
    headers
});

impl ZhiXueYunResolver {
    fn get_auth_headers(&self, token: Option<&str>) -> Result<HeaderMap, ResolverError> {
        // Get or build cached base headers
        let base_headers = {
            let cached = self.cached_headers.read().unwrap();
            if let Some(ref h) = *cached {
                h.clone()
            } else {
                drop(cached);
                let base = (*BASE_HEADERS).clone();
                let mut cached = self.cached_headers.write().unwrap();
                *cached = Some(base.clone());
                base
            }
        };

        let mut headers = base_headers;
        if let Some(t) = token {
            headers.insert(reqwest::header::AUTHORIZATION, format!("Bearer__{}", t.trim_start_matches("Bearer__").trim()).parse().unwrap());
        }
        Ok(headers)
    }

    /// 获取分类下的所有课程ID
    fn get_courses_by_category(&self, category_id: &str, token: &str) -> anyhow::Result<Vec<CourseInfo>> {
        let url = format!(
            "https://kc.zhixueyun.com/api/v1/course-study/subject/chapter-progress?courseId={}&knowledgePaymentEnable=false",
            category_id
        );

        let headers = self.get_auth_headers(Some(token))?;
        let response = self.client
            .get(&url)
            .headers(headers.clone())
            .send()
            .map_err(|e| anyhow::anyhow!("课程查询 API 请求失败: {}", e))?;

        if !response.status().is_success() {
            anyhow::bail!("课程查询 API 返回错误: {}", response.status());
        }

        let body = response.text()
            .map_err(|e| anyhow::anyhow!("读取课程响应失败: {}", e))?;

        let json: serde_json::Value = serde_json::from_str(&body)
            .map_err(|e| anyhow::anyhow!("课程 JSON 解析失败: {}", e))?;

        let mut courses = Vec::new();

        if let Some(chapters) = json.as_array() {
            for chapter in chapters {
                if let Some(sections) = chapter.get("courseChapterSections").and_then(|s| s.as_array()) {
                    for section in sections {
                        if section.get("sectionType").and_then(|v| v.as_i64()) == Some(10) {
                            if let Some(id) = section.get("resourceId").and_then(|v| v.as_str()) {
                                let name = section.get("name").and_then(|v| v.as_str())
                                    .unwrap_or("未知课程").to_string();
                                courses.push(CourseInfo { id: id.to_string(), name });
                            }
                        }
                    }
                }
            }
        }

        Ok(courses)
    }

    /// 获取课程的所有文档/视频
    fn get_all_documents(&self, course_id: &str, token: &str) -> anyhow::Result<Vec<DocumentInfo>> {
        let url = format!(
            "https://kc.zhixueyun.com/api/v1/course-study/subject/chapter-progress?courseId={}&knowledgePaymentEnable=false",
            course_id
        );

        let headers = self.get_auth_headers(Some(token))?;
        let response = self.client
            .get(&url)
            .headers(headers.clone())
            .send()
            .map_err(|e| anyhow::anyhow!("网络请求失败: {}", e))?;

        if !response.status().is_success() {
            anyhow::bail!("API 请求失败: {}", response.status());
        }

        let body = response.text()
            .map_err(|e| anyhow::anyhow!("读取响应失败: {}", e))?;

        let json: serde_json::Value = serde_json::from_str(&body)?;

        let mut documents = Vec::new();

        if let Some(chapters) = json.as_array() {
            for chapter in chapters {
                let chapter_name = chapter.get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("未知章节")
                    .to_string();

                if let Some(sections) = chapter.get("courseChapterSections").and_then(|s| s.as_array()) {
                    for section in sections {
                        let section_type = section.get("sectionType").and_then(|v| v.as_i64());
                        if section_type == Some(10) || section_type == Some(6) {
                            let doc_id = section.get("resourceId")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string())
                                .or_else(|| section.get("id").and_then(|v| v.as_str()).map(|s| s.to_string()));

                            let doc_name = section.get("name")
                                .and_then(|v| v.as_str())
                                .unwrap_or("未知文档")
                                .to_string();

                            
                            if let Some(id) = doc_id {
                                documents.push(DocumentInfo {
                                    id,
                                    name: doc_name,
                                    chapter_name: chapter_name.clone(),
                                });
                            }
                        }
                    }
                }
            }
        }

        Ok(documents)
    }

    /// 获取文件预览信息（带重试）
    fn get_file_preview_with_retry(&self, resource_id: &str, token: &str, max_retries: u32) -> anyhow::Result<FilePreview> {
        let mut last_error = None;

        for attempt in 0..max_retries {
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64;
            let url = format!(
                "https://kc.zhixueyun.com/api/v1/tools-center/file-cloud/preview-new?id={}&_={}",
                resource_id, timestamp
            );

            let headers = self.get_auth_headers(Some(token))?;

            match self.client.get(&url).headers(headers.clone()).send() {
                Ok(response) if response.status().is_success() => {
                    if let Ok(body) = response.text() {
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) {
                            let file_url = json.get("previewUrl")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string())
                                .or_else(|| json.get("url").and_then(|v| v.as_str()).map(|s| s.to_string()));

                            if let Some(url_str) = file_url {
                                let extention = json.get("extention").and_then(|v| v.as_str()).unwrap_or("mp4");
                                let filename = json.get("filename").and_then(|v| v.as_str())
                                    .map(|s| s.to_string())
                                    .unwrap_or_else(|| format!("document.{}", extention));
                                let file_size = json.get("size").and_then(|v| v.as_u64()).unwrap_or(0);

                                return Ok(FilePreview {
                                    secure_url: url_str,
                                    filename,
                                    file_size,
                                });
                            }
                        }
                    }
                }
                Ok(response) => {
                    last_error = Some(format!("HTTP {}", response.status()));
                }
                Err(e) => {
                    last_error = Some(e.to_string());
                }
            }

            if attempt < max_retries - 1 {
                debug!("Preview retry {} for {}", attempt + 1, resource_id);
            }
        }

        anyhow::bail!("获取预览失败 (重试 {} 次): {}", max_retries, last_error.unwrap_or_default())
    }

    /// 下载文件（带重试）
    fn download_file_with_retry(&self, url: &str, output_path: &std::path::Path, max_retries: u32) -> anyhow::Result<u64> {
        let mut last_error = None;

        for attempt in 0..max_retries {
            let headers = self.get_auth_headers(None)?;

            match self.client.get(url).headers(headers.clone()).send() {
                Ok(response) if response.status().is_success() => {
                    match response.bytes() {
                        Ok(bytes) => {
                            let size = bytes.len() as u64;
                            std::fs::write(output_path, bytes)?;
                            return Ok(size);
                        }
                        Err(e) => last_error = Some(format!("读取失败: {}", e)),
                    }
                }
                Ok(response) => {
                    last_error = Some(format!("HTTP {}", response.status()));
                }
                Err(e) => {
                    last_error = Some(e.to_string());
                }
            }

            if attempt < max_retries - 1 {
                debug!("Download retry {} for {}", attempt + 1, url);
            }
        }

        anyhow::bail!("下载失败 (重试 {} 次): {}", max_retries, last_error.unwrap_or_default())
    }

    /// 下载单个文件
    #[allow(dead_code)]
    pub fn download_file(&self, url: &str, token: &str, output_path: &std::path::Path) -> anyhow::Result<u64> {
        let course_id = Self::extract_course_id(url)
            .ok_or_else(|| anyhow::anyhow!("无法从 URL 中提取 courseId"))?;

        let docs = self.get_all_documents(&course_id, token)?;
        let doc = docs.into_iter().next()
            .ok_or_else(|| anyhow::anyhow!("未找到文档"))?;

        let preview = self.get_file_preview_with_retry(&doc.id, token, 3)?;
        info!("下载: {} ({} bytes)", preview.filename, preview.file_size);

        let size = self.download_file_with_retry(&preview.secure_url, output_path, 3)?;
        info!("已保存到: {:?}", output_path);
        Ok(size)
    }

    /// 批量下载（带进度回调）
    pub fn download_all_with_progress(
        &self,
        url: &str,
        token: &str,
        output_dir: &std::path::Path,
        progress: Option<ProgressCallback>,
        _parallel: usize,
    ) -> anyhow::Result<Vec<DownloadResult>> {
        let category_id = Self::extract_course_id(url)
            .ok_or_else(|| anyhow::anyhow!("无法从 URL 中提取 categoryId"))?;

        info!("获取分类 {} 下的所有课程", category_id);
        let courses = self.get_courses_by_category(&category_id, token)?;
        info!("找到 {} 个课程", courses.len());

        std::fs::create_dir_all(output_dir)?;

        // 收集所有需要下载的文件
        let mut all_files = Vec::new();
        for course in &courses {
            let docs = self.get_all_documents(&course.id, token)?;
            for doc in docs {
                all_files.push((course.clone(), doc));
            }
        }

        info!("共有 {} 个文件需要下载", all_files.len());

        let total_files = all_files.len();
        let completed = Arc::new(AtomicU64::new(0));
        let total_bytes = Arc::new(AtomicU64::new(0));
        let (tx, rx) = channel::<DownloadResult>();

        // 并行下载
        let tx = Arc::new(tx);
        thread::scope(|s| {
            for (course, doc) in all_files {
                let tx = tx.clone();
                let ctx = DownloadContext {
                    progress: progress.clone(),
                    completed: completed.clone(),
                    total_bytes: total_bytes.clone(),
                };
                s.spawn(move || {
                    let result = self.download_single_file(&course, &doc, output_dir, token, &ctx);
                    let _ = tx.send(result);
                });
            }
        });

        let mut results = Vec::new();
        while let Ok(result) = rx.try_recv() {
            results.push(result);
            if results.len() >= total_files {
                break;
            }
        }

        info!("下载完成: {} / {} 个文件", results.len(), total_files);
        Ok(results)
    }

    /// 下载单个文件（内部使用）
    fn download_single_file(
        &self,
        course: &CourseInfo,
        doc: &DocumentInfo,
        output_dir: &std::path::Path,
        token: &str,
        ctx: &DownloadContext,
    ) -> DownloadResult {
        let course_dir = output_dir.join(sanitize_filename(&course.name));

        match self.get_file_preview_with_retry(&doc.id, token, 3) {
            Ok(preview) => {
                // 生成文件名: {课程名称}/{章节名称}/{文档名称}.{扩展名}
                let ext = preview.filename.rsplit('.').next().unwrap_or("mp4");
                let file_name = format!("{}/{}/{}.{}",
                    sanitize_filename(&course.name),
                    sanitize_filename(&doc.chapter_name),
                    sanitize_filename(&doc.name),
                    ext
                );
                let output_path = course_dir.join(&file_name);

                // 创建子目录
                if let Some(parent) = output_path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }

                match self.download_file_with_retry(&preview.secure_url, &output_path, 3) {
                    Ok(size) => {
                        let _ = ctx.total_bytes.fetch_add(size, Ordering::Relaxed);
                        let n = ctx.completed.fetch_add(1, Ordering::Relaxed) + 1;
                        if let Some(cb) = &ctx.progress {
                            cb(n, 0, &file_name);
                        }
                        DownloadResult {
                            filename: file_name,
                            success: true,
                            error: None,
                            size,
                        }
                    }
                    Err(e) => DownloadResult {
                        filename: doc.name.clone(),
                        success: false,
                        error: Some(e.to_string()),
                        size: 0,
                    }
                }
            }
            Err(e) => DownloadResult {
                filename: doc.name.clone(),
                success: false,
                error: Some(e.to_string()),
                size: 0,
            }
        }
    }

    /// 下载单个文件（公开接口，返回 DownloadResult）
    #[allow(dead_code)]
    pub fn download_single(&self, url: &str, token: &str, output_path: &std::path::Path) -> anyhow::Result<DownloadResult> {
        let course_id = Self::extract_course_id(url)
            .ok_or_else(|| anyhow::anyhow!("无法从 URL 中提取 courseId"))?;

        let docs = self.get_all_documents(&course_id, token)?;
        let doc = docs.into_iter().next()
            .ok_or_else(|| anyhow::anyhow!("未找到文档"))?;

        let course = CourseInfo { id: course_id.clone(), name: "Unknown".to_string() };

        let result = self.download_single_file(
            &course,
            &doc,
            output_path.parent().unwrap_or(std::path::Path::new(".")),
            token,
            &DownloadContext {
                progress: None,
                completed: Arc::new(AtomicU64::new(0)),
                total_bytes: Arc::new(AtomicU64::new(0)),
            },
        );

        if result.success {
            Ok(result)
        } else {
            anyhow::bail!(result.error.unwrap_or_default())
        }
    }

    /// 获取视频信息
    pub fn fetch_video_info(&self, url: &str, token: &str) -> anyhow::Result<VideoInfo> {
        let course_id = Self::extract_course_id(url)
            .ok_or_else(|| anyhow::anyhow!("无法从 URL 中提取 courseId"))?;

        let docs = self.get_all_documents(&course_id, token)?;
        let doc = docs.into_iter().next()
            .ok_or_else(|| anyhow::anyhow!("未找到文档"))?;

        Ok(VideoInfo {
            url: url.to_string(),
            title: doc.name,
            description: None,
            uploader: None,
            duration: None,
            thumbnail: None,
            formats: vec![],
            created_at: None,
            download_strategy: crate::types::DownloadStrategy::Native,
        })
    }

    /// 下载课程的所有文档
    #[allow(dead_code)]
    pub fn download_all_documents(&self, url: &str, token: &str, output_dir: &std::path::Path) -> anyhow::Result<Vec<String>> {
        let results = self.download_all_with_progress(url, token, output_dir, None, 4)?;
        Ok(results.into_iter().filter(|r| r.success).map(|r| r.filename).collect())
    }

    /// 使用分类ID批量下载
    #[allow(dead_code)]
    pub fn download_all_by_category(&self, url: &str, token: &str, output_dir: &std::path::Path) -> anyhow::Result<Vec<String>> {
        self.download_all_documents(url, token, output_dir)
    }
}

/// 清理文件名中的非法字符
fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

#[async_trait]
impl PlatformResolver for ZhiXueYunResolver {
    fn platform(&self) -> Platform {
        Platform::ZhiXueYun
    }

    fn supported_url_patterns(&self) -> Vec<&'static str> {
        url_matcher::get_platform_patterns(Platform::ZhiXueYun).to_vec()
    }

    async fn get_video_info(&self, url: &str) -> Result<VideoInfo, ResolverError> {
        let token = self.get_token()?;

        self.fetch_video_info(url, &token).map_err(ResolverError::from)
    }

    fn requires_auth(&self) -> bool {
        true
    }

    fn required_tokens(&self) -> Vec<&'static str> {
        vec!["authorization"]
    }

    fn download_strategy(&self) -> crate::types::DownloadStrategy {
        crate::types::DownloadStrategy::Native
    }
}