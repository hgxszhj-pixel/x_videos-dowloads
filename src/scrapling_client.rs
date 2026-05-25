//! Scrapling Client - 使用 scrapling 绕过 Cloudflare 获取页面
//!
//! 通过 Python subprocess 调用 scrapling StealthyFetcher 来获取被 Cloudflare 保护的页面

use crate::types::VideoInfo;
use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use shell_words::quote;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

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
            let mut child = Command::new(&python_path)
                .arg("-c")
                .arg(&script)
                .arg(quote(url_owned).into_owned())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true)
                .spawn()
                .context("Failed to spawn Python subprocess")?;

            let stdout = child.stdout.take().ok_or_else(|| anyhow!("Missing stdout"))?;
            let mut reader = BufReader::new(stdout).lines();

            // Use a simple loop with timeout checking
            let start = std::time::Instant::now();
            let deadline = std::time::Duration::from_secs(timeout_secs);

            loop {
                if start.elapsed() > deadline {
                    child.kill().ok();
                    return Err(anyhow!("Scrapling fetch timed out"));
                }

                match reader.next_line() {
                    Ok(Some(line)) => {
                        if !line.is_empty() {
                            return Ok(line);
                        }
                    }
                    Ok(None) => {
                        // Try waiting for process to finish
                        match child.try_wait() {
                            Ok(Some(_)) => break,
                            Ok(None) => continue,
                            Err(_) => break,
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

        // 从 HTML 中提取视频 URL 和标题
        let video_url = super::scrapling_extractor::extract_video_url(&response.html)
            .ok_or_else(|| anyhow!("未能在页面中找到视频 URL"))?;

        let title = super::scrapling_extractor::extract_title(&response.html)
            .unwrap_or_else(|| "X.com Video".to_string());

        let mut info = VideoInfo::new(title, video_url);

        // 提取所有可用格式
        let formats = super::scrapling_extractor::extract_all_video_sources(&response.html);
        if !formats.is_empty() {
            info.formats = formats;
        }

        Ok(info)
    }
}

/// 嵌入的 Python 脚本
const SCRAPLING_SCRIPT: &str = r#"
import sys
import json
from scrapling.fetchers import StealthyFetcher

url = sys.argv[1]

try:
    resp = StealthyFetcher.fetch(
        url,
        headless=True,
        solve_cloudflare=True,
        timeout=60000,
        network_idle=True,
    )
    result = {
        'html': resp.body.decode('utf-8', errors='replace') if hasattr(resp, 'body') else '',
        'url': resp.url if hasattr(resp, 'url') else url,
        'status': resp.status if hasattr(resp, 'status') else 200,
        'error': None
    }
except Exception as e:
    result = {'error': str(e), 'html': '', 'url': url, 'status': 0}

print(json.dumps(result))
"#;

/// 查找 Python 解释器
fn find_python_interpreter() -> Result<PathBuf> {
    let candidates = [
        "python3",
        "python",
        "/Users/hgxszhj/.pyenv/shims/python3",
        "/Users/hgxszhj/anaconda3/bin/python3",
    ];

    for candidate in &candidates {
        if let Ok(path) = which::which(candidate) {
            return Ok(path);
        }
    }

    Err(anyhow!(
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