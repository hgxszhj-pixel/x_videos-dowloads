//! AES-128 CBC 解密
//!
//! 支持 HLS 标准的 AES-128 加密分段解密

use aes::cipher::{BlockDecryptMut, KeyInit};
use aes::Aes128;
use anyhow::{anyhow, Result};
use generic_array::GenericArray;

/// AES-128-CBC 解密器
#[expect(dead_code)]
pub struct AesDecryptor;

#[expect(dead_code)]
impl AesDecryptor {
    /// 解密 AES-128-CBC 加密的数据 (PKCS7 padding)
    pub fn decrypt(key: &[u8; 16], iv: &[u8; 16], ciphertext: &[u8]) -> Result<Vec<u8>> {
        if !ciphertext.len().is_multiple_of(16) {
            return Err(anyhow!("ciphertext must be 16-byte aligned"));
        }

        let iv_arr = GenericArray::from_slice(iv);
        let key_arr = GenericArray::from_slice(key);

        let mut cipher = Aes128::new(key_arr);

        let mut result = Vec::with_capacity(ciphertext.len());
        let mut prev_block = *iv_arr;

        for chunk in ciphertext.chunks(16) {
            let block = GenericArray::from_slice(chunk);

            // CBC 解密: 解密后 XOR 前一个密文块
            let mut block_copy = *block;
            cipher.decrypt_block_mut(&mut block_copy);

            let mut decrypted = [0u8; 16];
            for j in 0..16 {
                decrypted[j] = block_copy[j] ^ prev_block[j];
            }

            prev_block = *block;
            result.extend_from_slice(&decrypted);
        }

        // 去除 PKCS7 padding
        let padding = result.last().copied().unwrap_or(0) as usize;
        if padding > 0 && padding <= 16 && result.len() >= padding {
            let is_valid = result[result.len() - padding..]
                .iter()
                .all(|&b| b == padding as u8);
            if is_valid {
                result.truncate(result.len() - padding);
            }
        }

        Ok(result)
    }
}

/// 从 key.bin URL 获取密钥
pub async fn fetch_key(client: &reqwest::Client, key_url: &str) -> Result<[u8; 16]> {
    let response = client.get(key_url).send().await?;
    let key_bytes = response.bytes().await?;

    if key_bytes.len() != 16 {
        return Err(anyhow!("key must be 16 bytes, got {}", key_bytes.len()));
    }

    let mut key = [0u8; 16];
    key.copy_from_slice(&key_bytes);
    Ok(key)
}

/// 解析 IV 字符串（0x... 格式）
///
/// 边界（2026-10-30 补测时发现）：
/// - 长度检查按**字节**，而切片 `&hex[i..i+2]` 要求落在**字符边界**上 ——
///   8 个 emoji 正好 32 字节，能过长度检查，却在 i=2 处切在字符中间 → panic。
///   所以必须先验 ASCII 再切片。IV 来自远端 m3u8，不是可信输入。
/// - 前缀大小写都接受（规范写 `0x`，部分源写 `0X`）；只剥**一层** ——
///   旧实现用 `trim_start_matches` 会把 `0x0x…` 也一起剥掉。
pub fn parse_iv(hex: &str) -> Result<[u8; 16]> {
    let hex = hex.trim();
    let hex = hex
        .strip_prefix("0x")
        .or_else(|| hex.strip_prefix("0X"))
        .unwrap_or(hex);

    if !hex.is_ascii() {
        return Err(anyhow!("IV must be ASCII hex chars"));
    }
    if hex.len() != 32 {
        return Err(anyhow!("IV must be 32 hex chars (16 bytes)"));
    }

    let bytes: Vec<u8> = (0..32)
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| anyhow!("invalid hex")))
        .collect::<Result<Vec<_>>>()?;

    let mut iv = [0u8; 16];
    iv.copy_from_slice(&bytes);
    Ok(iv)
}

/// 便捷函数：解密 AES-128-CBC 加密的数据
#[expect(dead_code)]
pub fn decrypt_aes128_cbc(key: &[u8], iv: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>> {
    if key.len() != 16 {
        return Err(anyhow!("key must be 16 bytes"));
    }
    if iv.len() != 16 {
        return Err(anyhow!("iv must be 16 bytes"));
    }

    let mut key_arr = [0u8; 16];
    let mut iv_arr = [0u8; 16];
    key_arr.copy_from_slice(key);
    iv_arr.copy_from_slice(iv);

    AesDecryptor::decrypt(&key_arr, &iv_arr, ciphertext)
}

#[cfg(test)]
#[allow(unused_imports)] // bench 编译上下文看不到 super::* 调用方，test 实际使用
mod tests {
    use super::*;

    #[test]
    fn test_parse_iv() {
        let iv = parse_iv("0x12345678901234567890123456789012").unwrap();
        assert_eq!(iv[0], 0x12);
        assert_eq!(iv[1], 0x34);
    }

    /// 边界补测（2026-10-30）。
    ///
    /// 关键一条是**非 ASCII 不能 panic** —— IV 来自远端 m3u8，不是可信输入。
    /// 旧实现按字节查长度（8 个 emoji = 32 字节，能过），
    /// 却按字符切片 `&hex[i..i+2]` → 切在多字节字符中间 → panic。
    #[test]
    fn test_parse_iv_boundary() {
        // 前缀大小写都接受
        assert_eq!(
            parse_iv("0X12345678901234567890123456789012").unwrap()[0],
            0x12
        );
        // 无前缀也接受（旧行为保持）
        assert_eq!(
            parse_iv("12345678901234567890123456789012").unwrap()[0],
            0x12
        );
        // 大写 hex 数字（32 位）
        assert_eq!(
            parse_iv("0xABCDEFabcdefABCDEFabcdefABCDEF12").unwrap()[0],
            0xAB
        );
        // 长度不对
        assert!(parse_iv("0x1234").is_err());
        assert!(parse_iv("").is_err());
        // 非法 hex
        assert!(parse_iv("0xzzzz5678901234567890123456789012").is_err());

        // 8 个 emoji 恰好 32 字节 —— 旧实现在这里 panic
        let emoji = format!("0x{}", "😀".repeat(8));
        assert!(
            parse_iv(&emoji).is_err(),
            "非 ASCII IV 必须报错而不是 panic"
        );

        // 多字节字符 + 十六进制混合，长度同样凑够 32 字节
        let mixed = format!("0x{}abcd", "😀".repeat(7));
        assert!(parse_iv(&mixed).is_err());
    }
}
