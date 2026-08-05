//! Webview ↔ Rust 拖放 URL bridge
//!
//! 桌面端 Dioxus 0.7 通过 `document::eval()` 注入一次性 JS，绑定 `window.drop`，
//! 抓取外部浏览器拖入的 text/uri-list，回推到 Rust 写入 `state.url`。
//! 实现 HANDOFF §明日继续 #5 的解锁（⌛ 平台限制）。

use dioxus::prelude::*;
use tracing::warn;

use super::types::AppState;

/// 注入到 webview 的脚本（IIFE，幂等；macOS WKWebView 默认支持外部浏览器 drop）
const BRIDGE_JS: &str = r#"
(function () {
    if (window.__dioxus_drag_drop_installed) return;
    window.__dioxus_drag_drop_installed = true;

    // 默认阻止 dragover，否则浏览器打开拖入的 URL 而不是 drop 给我们
    window.addEventListener('dragover', function (e) {
        e.preventDefault();
    }, false);

    // drop 拦截：从 dataTransfer 读 'text/uri-list' (URL) / 'text/plain' / 'text/html' 兜底
    window.addEventListener('drop', function (e) {
        e.preventDefault();
        const dt = e.dataTransfer;
        if (!dt) return;
        const url = dt.getData('text/uri-list')
                 || dt.getData('text/plain')
                 || dt.getData('text/html');
        if (url && url.trim().length > 0) {
            dioxus.send(url.trim());
        }
    }, false);
})();
"#;

/// 在 AppComponent mount 时调用一次；后台 task 长寿命监听 JS 推 URL，
/// 直接写入 Signal（dioxus::spawn 在 dioxus runtime 跑，无需 Send 约束）。
/// 错误（Eval 任务关闭 / 序列化失败）打 warn 日志并退出循环，避免静默吞错。
pub(super) fn install_drag_drop_bridge(mut state: Store<AppState>) {
    spawn(async move {
        let mut eval = document::eval(BRIDGE_JS);
        loop {
            match eval.recv::<String>().await {
                Ok(url) => {
                    state.write().url = url;
                    state.write().status_message = "拖放 URL 已捕获".to_string();
                }
                Err(e) => {
                    warn!(
                        "[bridge] drag-drop URL receive failed (Eval task likely closed): {}",
                        e
                    );
                    break;
                }
            }
        }
    });
}
