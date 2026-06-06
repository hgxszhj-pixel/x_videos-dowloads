//! A-Bogus 签名模块
//!
//! 用于 DouYin API 请求签名
//! 参考 F2 (Python) abogus.py 实现
//! 使用 SM3 国密哈希算法

use sm3::Digest;

/// A-Bogus 签名生成器
pub struct ABogus {
    /// 固定密钥
    key: [u8; 3],
}

impl ABogus {
    /// 创建新的 ABogus 实例
    pub fn new() -> Self {
        Self {
            key: [0x00, 0x01, 0x0e],
        }
    }

    /// 计算 SM3 哈希并返回字节数组
    fn sm3_to_array(&self, input: &[u8]) -> Vec<u8> {
        let mut hasher = sm3::Digest::new();
        hasher.update(input);
        let result: [u8; 32] = hasher.finalize();
        result.to_vec()
    }

    /// RC4 加密
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
            let k = s[(s[i] as usize + s[j] as usize) % 256] as u8;
            result.push(byte ^ k);
        }

        result
    }

    /// 自定义 Base64 编码 (用于 A-Bogus)
    fn custom_base64_encode(&self, data: &[u8]) -> String {
        // A-Bogus 使用的 Base64 字符表
        let chars = "Dkdpgh4ZKsQB80/Mfvw36XI1R25-WUAlEi7NLboqYTOPuzmFjJnryx9HVGcaStCe=";
        let chars_bytes = chars.as_bytes();
        let mut result = String::new();

        for chunk in data.chunks(3) {
            let b0 = chunk[0] as usize;
            let b1 = chunk.get(1).copied().unwrap_or(0) as usize;
            let b2 = chunk.get(2).copied().unwrap_or(0) as usize;

            result.push(chars_bytes[b0 >> 2] as char);
            result.push(chars_bytes[((b0 & 0x03) << 4) | (b1 >> 4)] as char);

            if chunk.len() > 1 {
                result.push(chars_bytes[((b1 & 0x0f) << 2) | (b2 >> 6)] as char);
            }
            if chunk.len() > 2 {
                result.push(chars_bytes[b2 & 0x3f] as char);
            }
        }

        result
    }

    /// 生成 A-Bogus 签名
    /// 参数:
    /// - params: URL 查询参数字符串 (不包含 ?)
    /// - body: 请求体字符串 (POST 数据)
    /// - user_agent: User-Agent 字符串
    /// - aid: 应用 ID
    /// - page_id: 页面 ID
    pub fn sign(&self, params: &str, body: &str, user_agent: &str, aid: &str, page_id: &str) -> String {
        // 1. 计算 params 的 SM3 哈希 (两次)
        let params_bytes = params.as_bytes();
        let params_sm3_1 = self.sm3_to_array(params_bytes);
        let params_sm3_2 = self.sm3_to_array(&params_sm3_1);

        // 2. 计算 body 的 SM3 哈希 (两次)
        let body_bytes = body.as_bytes();
        let body_sm3_1 = if body_bytes.is_empty() {
            vec![0u8; 32]
        } else {
            self.sm3_to_array(body_bytes)
        };
        let body_sm3_2 = if body_bytes.is_empty() {
            vec![0u8; 32]
        } else {
            self.sm3_to_array(&body_sm3_1)
        };

        // 3. RC4 加密 User-Agent
        let ua_encrypted = self.rc4_encrypt(&self.key, user_agent.as_bytes());
        let ua_b64 = self.custom_base64_encode(&ua_encrypted);

        // 4. 构建数组
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        // 5. 构建 directory values 数组
        let mut dir_values = [
            64.0,                      // clientU
            1.0,                       // clientV
            (timestamp % 1000000) as f64 / 1000000.0, // ts
            aid.parse::<f64>().unwrap_or(6383.0),     // aid
            page_id.parse::<f64>().unwrap_or(1.0),    // pageId
        ];

        // 添加 params SM3 哈希的部分字节
        if params_sm3_2.len() >= 16 {
            dir_values[5] = params_sm3_2[0] as f64 / 255.0;
            dir_values[6] = params_sm3_2[1] as f64 / 255.0;
            dir_values[7] = params_sm3_2[14] as f64 / 255.0;
            dir_values[8] = params_sm3_2[15] as f64 / 255.0;
        }

        // 添加 body SM3 哈希的部分字节
        if body_sm3_2.len() >= 16 {
            dir_values[9] = body_sm3_2[0] as f64 / 255.0;
            dir_values[10] = body_sm3_2[1] as f64 / 255.0;
            dir_values[11] = body_sm3_2[14] as f64 / 255.0;
            dir_values[12] = body_sm3_2[15] as f64 / 255.0;
        }

        // 6. 计算 XOR checksum
        let mut checksum: u8 = 0;
        for &v in &dir_values {
            checksum ^= ((v * 255.0) as u8);
        }
        for &b in &ua_encrypted {
            checksum ^= b;
        }

        // 7. 构建最终签名
        let mut signature_data = Vec::new();
        signature_data.extend_from_slice(&[checksum]);
        signature_data.extend_from_slice(&params_sm3_2[..16]);
        signature_data.extend_from_slice(&body_sm3_2[..16]);
        signature_data.extend_from_slice(&ua_encrypted);

        // 8. Base64 编码
        self.custom_base64_encode(&signature_data)
    }
}

impl Default for ABogus {
    fn default() -> Self {
        Self::new()
    }
}

/// 生成 DouYin verify_fp
pub fn gen_verify_fp() -> String {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // base36 编码时间戳
    let ts_base36 = format!("{:x}", timestamp);

    // 生成伪 UUID
    let uuid_part = format!(
        "{:08x}-{:04x}-4{:03x}-{:04x}",
        rand::random::<u32>(),
        rand::random::<u16>(),
        rand::random::<u16>() & 0x0fff,
        rand::random::<u16>() & 0x3fff | 0x8000
    );

    format!("verify_{}_{}", ts_base36, uuid_part)
}

/// 生成 DouYin s_v_web_id
pub fn gen_s_v_web_id() -> String {
    gen_verify_fp()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verify_fp_generation() {
        let verify_fp = gen_verify_fp();
        assert!(verify_fp.starts_with("verify_"));
        assert!(verify_fp.len() > 20);
    }

    #[test]
    fn test_abogus_creation() {
        let ab = ABogus::new();
        assert_eq!(ab.key, [0x00, 0x01, 0x0e]);
    }

    #[test]
    fn test_sm3_hash() {
        let ab = ABogus::new();
        let hash = ab.sm3_to_array(b"test");
        assert_eq!(hash.len(), 32);
    }
}
