//! AES-128 CBC 解密
//!
//! 支持 HLS 标准的 AES-128 加密分段解密

use anyhow::{anyhow, Result};
use aes::Aes128;
use aes::cipher::{BlockDecryptMut, KeyInit};
use generic_array::GenericArray;
use typenum::U16;

/// AES-128-CBC 解密器
pub struct AesDecryptor;

impl AesDecryptor {
    /// 解密 AES-128-CBC 加密的数据 (PKCS7 padding)
    pub fn decrypt(key: &[u8; 16], iv: &[u8; 16], ciphertext: &[u8]) -> Result<Vec<u8>> {
        if ciphertext.len() % 16 != 0 {
            return Err(anyhow!("ciphertext must be 16-byte aligned"));
        }

        let iv_arr = GenericArray::from_slice(iv);
        let key_arr = GenericArray::from_slice(key);

        let mut cipher = Aes128::new(&key_arr);

        let mut result = Vec::with_capacity(ciphertext.len());
        let mut prev_block = iv_arr.clone();

        for chunk in ciphertext.chunks(16) {
            let block = GenericArray::from_slice(chunk);

            // CBC 解密: 解密后 XOR 前一个密文块
            let mut block_copy = block.clone();
            cipher.decrypt_block_mut(&mut block_copy);

            let mut decrypted = [0u8; 16];
            for j in 0..16 {
                decrypted[j] = block_copy[j] ^ prev_block[j];
            }

            prev_block = block.clone();
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
pub fn parse_iv(hex: &str) -> Result<[u8; 16]> {
    let hex = hex.trim_start_matches("0x");
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
mod tests {
    use super::*;

    #[test]
    fn test_parse_iv() {
        let iv = parse_iv("0x12345678901234567890123456789012").unwrap();
        assert_eq!(iv[0], 0x12);
        assert_eq!(iv[1], 0x34);
    }
}
