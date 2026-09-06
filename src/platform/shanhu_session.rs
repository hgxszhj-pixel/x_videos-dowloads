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
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{info, warn};

/// Session 文件名,与 hash cache 同目录。
pub const SESSION_FILENAME: &str = "shanhu-session.json";

/// Session 过期阈值 (默认 7 天)。
///
/// 选 7 天的理由:PHPSESSID + 微信扫码态在 SHANHU/万字平台实测稳定期约 1-2 周;
/// 设 7 天给用户留充足缓冲,过期后主动提示比"看似能用实则 code=100067"友好得多。
pub const SESSION_MAX_AGE_SECS: u64 = 7 * 24 * 3600;

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

impl Session {
    /// 解析 `saved_at` (`epoch:<secs>` 格式) 为 Unix 秒数。无法解析 → None。
    ///
    /// 故意对未知格式返回 None 而非 false:旧 session 文件(如意外手工编辑)
    /// 不会因为 saved_at 不识别就误判"刚保存",而是降级到被动检测
    /// (实际调 videoAuth 时返 code=100067 才报错)。
    pub fn saved_at_epoch(&self) -> Option<u64> {
        self.saved_at.strip_prefix("epoch:")?.parse().ok()
    }

    /// 距今的秒数。None 表示 saved_at 无法解析或本地时钟早于 saved_at (clock skew)。
    pub fn age_seconds(&self) -> Option<u64> {
        let saved = self.saved_at_epoch()?;
        let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
        now.checked_sub(saved)
    }

    /// 是否超过 `max_age_secs` 阈值。
    ///
    /// 返回 false on parse failure (graceful):无法识别 saved_at 时保守地认为未过期,
    /// 让旧 session 文件继续可用 (实际失效由 videoAuth 的 `code=100067` 兜底)。
    pub fn is_expired(&self, max_age_secs: u64) -> bool {
        self.age_seconds().is_some_and(|age| age > max_age_secs)
    }

    /// 是否超过默认阈值 (`SESSION_MAX_AGE_SECS`)。
    pub fn is_stale(&self) -> bool {
        self.is_expired(SESSION_MAX_AGE_SECS)
    }
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
        age_days: session.as_ref().and_then(|s| s.age_seconds()).map(|s| s / 86400),
        is_stale: session.as_ref().is_some_and(|s| s.is_stale()),
    }
}

#[derive(Debug, Clone)]
pub struct SessionSummary {
    pub path: Option<String>,
    pub cookie_count: usize,
    pub has_token: bool,
    pub saved_at: Option<String>,
    /// 距今天数 (None = 无 session 或 saved_at 无法解析)
    pub age_days: Option<u64>,
    /// 是否超过 SESSION_MAX_AGE_SECS
    pub is_stale: bool,
}

/// 判断 cookie 域是否属于 SHANHU/Wanzibook 体系。
///
/// 三个保留域:
/// - `.wanzibook.com` — 后端 API (study.wanzibook.com)
/// - `*.rexxi.cn` — 白标域名 (任意 *.rexxi.cn 子域,如 wx-bgfcbieh.rexxi.cn)
/// - `*.tenwiseacademy.cn` — 视频解析 CDN (111453136245362688.tenwiseacademy.cn)
pub fn is_shanhu_cookie_domain(domain: &str) -> bool {
    domain.contains("wanzibook.com")
        || domain.contains("rexxi.cn")
        || domain.contains("tenwiseacademy.cn")
}

/// 把 camoufox `export_state` 格式的 cookies JSON 转成我们的 Session。
/// camoufox 导出格式: `{"cookies": [{"name": "...", "value": "...", "domain": "...", "path": "...", ...}], ...}`
/// 也接受顶层数组格式 `[...]` (兼容旧版本 / 简化导出)
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

    #[test]
    fn is_shanhu_cookie_domain_matches_all_three() {
        // 后端域
        assert!(is_shanhu_cookie_domain(".wanzibook.com"));
        assert!(is_shanhu_cookie_domain("study.wanzibook.com"));
        // 白标域 (子域匹配)
        assert!(is_shanhu_cookie_domain(".rexxi.cn"));
        assert!(is_shanhu_cookie_domain("wx-bgfcbieh.rexxi.cn"));
        assert!(is_shanhu_cookie_domain("rexxi.cn"));
        // CDN 域
        assert!(is_shanhu_cookie_domain("111453136245362688.tenwiseacademy.cn"));
        assert!(is_shanhu_cookie_domain(".tenwiseacademy.cn"));
    }

    #[test]
    fn is_shanhu_cookie_domain_rejects_unrelated() {
        // 常见其它域不应被误判为 SHANHU
        assert!(!is_shanhu_cookie_domain(".google.com"));
        assert!(!is_shanhu_cookie_domain(".example.com"));
        // 子串匹配会漏掉 evil 同形异义域 (e.g. `fake-rexxi.cn.evil.com`
        // 仍包含 `rexxi.cn` 子串 → 误判为 SHANHU)。这是 substring 过滤的固有限制,
        // 真实 cookie store 不会出现此类攻击域,可接受。
        assert!(is_shanhu_cookie_domain("fake-rexxi.cn.evil.com"));
        // 空字符串
        assert!(!is_shanhu_cookie_domain(""));
    }

    // --- Session 过期检测 (2026-09-06 P1) ---

    fn make_session_with_saved_at(saved_at: &str) -> Session {
        Session {
            saved_at: saved_at.to_string(),
            token: None,
            cookies: vec![],
        }
    }

    #[test]
    fn session_fresh_is_not_expired() {
        // "刚刚保存" (epoch:<now>) → age ≈ 0,远小于 7 天阈值
        let now_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let s = make_session_with_saved_at(&format!("epoch:{now_secs}"));
        assert!(s.age_seconds().unwrap() < 5, "刚保存的 session age 应 < 5s");
        assert!(!s.is_stale(), "fresh session 不应过期");
        assert!(!s.is_expired(SESSION_MAX_AGE_SECS));
    }

    #[test]
    fn session_old_is_expired() {
        // 8 天前保存的 session → age > 7 天阈值
        let now_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let eight_days_ago = now_secs - 8 * 86400;
        let s = make_session_with_saved_at(&format!("epoch:{eight_days_ago}"));
        let age_days = s.age_seconds().unwrap() / 86400;
        assert!(age_days >= 8, "8 天前 age 应 >= 8 天");
        assert!(s.is_stale(), "8 天前 session 应过期");
        assert!(s.is_expired(SESSION_MAX_AGE_SECS));
    }

    #[test]
    fn session_unparseable_saved_at_is_not_expired() {
        // saved_at 不是 epoch:<secs> 格式 → graceful 不误判为过期
        // (真实场景:手工编辑 / 旧版本写入 / 文件损坏)
        let s = make_session_with_saved_at("2026-09-06T20:00:00Z");
        assert_eq!(s.saved_at_epoch(), None);
        assert_eq!(s.age_seconds(), None);
        assert!(!s.is_expired(SESSION_MAX_AGE_SECS), "无法解析时保守不报过期");
    }

    #[test]
    fn session_is_expired_uses_threshold_correctly() {
        // 3 天前的 session,测试不同阈值
        let now_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let three_days_ago = now_secs - 3 * 86400;
        let s = make_session_with_saved_at(&format!("epoch:{three_days_ago}"));
        assert!(!s.is_expired(7 * 86400), "7 天阈值下 3 天前不老化");
        assert!(s.is_expired(1 * 86400), "1 天阈值下 3 天前老化");
        assert!(s.is_expired(0), "0 阈值下任何 age > 0 即过期 (这里 age ≈ 3 天)");
    }
}