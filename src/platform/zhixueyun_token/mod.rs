use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::SystemTime;

/// 缓存 cookie 数据
struct CookieCache {
    token: String,
    path: PathBuf,
    mtime: std::time::SystemTime,
}

/// 全局 cookie 缓存（跨调用复用）
static COOKIE_CACHE: OnceLock<Option<CookieCache>> = OnceLock::new();

/// 自动检测并加载智学云 cookies（带缓存）
pub fn load_zhixueyun_cookies() -> Option<String> {
    let paths = &[
        dirs::home_dir()?
            .join(".cookies")
            .join("zhixueyun_cookies.txt"),
        dirs::home_dir()?
            .join(".config")
            .join("x-video-downloader")
            .join("cookies")
            .join("zhixueyun_cookies.txt"),
        dirs::home_dir()?.join("Downloads").join("wddownload.txt"),
        dirs::home_dir()?
            .join("Downloads")
            .join("15fbd615-fe0f-4235-aed8-469b7a170163.txt"),
    ];

    // 尝试返回缓存（如果文件未修改则直接返回）
    if let Some(cache) = COOKIE_CACHE.get() {
        if let Some(c) = cache.as_ref() {
            if let Ok(metadata) = fs::metadata(&c.path) {
                if let Ok(mtime) = metadata.modified() {
                    if mtime <= c.mtime {
                        tracing::debug!("使用缓存的智学云 token (文件未修改)");
                        return Some(c.token.clone());
                    }
                }
            }
        }
    }

    // 文件已修改或无缓存，重新读取
    for path in paths {
        if path.exists() {
            if let Ok(content) = fs::read_to_string(path) {
                if let Some(token) = extract_authorization_token(&content) {
                    let mtime = fs::metadata(path).and_then(|m| m.modified()).ok();
                    tracing::info!("从 {:?} 加载到智学云 token", path);

                    // 缓存结果
                    let cache = CookieCache {
                        token: token.clone(),
                        path: path.clone(),
                        mtime: mtime.unwrap_or(SystemTime::UNIX_EPOCH),
                    };
                    COOKIE_CACHE.set(Some(cache)).ok();

                    return Some(token);
                }
            }
        }
    }

    tracing::debug!("未找到智学云 cookies 文件");
    None
}

/// 从 Netscape 格式 cookies 文件提取 authorization token
pub fn extract_authorization_token(content: &str) -> Option<String> {
    for line in content.lines() {
        // 跳过注释和空行
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }

        let fields: Vec<&str> = line.split('\t').collect();
        // Netscape format: domain, tailmatch, path, secure, expires, name, value
        if fields.len() >= 7 {
            let name = fields[5];
            let value = fields[6];

            if name == "authorization" && value.starts_with("Bearer__") {
                // 去掉 "Bearer__" 前缀
                return Some(value.trim_start_matches("Bearer__").to_string());
            }
        }
    }
    None
}
