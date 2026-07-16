//! Dioxus 0.7 use_action 包装层
//!
//! 替代手写 `spawn(async move { ... })` + `is_fetching`/`error_display` 状态管理。
//! Action::call 自动取消上一 in-flight（latest-wins），适合用户多次快速提交 URL。
//! use_effect 监听 fetch_action.value() 变化并写回 AppState 供 RSX 消费。

use dioxus::prelude::*;

use super::handlers::fetch_video_info_impl;
use super::types::{AppState, BatchVideoState, ErrorDisplay, FetchResult, VideoInfoDisplay};

/// anyhow::Error 自动 impl Into<CapturedError>，所以 action 用 anyhow::Error 作 E
/// 创建 fetch action：输入 URL 文本（空格/逗号分隔支持批量），输出 FetchResult
/// 错误类型用 anyhow::Error（auto impl Into<CapturedError>），sync 时再转为 ErrorDisplay
pub(super) fn use_fetch_action() -> Action<(String,), FetchResult> {
    use_action(move |url_text: String| async move {
        let url_list: Vec<String> = url_text
            .split([' ', ','])
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        if url_list.is_empty() {
            return Err(anyhow::anyhow!("请输入有效的 URL（提示：请输入视频链接后重试）"));
        }

        if url_list.len() == 1 {
            match fetch_video_info_impl(&url_list[0]).await {
                Ok(info) => Ok(FetchResult::Single(info)),
                Err(err) => Err(anyhow::anyhow!("URL 解析失败：{}（提示：URL 无效或平台不支持）", err)),
            }
        } else {
            let mut results: Vec<VideoInfoDisplay> = Vec::new();
            for url in &url_list {
                match fetch_video_info_impl(url).await {
                    Ok(info) => results.push(info),
                    Err(e) => {
                        return Err(anyhow::anyhow!("批量 URL 解析失败：{}（提示：检查 URL 列表）", e));
                    }
                }
            }
            Ok(FetchResult::Batch(results))
        }
    })
}

/// 把 fetch action 的 value 同步到 AppState + video_info
/// 替代手写 70 行 submit_urls 闭包
pub(super) fn sync_fetch_action_to_state(
    mut state: Store<AppState>,
    mut video_info: Signal<VideoInfoDisplay>,
    fetch_action: Action<(String,), FetchResult>,
) {
    use_effect(move || {
        state.write().is_fetching = fetch_action.pending();

        match fetch_action.value() {
            Some(Ok(sig)) => {
                // sig: ReadSignal<FetchResult>（外层 Result 是 action error wrap）
                match sig.cloned() {
                    FetchResult::Single(info) => {
                        let mut vi = video_info.write();
                        vi.url = info.url.clone();
                        vi.original_url = info.original_url.clone();
                        vi.title = info.title.clone();
                        vi.uploader = info.uploader.clone();
                        vi.duration = info.duration.clone();
                        vi.formats = info.formats.clone();
                        vi.download_strategy = info.download_strategy;
                        drop(vi);
                        state.write().has_video_info = true;
                        state.write().batch_video_state = BatchVideoState::default();
                        state.write().status_message = "Video info fetched".to_string();
                        state.write().error_display = None;
                    }
                    FetchResult::Batch(items) => {
                        let count = items.len();
                        state.write().batch_video_state = BatchVideoState {
                            visible: true,
                            items,
                            status: format!("获取了 {} 个视频信息", count),
                        };
                        state.write().has_video_info = false;
                        state.write().status_message = "Video info fetched".to_string();
                        state.write().error_display = None;
                    }
                }
            }
            Some(Err(captured)) => {
                // anyhow::Error 转 ErrorDisplay（简单 message 提取）
                state.write().is_fetching = false;
                state.write().error_display = Some(ErrorDisplay::new(
                    "parse",
                    format!("{}", captured),
                    "URL 无效或平台不支持",
                ));
                state.write().status_message = "Failed to fetch video info".to_string();
            }
            None => {}
        }
    });
}