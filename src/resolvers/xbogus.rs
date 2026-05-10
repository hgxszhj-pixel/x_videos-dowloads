//! X-Bogus 签名模块
//!
//! 用于 TikTok API 请求签名
//! 参考 F2 (Python) xbogus.py 实现

use md5;
use std::collections::HashMap;

/// X-Bogus 使用的 Base64 字符表 (64字符)
const X_BOGUS_BASE64_CHARS: &str = "Dkdpgh4ZKsQB80/Mfvw36XI1R25-WUAlEi7NLboqYTOPuzmFjJnryx9HVGcaStCe=";

/// X-Bogus 签名生成器
pub struct XBogus {
    /// MD5 密钥
    key: [u8; 3],
}

impl XBogus {
    /// 创建新的 XBogus 实例
    pub fn new() -> Self {
        Self {
            key: [0x00, 0x01, 0x0c],
        }
    }

    /// 生成 X-Bogus 签名
    /// 参数:
    /// - url_params: URL 查询参数字符串 (不包含 ?)
    /// - user_agent: User-Agent 字符串
    /// - timestamp: 时间戳 (毫秒)
    pub fn sign(&self, url_params: &str, user_agent: &str, timestamp: i64) -> String {
        // 1. MD5 hash User-Agent with key
        let ua_hash = self.md5_with_key(user_agent.as_bytes());

        // 2. Base64 encode RC4-encrypted UA
        let ua_encrypted = self.rc4_encrypt(&ua_hash, user_agent.as_bytes());
        let ua_b64 = self.custom_base64_encode(&ua_encrypted);

        // 3. MD5 encrypt URL params
        let url_hash = self.md5_str_to_array(url_params.as_bytes());

        // 4. Build new_array with:
        // constant (64), multiplier (0.00390625), 1, 12,
        // URL hash bytes [14,15], UA hash bytes [14,15],
        // timestamp bytes, constant (536919696)
        let mut new_array = vec![
            64.0f64,
            0.00390625f64,
            1.0f64,
            12.0f64,
        ];

        // Add URL hash bytes [14,15] (as floats divided by 255)
        new_array.push(url_hash[14] as f64 / 255.0);
        new_array.push(url_hash[15] as f64 / 255.0);

        // Add UA hash bytes [14,15]
        new_array.push(ua_hash[14] as f64 / 255.0);
        new_array.push(ua_hash[15] as f64 / 255.0);

        // Add timestamp (as 8 bytes, little-endian doubles)
        let ts_bytes = self.i64_to_bytes(timestamp);
        for &b in &ts_bytes {
            new_array.push(b as f64 / 255.0);
        }

        // Add constant 536919696
        let const_bytes = self.i32_to_bytes(536919696i32);
        for &b in &const_bytes {
            new_array.push(b as f64 / 255.0);
        }

        // 5. XOR all bytes together for checksum
        let checksum = self.calculate_checksum(&new_array);

        // 6. RC4 encrypt with key 0xFF (ÿ)
        let checksum_bytes = [checksum];
        let encrypted_checksum = self.rc4_encrypt(&[0xFF], &checksum_bytes);

        // 7. Build final signature string: ua_b64 . "_" . url_b64 . "_" . checksum_b64
        let url_encrypted = self.rc4_encrypt(&ua_hash, url_params.as_bytes());
        let url_b64 = self.custom_base64_encode(&url_encrypted);
        let checksum_b64 = self.custom_base64_encode(&encrypted_checksum);

        format!("{}_{}_{}", ua_b64, url_b64, checksum_b64)
    }

    /// MD5 hash with custom key
    fn md5_with_key(&self, data: &[u8]) -> Vec<u8> {
        let mut input = Vec::with_capacity(self.key.len() + data.len());
        input.extend_from_slice(&self.key);
        input.extend_from_slice(data);
        let result = md5::compute(&input);
        result.to_vec()
    }

    /// MD5 string to array (converts hex string to integer array via lookup table)
    fn md5_str_to_array(&self, data: &[u8]) -> Vec<u8> {
        let result = md5::compute(data);
        result.to_vec()
    }

    /// RC4 encrypt
    fn rc4_encrypt(&self, key: &[u8], data: &[u8]) -> Vec<u8> {
        let mut s: Vec<u8> = (0..=255).collect();
        let mut j: usize = 0;

        // Key scheduling
        for i in 0..256 {
            j = (j + s[i] as usize + key[i % key.len()] as usize) % 256;
            s.swap(i, j);
        }

        // PRGA
        let mut result = Vec::with_capacity(data.len());
        let mut i: usize = 0;
        let mut j: usize = 0;

        for &byte in data {
            i = (i + 1) % 256;
            j = (j + s[i] as usize) % 256;
            s.swap(i, j);
            let k = s[(s[i] as usize + s[j] as usize) % 256];
            result.push(byte ^ k);
        }

        result
    }

    /// Custom Base64 encode using X-Bogus alphabet
    fn custom_base64_encode(&self, data: &[u8]) -> String {
        let chars = X_BOGUS_BASE64_CHARS.as_bytes();
        let mut result = String::new();

        // Process 3-byte groups
        for chunk in data.chunks(3) {
            let b0 = chunk[0] as usize;
            let b1 = chunk.get(1).copied().unwrap_or(0) as usize;
            let b2 = chunk.get(2).copied().unwrap_or(0) as usize;

            result.push(chars[b0 >> 2] as char);
            result.push(chars[((b0 & 0x03) << 4) | (b1 >> 4)] as char);

            if chunk.len() > 1 {
                result.push(chars[((b1 & 0x0f) << 2) | (b2 >> 6)] as char);
            }
            if chunk.len() > 2 {
                result.push(chars[b2 & 0x3f] as char);
            }
        }

        result
    }

    /// Calculate XOR checksum
    fn calculate_checksum(&self, array: &[f64]) -> u8 {
        let mut checksum: u8 = 0;
        for &val in array {
            let byte = (val * 255.0) as u8;
            checksum ^= byte;
        }
        checksum
    }

    /// i64 to 8 bytes (big-endian)
    fn i64_to_bytes(&self, val: i64) -> Vec<u8> {
        val.to_be_bytes().to_vec()
    }

    /// i32 to 4 bytes (big-endian)
    fn i32_to_bytes(&self, val: i32) -> Vec<u8> {
        val.to_be_bytes().to_vec()
    }
}

impl Default for XBogus {
    fn default() -> Self {
        Self::new()
    }
}

/// 生成 TikTok API 签名 URL
/// 将 X-Bogus 签名附加到 URL
pub fn sign_tiktok_url(url: &str, user_agent: &str) -> String {
    // 解析 URL 分离 query string
    let parts: Vec<&str> = url.split('?').collect();
    let base_url = parts[0];
    let query_string = parts.get(1).unwrap_or(&"");

    if query_string.is_empty() {
        return url.to_string();
    }

    let xbogus = XBogus::new();
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;

    let signature = xbogus.sign(query_string, user_agent, timestamp);

    format!("{}?{}&X-Bogus={}", base_url, query_string, signature)
}

/// 从 cookies 构建 Netscape 格式 cookie 字符串
pub fn build_cookie_string(tokens: &HashMap<String, String>) -> String {
    let mut lines = String::from("# Netscape HTTP Cookie File\n");
    lines.push_str(".tiktok.com\tTRUE\t/\tTRUE\t0\t");

    for (name, value) in tokens {
        lines.push_str(name);
        lines.push('\t');
        lines.push_str(value);
        lines.push('\n');
    }

    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xbogus_creation() {
        let xb = XBogus::new();
        assert_eq!(xb.key, [0x00, 0x01, 0x0c]);
    }

    #[test]
    fn test_md5_with_key() {
        let xb = XBogus::new();
        let result = xb.md5_with_key(b"test");
        assert_eq!(result.len(), 16);
    }

    #[test]
    fn test_rc4_encrypt() {
        let xb = XBogus::new();
        let key = [0x01, 0x02, 0x03];
        let data = b"hello";
        let encrypted = xb.rc4_encrypt(&key, data);
        // RC4 should produce same result on decrypt
        let decrypted = xb.rc4_encrypt(&key, &encrypted);
        assert_eq!(decrypted, data);
    }

    #[test]
    fn test_custom_base64() {
        let xb = XBogus::new();
        let data = b"hello world";
        let encoded = xb.custom_base64_encode(data);
        assert!(!encoded.is_empty());
        assert!(encoded.chars().all(|c| X_BOGUS_BASE64_CHARS.contains(c)));
    }

    #[test]
    fn test_sign_url() {
        let url = "https://www.tiktok.com/api/post/item_list/?aid=1234&app_name=test";
        let ua = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36";
        let signed = sign_tiktok_url(url, ua);
        assert!(signed.contains("X-Bogus="));
    }
}
