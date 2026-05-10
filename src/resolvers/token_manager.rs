//! Token 管理模块
//!
//! 集中管理各平台所需的认证 token

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// 平台 Token 配置
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlatformTokens {
    /// TikTok msToken
    pub tiktok_ms_token: Option<String>,
    /// TikTok ttwid
    pub tiktok_ttwid: Option<String>,
    /// TikTok webid
    pub tiktok_webid: Option<String>,
    /// DouYin msToken
    pub douyin_ms_token: Option<String>,
    /// Bilibili SESSDATA
    pub bilibili_sessdata: Option<String>,
    /// Bilibili bili_jct
    pub bilibili_bili_jct: Option<String>,
    /// Bilibili buvid3
    pub bilibili_buvid3: Option<String>,
    /// Weibo SUB cookie
    pub weibo_sub: Option<String>,
}

/// Token 管理器
pub struct TokenManager {
    tokens: PlatformTokens,
    token_file: PathBuf,
}

impl TokenManager {
    /// 创建新的 TokenManager
    pub fn new(data_dir: &Path) -> Self {
        let token_file = data_dir.join("tokens.json");
        let tokens = if token_file.exists() {
            std::fs::read_to_string(&token_file)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            PlatformTokens::default()
        };

        Self { tokens, token_file }
    }

    /// 保存 tokens 到文件
    pub fn save(&self) -> anyhow::Result<()> {
        if let Some(parent) = self.token_file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(
            &self.token_file,
            serde_json::to_string_pretty(&self.tokens)?,
        )?;
        Ok(())
    }

    /// 设置 TikTok msToken
    pub fn set_tiktok_ms_token(&mut self, token: String) {
        self.tokens.tiktok_ms_token = Some(token);
    }

    /// 设置 TikTok ttwid
    pub fn set_tiktok_ttwid(&mut self, token: String) {
        self.tokens.tiktok_ttwid = Some(token);
    }

    /// 获取 TikTok tokens
    pub fn get_tiktok_tokens(&self) -> HashMap<String, String> {
        let mut tokens = HashMap::new();
        if let Some(ref t) = self.tokens.tiktok_ms_token {
            tokens.insert("msToken".to_string(), t.clone());
        }
        if let Some(ref t) = self.tokens.tiktok_ttwid {
            tokens.insert("ttwid".to_string(), t.clone());
        }
        tokens
    }

    /// 设置 Bilibili SESSDATA
    pub fn set_bilibili_sessdata(&mut self, token: String) {
        self.tokens.bilibili_sessdata = Some(token);
    }

    /// 设置 Bilibili bili_jct
    pub fn set_bilibili_bili_jct(&mut self, token: String) {
        self.tokens.bilibili_bili_jct = Some(token);
    }

    /// 获取 Bilibili cookies 作为 HashMap
    pub fn get_bilibili_cookies(&self) -> HashMap<String, String> {
        let mut cookies = HashMap::new();
        if let Some(ref s) = self.tokens.bilibili_sessdata {
            cookies.insert("SESSDATA".to_string(), s.clone());
        }
        if let Some(ref j) = self.tokens.bilibili_bili_jct {
            cookies.insert("bili_jct".to_string(), j.clone());
        }
        if let Some(ref b) = self.tokens.bilibili_buvid3 {
            cookies.insert("buvid3".to_string(), b.clone());
        }
        cookies
    }

    /// 检查是否有 Bilibili 认证
    pub fn has_bilibili_auth(&self) -> bool {
        self.tokens.bilibili_sessdata.is_some()
    }

    /// 检查是否有 TikTok 认证
    pub fn has_tiktok_auth(&self) -> bool {
        self.tokens.tiktok_ms_token.is_some()
    }

    /// 设置 DouYin msToken
    pub fn set_douyin_ms_token(&mut self, token: String) {
        self.tokens.douyin_ms_token = Some(token);
    }

    /// 获取 DouYin tokens
    pub fn get_douyin_tokens(&self) -> HashMap<String, String> {
        let mut tokens = HashMap::new();
        if let Some(ref t) = self.tokens.douyin_ms_token {
            tokens.insert("msToken".to_string(), t.clone());
        }
        tokens
    }

    /// 检查是否有 DouYin 认证
    pub fn has_douyin_auth(&self) -> bool {
        self.tokens.douyin_ms_token.is_some()
    }

    /// 生成 TikTok msToken (参考 F2 TokenManager.gen_real_msToken)
    /// 通过 HTTP POST 获取，失败则使用 fallback 随机生成
    pub async fn generate_ms_token(&mut self) -> anyhow::Result<()> {
        use reqwest::Client;
        use std::time::Duration;

        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()?;

        // F2 使用的端点
        let url = "https://live.douyin.com/aweme/push/web/effect/get_region_list";

        let response = client
            .post(url)
            .header("User-Agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36")
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "magic": "8a6f7e3b9c4d2e1f0a5b8c7d6e9f3a2b",
                "version": "1.0.0",
                "dataType": 2,
                "strData": "",
                "ulr": "",
                "tspFromClient": 0
            }))
            .send()
            .await;

        match response {
            Ok(resp) => {
                // 从响应 cookies 提取 msToken
                if let Some(ms_token) = resp.cookies().find(|c| c.name() == "msToken") {
                    let value = ms_token.value().to_string();
                    let len = value.len();
                    // 验证长度 (164 或 184 字符)
                    if len == 164 || len == 184 {
                        self.tokens.tiktok_ms_token = Some(value);
                        tracing::info!("Generated real msToken (length: {})", len);
                        return self.save().map_err(|e| anyhow::anyhow!("{}", e));
                    }
                }
                // 未能获取有效 msToken，使用 fallback
                tracing::warn!("Failed to get real msToken, using fallback");
                self.generate_fallback_ms_token();
            }
            Err(e) => {
                tracing::warn!("Failed to request msToken endpoint: {}, using fallback", e);
                self.generate_fallback_ms_token();
            }
        }

        self.save().map_err(|e| anyhow::anyhow!("{}", e))
    }

    /// 生成 fallback msToken (参考 F2 gen_false_msToken)
    /// 生成 182 字符随机字符串 + "=="
    fn generate_fallback_ms_token(&mut self) {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let chars: Vec<char> = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/=".chars().collect();
        let mut token: String = (0..182).map(|_| chars[rng.gen_range(0..chars.len())]).collect();
        token.push_str("==");
        let len = token.len();
        self.tokens.tiktok_ms_token = Some(token);
        tracing::debug!("Generated fallback msToken (length: {})", len);
    }

    /// 生成 TikTok ttwid (参考 F2 TokenManager.gen_ttwid)
    pub async fn generate_ttwid(&mut self) -> anyhow::Result<()> {
        use reqwest::Client;
        use std::time::Duration;

        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()?;

        let url = "https://www.tiktok.com/api/register/wtv/";

        let response = client
            .post(url)
            .header("User-Agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36")
            .header("Content-Type", "application/json")
            .send()
            .await;

        match response {
            Ok(resp) => {
                if let Some(ttwid) = resp.cookies().find(|c| c.name() == "ttwid") {
                    self.tokens.tiktok_ttwid = Some(ttwid.value().to_string());
                    tracing::info!("Generated real ttwid");
                    return self.save().map_err(|e| anyhow::anyhow!("{}", e));
                }
                tracing::warn!("Failed to get real ttwid, using fallback");
                self.tokens.tiktok_ttwid = Some("1%7C00000000000000000000000000000000000000000000000000000000000000000".to_string());
            }
            Err(e) => {
                tracing::warn!("Failed to request ttwid endpoint: {}, using fallback", e);
                self.tokens.tiktok_ttwid = Some("1%7C00000000000000000000000000000000000000000000000000000000000000000".to_string());
            }
        }

        self.save().map_err(|e| anyhow::anyhow!("{}", e))
    }

    /// 生成 TikTok webid (参考 F2 TokenManager.gen_webid)
    pub async fn generate_webid(&mut self) -> anyhow::Result<()> {
        use reqwest::Client;
        use std::time::Duration;

        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()?;

        let url = "https://www.tiktok.com/api/webid/generate/";

        let response = client
            .post(url)
            .header("User-Agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36")
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "app_id": 1233,
                "referer": "",
                "url": "",
                "user_agent": "",
                "user_unique_id": ""
            }))
            .send()
            .await;

        match response {
            Ok(resp) => {
                if let Ok(json) = resp.json::<serde_json::Value>().await {
                    if let Some(data) = json.get("data") {
                        if let Some(web_id) = data.get("web_id").and_then(|w| w.as_str()) {
                            self.tokens.tiktok_webid = Some(web_id.to_string());
                            tracing::info!("Generated webid: {}", web_id);
                            return self.save().map_err(|e| anyhow::anyhow!("{}", e));
                        }
                    }
                }
                tracing::warn!("Failed to parse webid response, using fallback");
                self.generate_fallback_webid();
            }
            Err(e) => {
                tracing::warn!("Failed to request webid endpoint: {}, using fallback", e);
                self.generate_fallback_webid();
            }
        }

        self.save().map_err(|e| anyhow::anyhow!("{}", e))
    }

    /// 生成 fallback webid
    fn generate_fallback_webid(&mut self) {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let webid: u64 = rng.gen_range(7000000000000000000..9000000000000000000);
        self.tokens.tiktok_webid = Some(webid.to_string());
        tracing::debug!("Generated fallback webid: {}", webid);
    }

    /// 批量生成所有 TikTok tokens
    pub async fn refresh_tiktok_tokens(&mut self) -> anyhow::Result<()> {
        self.generate_ms_token().await?;
        self.generate_ttwid().await?;
        self.generate_webid().await
    }
}

impl Default for TokenManager {
    fn default() -> Self {
        Self {
            tokens: PlatformTokens::default(),
            token_file: PathBuf::from("tokens.json"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tiktok_tokens() {
        let mut tm = TokenManager::default();
        tm.set_tiktok_ms_token("test_ms_token".to_string());
        tm.set_tiktok_ttwid("test_ttwid".to_string());

        let tokens = tm.get_tiktok_tokens();
        assert_eq!(tokens.get("msToken"), Some(&"test_ms_token".to_string()));
        assert_eq!(tokens.get("ttwid"), Some(&"test_ttwid".to_string()));
    }

    #[test]
    fn test_bilibili_cookies() {
        let mut tm = TokenManager::default();
        tm.set_bilibili_sessdata("test_sessdata".to_string());
        tm.set_bilibili_bili_jct("test_bili_jct".to_string());

        let cookies = tm.get_bilibili_cookies();
        assert_eq!(cookies.get("SESSDATA"), Some(&"test_sessdata".to_string()));
        assert_eq!(cookies.get("bili_jct"), Some(&"test_bili_jct".to_string()));
    }
}