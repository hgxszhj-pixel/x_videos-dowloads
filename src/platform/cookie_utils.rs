use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;
use tempfile::TempDir;

pub struct CookieFileManager {
    temp_dir: OnceLock<TempDir>,
}

impl CookieFileManager {
    pub fn new() -> Self {
        Self {
            temp_dir: OnceLock::new(),
        }
    }

    pub fn get_cookie_file(&self, cookies: &HashMap<String, String>, platform: &str) -> Option<PathBuf> {
        if cookies.is_empty() {
            return None;
        }

        let temp_dir = self.temp_dir.get_or_init(|| {
            tempfile::TempDir::new().expect("Failed to create temp dir")
        });

        let cookie_file = temp_dir.path().join(format!("{}_cookies.txt", platform));

        // Build Netscape format cookie file
        let mut content = String::from("# Netscape HTTP Cookie File\n");
        for (name, value) in cookies {
            content.push_str(&format!(
                ".{}\tTRUE\t/\tTRUE\t0\t{}\t{}\n",
                platform, name, value
            ));
        }

        std::fs::write(&cookie_file, content).ok()?;

        // 设置严格的文件权限 (0o600 = 仅所有者可读写)
        // 防止其他用户读取敏感 cookie 信息
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            let _ = std::fs::set_permissions(&cookie_file, perms);
        }

        Some(cookie_file)
    }
}

impl Default for CookieFileManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cookie_file_manager_empty() {
        let manager = CookieFileManager::new();
        let cookies = HashMap::new();
        assert!(manager.get_cookie_file(&cookies, "test").is_none());
    }

    #[test]
    fn test_cookie_file_manager_with_cookies() {
        let manager = CookieFileManager::new();
        let mut cookies = HashMap::new();
        cookies.insert("msToken".to_string(), "test_token".to_string());

        let path = manager.get_cookie_file(&cookies, "douyin");
        assert!(path.is_some());
        let path = path.unwrap();
        assert!(path.exists());

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("msToken"));
        assert!(content.contains("test_token"));
    }

    #[test]
    fn test_cookie_file_manager_reuses_temp_dir() {
        let manager = CookieFileManager::new();
        let mut cookies1 = HashMap::new();
        cookies1.insert("token1".to_string(), "value1".to_string());

        let path1 = manager.get_cookie_file(&cookies1, "platform1").unwrap();

        let mut cookies2 = HashMap::new();
        cookies2.insert("token2".to_string(), "value2".to_string());
        let path2 = manager.get_cookie_file(&cookies2, "platform2").unwrap();

        // Both should be in the same temp directory
        assert_eq!(path1.parent(), path2.parent());
    }
}