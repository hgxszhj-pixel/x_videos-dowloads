//! 应用程序配置管理
//!
//! 支持从配置文件 (~/.config/x_video_downloader.toml) 加载配置
//! 配置优先级: CLI参数 > 环境变量 > 配置文件 > 默认值

use anyhow::Result;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 应用程序配置
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AppConfig {
    /// 默认输出目录
    #[serde(default = "default_output_dir")]
    pub output_dir: PathBuf,

    /// 默认代理
    #[serde(default)]
    pub proxy: Option<String>,

    /// 默认 User-Agent
    #[serde(default = "default_user_agent")]
    pub user_agent: String,

    /// 默认 Cookie 文件路径
    #[serde(default)]
    pub cookie_file: Option<PathBuf>,

    /// 默认下载格式
    #[serde(default)]
    pub format: Option<String>,

    /// 是否默认下载字幕
    #[serde(default)]
    pub subtitles: bool,

    /// 是否默认仅下载音频
    #[serde(default)]
    pub audio_only: bool,

    /// 并发下载数
    #[serde(default = "default_concurrent_downloads")]
    pub concurrent_downloads: usize,

    /// 重试次数
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,

    /// 协作服务器地址
    #[serde(default = "default_server_addr")]
    pub server_addr: String,

    /// 默认设备名称
    #[serde(default = "default_device_name")]
    pub device_name: String,

    /// 日志级别
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

fn default_output_dir() -> PathBuf {
    directories::UserDirs::new()
        .and_then(|dirs| dirs.video_dir().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn default_user_agent() -> String {
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36".to_string()
}

fn default_concurrent_downloads() -> usize {
    3
}

fn default_max_retries() -> u32 {
    5
}

fn default_server_addr() -> String {
    "ws://localhost:9000".to_string()
}

fn default_device_name() -> String {
    "MyDevice".to_string()
}

fn default_log_level() -> String {
    "info".to_string()
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            output_dir: default_output_dir(),
            proxy: None,
            user_agent: default_user_agent(),
            cookie_file: None,
            format: None,
            subtitles: false,
            audio_only: false,
            concurrent_downloads: default_concurrent_downloads(),
            max_retries: default_max_retries(),
            server_addr: default_server_addr(),
            device_name: default_device_name(),
            log_level: default_log_level(),
        }
    }
}

impl AppConfig {
    /// 获取配置文件路径
    pub fn config_path() -> Option<PathBuf> {
        ProjectDirs::from("com", "x-video-downloader", "x-video-downloader")
            .map(|proj| proj.config_dir().join("config.toml"))
    }

    /// 从配置文件加载
    pub fn load() -> Result<Self> {
        let config_path = Self::config_path();

        if let Some(path) = config_path {
            if path.exists() {
                let content = std::fs::read_to_string(&path)?;
                let config: AppConfig = toml::from_str(&content)?;
                tracing::debug!("已加载配置文件: {:?}", path);
                return Ok(config);
            }
        }

        tracing::debug!("未找到配置文件，使用默认配置");
        Ok(Self::default())
    }

    /// 保存配置到文件
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()
            .ok_or_else(|| anyhow::anyhow!("无法确定配置文件路径"))?;

        // 确保目录存在
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = toml::to_string_pretty(self)?;
        std::fs::write(&path, content)?;

        tracing::info!("配置已保存到: {:?}", path);
        Ok(())
    }

    /// 创建默认配置文件（如果不存在）
    pub fn init_config_file() -> Result<Option<PathBuf>> {
        let path = Self::config_path().ok_or_else(|| anyhow::anyhow!("无法确定配置文件路径"))?;

        if path.exists() {
            return Ok(None);
        }

        // 确保目录存在
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let config = Self::default();
        let content = toml::to_string_pretty(&config)?;
        std::fs::write(&path, content)?;

        tracing::info!("已创建默认配置文件: {:?}", path);
        Ok(Some(path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AppConfig::default();
        assert_eq!(config.concurrent_downloads, 3);
        assert_eq!(config.max_retries, 5);
        assert_eq!(config.server_addr, "ws://localhost:9000");
    }

    #[test]
    fn test_config_path() {
        let path = AppConfig::config_path();
        println!("Config path: {:?}", path);
        // 路径应该包含 x-video-downloader
        if let Some(p) = path {
            assert!(p.to_string_lossy().contains("x-video-downloader"));
        }
    }
}
