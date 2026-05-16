//! ZhiXueYun 平台解析器
//!
//! 支持 kc.zhixueyun.com 智学云平台的 PDF 文档下载

use crate::resolvers::{CookiePool, PlatformResolver, ResolverError};
use crate::types::VideoInfo;
use crate::history::Platform;
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderName};
use std::sync::Arc;
use tracing::{info, debug, warn};

/// ZhiXueYun 解析器
pub struct ZhiXueYunResolver {
    cookie_pool: Arc<CookiePool>,
    client: Client,
}

impl ZhiXueYunResolver {
    /// 创建新的 ZhiXueYunResolver
    pub fn new(cookie_pool: Arc<CookiePool>) -> Self {
        Self {
            cookie_pool,
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_default(),
        }
    }

    /// 从 URL 中提取 course ID
    fn extract_course_id(url: &str) -> Option<String> {
        url.split("courseId=")
            .nth(1)
            .and_then(|s| s.split('&').next())
            .map(|s| s.to_string())
    }

    /// 从 URL 中提取 resource ID (文档 ID)
    fn extract_resource_id(url: &str) -> Option<String> {
        url.split("resourceId=")
            .nth(1)
            .and_then(|s| s.split('&').next())
            .map(|s| s.to_string())
    }

    /// 获取认证头
    fn get_auth_headers(&self, token: Option<&str>) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(reqwest::header::USER_AGENT, "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/148.0.0.0 Safari/537.36".parse().unwrap());
        headers.insert(reqwest::header::ACCEPT, "*/*".parse().unwrap());
        headers.insert(reqwest::header::ACCEPT_LANGUAGE, "zh-CN,zh;q=0.9".parse().unwrap());
        headers.insert(reqwest::header::DNT, "1".parse().unwrap());
        headers.insert(HeaderName::from_static("x-requested-with"), "XMLHttpRequest".parse().unwrap());
        headers.insert(HeaderName::from_static("referer"), "https://kc.zhixueyun.com/".parse().unwrap());
        if let Some(t) = token {
            headers.insert(reqwest::header::AUTHORIZATION, format!("Bearer__{}", t.trim_start_matches("Bearer__")).parse().unwrap());
        }
        headers
    }

    /// 从课程信息获取文档资源
    fn get_document_info(&self, course_id: &str, token: &str) -> anyhow::Result<DocumentInfo> {
        // 首先获取课程章节信息，其中包含文档列表
        let api_url = format!(
            "https://kc.zhixueyun.com/api/v1/course-study/subject/chapter-progress?courseId={}&knowledgePaymentEnable=false",
            course_id
        );

        let headers = self.get_auth_headers(Some(token));

        let response = self.client
            .get(&api_url)
            .headers(headers.clone())
            .send()
            .map_err(|e| anyhow::anyhow!("网络请求失败: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            warn!("API 返回错误状态 {}: {}", status, body);
            anyhow::bail!("API 请求失败: {}", status);
        }

        let body = response.text()
            .map_err(|e| anyhow::anyhow!("读取响应失败: {}", e))?;

        debug!("Chapter progress response: {}", body);

        // 解析 JSON 响应获取文档信息
        // 响应是数组: [{ courseChapterSections: [{ resourceId, name, sectionType }] }]
        let json: serde_json::Value = serde_json::from_str(&body)
            .map_err(|e| anyhow::anyhow!("JSON 解析失败: {} - body: {}", e, &body[..body.len().min(500)]))?;

        // 找到第一个 PDF 类型的章节 (sectionType == 10)
        // 注意: id 字段才是 file-cloud/preview API 需要的 preview ID
        let mut document_id = None;
        let mut document_name = None;

        if let Some(chapters) = json.as_array() {
            for chapter in chapters {
                if let Some(sections) = chapter.get("courseChapterSections").and_then(|s| s.as_array()) {
                    for section in sections {
                        // sectionType 10 = PDF 文档
                        if section.get("sectionType").and_then(|v| v.as_i64()) == Some(10) {
                            // 使用 id 字段作为 preview ID (不是 resourceId 或 attachmentId)
                            document_id = section.get("id").and_then(|v| v.as_str()).map(|s| s.to_string());
                            document_name = section.get("name").and_then(|v| v.as_str()).map(|s| s.to_string());
                            break;
                        }
                    }
                }
                if document_id.is_some() {
                    break;
                }
            }
        }

        let document_id = document_id.ok_or_else(|| anyhow::anyhow!("未找到 PDF 文档"))?;
        let document_name = document_name.unwrap_or_else(|| format!("课程文档_{}", &document_id[..8]));

        Ok(DocumentInfo {
            id: document_id,
            name: document_name,
            file_size: None, // API 响应中无文件大小，需额外获取
        })
    }

    /// 从 preview API 获取文件信息
    fn get_file_preview(&self, resource_id: &str, token: &str) -> anyhow::Result<FilePreview> {
        let url = format!(
            "https://kc.zhixueyun.com/api/v1/tools-center-v2/file-cloud/preview?id={}",
            resource_id
        );

        let headers = self.get_auth_headers(Some(token));

        let response = self.client
            .get(&url)
            .headers(headers.clone())
            .send()
            .map_err(|e| anyhow::anyhow!("预览 API 请求失败: {}", e))?;

        if !response.status().is_success() {
            anyhow::bail!("预览 API 返回错误: {}", response.status());
        }

        let body = response.text()
            .map_err(|e| anyhow::anyhow!("读取预览响应失败: {}", e))?;

        let json: serde_json::Value = serde_json::from_str(&body)
            .map_err(|e| anyhow::anyhow!("预览 JSON 解析失败: {}", e))?;

        let url = json.get("url")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("预览响应中未找到 URL"))?;

        let filename = json.get("filename")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| "document.pdf".to_string());

        let file_size = json.get("size")
            .and_then(|v| v.as_u64());

        Ok(FilePreview {
            secure_url: url,
            filename,
            file_size,
        })
    }

    /// 获取 PDF 下载链接
    fn get_pdf_url(&self, _document_id: &str, _token: &str) -> String {
        // PDF URL 通过 preview API 获取，不是直接构造
        String::new()
    }

    /// 获取 PDF 文档信息
    pub fn fetch_pdf_info(&self, url: &str, token: &str) -> anyhow::Result<VideoInfo> {
        let course_id = Self::extract_course_id(url)
            .ok_or_else(|| anyhow::anyhow!("无法从 URL 中提取 courseId"))?;

        debug!("Fetching PDF info for course: {}", course_id);

        let doc_info = self.get_document_info(&course_id, token)?;

        info!("Found document: {} (ID: {})", doc_info.name, doc_info.id);

        Ok(VideoInfo {
            url: url.to_string(),
            title: doc_info.name,
            description: None,
            uploader: None,
            duration: None,
            thumbnail: None,
            formats: vec![],
            created_at: None,
        })
    }

    /// 直接下载 PDF 文件
    pub fn download_pdf(&self, url: &str, token: &str, output_path: &std::path::Path) -> anyhow::Result<()> {
        let course_id = Self::extract_course_id(url)
            .ok_or_else(|| anyhow::anyhow!("无法从 URL 中提取 courseId"))?;

        info!("Downloading PDF for course: {}", course_id);

        // 1. 先获取章节信息
        let doc_info = self.get_document_info(&course_id, token)?;

        // 2. 调用 preview API 获取下载 URL
        let preview = self.get_file_preview(&doc_info.id, token)?;
        info!("File: {} ({} bytes)", preview.filename, preview.file_size.unwrap_or(0));

        // 3. 下载文件
        let full_url = if preview.secure_url.starts_with("/") {
            format!("https://kc.zhixueyun.com{}", preview.secure_url)
        } else {
            preview.secure_url.clone()
        };

        debug!("Downloading from: {}", full_url);

        let headers = self.get_auth_headers(Some(token));

        let response = self.client
            .get(&full_url)
            .headers(headers.clone())
            .send()
            .map_err(|e| anyhow::anyhow!("下载请求失败: {}", e))?;

        if !response.status().is_success() {
            anyhow::bail!("下载失败: {} - {}", response.status(), response.text().unwrap_or_default());
        }

        let bytes = response.bytes()
            .map_err(|e| anyhow::anyhow!("读取下载内容失败: {}", e))?;

        std::fs::write(output_path, bytes)?;
        info!("PDF 已下载到: {:?}", output_path);
        Ok(())
    }
}

/// 文档信息
struct DocumentInfo {
    id: String,
    name: String,
    file_size: Option<u64>,
}

/// 文件预览信息
struct FilePreview {
    secure_url: String,
    filename: String,
    file_size: Option<u64>,
}

impl PlatformResolver for ZhiXueYunResolver {
    fn platform(&self) -> Platform {
        Platform::ZhiXueYun
    }

    fn supported_url_patterns(&self) -> Vec<&'static str> {
        vec!["kc.zhixueyun.com"]
    }

    fn get_video_info(&self, url: &str) -> anyhow::Result<VideoInfo, ResolverError> {
        let token = self.cookie_pool.get()
            .and_then(|cookies| cookies.get("authorization").cloned())
            .or_else(|| Some("d452ee129814c2549a2d7b4448a48ea9".to_string()))
            .ok_or_else(|| ResolverError::AuthenticationRequired)?;

        self.fetch_pdf_info(url, &token).map_err(ResolverError::from)
    }

    fn requires_auth(&self) -> bool {
        true
    }

    fn required_tokens(&self) -> Vec<&'static str> {
        vec!["authorization"]
    }
}