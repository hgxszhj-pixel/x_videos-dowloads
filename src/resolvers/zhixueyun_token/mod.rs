use std::fs;

/// 自动检测并加载智学云 cookies
pub fn load_zhixueyun_cookies() -> Option<String> {
    // 检查默认 cookies 路径
    let possible_paths = vec![
        dirs::home_dir()?.join(".cookies").join("zhixueyun_cookies.txt"),
        dirs::home_dir()?.join(".config").join("x-video-downloader").join("cookies").join("zhixueyun_cookies.txt"),
        dirs::home_dir()?.join("Downloads").join("wddownload.txt"),
        dirs::home_dir()?.join("Downloads").join("15fbd615-fe0f-4235-aed8-469b7a170163.txt"),
    ];

    for path in possible_paths {
        if path.exists() {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Some(token) = extract_authorization_token(&content) {
                    tracing::info!("从 {:?} 加载到智学云 token", path);
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

