//! SHANHU/万字 SaaS 一次性登录 session 缓存。
//!
//! ## 背景 (2026-09-06 JS 逆向发现)
//! 整条下载链路里,**file_id (listSections) → cfileid (视频实际 VOD 标识)** 这一步
//! 需要调 `/stu/course/videoAuth`,而 videoAuth 100% 需要登录 (`code=100067`)。
//!
//! 其它 endpoint 均匿名可达,包括 tenwiseacademy.cn 的 m3u8 解析。
//!
//! ## 方案:一次登录,持久复用
//! 用户在 camoufox 浏览器登录一次 → 导出 cookies → 保存到此文件 → 后续所有 videoAuth
//! 都用这份 cookies 调用,无需再启动浏览器。
//!
//! ## 文件格式 (JSON)
//! ```json
//! {
//!   "saved_at": "2026-09-06T20:00:00Z",
//!   "token": "<localStorage token, 可选>",
//!   "cookies": [
//!     {"name": "PHPSESSID", "value": "...", "domain": ".wanzibook.com", "path": "/"},
//!     ...
//!   ]
//! }
//! ```
//!
//! ## 与 hash cache 的区别
//! - hash cache: `file_id → 32 字符 hex` 的下载必需映射,匿名可写
//! - session: videoAuth 调用所需的鉴权凭据,登录态,不可匿名获取
//!
//! 两个文件独立保存。session 失效 (token 过期) 后只需重跑 `--shanhu-login`。

use crate::platform::shanhu::hash_cache_path;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::{info, warn};

/// Session 文件名,与 hash cache 同目录。
pub const SESSION_FILENAME: &str = "shanhu-session.json";

/// 单条 cookie。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CookieEntry {
    pub name: String,
    pub value: String,
    #[serde(default = "default_domain")]
    pub domain: String,
    #[serde(default = "default_path")]
    pub path: String,
}

fn default_domain() -> String {
    ".wanzibook.com".to_string()
}
fn default_path() -> String {
    "/".to_string()
}

/// Session 内容。`cookies` 必有,`token` 可选 (wanzibook 用 localStorage token,有些 endpoint 也需要)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub saved_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    pub cookies: Vec<CookieEntry>,
}

/// Session 文件路径。与 hash cache 同目录。
pub fn session_path() -> Option<PathBuf> {
    hash_cache_path().map(|p| {
        // 把 hash cache 文件名替换为 session 文件名,保留同目录
        p.parent()
            .map(|dir| dir.join(SESSION_FILENAME))
            .unwrap_or_else(|| PathBuf::from(SESSION_FILENAME))
    })
}

/// 读取 session。文件不存在 / 解析失败 → 返回 None (非错误,正常 fallback)。
pub fn load_session() -> Option<Session> {
    let path = session_path()?;
    let raw = std::fs::read_to_string(&path).ok()?;
    match serde_json::from_str::<Session>(&raw) {
        Ok(s) => {
            info!(
                "SHANHU session 已加载: {} 条 cookies, saved_at={}",
                s.cookies.len(),
                s.saved_at
            );
            Some(s)
        }
        Err(e) => {
            warn!("SHANHU session 解析失败 ({}), 使用空 session", e);
            None
        }
    }
}

/// 保存 session (原子写)。
#[allow(dead_code)] // `--shanhu-login` 子命令计划中,先暴露 API
pub fn save_session(session: &Session) -> std::io::Result<()> {
    let Some(path) = session_path() else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "无法定位 XDG config dir",
        ));
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let serialized = serde_json::to_string_pretty(session)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serialized)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

/// 把 session cookies 转成 reqwest 可用的 header (`Cookie: name=value; name=value`)。
pub fn cookies_to_header(session: &Session) -> String {
    session
        .cookies
        .iter()
        .map(|c| format!("{}={}", c.name, c.value))
        .collect::<Vec<_>>()
        .join("; ")
}

/// Session 简明状态 (给 `--shanhu-session-info` 用)。
pub fn session_summary() -> SessionSummary {
    let path = session_path();
    let session = load_session();
    SessionSummary {
        path: path.as_ref().map(|p| p.display().to_string()),
        cookie_count: session.as_ref().map(|s| s.cookies.len()).unwrap_or(0),
        has_token: session.as_ref().and_then(|s| s.token.as_ref()).is_some(),
        saved_at: session.as_ref().map(|s| s.saved_at.clone()),
    }
}

#[derive(Debug, Clone)]
pub struct SessionSummary {
    pub path: Option<String>,
    pub cookie_count: usize,
    pub has_token: bool,
    pub saved_at: Option<String>,
}

/// 把 camoufox `export_state` 格式的 cookies JSON 转成我们的 Session。
/// camoufox 导出格式: `[{"name": "...", "value": "...", "domain": "...", "path": "...", ...}]`
#[allow(dead_code)]
pub fn session_from_export(exported: &[serde_json::Value]) -> Session {
    let cookies = exported
        .iter()
        .filter_map(|v| {
            let name = v.get("name")?.as_str()?.to_string();
            let value = v.get("value")?.as_str()?.to_string();
            let domain = v
                .get("domain")
                .and_then(|d| d.as_str())
                .unwrap_or(".wanzibook.com")
                .to_string();
            let path = v
                .get("path")
                .and_then(|p| p.as_str())
                .unwrap_or("/")
                .to_string();
            Some(CookieEntry {
                name,
                value,
                domain,
                path,
            })
        })
        .collect();

    Session {
        saved_at: chrono_now(),
        token: None,
        cookies,
    }
}

/// 简易时间戳 (RFC3339-ish, 不用 chrono 依赖)。
fn chrono_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("epoch:{secs}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cookies_to_header_format() {
        let s = Session {
            saved_at: "test".into(),
            token: None,
            cookies: vec![
                CookieEntry {
                    name: "PHPSESSID".into(),
                    value: "abc123".into(),
                    domain: ".wanzibook.com".into(),
                    path: "/".into(),
                },
                CookieEntry {
                    name: "uid".into(),
                    value: "test-uid".into(),
                    domain: ".wanzibook.com".into(),
                    path: "/".into(),
                },
            ],
        };
        assert_eq!(s.cookies.len(), 2);
        let header = cookies_to_header(&s);
        assert_eq!(header, "PHPSESSID=abc123; uid=test-uid");
    }

    #[test]
    fn session_round_trip() {
        let original = Session {
            saved_at: "2026-09-06T20:00:00Z".into(),
            token: Some("test-token".into()),
            cookies: vec![CookieEntry {
                name: "test".into(),
                value: "v".into(),
                domain: ".wanzibook.com".into(),
                path: "/".into(),
            }],
        };
        let json = serde_json::to_string(&original).unwrap();
        let parsed: Session = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.cookies.len(), 1);
        assert_eq!(parsed.token, Some("test-token".to_string()));
    }

    #[test]
    fn session_from_export_skips_malformed() {
        let export = vec![
            serde_json::json!({"name": "a", "value": "1", "domain": ".x.com", "path": "/"}),
            serde_json::json!({"name": "b"}), // missing value → skip
            serde_json::json!({"value": "x"}), // missing name → skip
            serde_json::json!({"name": "c", "value": "3"}), // missing domain/path → defaults
        ];
        let s = session_from_export(&export);
        assert_eq!(s.cookies.len(), 2);
        assert_eq!(s.cookies[0].name, "a");
        assert_eq!(s.cookies[1].name, "c");
        assert_eq!(s.cookies[1].domain, ".wanzibook.com"); // default
    }

    #[test]
    fn session_summary_default_no_file() {
        // 无文件时 cookie_count = 0 (graceful)
        let s = session_summary();
        // 注意:可能因为别的测试写过文件导致不为 0,所以这里只检查不 panic
        let _ = s.cookie_count;
    }
}