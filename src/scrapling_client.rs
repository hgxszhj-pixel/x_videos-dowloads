//! Scrapling Client - 使用 scrapling 绕过 Cloudflare 获取页面
//!
//! 通过 Python subprocess 调用 scrapling StealthyFetcher 来获取被 Cloudflare 保护的页面

use crate::types::{VideoInfo, DownloadStrategy};
use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::io::{BufReader as StdBufReader, BufRead};
use std::process::{Command as StdCommand, Stdio};
use std::sync::OnceLock;

/// Scrapling 响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScraplingResponse {
    /// HTML 内容
    pub html: String,
    /// 实际 URL
    pub url: String,
    /// HTTP 状态码
    pub status: u16,
    /// 错误信息（如果有）
    #[serde(default)]
    pub error: Option<String>,
    /// 捕获的 m3u8 URL 列表
    #[serde(default)]
    pub m3u8_urls: Vec<String>,
    /// 捕获的 xe.course API 响应（含课程列表）
    #[serde(default)]
    pub course_list: Option<serde_json::Value>,
}

/// Scrapling 配置
#[derive(Debug, Clone)]
pub struct ScraplingConfig {
    /// Python 解释器路径
    pub python_path: Option<PathBuf>,
    /// 超时秒数
    pub timeout_secs: u64,
    /// 是否使用无头模式
    #[allow(dead_code)]
    pub headless: bool,
}

impl Default for ScraplingConfig {
    fn default() -> Self {
        Self {
            python_path: None,
            timeout_secs: 120,
            headless: true,
        }
    }
}

/// Scrapling 客户端
pub struct ScraplingClient {
    config: ScraplingConfig,
    python_path: PathBuf,
}

impl ScraplingClient {
    /// 创建新的客户端（使用默认配置）
    pub fn new() -> Result<Self> {
        Self::with_config(ScraplingConfig::default())
    }

    /// 使用自定义配置创建客户端
    pub fn with_config(config: ScraplingConfig) -> Result<Self> {
        let python_path = match config.python_path.clone() {
            Some(path) => path,
            None => find_python_interpreter()?,
        };

        if !python_path.exists() {
            return Err(anyhow!("Python 解释器不存在: {}", python_path.display()));
        }

        Ok(Self { config, python_path })
    }

    /// 获取页面（绕过 Cloudflare）
    pub async fn fetch(&self, url: &str) -> Result<ScraplingResponse> {
        let script = SCRAPLING_SCRIPT.to_string();
        let python_path = self.python_path.clone();
        let timeout_secs = self.config.timeout_secs;
        let url_owned = url.to_string();

        // Spawn blocking because Python subprocess spawn is a blocking operation
        let result = tokio::task::spawn_blocking(move || {
            let mut child = StdCommand::new(&python_path)
                .arg("-c")
                .arg(&script)
                .arg(&url_owned)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .context("Failed to spawn Python subprocess")?;

            let stdout = child.stdout.take().ok_or_else(|| anyhow!("Missing stdout"))?;
            let mut reader = StdBufReader::new(stdout);
            let mut line = String::new();

            // Use a simple loop with timeout checking
            let start = std::time::Instant::now();
            let deadline = std::time::Duration::from_secs(timeout_secs);

            loop {
                if start.elapsed() > deadline {
                    let _ = child.kill();
                    return Err(anyhow!("Scrapling fetch timed out"));
                }

                line.clear();
                match reader.read_line(&mut line) {
                    Ok(0) => {
                        // Wait for process to finish (blocking, appropriate in spawn_blocking)
                        match child.wait() {
                            Ok(_) => break,
                            Err(_) => break,
                        }
                    }
                    Ok(_) => {
                        if !line.is_empty() {
                            return Ok(line);
                        }
                    }
                    Err(_) => break,
                }
            }

            Err(anyhow!("No output from Python"))
        })
        .await
        .context("Spawn blocking task failed")??;

        let response: ScraplingResponse = serde_json::from_str(&result)
            .map_err(|e| anyhow!("Failed to parse JSON: {} - Output: {}", e, &result))?;

        if let Some(ref err) = response.error {
            return Err(anyhow!("Scrapling error: {}", err));
        }

        Ok(response)
    }

    /// 获取视频信息（通过 Scrapling 解析 X.com 页面）
    pub async fn get_video_info(&self, url: &str) -> Result<VideoInfo> {
        let response = self.fetch(url).await?;

        // 优先使用捕获的 m3u8 URL
        let video_url = if !response.m3u8_urls.is_empty() {
            tracing::info!("Scrapling 捕获到 {} 个 m3u8 URL", response.m3u8_urls.len());
            Some(response.m3u8_urls[0].clone())
        } else {
            // 回退到 HTML 提取
            super::scrapling_extractor::extract_video_url(&response.html)
        };

        let video_url = video_url.ok_or_else(|| anyhow!("未能在页面中找到视频 URL"))?;

        let title = super::scrapling_extractor::extract_title(&response.html)
            .unwrap_or_else(|| "Scraping Video".to_string());

        let mut info = VideoInfo::new(title, video_url.clone(), None);

        // 如果捕获到 m3u8 URL，使用原生 HLS 下载
        if !response.m3u8_urls.is_empty() {
            info.download_strategy = DownloadStrategy::Hls;
        }

        // 提取所有可用格式
        let formats = super::scrapling_extractor::extract_all_video_sources(&response.html);
        if !formats.is_empty() {
            info.formats = formats;
        }

        Ok(info)
    }
}

/// 嵌入的 Python 脚本（使用 patchright 支持 m3u8 URL 拦截）
const SCRAPLING_SCRIPT: &str = r#"
import sys
import json
from patchright.sync_api import sync_playwright

url = sys.argv[1]
m3u8_urls = []
course_list = None

try:
    with sync_playwright() as p:
        browser = p.chromium.launch(headless=True)
        context = browser.new_context()
        page = context.new_page()

        # 用 dict 包装以便在闭包中修改
        _state = {'course_list': None}

        def handle_response(response):
            url_lower = response.url.lower()
            if 'm3u8' in url_lower:
                m3u8_urls.append(response.url)
                print(f"[DEBUG] Captured m3u8: {response.url}", file=sys.stderr)
            # 捕获 xe.course.business.middle.page.data.get 响应
            if 'xe.course.business.middle.page.data.get' in response.url and not _state['course_list']:
                try:
                    body = response.text()
                    if body.startswith('{'):
                        data = json.loads(body)
                        if data.get('data', {}).get('list'):
                            _state['course_list'] = data
                            print(f"[DEBUG] Captured course list: {len(data['data']['list'])} courses", file=sys.stderr)
                except:
                    pass

        page.on("response", handle_response)

        # 导航到目标 URL
        page.goto(url, wait_until="networkidle", timeout=30000)

        # 等待额外时间让 JS 加载视频
        page.wait_for_timeout(3000)

        # 获取渲染后的 HTML
        html = page.content()

        result = {
            'html': html,
            'url': page.url,
            'status': 200,
            'error': None,
            'm3u8_urls': m3u8_urls,
            'course_list': _state['course_list']
        }

        browser.close()

except Exception as e:
    result = {'error': str(e), 'html': '', 'url': url, 'status': 0, 'm3u8_urls': [], 'course_list': None}

print(json.dumps(result))
"#;

/// 查找 Python 解释器
static PYTHON_INTERPRETER: OnceLock<Option<PathBuf>> = OnceLock::new();

fn find_python_interpreter() -> Result<PathBuf> {
    PYTHON_INTERPRETER.get_or_init(|| {
        let candidates = [
            "python3",
            "python",
            "/Users/hgxszhj/.pyenv/shims/python3",
            "/Users/hgxszhj/anaconda3/bin/python3",
        ];

        for candidate in &candidates {
            if let Ok(path) = which::which(candidate) {
                return Some(path);
            }
        }

        None
    }).clone().ok_or_else(|| anyhow!(
        "找不到 Python 解释器。请确保 python3 在 PATH 中或安装 scrapling:\n\
         pip install scrapling curl_cffi patchright browserforge"
    ))
}

/// 检查 URL 是否需要 Cloudflare 绕过
#[allow(dead_code)]
pub fn needs_cloudflare_bypass(url: &str) -> bool {
    url.contains("x.com") || url.contains("twitter.com")
}

impl Default for ScraplingClient {
    fn default() -> Self {
        Self::new().expect("Failed to create ScraplingClient")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_needs_cloudflare_bypass() {
        assert!(needs_cloudflare_bypass("https://x.com/user/status/123"));
        assert!(needs_cloudflare_bypass("https://twitter.com/user/status/123"));
        assert!(!needs_cloudflare_bypass("https://youtube.com/watch?v=abc"));
    }
}