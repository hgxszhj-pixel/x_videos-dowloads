//! macOS-only: 从 ego-lite Chrome SQLite 解密 SHANHU session cookies。
//!
//! ## 背景 (2026-09-14 BUG-D 调查)
//! SHANHU/万字 SaaS 视频下载链路 (file_id → videoAuth → cfileid/m3u8) 需要登录态。
//! 之前流程是 camoufox MCP 启浏览器扫码 → 导出 state.json → `--shanhu-login`。
//! 但实际跑下来 session 文件 cookies=0, 排查发现 camoufox 导出没成功
//! (见 MEMORY.md:bug-d-isolation-and-cookies-zero)。
//!
//! ## 替代方案
//! 用户日常用 ego-lite 浏览器 (macOS 原生 Chromium 内核, profile 路径
//! `~/Library/Application Support/Citro Labs/ego lite/`) 已登录 SHANHU。
//! 复用 [`crate::drission_client::decrypt_ego_lite_cookies_for_host`]
//! (AES-128-CBC + PBKDF2-HMAC-SHA1("saltysalt", 1003, 16) + Keychain shell)
//! 读 SQLite → 解密 → host 过滤, 一行命令导入 session, 跳过 camoufox 流程。
//!
//! ## 局限 (Phase 1)
//! - localStorage token 不导入 (需 LevelDB 解码), 用户仍需单独 `--shanhu-login-token`
//! - 仅 macOS (`#[cfg(target_os = "macos")]`), Linux/Win 走原 `--shanhu-login` 流程
//!
//! ## References
//! - MEMORY.md:chromium-macos-cookie-decryption — Keychain/PBKDF2/v10 格式
//! - MEMORY.md:ego-lite-profile-real-path — `Citro Labs/ego lite` 路径
//! - MEMORY.md:tulingpyton-ego-lite-cookie-injection — 小鹅通端到端验证
//! - MEMORY.md:ego-lite-cookie-host-parameterization — host filter 参数化教训
//! - MEMORY.md:bug-d-isolation-and-cookies-zero — 本次 BUG-D 调查触发

#[cfg(target_os = "macos")]
use crate::drission_client::decrypt_ego_lite_cookies_for_host;
#[cfg(target_os = "macos")]
use super::shanhu_session::{
    is_shanhu_cookie_domain, save_session, session_from_export, session_summary,
};

/// 从 macOS ego-lite 浏览器导入 SHANHU session cookies。
///
/// 调用栈:
/// 1. `decrypt_ego_lite_cookies_for_host("wanzibook.com", &["rexxi.cn", ...])`
///    → 解密 ego-lite SQLite cookies + 过滤 SHANHU 3 域 → cookie header 字符串
/// 2. 解析 `name=value; name=value` → `Vec<serde_json::Value>`
/// 3. `is_shanhu_cookie_domain` 二次防御过滤
/// 4. `session_from_export` → `Session { saved_at: epoch:<now>, cookies, token: None }`
/// 5. `save_session` 原子写到 `~/Library/Application Support/com.x-video-downloader.x-video-downloader/shanhu-session.json`
/// 6. 输出 `session_summary()` 验证
///
/// ## Errors
/// - ego-lite profile 不存在 / Cookies SQLite 缺失 → 用户没装 ego-lite
/// - Keychain `ego safe storage` 读取失败 → 用户拒绝授权或从未登录
/// - 0 个匹配 cookie → 用户未在 ego-lite 登录 SHANHU 域
#[cfg(target_os = "macos")]
pub fn import_from_ego_lite() -> anyhow::Result<()> {
    // 1. 解密 + host 过滤 (复用 drission_client 现成 helper)
    let cookie_header = decrypt_ego_lite_cookies_for_host(
        "wanzibook.com",
        &["rexxi.cn", "wanzibook.com", "tenwiseacademy.cn"],
    )
    .map_err(|e| {
        anyhow::anyhow!(
            "ego-lite 解密失败: {e}\n  排查:\n  1. ego-lite 是否已登录 SHANHU (rexxi.cn / wanzibook.com / tenwiseacademy.cn)?\n  2. macOS Keychain 是否允许 Terminal 访问 'ego safe storage'?\n  3. 备选: --shanhu-login-cookie <COOKIE_STRING> 手动喂"
        )
    })?;

    // 2. 解析 cookie header → JSON cookies 数组 (统一 .wanzibook.com)
    //    解析逻辑镜像 src/main.rs:406 handle_shanhu_login_cookie, 保证行为一致。
    //    按 Rule 3 "Surgical Changes" 不提取到 shanhu_session.rs 公共 helper —
    //    改 handle_shanhu_login_cookie 风险/收益不划算, 接受 ~20 行重复。
    let cookies: Vec<serde_json::Value> = cookie_header
        .split(';')
        .filter_map(|piece| {
            let piece = piece.trim();
            if piece.is_empty() {
                return None;
            }
            let (name, value) = piece.split_once('=')?;
            let name = name.trim();
            let value = value.trim();
            if name.is_empty() || value.is_empty() {
                return None;
            }
            Some(serde_json::json!({
                "name": name,
                "value": value,
                "domain": ".wanzibook.com",
                "path": "/",
                "expires": -1,
                "httpOnly": true,
                "secure": true,
                "sameSite": "Lax",
            }))
        })
        .collect();

    if cookies.is_empty() {
        anyhow::bail!(
            "ego-lite cookie header 解析后为空\n  解密出: {:?}\n  提示: cookies 解析格式应为 'name1=value1; name2=value2; ...'",
            cookie_header
        );
    }

    // 3. domain 二次过滤 (防御: 避免 decrypt helper 漏过的非 SHANHU 域 cookie)
    let total = cookies.len();
    let shanhu_cookies: Vec<serde_json::Value> = cookies
        .into_iter()
        .filter(|c| {
            let domain = c.get("domain").and_then(|d| d.as_str()).unwrap_or("");
            is_shanhu_cookie_domain(domain)
        })
        .collect();
    let kept = shanhu_cookies.len();
    let skipped = total - kept;

    if kept == 0 {
        anyhow::bail!(
            "ego-lite 解密出 {} 个 cookies 但均不属 SHANHU 域 (.wanzibook.com / .rexxi.cn / .tenwiseacademy.cn)",
            total
        );
    }

    // 4. 构造 Session + 原子写 (复用 save_session + session_from_export)
    let session = session_from_export(&shanhu_cookies);
    save_session(&session)?;

    // 5. 输出摘要 (与 handle_shanhu_login_cookie 风格对齐)
    println!("✓ SHANHU session 已从 ego-lite 导入");
    println!("  源:   ~/Library/Application Support/Citro Labs/ego lite/Default/Cookies");
    println!(
        "  cookie 数: {} (跳过 {} 个非 SHANHU 域)",
        kept, skipped
    );
    let s = session_summary();
    if let Some(p) = s.path {
        println!("  保存到: {}", p);
    }
    println!(
        "  含 token:  {}",
        if s.has_token {
            "是"
        } else {
            "否 (Phase 2 follow-up: 需 LevelDB 解码)"
        }
    );
    println!(
        "  保存时间:  {}",
        s.saved_at.as_deref().unwrap_or("<未知>")
    );
    println!();
    println!("  ⚠️  localStorage token 未自动导入 (Chrome Cookies SQLite 不含 localStorage)");
    println!("     请单独跑: --shanhu-login-token <TOKEN> 喂 token");
    println!("     (DevTools → Application → Local Storage → token 字段值)");
    println!();
    println!("  videoAuth 自动发现已可用 — 缺失章节会通过 session cookie 自动获取 cfileid");
    println!("  更新:ego-lite 重新登录后再跑本命令 (覆盖保存)");

    Ok(())
}
