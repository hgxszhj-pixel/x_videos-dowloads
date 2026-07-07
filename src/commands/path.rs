//! 下载命令的路径处理工具

use std::path::PathBuf;

use anyhow::Result;

/// 获取默认输出目录（系统视频目录）
pub fn get_default_output_dir() -> PathBuf {
    directories::UserDirs::new()
        .and_then(|dirs| dirs.video_dir().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// 验证并规范化输出目录为绝对路径
pub fn validate_output_dir(dir: &PathBuf) -> Result<PathBuf> {
    let absolute = if dir.is_absolute() {
        dir.clone()
    } else {
        std::fs::canonicalize(dir)
            .map_err(|e| anyhow::anyhow!("无法将路径转换为绝对路径 {:?}: {}", dir, e))?
    };

    // 验证目录是否存在或是可创建的
    if !absolute.exists() {
        if let Some(parent) = absolute.parent() {
            if !parent.exists() && parent.to_string_lossy() != "" {
                anyhow::bail!("父目录不存在: {:?}", parent);
            }
        }
    } else if !absolute.is_dir() {
        anyhow::bail!("输出路径不是目录: {:?}", absolute);
    }

    Ok(absolute)
}

/// 从 URL 路径提取有意义的文件名
pub fn extract_filename_from_url(url: &str) -> Option<String> {
    // 解析 URL
    let parsed = url::Url::parse(url).ok()?;
    let path = parsed.path();

    // 跳过根路径
    if path.is_empty() || path == "/" {
        return None;
    }

    // 获取路径最后一段，去除扩展名
    let filename = std::path::Path::new(path)
        .file_stem()?
        .to_str()?
        .trim();

    // 过滤掉无意义的名称
    if filename.is_empty()
        || filename == "index"
        || filename == "play"
        || filename == "video"
        || filename == "index.html"
    {
        return None;
    }

    // 清理特殊字符
    let cleaned: String = filename
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();

    if cleaned.len() < 3 {
        return None;
    }

    Some(cleaned)
}