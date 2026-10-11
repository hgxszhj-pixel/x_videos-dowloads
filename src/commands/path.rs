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
    let filename = std::path::Path::new(path).file_stem()?.to_str()?.trim();

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
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();

    if cleaned.len() < 3 {
        return None;
    }

    Some(cleaned)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// happy path: 末段非 index/play 提取 file_stem
    #[test]
    fn extract_filename_happy_path() {
        assert_eq!(
            extract_filename_from_url("https://cdn.example.com/abc123.mp4"),
            Some("abc123".to_string())
        );
    }

    /// URL 末段是占位词(index/play/video/index.html) → None
    #[test]
    fn extract_filename_filters_placeholders() {
        assert_eq!(
            extract_filename_from_url("https://example.com/index.html"),
            None
        );
        assert_eq!(extract_filename_from_url("https://example.com/play"), None);
        assert_eq!(extract_filename_from_url("https://example.com/video"), None);
    }

    /// 根路径 / 空路径 → None
    #[test]
    fn extract_filename_returns_none_for_root() {
        assert_eq!(extract_filename_from_url("https://example.com/"), None);
        assert_eq!(extract_filename_from_url("https://example.com"), None);
    }

    /// 特殊字符清理为下划线 + 长度 < 3 → None
    #[test]
    fn extract_filename_sanitizes_and_filters_short() {
        // 末段 "ab!" 清理后 "ab_" 长度 3,保留
        assert_eq!(
            extract_filename_from_url("https://example.com/x/ab!.html"),
            Some("ab_".to_string())
        );
        // "a!" 清理后 "a_" 长度 2,<3 → None
        assert_eq!(
            extract_filename_from_url("https://example.com/a!.html"),
            None
        );
    }

    /// 非法 URL → None(不 panic)
    #[test]
    fn extract_filename_invalid_url_returns_none() {
        assert_eq!(extract_filename_from_url("not a url"), None);
        assert_eq!(extract_filename_from_url(""), None);
    }

    /// get_default_output_dir 返回 PathBuf(不测具体路径,因 platform 差异)
    #[test]
    fn get_default_output_dir_returns_pathbuf() {
        let p = get_default_output_dir();
        // 不会 panic + 返回有效 PathBuf(任何 PathBuf 都满足 IsValid,故只验 not panic)
        let _ = p.to_string_lossy();
    }
}
