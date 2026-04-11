//! WebSocket 认证模块
//!
//! 使用 HMAC-SHA256 基于 team_id + device_id + 共享密钥生成认证 token

use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::fmt;

type HmacSha256 = Hmac<Sha256>;

/// 认证 token 结构
#[derive(Debug, Clone)]
pub struct AuthToken {
    pub team_id: uuid::Uuid,
    pub device_id: uuid::Uuid,
    pub timestamp: u64,
    pub signature: String,
}

impl fmt::Display for AuthToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}:{}:{}", self.team_id, self.device_id, self.timestamp, self.signature)
    }
}

/// 认证错误
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("Token 已过期")]
    TokenExpired,
    #[error("签名验证失败")]
    InvalidSignature,
    #[error("Token 格式错误")]
    InvalidFormat,
}

/// 获取共享密钥
///
/// 从环境变量 WEBSOCKET_SHARED_SECRET 读取，如未设置则使用默认开发密钥
fn get_shared_secret() -> String {
    std::env::var("WEBSOCKET_SHARED_SECRET")
        .unwrap_or_else(|_| "ws-auth-dev-secret-key-do-not-use-in-production".to_string())
}

impl AuthToken {
    /// 生成认证 token
    ///
    /// token = HMAC-SHA256(team_id:device_id:timestamp, shared_secret)
    pub fn generate(team_id: uuid::Uuid, device_id: uuid::Uuid) -> Self {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let message = format!("{}:{}:{}", team_id, device_id, timestamp);
        let signature = Self::hmac_sign(&message);

        Self {
            team_id,
            device_id,
            timestamp,
            signature,
        }
    }

    /// 从字符串解析 token（格式: team_id:device_id:timestamp:signature）
    pub fn parse(token_str: &str) -> Result<Self, AuthError> {
        // 使用 rsplit_once 分割，最后一个 : 后面是 signature
        let Some((prefix, signature)) = token_str.rsplit_once(':') else {
            return Err(AuthError::InvalidFormat);
        };
        let Some((mid, timestamp_str)) = prefix.rsplit_once(':') else {
            return Err(AuthError::InvalidFormat);
        };
        let Some((team_id_str, device_id_str)) = mid.rsplit_once(':') else {
            return Err(AuthError::InvalidFormat);
        };

        let team_id = uuid::Uuid::parse_str(team_id_str)
            .map_err(|_| AuthError::InvalidFormat)?;
        let device_id = uuid::Uuid::parse_str(device_id_str)
            .map_err(|_| AuthError::InvalidFormat)?;
        let timestamp: u64 = timestamp_str.parse()
            .map_err(|_| AuthError::InvalidFormat)?;

        Ok(Self {
            team_id,
            device_id,
            timestamp,
            signature: signature.to_string(),
        })
    }

    /// 验证 token
    ///
    /// - 检查 timestamp 是否在有效期内（5分钟窗口）
    /// - 检查签名是否匹配
    pub fn verify(&self) -> Result<(), AuthError> {
        // 验证时间戳（5分钟窗口）- 先检查过期，避免不必要的签名计算
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        if now > self.timestamp + 300 {
            return Err(AuthError::TokenExpired);
        }

        // 验证签名
        let message = format!("{}:{}:{}", self.team_id, self.device_id, self.timestamp);
        let expected_sig = Self::hmac_sign(&message);

        if self.signature != expected_sig {
            return Err(AuthError::InvalidSignature);
        }

        Ok(())
    }

    /// HMAC-SHA256 签名
    fn hmac_sign(message: &str) -> String {
        let secret = get_shared_secret();
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
            .expect("HMAC can take key of any size");
        mac.update(message.as_bytes());
        let result = mac.finalize();
        hex::encode(result.into_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_generate_and_verify() {
        let team_id = uuid::Uuid::new_v4();
        let device_id = uuid::Uuid::new_v4();

        let token = AuthToken::generate(team_id, device_id);
        assert_eq!(token.team_id, team_id);
        assert_eq!(token.device_id, device_id);

        // 验证应该成功
        assert!(token.verify().is_ok());
    }

    #[test]
    fn test_token_serialization() {
        let team_id = uuid::Uuid::new_v4();
        let device_id = uuid::Uuid::new_v4();

        let token = AuthToken::generate(team_id, device_id);
        let token_str = token.to_string();

        let parsed = AuthToken::parse(&token_str).unwrap();
        assert_eq!(parsed.team_id, team_id);
        assert_eq!(parsed.device_id, device_id);
        assert!(parsed.verify().is_ok());
    }

    #[test]
    fn test_invalid_signature() {
        let team_id = uuid::Uuid::new_v4();
        let device_id = uuid::Uuid::new_v4();

        let mut token = AuthToken::generate(team_id, device_id);
        token.signature = "invalid".to_string();

        assert!(matches!(token.verify(), Err(AuthError::InvalidSignature)));
    }

    #[test]
    fn test_expired_token() {
        let team_id = uuid::Uuid::new_v4();
        let device_id = uuid::Uuid::new_v4();

        let mut token = AuthToken::generate(team_id, device_id);
        token.timestamp = 0; // 设置为 0 会导致过期检查失败

        assert!(matches!(token.verify(), Err(AuthError::TokenExpired)));
    }

    #[test]
    fn test_invalid_format() {
        assert!(AuthToken::parse("invalid").is_err());
        assert!(AuthToken::parse("a:b:c").is_err());
        assert!(AuthToken::parse("not-uuid:device:timestamp:signature").is_err());
    }
}
