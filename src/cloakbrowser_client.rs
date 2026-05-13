use std::collections::HashMap;

use anyhow::Context;
use serde::Deserialize;

/// CloakBrowser client for browser automation
pub struct CloakBrowserClient {
    config: CloakBrowserConfig,
}

#[derive(Clone)]
pub struct CloakBrowserConfig {
    pub headless: bool,
}

impl CloakBrowserClient {
    pub fn new(config: CloakBrowserConfig) -> Self {
        Self { config }
    }

    /// 获取指定 URL 的 cookies
    pub async fn get_cookies(&self, url: &str) -> anyhow::Result<HashMap<String, String>> {
        let script = self.build_cookie_script(url);
        let output = self.run_script(&script).await?;

        #[derive(Deserialize)]
        struct CookieResult {
            success: bool,
            cookies: HashMap<String, String>,
        }

        let result: CookieResult = serde_json::from_str(&output)
            .context("Failed to parse cookie result")?;

        if !result.success {
            anyhow::bail!("Failed to get cookies from {}", url);
        }

        Ok(result.cookies)
    }

    /// 构建获取 cookies 的 Python 脚本
    fn build_cookie_script(&self, url: &str) -> String {
        format!(
            r###"
import sys
import json
from cloakbrowser import launch

def get_cookies(url):
    try:
        browser = launch(headless={})
        page = browser.new_page()
        page.goto(url, wait_until="domcontentloaded", timeout=60000)
        page.wait_for_timeout(2000)

        cookies = page.cookies()
        browser.close()

        return {{
            "success": True,
            "cookies": cookies
        }}
    except Exception as e:
        return {{
            "success": False,
            "cookies": {{}},
            "error": str(e)
        }}

result = get_cookies("{}")
print(json.dumps(result))
"###,
            self.config.headless, url
        )
    }

    async fn run_script(&self, script: &str) -> anyhow::Result<String> {
        // Placeholder implementation - actual implementation would run the Python script
        let output = std::process::Command::new("python3")
            .args(["-c", script])
            .output()
            .context("Failed to execute Python script")?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if !output.status.success() {
            anyhow::bail!("Script failed: {}", stderr);
        }

        Ok(stdout)
    }
}

impl Default for CloakBrowserConfig {
    fn default() -> Self {
        Self { headless: true }
    }
}
