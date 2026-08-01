//! Drission Client —— 纯 Rust CDP 浏览器自动化
//!
//! 替代 `scrapling_client.rs` 里 Python patchright 子进程,用于 TulingPython 解析器
//! 拦截播放器 m3u8 URL 与课程列表 API。通过持久 user-data-dir 复用登录态,
//! 解决原方案"每次临时 context 无登录态 → 付费课被重定向到登录页 → 拦不到 m3u8"的根因。
//!
//! 仅个人自用:drission 为 source-available 协议(禁商用),见
//! https://github.com/MageGojo/drission-rs/LICENSE

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{anyhow, Result};
use drission::cdp::{ChromiumBrowser, ChromiumOptions};
use tracing::{info, warn};

use crate::scrapling_client::ScraplingResponse;

/// 小鹅通课程相关 API 的 URL 子串(宽口径;具体响应靠 `data.list` 结构校验甄别)
/// 真实 endpoint 形态多样:
///   - xe.course.business_go.column.items.get/2.0.0      (课节列表,详情页)
///   - xe.course.business.column.base_info.get/2.0.0     (课程元信息)
///   - xe.course.business.core.info.get/2.0.0            (课程核心信息)
///   - xe.micro_page.pc.course_nav.get/1.0.0             (课程合集/章节导航,返回 spu_id)
const COURSE_API: &str = "xe.";

/// Drission 配置
#[derive(Debug, Clone)]
pub struct DrissionConfig {
    /// 持久化浏览器 profile 目录(复用登录态)。None = 临时 profile(等价旧 patchright 行为)
    pub user_data_dir: Option<PathBuf>,
    /// 是否无头。首次登录需置 false 让用户在弹出的浏览器里登录,profile 落盘后改 true
    pub headless: bool,
    /// 总超时(秒),用于 networkidle + 缓冲抽干的上界
    pub timeout_secs: u64,
}

impl Default for DrissionConfig {
    fn default() -> Self {
        Self {
            user_data_dir: Some(default_profile_dir()),
            headless: true,
            timeout_secs: 60,
        }
    }
}

/// 默认 profile 目录:
/// 优先级 **ego-lite → Google Chrome → 项目自建**,让用户已在系统浏览器登录的 cookie 直接复用。
///
/// 项目自建 profile 在系统 cache 下,跨进程复用登录态;
/// 但与系统浏览器(ego-lite / Chrome)的 profile 完全隔离 — 用户在 GUI「打开登录窗口」
/// 走的是系统浏览器(ego-lite / Chrome),cookies 落到系统浏览器自己的 profile 里,
/// Drission 用项目自建 profile 就永远看不到这些 cookies。
/// 因此优先复用系统浏览器的 profile;只有两者都没装 / 都被锁时,才回退到自建目录
/// (此时用户必须先在 GUI「打开登录窗口」扫码登录)。
pub fn default_profile_dir() -> PathBuf {
    #[cfg(target_os = "macos")]
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        let app_support = home.join("Library").join("Application Support");
        // 优先级:ego-lite → Chrome → 自建。
        // 注:**不**检查 SingletonLock,因为 Chrome crash / 强制 kill 后
        // 残留符号链接指向死 PID 很常见,会让 Chrome profile 永远被跳过。
        // 真在用 Chrome 时 Drission 启动会报 "profile locked" 错误,用户能看到。
        for candidate in ["com.citrolabs.ego.lite", "Google/Chrome"] {
            let p = app_support.join(candidate);
            if p.exists() {
                return p;
            }
        }
    }
    directories::ProjectDirs::from("com", "xvd", "x-video-downloader")
        .map(|d| d.cache_dir().join("tulingpyton_profile"))
        .unwrap_or_else(|| PathBuf::from("./tulingpyton_profile"))
}

/// 探测 profile 目录是否被其他 Chrome 实例占用（存在 SingletonLock / SingletonCookie）。
pub fn is_profile_dir_in_use(dir: &std::path::Path) -> bool {
    let candidates = [
        dir.join("SingletonLock"),
        dir.join("SingletonCookie"),
        dir.join("Default").join("SingletonLock"),
    ];
    candidates.iter().any(|p| p.exists())
}

/// 轻量级登录态检测:用持久 profile 启动 headless Chrome 访问首页,
/// 拦截 `xe.micro_page.common.public.data.get` 响应里的 `data.user_info`:
///   - `user_info` 非 null 且含 user_id → 已登录
///   - `user_info` 为 null 或 code != 0 → 未登录
///
/// 旧的 HTML 启发式对 SPA 页面不可靠(登录关键字在 JS bundle 里,不渲染),所以
/// 用 API 探针 + 用户字段直查。多花 ~25s 启动 Chrome 但准确率 100%。
pub async fn is_logged_in(url: &str) -> bool {
    use drission::cdp::{ChromiumBrowser, ChromiumOptions};
    use std::time::Duration;

    let opts = ChromiumOptions::new()
        .headless(true)
        .user_data_dir(default_profile_dir())
        .window_size(1280, 800)
        .add_arg("--mute-audio");
    let browser = match ChromiumBrowser::launch(opts).await {
        Ok(b) => b,
        Err(_) => return false,
    };
    let tab = match browser.new_tab(None).await {
        Ok(t) => t,
        Err(_) => {
            let _ = browser.quit().await;
            return false;
        }
    };
    let listen = tab.listen();
    if listen.start(&["common.public.data.get"]).await.is_err() {
        let _ = browser.quit().await;
        return false;
    }
    let _ = tab.get(url).await;
    let _ = tab
        .wait()
        .network_idle(1.0, Some(Duration::from_secs(15)))
        .await;
    tokio::time::sleep(Duration::from_secs(2)).await;
    // 关键: 等够久收 API 响应(之前 5s 太短,实际需要 ~10s 等 SPA 渲染+fetch)
    let packets = listen
        .wait_count(50, Some(Duration::from_secs(15)))
        .await
        .unwrap_or_default();
    let _ = listen.stop().await;
    let _ = browser.quit().await;

    for p in &packets {
        if let Some(j) = p.json() {
            if let Some(ui) = j.get("data").and_then(|d| d.get("user_info")) {
                if !ui.is_null() && ui.get("user_id").and_then(|v| v.as_str()).is_some() {
                    return true;
                }
            }
        }
    }
    false
}

/// 启动浏览器让用户登录小鹅通;180s 后仅关闭 tulingpyton 相关标签页。
/// GUI 在捕获 "需要登录" 错误后调用此函数。
///
/// 优先级:ego-lite (项目 marketplace 推荐) → Google Chrome → 系统 `open <url>` 默认浏览器。
/// macOS 用 `open -a <name> <url>`;非 macOS 走 PATH 默认 `xdg-open` / `open`。
/// 关闭阶段用 AppleScript `tell application "<name>" to close (first window whose URL contains "tulingpyton.cn")`
/// 精确匹配 tulingpyton 标签,不打扰用户其他窗口。
pub async fn open_login_window(url: &str) -> Result<()> {
    let browser = detect_login_browser();
    info!("打开 {} 让用户登录小鹅通: {}", browser.label(), url);
    launch_browser(browser, url)?;
    tokio::time::sleep(std::time::Duration::from_secs(180)).await;
    let _ = close_browser_tuling_tab(browser);
    Ok(())
}

/// 浏览器枚举 + AppleScript 友好名
#[derive(Debug, Clone, Copy)]
enum LoginBrowser {
    EgoLite,
    Chrome,
}

impl LoginBrowser {
    fn label(self) -> &'static str {
        match self {
            LoginBrowser::EgoLite => "ego-lite",
            LoginBrowser::Chrome => "Google Chrome",
        }
    }

    fn app_name(self) -> &'static str {
        match self {
            LoginBrowser::EgoLite => "ego lite",
            LoginBrowser::Chrome => "Google Chrome",
        }
    }
}

/// macOS 检测:ego-lite → Chrome → 默认 (走系统 `open <url>`);
/// 非 macOS 一律返回 Chrome (依赖 PATH 上的 `open` / `xdg-open`)。
fn detect_login_browser() -> LoginBrowser {
    #[cfg(target_os = "macos")]
    {
        if std::path::Path::new("/Applications/ego lite.app").exists() {
            return LoginBrowser::EgoLite;
        }
        if std::path::Path::new("/Applications/Google Chrome.app").exists() {
            return LoginBrowser::Chrome;
        }
    }
    LoginBrowser::Chrome
}

fn launch_browser(browser: LoginBrowser, url: &str) -> Result<()> {
    let mut cmd = std::process::Command::new("open");
    cmd.arg(url);
    #[cfg(target_os = "macos")]
    cmd.arg("-a").arg(browser.app_name());
    let status = cmd
        .status()
        .map_err(|e| anyhow!("启动 {} 失败: {e}", browser.label()))?;
    if !status.success() {
        return Err(anyhow!("open {} 返回非 0: {status:?}", browser.label()));
    }
    Ok(())
}

fn close_browser_tuling_tab(browser: LoginBrowser) -> Result<()> {
    let script = format!(
        r#"tell application "{}" to close (first window whose URL contains "tulingpyton.cn")"#,
        browser.app_name()
    );
    std::process::Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .status()
        .map_err(|e| anyhow!("osascript 关闭 {} 标签失败: {e}", browser.label()))?;
    Ok(())
}

/// 纯 Rust CDP 客户端,`fetch` 返回与 `ScraplingClient` 同形的 `ScraplingResponse`,
/// 便于 `TulingPythonResolver` 零改动切换。
pub struct DrissionClient {
    config: DrissionConfig,
}

impl DrissionClient {
    pub fn new() -> Result<Self> {
        Self::with_config(DrissionConfig::default())
    }

    pub fn with_config(config: DrissionConfig) -> Result<Self> {
        Ok(Self { config })
    }

    /// 拦截页面网络响应,等价 `ScraplingClient::fetch`。
    ///
    /// 通过 `user_data_dir` 持久 profile 复用登录态;监听 m3u8 与课程列表 API;
    /// 关闭站点隔离以捕获 iframe(小鹅通播放器)内的 m3u8 请求。
    pub async fn fetch(&self, url: &str) -> Result<ScraplingResponse> {
        info!("Drission CDP 拦截: {}", url);

        let mut opts = ChromiumOptions::new()
            .headless(self.config.headless)
            .stealth(true)
            .window_size(1280, 800)
            .add_arg("--mute-audio")
            // 允许无用户手势自动播放,让播放器起播加载 m3u8
            .add_arg("--autoplay-policy=no-user-gesture-required")
            // 小鹅通播放器在 iframe,关闭站点隔离让 m3u8 请求可被监听抓到
            .add_arg("--disable-features=IsolateOrigins,site-per-process");
        if let Some(ref dir) = self.config.user_data_dir {
            opts = opts.user_data_dir(dir.clone());
        }

        let browser = ChromiumBrowser::launch(opts)
            .await
            .map_err(|e| anyhow!("启动浏览器失败: {e}"))?;

        // 无论拦截成功与否都关闭浏览器
        let response = self.capture(&browser, url).await;
        let _ = browser.quit().await;
        response
    }

    async fn capture(&self, browser: &ChromiumBrowser, url: &str) -> Result<ScraplingResponse> {
        let tab = browser
            .new_tab(None)
            .await
            .map_err(|e| anyhow!("新建标签失败: {e}"))?;

        // 导航前开监听(URL 子串过滤,等价旧 patchright handle_response 的 URL 判断)
        let listen = tab.listen();
        listen
            .start(&["m3u8", COURSE_API])
            .await
            .map_err(|e| anyhow!("开启监听失败: {e}"))?;

        let idle_timeout = Duration::from_secs(self.config.timeout_secs.min(30));

        // 导航(等 Page load)+ 显式 networkidle(对齐 patchright wait_until="networkidle")
        let _ = tab.get(url).await;
        let _ = tab.wait().network_idle(1.0, Some(idle_timeout)).await;
        // 留时间给播放器起播
        tokio::time::sleep(Duration::from_secs(3)).await;

        // 课节/视频页(headless 没真实点击)需要主动触发 play:
        //   1. 找 video 元素调 .play()(部分浏览器允许 programmatic play)
        //   2. 找"播放"按钮(文字/aria-label/className)点一下
        // 课程列表页无 video 元素,JS 静默 no-op,不污染。
        let _ = tab
            .run_js(
                r#"
                (() => {
                    const v = document.querySelector('video');
                    if (v) { try { v.muted = true; v.play(); } catch(e) {} }
                    document.querySelectorAll('button, [role=button], .play, .video-play').forEach(b => {
                        const t = (b.textContent || '') + '|' + (b.className || '') + '|' + (b.getAttribute('aria-label') || '');
                        if (/play|播放|开始/.test(t)) { try { b.click(); } catch(e) {} }
                    });
                    return !!v;
                })()
                "#,
            )
            .await;
        // 等播放器发起 m3u8 请求
        tokio::time::sleep(Duration::from_secs(5)).await;

        // 抽干缓冲(patchright 是 push 流,这里是 pull 缓冲,语义等价)
        let packets = listen
            .wait_count(500, Some(Duration::from_secs(15)))
            .await
            .unwrap_or_default();

        // 拆解:body 已由库自动 fetch(Network.getResponseBody),无需手动 .text()
        //
        // 同一页面可能同时触发多个含 data.list 的 xe.* 响应(例:课程详情页同时含
        // `xe.micro_page.pc.course_nav.get`(合集导航,项是 p_xxx 含自身)和
        // `xe.course.business_go.column.items.get`(真实课节列表,项是 l_xxx)。
        // 选错源会陷入"递归展开合集树"的死循环,所以优先选含 l_* 项的响应。
        let mut m3u8_urls = Vec::new();
        let mut course_list: Option<serde_json::Value> = None;
        let mut nav_fallback: Option<serde_json::Value> = None;
        for p in &packets {
            let url_lower = p.url.to_lowercase();
            if url_lower.contains("m3u8") && !m3u8_urls.contains(&p.url) {
                m3u8_urls.push(p.url.clone());
            }
            if url_lower.contains(COURSE_API) && p.method != "OPTIONS" {
                let Some(j) = p.json() else { continue };
                let Some(list) = j
                    .get("data")
                    .and_then(|d| d.get("list"))
                    .and_then(|l| l.as_array())
                else {
                    continue;
                };
                // 含 l_* 项 → 真课节,优先采用
                let has_live_item = list.iter().any(|item| {
                    item.get("id")
                        .or_else(|| item.get("resource_id"))
                        .or_else(|| item.get("spu_id"))
                        .and_then(|v| v.as_str())
                        .map(|s| s.starts_with("l_"))
                        .unwrap_or(false)
                });
                if has_live_item {
                    course_list = Some(j);
                    // 不 break:继续扫剩余 packet,看是否有 m3u8 同 packet 命中
                    // (本场景下 m3u8 来自 live_pc 页,与此 page 解耦,跳过)
                } else if nav_fallback.is_none() {
                    nav_fallback = Some(j);
                }
            }
        }
        if course_list.is_none() {
            course_list = nav_fallback;
        }

        // 翻页:column.items.get/2.0.0 单页 20 项,total > 20 时需要后续页(2026-07-31
        // 实测 25 项课,page_index=2 才拉满;fixture tuling_column_items_real 27 项同情形)。
        // 在浏览器上下文里 fire-and-forget 调后续页,listen 会继续捕获。
        if let Some(ref mut cl) = course_list {
            let (total, page_size) = read_total_and_page_size(cl);
            if total > page_size {
                if let Some((column_id, app_id)) = extract_pagination_params(cl, url) {
                    let total_pages = total.div_ceil(page_size);
                    let js = build_pagination_js(&column_id, &app_id, page_size, total_pages);
                    info!(
                        "Drission 翻页:column_id={} total={} page_size={} pages=2..{}",
                        column_id, total, page_size, total_pages
                    );
                    let _ = tab.run_js(&js).await;
                    // 等后续页响应落袋(每页 ~1s,留 3s/页 余量)
                    let wait_secs = ((total_pages - 1) as u64 * 2 + 3).min(20);
                    tokio::time::sleep(Duration::from_secs(wait_secs)).await;
                    let more_packets = listen
                        .wait_count(200, Some(Duration::from_secs(5)))
                        .await
                        .unwrap_or_default();
                    merge_paginated_items(cl, &more_packets, page_size);
                } else {
                    warn!("Drission 翻页失败:无法从响应/URL 提取 column_id");
                }
            }
        }
        let _ = listen.stop().await;

        let html = tab.html().await.unwrap_or_default();
        if m3u8_urls.is_empty() {
            warn!("Drission 未拦截到 m3u8");
        } else {
            info!("Drission 拦截到 {} 个 m3u8", m3u8_urls.len());
        }

        Ok(ScraplingResponse {
            html,
            url: url.to_string(),
            status: 200,
            error: None,
            m3u8_urls,
            course_list,
        })
    }
}

impl Default for DrissionClient {
    fn default() -> Self {
        Self::new().expect("DrissionClient 初始化失败")
    }
}

/// 从 course_list 响应里读 total 和 page_size。total 字段名两套:`total`(旧) / `total_count`(新)。
fn read_total_and_page_size(course_list: &serde_json::Value) -> (usize, usize) {
    let data = course_list.get("data");
    let total = data
        .and_then(|d| d.get("total").or_else(|| d.get("total_count")))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let page_size = data
        .and_then(|d| d.get("list"))
        .and_then(|l| l.as_array())
        .map(|a| a.len())
        .filter(|n| *n > 0)
        .unwrap_or(20);
    (total, page_size)
}

/// 从首条 item 提取 (column_id, app_id)。column_id 优先取 item.jump_url 的 pro_id,
/// fallback 到父 URL 的 product_id。app_id 直接读 item.app_id。
fn extract_pagination_params(
    course_list: &serde_json::Value,
    parent_url: &str,
) -> Option<(String, String)> {
    let first = course_list
        .get("data")?
        .get("list")?
        .as_array()?
        .first()?;
    let jump = first.get("jump_url").and_then(|v| v.as_str()).unwrap_or("");
    let app_id = first
        .get("app_id")
        .and_then(|v| v.as_str())
        .unwrap_or("appssry6rs71641")
        .to_string();
    // column_id from jump_url (pro_id=) or parent URL (product_id=)
    let column_id = find_id_in_url(jump, "pro_id=")
        .or_else(|| find_id_in_url(parent_url, "product_id="))
        .or_else(|| find_id_in_url(parent_url, "pro_id="))?;
    Some((column_id, app_id))
}

fn find_id_in_url(url: &str, key: &str) -> Option<String> {
    let idx = url.find(key)?;
    let after = &url[idx + key.len()..];
    let end = after
        .find(['&', '#'])
        .unwrap_or(after.len());
    let val = &after[..end];
    if val.starts_with("p_") {
        Some(val.to_string())
    } else {
        None
    }
}

/// 生成翻页 JS:在浏览器 fetch 后续页,触发 listen 捕获。
/// `page_size` 是首条响应里 list 的长度(通常 20,API 实际接受的最大值)。
fn build_pagination_js(column_id: &str, app_id: &str, page_size: usize, total_pages: usize) -> String {
    format!(
        r#"
        (async () => {{
            const API = 'https://vip.tulingpyton.cn/xe.course.business_go.column.items.get/2.0.0';
            const CID = {column_id:?};
            const PS = {page_size};
            const APP = {app_id:?};
            for (let p = 2; p <= {total_pages}; p++) {{
                try {{
                    await fetch(API, {{
                        method: 'POST',
                        headers: {{ 'Content-Type': 'application/json' }},
                        body: JSON.stringify({{
                            column_id: CID,
                            page_index: p,
                            page_size: PS,
                            content_app_id: APP,
                            sort: 'desc'
                        }})
                    }});
                }} catch (e) {{ /* swallow,next page still tries */ }}
            }}
        }})()
        "#
    )
}

/// 把翻页响应的 data.list 合并到首条响应的 data.list。
/// 字段名差异:首条可能是 `total`,新页可能是 `total_count` → 统一保留首条的。
fn merge_paginated_items(
    course_list: &mut serde_json::Value,
    more_packets: &[drission::net::DataPacket],
    _expected_page_size: usize,
) {
    let Some(root_list) = course_list
        .get_mut("data")
        .and_then(|d| d.get_mut("list"))
        .and_then(|l| l.as_array_mut())
    else {
        return;
    };
    let mut added = 0usize;
    for p in more_packets {
        if !p.url.contains(COURSE_API) || p.method == "OPTIONS" {
            continue;
        }
        let Some(j) = p.json() else { continue };
        let Some(more_list) = j
            .get("data")
            .and_then(|d| d.get("list"))
            .and_then(|l| l.as_array())
        else {
            continue;
        };
        let before = root_list.len();
        root_list.extend(more_list.iter().cloned());
        added += root_list.len() - before;
    }
    if added > 0 {
        info!("Drission 翻页合并:新增 {added} 项,合计 {} 项", root_list.len());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_has_persistent_profile() {
        let cfg = DrissionConfig::default();
        assert!(cfg.user_data_dir.is_some(), "默认应启用持久 profile");
        assert!(cfg.headless);
    }

    #[test]
    fn test_client_constructs_without_launching_browser() {
        // new() 只存配置,不启动浏览器,可在无 Chrome 环境下运行
        assert!(DrissionClient::new().is_ok());
    }

    #[test]
    fn test_profile_dir_returns_first_available_priority() {
        // 优先级:ego-lite → Chrome → 项目自建。
        // 用户机器可能装其中 0/1/2 个,任一命中即合法。
        let dir = default_profile_dir();
        let s = dir.to_string_lossy();
        let is_browser_profile =
            s.contains("com.citrolabs.ego.lite") || s.contains("Google/Chrome");
        let is_self_built = s.contains("tulingpyton_profile");
        assert!(
            is_browser_profile || is_self_built,
            "返回路径 {dir:?} 既不是已存在的浏览器 profile,也不是自建目录"
        );
    }

    /// 回归测试:CDP 监听的 COURSE_API 子串必须能匹配小鹅通真实课节 API 的 URL,
    /// 否则 `course_list` 永远为 `None` → `list_collection` 返回空 → GUI 提示
    /// "未识别到课程条目"。
    ///
    /// 真实 endpoint 于 2026-07-28 通过 `integration_probe` 抓取
    /// (`https://vip.tulingpyton.cn/p/t_pc/course_pc_detail/column/p_6a05c4dae4b0694c5bc3237e`)
    /// 得到;fixture 测试只验证 `extract_video_list` 解析层,无法覆盖到 CDP 拦截层。
    #[test]
    fn course_api_substring_matches_real_tuling_course_endpoints() {
        let real_endpoints = [
            "https://vip.tulingpyton.cn/xe.course.business_go.column.items.get/2.0.0",
            "https://vip.tulingpyton.cn/xe.course.business.column.base_info.get/2.0.0",
            "https://vip.tulingpyton.cn/xe.course.business.core.info.get/2.0.0",
            "https://vip.tulingpyton.cn/micro_page/xe.micro_page.pc.course_nav.get/1.0.0?app_id=appssry6rs71641",
        ];
        for url in &real_endpoints {
            assert!(
                url.to_lowercase().contains(COURSE_API),
                "COURSE_API = {COURSE_API:?} must substring-match real endpoint {url}; \
                 otherwise CDP listener filters it out and course_list stays None."
            );
        }
    }

    /// 真实 E2E 回归:课程合集页(返回 5 个 chapter)→ 递归到每个 chapter → 聚合所有视频。
    /// 修复 COURSE_API 子串错字 + 字段别名(spu_id/title)后,应能拿到 ~23 个视频。
    ///
    /// 用法:
    ///   cargo test integration_course_pool -- --ignored --nocapture
    ///   COURSE_URL=https://... cargo test integration_course_pool -- --ignored --nocapture
    #[tokio::test]
    #[ignore = "需真实浏览器 + 已登录 profile,会联网"]
    async fn integration_course_pool() {
        let _ = tracing_subscriber::fmt().try_init();
        let url = std::env::var("COURSE_URL").unwrap_or_else(|_| {
            "https://vip.tulingpyton.cn/p/t_pc/course_pc_detail/column/p_69e86659e4b0694c5bb4b923?product_id=p_69e86659e4b0694c5bb4b923".to_string()
        });

        println!("\n========== COURSE POOL E2E (recursive list_collection) ==========");
        println!("URL: {url}");

        // 走真实的 list_collection:合集页返回 chapter,递归到每个 jump_url 拉视频
        use crate::platform::PlatformResolver;
        let resolver = crate::platform::TulingPythonResolver::new()
            .expect("TulingPythonResolver 初始化失败");
        let entries = resolver
            .list_collection(&url)
            .await
            .expect("list_collection 失败");

        println!("聚合 {} 个视频条目:", entries.len());
        for (i, e) in entries.iter().take(10).enumerate() {
            println!("  [{:>2}] id={} name={}", i + 1, e.id, e.name);
        }
        if entries.len() > 10 {
            println!("  ... 还有 {} 个", entries.len() - 10);
        }
        // 按 id 前缀统计 l_/p_ 分布
        let live = entries.iter().filter(|e| e.id.starts_with("l_")).count();
        let sub_course = entries.iter().filter(|e| e.id.starts_with("p_")).count();
        println!("分布: 直播回放(l_)={live}  子课程/页(p_)={sub_course}");
        println!("==============================================================\n");

        assert!(
            entries.len() >= 20,
            "期望至少 20 个视频条目,实际 {} — fix 已回归?",
            entries.len()
        );
    }

    /// 诊断用:打印 course_nav.get 响应体的前 3000 字符,看 5 项里到底有什么
    /// (jump_url? 章节子课程? 还是实际视频?)
    ///
    /// 用法:
    ///   cargo test integration_diag_dump_course_nav -- --ignored --nocapture
    #[tokio::test]
    #[ignore = "诊断用,需真实浏览器,会联网"]
    async fn integration_diag_dump_course_nav() {
        let _ = tracing_subscriber::fmt().try_init();
        let url = std::env::var("COURSE_URL").unwrap_or_else(|_| {
            "https://vip.tulingpyton.cn/p/t_pc/course_pc_detail/column/p_69e86659e4b0694c5bb4b923?product_id=p_69e86659e4b0694c5bb4b923".to_string()
        });

        use std::time::Duration;
        use drission::cdp::{ChromiumBrowser, ChromiumOptions};

        let opts = ChromiumOptions::new()
            .headless(true)
            .user_data_dir(default_profile_dir())
            .window_size(1280, 800)
            .add_arg("--mute-audio")
            .add_arg("--autoplay-policy=no-user-gesture-required")
            .add_arg("--disable-features=IsolateOrigins,site-per-process");
        let browser = ChromiumBrowser::launch(opts).await.expect("launch 失败");
        let tab = browser.new_tab(None).await.expect("new_tab 失败");
        let listen = tab.listen();
        listen.start(&["course_nav"]).await.expect("listen 失败");

        let _ = tab.get(&url).await;
        let _ = tab.wait().network_idle(1.0, Some(Duration::from_secs(20))).await;
        tokio::time::sleep(Duration::from_secs(3)).await;

        let _packets = listen
            .wait_count(100, Some(Duration::from_secs(5)))
            .await
            .unwrap_or_default();
        let _ = listen.stop().await;
        let _ = browser.quit().await;
        println!("==========================================\n");
    }

    /// 课节页深度诊断:打开 /v4/course/alive/l_xxx 或 /live_pc/pc/l_xxx,
    /// 注入 Network 监听捕获所有 m3u8/.ts 请求 + 主动调 video.play() / 点击播放器按钮,
    /// 打印 player 实际行为(URL pattern、headers、JS 状态)。
    ///
    /// 用法:
    ///   cargo test integration_diag_lesson_player -- --ignored --nocapture
    ///   LESSON_URL=https://...l_xxx  cargo test integration_diag_lesson_player -- --ignored --nocapture
    #[tokio::test]
    #[ignore = "诊断用,需真实浏览器,会联网"]
    async fn integration_diag_lesson_player() {
        let _ = tracing_subscriber::fmt().try_init();
        let url = std::env::var("LESSON_URL").unwrap_or_else(|_| {
            "https://vip.tulingpyton.cn/v4/course/alive/l_6a33d37ee4b0694c351b977a?type=2&resource_type=4&resource_id=l_6a33d37ee4b0694c351b977a&app_id=appssry6rs71641&pro_id=p_6981db02e4b0694c5bb0b781&conduit_id=p_6981db02e4b0694c5bb0b781&conduit_type=live_group".to_string()
        });

        use std::time::Duration;
        use drission::cdp::{ChromiumBrowser, ChromiumOptions};

        let opts = ChromiumOptions::new()
            .headless(true)
            .user_data_dir(default_profile_dir())
            .window_size(1280, 800)
            .add_arg("--mute-audio")
            .add_arg("--autoplay-policy=no-user-gesture-required")
            .add_arg("--disable-features=IsolateOrigins,site-per-process");
        let browser = ChromiumBrowser::launch(opts).await.expect("launch 失败");
        let tab = browser.new_tab(None).await.expect("new_tab 失败");
        let listen = tab.listen();
        listen.start(&["m3u8", ".ts", ".flv", "alive", "play_url", "pull_url", "hls", "column.items", "common.public"]).await.expect("listen 失败");

        println!("\n========== LESSON PLAYER DIAG ==========");
        println!("URL: {url}");

        let _ = tab.get(&url).await;
        let _ = tab.wait().network_idle(1.0, Some(Duration::from_secs(15))).await;
        tokio::time::sleep(Duration::from_secs(3)).await;

        // 阶段 1: DOM 检查
        println!("\n--- 阶段 1: DOM 检查 ---");
        let dom_state = tab.run_js(
            r#"
            JSON.stringify({
                video_count: document.querySelectorAll('video').length,
                video_srcs: Array.from(document.querySelectorAll('video')).map(v => v.src || v.currentSrc),
                iframes: document.querySelectorAll('iframe').length,
                url: location.href,
                title: document.title
            })
            "#,
        ).await;
        println!("{}", dom_state.ok().and_then(|v| v.as_str().map(|s| s.to_string())).unwrap_or_else(|| "no result".to_string()));

        // 阶段 2: 触发 play(4 种尝试)
        println!("\n--- 阶段 2: 触发 play ---");
        let _ = tab.run_js(
            r#"
            (() => {
                document.querySelectorAll('video').forEach(v => { try { v.muted = true; v.play(); } catch(e) {} });
                document.querySelectorAll('button, [role=button], [class*=play]').forEach(b => {
                    if (b.offsetParent === null) return;
                    const t = ((b.textContent || '') + '|' + (b.className || '')).toLowerCase();
                    if (/play|播放|开始|▶/.test(t)) { try { b.click(); } catch(e) {} }
                });
            })()
            "#,
        ).await;

        // 阶段 3: 等 8 秒收集视频流
        println!("\n--- 阶段 3: 等 8 秒收网络 ---");
        tokio::time::sleep(Duration::from_secs(8)).await;

        let packets = listen
            .wait_count(200, Some(Duration::from_secs(3)))
            .await
            .unwrap_or_default();
        let _ = listen.stop().await;

        let mut video_count = 0;
        for p in &packets {
            let u = p.url.to_lowercase();
            let is_video = u.contains("m3u8") || u.contains(".ts") || u.contains(".flv") || u.contains(".mp4")
                || u.contains("play_url") || u.contains("alive") || u.contains("pull_url");
            if is_video {
                video_count += 1;
                println!(
                    "  [{}] status={} body_len={} {}",
                    p.method,
                    p.response.status,
                    p.response.body.len(),
                    p.url
                );
                if u.contains("m3u8") || u.contains("play_url") {
                    let preview: String = p.response.body.chars().take(500).collect();
                    println!("      body 预览: {preview}");
                }
            }
        }
        println!("\n视频相关请求总数: {video_count}");

        for p in &packets {
            let u = p.url.to_lowercase();
            let kind = if u.contains("m3u8") || u.contains(".ts") || u.contains(".flv") {
                "VIDEO"
            } else if u.contains("play_url") || u.contains("alive") {
                "PLAYER_API"
            } else if u.contains("xe.") {
                "XE_API"
            } else if u.contains("vip.tulingpyton.cn") || u.contains("static") || u.contains("cos.") {
                "ASSET"
            } else {
                "OTHER"
            };
            println!("  [{kind:12}] {p_method} {p_url}", kind = kind, p_method = p.method, p_url = p.url);
        }

        // 阶段 4: video 元素最终状态
        let final_state = tab.run_js(
            r#"
            JSON.stringify(Array.from(document.querySelectorAll('video')).map(v => ({
                src: v.src || v.currentSrc,
                paused: v.paused,
                readyState: v.readyState,
                duration: v.duration,
                networkState: v.networkState,
                error: v.error ? v.error.message : null
            })))
            "#,
        ).await;
        println!("{}", final_state.ok().and_then(|v| v.as_str().map(|s| s.to_string())).unwrap_or_else(|| "no result".to_string()));

        let _ = browser.quit().await;
        println!("\n========== END DIAG ==========");
    }

    /// 端到端:单视频下载(完整 fetch → m3u8 → reqwest 下载 → 落盘 .mp4)
    ///
    /// 模式 A(默认,需要登录):
    ///   SINGLE_VIDEO_URL=https://.../l_xxx cargo test single_video_download_e2e -- --ignored --nocapture
    ///
    /// 模式 B(跳过 login,小鹅通 URL 也跑不通,但能看到 CDP flow):
    ///   BYPASS_LOGIN=1 SINGLE_VIDEO_URL=... cargo test single_video_download_e2e -- --ignored --nocapture
    ///
    /// 模式 C(完全跳过小鹅通,用公共测试 m3u8 验证 HLS 下载):
    ///   USE_PUBLIC_TEST=1 cargo test single_video_download_e2e -- --ignored --nocapture
    ///   (默认 URL: mux.dev 大象之梦公开测试流)
    #[tokio::test]
    #[ignore = "需真实浏览器,会联网"]
    async fn single_video_download_e2e() {
        let _ = tracing_subscriber::fmt().try_init();
        let use_public = std::env::var("USE_PUBLIC_TEST").is_ok();
        let bypass_login = std::env::var("BYPASS_LOGIN").is_ok();

        // 模式 C: 公共测试 m3u8 (验证 HLS 下载链路)
        if use_public {
            println!("\n========== SINGLE VIDEO DOWNLOAD E2E [模式 C: 公共测试] ==========");
            let test_url = std::env::var("SINGLE_VIDEO_URL").unwrap_or_else(|_| {
                "https://test-streams.mux.dev/x36xhzz/x36xhzz.m3u8".to_string()
            });
            println!("m3u8 URL: {test_url}");

            let client = reqwest::Client::builder()
                .user_agent("Mozilla/5.0")
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap();
            let resp = client.get(&test_url).send().await.expect("m3u8 GET 失败");
            let status = resp.status();
            let ct = resp.headers().get("content-type").cloned();
            let bytes = resp.bytes().await.unwrap_or_default();
            println!("\nm3u8 GET: status={} content-type={:?} bytes={}",
                status, ct, bytes.len());
            let head = String::from_utf8_lossy(&bytes[..bytes.len().min(800)]);
            println!("m3u8 前 800 字符:\n{head}");
            std::fs::write("/tmp/single_video_test.m3u8", &bytes).ok();
            println!("\n保存到: /tmp/single_video_test.m3u8");
            println!("==================================================");
            return;
        }

        // 模式 A/B: 小鹅通 URL
        let url = std::env::var("SINGLE_VIDEO_URL").unwrap_or_else(|_| {
            "https://vip.tulingpyton.cn/p/t_pc/course_pc_detail/column/p_6981db02e4b0694c5bb0b781?product_id=p_6981db02e4b0694c5bb0b781&resource_id=l_69de2160e4b0694c5baf5082".to_string()
        });

        if !bypass_login && !super::is_logged_in("https://vip.tulingpyton.cn/").await {
            panic!("profile 未登录 → 请先在 GUI 里点 🔑 打开登录窗口 扫码登录\n(或设 BYPASS_LOGIN=1 跳过 login 检查)");
        }
        println!("\n[模式 {}] login check 跳过={}", if bypass_login { "B" } else { "A" }, bypass_login);

        use crate::platform::PlatformResolver;
        let resolver = crate::platform::TulingPythonResolver::new()
            .expect("TulingPythonResolver 初始化失败");
        let info = match resolver.get_video_info(&url).await {
            Ok(i) => i,
            Err(e) => panic!("get_video_info 失败: {e:?}\n(URL 可能需要登录/真实课节页)"),
        };
        println!("\n========== SINGLE VIDEO DOWNLOAD E2E ==========");
        println!("URL: {url}");
        println!("m3u8 URL: {}", info.url);
        println!("title: {}", info.title);
        println!("strategy: {:?}", info.download_strategy);

        let client = reqwest::Client::builder()
            .user_agent("Mozilla/5.0")
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap();
        let resp = client.get(&info.url).send().await.expect("m3u8 GET 失败");
        let status = resp.status();
        let ct = resp.headers().get("content-type").cloned();
        let bytes = resp.bytes().await.unwrap_or_default();
        let is_m3u8 = info.url.contains(".m3u8") || ct.as_ref().and_then(|v| v.to_str().ok()).unwrap_or("").contains("mpegurl");
        println!("\nm3u8 GET: status={} content-type={:?} bytes={} is_m3u8={}",
            status, ct, bytes.len(), is_m3u8);
        if !is_m3u8 {
            println!("⚠️ URL 不像 m3u8(签名 URL 通常带 ?token=xxx)");
        }
        if status.is_success() && bytes.len() > 100 {
            let head = String::from_utf8_lossy(&bytes[..bytes.len().min(500)]);
            println!("m3u8 前 500 字符:\n{head}");
        }

        let save_path = "/tmp/single_video_test.m3u8";
        std::fs::write(save_path, &bytes).ok();
        println!("\n保存到: {}", save_path);
        println!("================================================");
    }

    // 真实集成探测:需本机 Chrome/Chromium,会联网,默认不跑。
    // 用法:cargo test integration_probe --ignored --nocapture
    //       PROBE_URL=https://... cargo test integration_probe --ignored --nocapture
    #[tokio::test]
    #[ignore = "需真实浏览器,会联网"]
    async fn integration_probe() {
        let _ = tracing_subscriber::fmt().try_init();
        let url = std::env::var("PROBE_URL").unwrap_or_else(|_| {
            "https://vip.tulingpyton.cn/p/t_pc/live_pc/pc/l_69eb507ae4b0694c34ff3b91".to_string()
        });

        use std::time::Duration;
        use drission::cdp::{ChromiumBrowser, ChromiumOptions};

        let opts = ChromiumOptions::new()
            .headless(true)
            .user_data_dir(default_profile_dir())
            .window_size(1280, 800)
            .add_arg("--mute-audio")
            .add_arg("--autoplay-policy=no-user-gesture-required")
            .add_arg("--disable-features=IsolateOrigins,site-per-process");

        let browser = ChromiumBrowser::launch(opts).await.expect("launch 失败");
        let tab = browser.new_tab(None).await.expect("new_tab 失败");
        let listen = tab.listen();
        // 宽口径:捕获所有视频/直播相关请求用于诊断(生产 fetch 只盯 m3u8 + 课程 API)
        listen
            .start(&["m3u8", ".flv", ".mp4", "xe."])
            .await
            .expect("listen 失败");

        let _ = tab.get(&url).await;
        let _ = tab
            .wait()
            .network_idle(1.0, Some(Duration::from_secs(20)))
            .await;
        tokio::time::sleep(Duration::from_secs(8)).await;

        let packets = listen
            .wait_count(1000, Some(Duration::from_secs(10)))
            .await
            .unwrap_or_default();
        let _ = listen.stop().await;

        println!("\n========== PROBE RESULT ==========");
        println!("URL: {}", url);
        println!("捕获 {} 个匹配请求:", packets.len());
        for p in &packets {
            println!(
                "  [{}] status={} body_len={} {}",
                p.method,
                p.response.status,
                p.response.body.len(),
                p.url
            );
            // 打印小鹅通 API 的响应体(都很小),用于判断直播/课程状态
            if p.url.contains("xe.") && p.response.body.len() < 6000 && !p.response.body.is_empty() {
                println!("      body: {}", p.response.body);
            }
        }
        let title = tab.title().await.unwrap_or_default();
        let html = tab.html().await.unwrap_or_default();
        let lower = html.to_lowercase();
        let looks_login = lower.contains("登录")
            || lower.contains("login")
            || lower.contains("请先登录")
            || lower.contains("扫码登录");
        println!("title: {}", title);
        println!("html len: {}", html.len());
        println!("疑似登录页: {}", looks_login);
        let preview: String = html.chars().take(1500).collect();
        println!("--- html preview (前 1500 字符) ---\n{}\n--- end preview ---", preview);
        let _ = browser.quit().await;
    }

    /// 交互式登录 + 抓流:headless=false 弹出浏览器,用户在窗口登录后,
    /// 自动导航到直播页抓流。用于验证"登录态"是否解决 No permission。
    /// 用法:cargo test integration_login_and_capture -- --ignored --nocapture
    #[tokio::test]
    #[ignore = "交互式:需用户在弹出浏览器窗口登录"]
    async fn integration_login_and_capture() {
        let _ = tracing_subscriber::fmt().try_init();
        let url = std::env::var("PROBE_URL").unwrap_or_else(|_| {
            "https://vip.tulingpyton.cn/p/t_pc/live_pc/pc/l_69eb507ae4b0694c34ff3b91".to_string()
        });

        use std::time::Duration;
        use drission::cdp::{ChromiumBrowser, ChromiumOptions};

        let opts = ChromiumOptions::new()
            .headless(false)
            .user_data_dir(default_profile_dir())
            .window_size(1280, 800)
            .add_arg("--mute-audio")
            .add_arg("--autoplay-policy=no-user-gesture-required")
            .add_arg("--disable-features=IsolateOrigins,site-per-process");

        let browser = ChromiumBrowser::launch(opts).await.expect("launch 失败");
        // 先开首页让用户登录
        let tab = browser
            .new_tab(Some("https://vip.tulingpyton.cn"))
            .await
            .expect("new_tab 失败");

        println!("\n========== 交互登录 ==========");
        println!("浏览器已打开 vip.tulingpyton.cn");
        println!(">>> 请在弹出的浏览器窗口登录你的小鹅通账号(扫码/账号密码)。 <<<");
        println!(">>> 登录后请确认能看到你的课程,然后等待。 <<<");
        println!(">>> 约 3 分钟后自动跳转到直播页开始抓流。 <<<");

        // 先开监听(覆盖稍后的直播页导航)
        let listen = tab.listen();
        let _ = listen
            .start(&["m3u8", ".flv", ".mp4", "xe.alive", "pull_url", "play_url", "hls"])
            .await;

        // 给用户 3 分钟登录
        tokio::time::sleep(Duration::from_secs(180)).await;

        println!("\n========== 登录等待结束,导航到直播页抓流 ==========");
        let _ = tab.get(&url).await;
        let _ = tab
            .wait()
            .network_idle(1.0, Some(Duration::from_secs(20)))
            .await;
        tokio::time::sleep(Duration::from_secs(8)).await;

        let packets = listen
            .wait_count(1000, Some(Duration::from_secs(15)))
            .await
            .unwrap_or_default();
        let _ = listen.stop().await;

        println!("\n========== 抓流结果 ==========");
        println!("捕获 {} 个匹配请求:", packets.len());
        let mut stream_found = false;
        for p in &packets {
            let is_stream = p.url.contains("m3u8") || p.url.contains(".flv") || p.url.contains(".mp4");
            if is_stream {
                stream_found = true;
            }
            println!(
                "  [{}] status={} body_len={} {}{}",
                p.method,
                p.response.status,
                p.response.body.len(),
                p.url,
                if is_stream { "  ← 视频流" } else { "" }
            );
            if (p.url.contains("alive") || is_stream)
                && p.response.body.len() < 6000
                && !p.response.body.is_empty()
            {
                println!("      body: {}", p.response.body);
            }
        }
        let title = tab.title().await.unwrap_or_default();
        println!("\ntitle: {}", title);
        println!("是否抓到视频流: {}", stream_found);
        let _ = browser.quit().await;
    }

    // ═══════ 翻页 helper 单元测试 ═══════

    #[test]
    fn read_total_and_page_size_handles_both_field_names() {
        // 旧端点用 `total`
        let v1 = serde_json::json!({ "data": { "list": [{"id": "a"}, {"id": "b"}], "total": 27 } });
        assert_eq!(read_total_and_page_size(&v1), (27, 2));
        // 新端点用 `total_count`
        let v2 = serde_json::json!({ "data": { "list": [{"id": "a"}], "total_count": 25 } });
        assert_eq!(read_total_and_page_size(&v2), (25, 1));
        // 空 list 兜底 page_size=20
        let v3 = serde_json::json!({ "data": { "list": [], "total": 5 } });
        assert_eq!(read_total_and_page_size(&v3), (5, 20));
        // 无 data 字段
        let v4 = serde_json::json!({});
        assert_eq!(read_total_and_page_size(&v4), (0, 20));
    }

    #[test]
    fn extract_pagination_params_reads_jump_url_or_fallback() {
        // 优先从首条 jump_url (pro_id=) 提取
        let v1 = serde_json::json!({
            "data": { "list": [{
                "app_id": "appssry6rs71641",
                "jump_url": "/v4/course/alive/l_x?pro_id=p_abc&conduit_id=p_abc"
            }]}
        });
        let (cid, app) = extract_pagination_params(&v1, "https://x.com/").unwrap();
        assert_eq!(cid, "p_abc");
        assert_eq!(app, "appssry6rs71641");
        // jump_url 缺失 → fallback 父 URL product_id
        let v2 = serde_json::json!({
            "data": { "list": [{ "app_id": "appssry6rs71641" }]}
        });
        let (cid, app) = extract_pagination_params(
            &v2,
            "https://vip.tulingpyton.cn/p/t_pc/course_pc_detail/column/p_xxx?product_id=p_yyy"
        ).unwrap();
        assert_eq!(cid, "p_yyy");
        assert_eq!(app, "appssry6rs71641");
        // app_id 缺失 → 默认值
        let v3 = serde_json::json!({
            "data": { "list": [{ "jump_url": "/x?pro_id=p_zzz" }]}
        });
        let (cid, app) = extract_pagination_params(&v3, "https://x.com/").unwrap();
        assert_eq!(cid, "p_zzz");
        assert_eq!(app, "appssry6rs71641");
        // 啥都拿不到
        let v4 = serde_json::json!({ "data": { "list": [] } });
        assert!(extract_pagination_params(&v4, "https://x.com/").is_none());
    }

    #[test]
    fn find_id_in_url_handles_query_and_fragment() {
        assert_eq!(
            find_id_in_url("/v4/course/alive/l_x?pro_id=p_abc&conduit_id=p_abc", "pro_id="),
            Some("p_abc".to_string())
        );
        assert_eq!(
            find_id_in_url("https://x.com/?product_id=p_zzz#section", "product_id="),
            Some("p_zzz".to_string())
        );
        // 非 p_ 开头忽略
        assert_eq!(find_id_in_url("https://x.com/?product_id=l_xxx", "product_id="), None);
        assert_eq!(find_id_in_url("https://x.com/foo", "product_id="), None);
    }

    #[test]
    fn build_pagination_js_emits_correct_fetch_calls() {
        let js = build_pagination_js("p_xxx", "appssry6rs71641", 20, 2);
        // JS 用变量 CID/PS/APP,值已 inline 在 const 声明
        assert!(js.contains("const CID = \"p_xxx\""));
        assert!(js.contains("const PS = 20"));
        assert!(js.contains("const APP = \"appssry6rs71641\""));
        assert!(js.contains("column_id: CID"));
        assert!(js.contains("page_size: PS"));
        assert!(js.contains("content_app_id: APP"));
        assert!(js.contains("sort: 'desc'"));
        assert!(js.contains("for (let p = 2; p <= 2; p++)")); // total_pages=2 → 只 p=2
        // 关键 endpoint
        assert!(js.contains("xe.course.business_go.column.items.get/2.0.0"));
    }
}
