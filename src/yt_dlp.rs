//! YtDlp 集成模块
//!
//! 调用 yt-dlp 获取视频信息、解析格式、获取下载链接

use crate::types::{DownloadError, VideoFormat, VideoInfo};
use anyhow::Result;
use std::path::PathBuf;
use std::process::Command;
use tracing::{debug, info, warn};

/// YtDlp 命令行工具封装
#[derive(Clone)]
pub struct YtDlp {
    /// 可执行文件路径
    pub executable: PathBuf,
    /// 反爬虫配置
    anti_bot: AntiBotConfig,
}

/// 反爬虫配置
#[derive(Debug, Clone)]
pub struct AntiBotConfig {
    /// 是否启用
    pub enabled: bool,
    /// 自定义User-Agent
    pub user_agent: Option<String>,
    /// Cookie文件路径
    pub cookie_file: Option<PathBuf>,
    /// 代理
    pub proxy: Option<String>,
    /// 跳过 SSL 证书验证（默认关闭，仅测试用）
    pub skip_ssl_verify: bool,
}

impl Default for AntiBotConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            user_agent: None,
            cookie_file: None,
            proxy: None,
            skip_ssl_verify: false,
        }
    }
}

impl YtDlp {
    /// 创建新的 YtDlp 实例
    pub fn new() -> Self {
        // 优先使用用户本地安装的 yt-dlp（Python 3.12+ 版本）
        let executable = if cfg!(target_os = "macos") {
            let home = std::env::var("HOME").unwrap_or_default();
            let venv_path = format!("{}/.yt-dlp-venv/bin/yt-dlp", home);
            if std::path::Path::new(&venv_path).exists() {
                PathBuf::from(venv_path)
            } else {
                PathBuf::from("yt-dlp")
            }
        } else {
            PathBuf::from("yt-dlp")
        };

        Self {
            executable,
            anti_bot: AntiBotConfig::default(),
        }
    }

    /// 设置 User-Agent
    pub fn with_user_agent(mut self, user_agent: String) -> Self {
        self.anti_bot.user_agent = Some(user_agent);
        self
    }

    /// 设置 Cookie 文件
    pub fn with_cookie_file(mut self, cookie_file: PathBuf) -> Self {
        self.anti_bot.cookie_file = Some(cookie_file);
        self
    }

    /// 设置代理
    pub fn with_proxy(mut self, proxy: String) -> Self {
        self.anti_bot.proxy = Some(proxy);
        self
    }

    /// 获取随机 User-Agent
    fn get_user_agent(&self) -> String {
        if let Some(ref ua) = self.anti_bot.user_agent {
            return ua.clone();
        }

        let user_agents = [
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:133.0) Gecko/20100101 Firefox/133.0",
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.15; rv:133.0) Gecko/20100101 Firefox/133.0",
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36",
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15",
        ];

        // 使用线程安全随机数生成器
        use rand::thread_rng;
        use rand::seq::SliceRandom;
        let mut rng = thread_rng();
        user_agents.choose(&mut rng).unwrap_or(&user_agents[0]).to_string()
    }

    /// 获取视频信息
    pub fn get_video_info(&self, url: &str) -> Result<VideoInfo> {
        info!("获取视频信息: {}", url);

        // 构建命令参数 - 优化网络稳定性
        let mut args = vec![
            "--dump-json".to_string(),
            "--no-playlist".to_string(),
            // 重试配置 - 指数退避
            "--retries".to_string(),
            "10".to_string(),
            "--fragment-retries".to_string(),
            "10".to_string(),
            // 重试间隔（线性）
            "--retry-sleep".to_string(),
            "5".to_string(),
            // 超时配置 - 大文件需要更长超时
            "--socket-timeout".to_string(),
            "120".to_string(),
            // 减少连接问题
            "--http-chunk-size".to_string(),
            "10M".to_string(),
            // 禁用SSL验证（可选，用于测试）
            // "--no-check-certificate".to_string(),
            url.to_string(),
        ];

        // 添加反爬虫参数
        if self.anti_bot.enabled {
            args.push("--user-agent".to_string());
            args.push(self.get_user_agent());

            if let Some(ref cookie_file) = self.anti_bot.cookie_file {
                if cookie_file.exists() {
                    args.push("--cookies".to_string());
                    args.push(cookie_file.to_string_lossy().to_string());
                }
            }

            if let Some(ref proxy) = self.anti_bot.proxy {
                args.push("--proxy".to_string());
                args.push(proxy.clone());
            }

            // 添加额外网络优化（仅当 skip_ssl_verify 开启时）
            if self.anti_bot.skip_ssl_verify {
                args.push("--no-check-certificate".to_string());
            }
        }

        // Log only non-sensitive parts (args may contain cookie file paths and proxy URLs)
        let safe_args: Vec<String> = args.iter()
            .filter(|a| !a.contains("cookie") && !a.contains("proxy"))
            .map(|s| s.clone())
            .collect();
        debug!("执行命令: {:?} {:?}", self.executable, safe_args);

        let output = Command::new(&self.executable)
            .args(&args)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);

            // 内容类错误（用户操作可解决）
            if stderr.contains("No video could be found")
                || stderr.contains("Video unavailable")
                || stderr.contains("This video is not available")
            {
                return Err(DownloadError::content_unavailable("视频不存在或已被删除").into());
            }

            if stderr.contains("login") || stderr.contains("authenticate") || stderr.contains("Sign in") {
                return Err(DownloadError::authentication().into());
            }

            if stderr.contains("HTTP Error 429") || stderr.contains("rate limit") {
                return Err(DownloadError::rate_limited().into());
            }

            if stderr.contains("ffmpeg") || stderr.contains("concurrent") {
                // 技术类错误
                warn!("yt-dlp 技术错误: {}", stderr);
                return Err(DownloadError::ytdlp(&stderr).into());
            }

            // 其他错误
            warn!("yt-dlp 错误: {}", stderr);
            return Err(DownloadError::ytdlp(&stderr).into());
        }

        let json_str = String::from_utf8_lossy(&output.stdout);

        // 解析 JSON
        let json: serde_json::Value = serde_json::from_str(&json_str)?;

        let title = json["title"]
            .as_str()
            .unwrap_or("未知标题")
            .to_string();

        let description = json["description"].as_str().map(|s| s.to_string());

        let uploader = json["uploader"].as_str().map(|s| s.to_string());

        let duration = json["duration"].as_u64();

        let thumbnail = json["thumbnail"]
            .as_str()
            .map(|s| s.to_string());

        let mut video_info = VideoInfo::new(title, url.to_string());
        video_info.description = description;
        video_info.uploader = uploader;
        video_info.duration = duration;
        video_info.thumbnail = thumbnail;

        // 解析格式列表
        if let Some(formats) = json["formats"].as_array() {
            for fmt in formats {
                if let Some(format_id) = fmt["format_id"].as_str() {
                    let ext = fmt["ext"].as_str().unwrap_or("unknown").to_string();

                    let resolution = fmt["resolution"].as_str().map(|s| s.to_string());

                    let filesize = fmt["filesize"].as_u64().or(fmt["filesize_approx"].as_u64());

                    let bitrate = fmt["tbr"].as_u64();

                    let audio_only = ext == "m4a"
                        || ext == "mp3"
                        || ext == "webm"
                        || fmt["vcodec"].as_str() == Some("none");

                    let format_note = fmt["format_note"].as_str().map(|s| s.to_string());

                    video_info.formats.push(VideoFormat {
                        format_id: format_id.to_string(),
                        ext,
                        resolution,
                        filesize,
                        bitrate,
                        codec: None,
                        audio_only,
                        format_note,
                    });
                }
            }
        }

        info!("获取视频信息成功: {}", video_info.title);
        Ok(video_info)
    }

    /// 构建下载命令参数（消除代码重复）
    pub fn build_download_args(
        &self,
        url: &str,
        output_path: &str,
        format: Option<&str>,
        audio_only: bool,
        subtitles: bool,
    ) -> Vec<String> {
        let mut args = vec![
            "--no-warnings".to_string(),
            "--newline".to_string(),
            // Force progress output even in non-TTY mode
            "--progress-template".to_string(),
            "download:%(progress.downloaded_bytes)s/%(progress.total_bytes)s %(progress.percent)s%%".to_string(),
            "-o".to_string(),
            output_path.to_string(),
            "--concurrent-fragments".to_string(),
            "4".to_string(),
            "--fragment-retries".to_string(),
            "10".to_string(),
            "--retries".to_string(),
            "10".to_string(),
            "--socket-timeout".to_string(),
            "60".to_string(),
            "--no-abort-on-error".to_string(),
        ];

        // 仅在 skip_ssl_verify 开启时跳过 SSL 验证
        if self.anti_bot.skip_ssl_verify {
            args.push("--no-check-certificate".to_string());
        }

        // 格式
        if let Some(f) = format {
            args.push("-f".to_string());
            args.push(f.to_string());
        } else {
            // 默认格式：优先选择 mp4 格式的视频+音频，分离格式用 / 分隔
            // 对于 twitter：使用 http-* 格式而非 hls-* 格式更稳定
            args.push("-f".to_string());
            args.push("bestvideo[ext=mp4]+bestaudio[ext=m4a]/best[ext=mp4]/best".to_string());
        }

        // 音频
        if audio_only {
            args.push("-x".to_string());
            args.push("--audio-format".to_string());
            args.push("mp3".to_string());
        }

        // 字幕
        if subtitles {
            args.push("--write-subs".to_string());
            args.push("--sub-lang".to_string());
            args.push("en,zh-Hans,zh-Hant".to_string());
        }

        // User-Agent
        args.push("--user-agent".to_string());
        args.push(self.get_user_agent());

        // Cookie
        if let Some(ref cookie_file) = self.anti_bot.cookie_file {
            if cookie_file.exists() {
                args.push("--cookies".to_string());
                args.push(cookie_file.to_string_lossy().to_string());
            }
        }

        // 代理
        if let Some(ref proxy) = self.anti_bot.proxy {
            args.push("--proxy".to_string());
            args.push(proxy.clone());
        }

        // URL
        args.push(url.to_string());

        args
    }

    /// 检查 yt-dlp 是否可用
    pub fn is_available(&self) -> bool {
        Command::new(&self.executable)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// 获取 yt-dlp 版本
    pub fn version(&self) -> Option<String> {
        Command::new(&self.executable)
            .arg("--version")
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
    }
}

impl Default for YtDlp {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ytdlp_creation() {
        let yt = YtDlp::new();
        assert!(yt.executable.to_string_lossy().contains("yt-dlp"));
        assert!(yt.anti_bot.enabled);
    }

    #[test]
    fn test_ytdlp_builder() {
        let yt = YtDlp::new()
            .with_user_agent("test-agent".to_string());

        assert_eq!(yt.anti_bot.user_agent, Some("test-agent".to_string()));
    }

    #[test]
    fn test_antibot_config_default() {
        let config = AntiBotConfig::default();
        assert!(config.enabled);
        assert!(config.user_agent.is_none());
        assert!(config.cookie_file.is_none());
        assert!(config.proxy.is_none());
        assert!(!config.skip_ssl_verify);
    }

    #[test]
    fn test_ytdlp_builder_chain() {
        let yt = YtDlp::new()
            .with_user_agent("test-agent".to_string())
            .with_proxy("http://proxy:8080".to_string());

        assert_eq!(yt.anti_bot.user_agent, Some("test-agent".to_string()));
        assert_eq!(yt.anti_bot.proxy, Some("http://proxy:8080".to_string()));
    }

    #[test]
    fn test_build_download_args_basic() {
        let yt = YtDlp::new();
        let args = yt.build_download_args(
            "https://example.com/video",
            "/tmp/output.mp4",
            None,
            false,
            false,
        );

        assert!(args.contains(&"--no-warnings".to_string()));
        assert!(args.contains(&"--newline".to_string()));
        assert!(args.contains(&"-o".to_string()));
        assert!(args.contains(&"/tmp/output.mp4".to_string()));
        assert!(args.contains(&"https://example.com/video".to_string()));
    }

    #[test]
    fn test_build_download_args_audio() {
        let yt = YtDlp::new();
        let args = yt.build_download_args(
            "https://example.com/video",
            "/tmp/output.mp4",
            None,
            true, // audio_only
            false,
        );

        assert!(args.contains(&"-x".to_string()));
        assert!(args.contains(&"--audio-format".to_string()));
        assert!(args.contains(&"mp3".to_string()));
    }

    #[test]
    fn test_build_download_args_with_format() {
        let yt = YtDlp::new();
        let args = yt.build_download_args(
            "https://example.com/video",
            "/tmp/output.mp4",
            Some("best[ext=mp4]"),
            false,
            false,
        );

        assert!(args.contains(&"-f".to_string()));
        assert!(args.contains(&"best[ext=mp4]".to_string()));
    }

    #[test]
    fn test_build_download_args_with_subtitles() {
        let yt = YtDlp::new();
        let args = yt.build_download_args(
            "https://example.com/video",
            "/tmp/output.mp4",
            None,
            false,
            true, // subtitles
        );

        assert!(args.contains(&"--write-subs".to_string()));
        assert!(args.contains(&"--sub-lang".to_string()));
        assert!(args.contains(&"en,zh-Hans,zh-Hant".to_string()));
    }

    #[test]
    fn test_get_user_agent() {
        let yt = YtDlp::new();
        let ua = yt.get_user_agent();
        assert!(ua.starts_with("Mozilla/5.0"));
        assert!(ua.len() > 50);
    }
}
