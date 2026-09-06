//! Dioxus 0.7 use_action 包装层
//!
//! 替代手写 `spawn(async move { ... })` + `is_fetching`/`error_display` 状态管理。
//! Action::call 自动取消上一 in-flight（latest-wins），适合用户多次快速提交 URL。
//! use_effect 监听 fetch_action.value() 变化并写回 AppState 供 RSX 消费。

use dioxus::prelude::*;

use super::handlers::{fetch_course_collection_impl, fetch_smart_impl, fetch_video_info_impl};
use super::types::{
    AppState, BatchVideoState, CourseCollectionState, ErrorDisplay, FetchResult, VideoInfoDisplay,
};

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
            return Err(anyhow::anyhow!(
                "请输入有效的 URL（提示：请输入视频链接后重试）"
            ));
        }

        if url_list.len() == 1 {
            match fetch_smart_impl(&url_list[0]).await {
                Ok(result) => Ok(result),
                Err(err) => Err(anyhow::anyhow!(
                    "URL 解析失败：{}（提示：URL 无效或平台不支持）",
                    err
                )),
            }
        } else {
            let mut results: Vec<VideoInfoDisplay> = Vec::new();
            for url in &url_list {
                match fetch_video_info_impl(url).await {
                    Ok(info) => results.push(info),
                    Err(e) => {
                        return Err(anyhow::anyhow!(
                            "批量 URL 解析失败：{}（提示：检查 URL 列表）",
                            e
                        ));
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
                        state.write().batch_video_state = BatchVideoState {
                            visible: true,
                            items,
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

/// 课程池分析 action：输入课程页 URL，输出 CourseCollectionState。
/// 解析失败时回退到 `CourseCollectionState::default()`（visible=false），
/// 并把"需要登录"等错误显示在 ErrorPanel 中。
pub(super) fn use_fetch_course_action() -> Action<(String,), CourseCollectionState> {
    use_action(move |url: String| async move {
        let url = url.trim().to_string();
        if url.is_empty() {
            return Err(anyhow::anyhow!("请输入课程页 URL"));
        }
        match fetch_course_collection_impl(&url).await {
            Ok(s) => Ok(s),
            Err(e) => {
                tracing::warn!("课程池分析失败：{e}");
                // 区分登录缺失 vs 其它错误
                if e.contains("需要登录") || e.contains("AuthenticationRequired") {
                    Err(anyhow::anyhow!("🔑 需要登录：{e}"))
                } else {
                    Err(anyhow::anyhow!("📚 课程池失败：{e}"))
                }
            }
        }
    })
}

/// 把课程池分析结果同步到 AppState：成功后切换 course_collection_state。
/// 失败时若不是登录错误，自动回退到 fetch_action 让用户拿到单视频信息。
pub(super) fn sync_course_action_to_state(
    mut state: Store<AppState>,
    mut fetch_action: Action<(String,), FetchResult>,
    course_action: Action<(String,), CourseCollectionState>,
) {
    use_effect(move || {
        state.write().is_fetching = course_action.pending();
        match course_action.value() {
            Some(Ok(sig)) => {
                let s = sig.cloned();
                if s.visible {
                    state.write().course_collection_state = s.clone();
                    state.write().status_message = "课程目录已加载".to_string();
                    state.write().error_display = None;
                }
            }
            Some(Err(captured)) => {
                let msg = format!("{captured}");
                let (error_type, suggestion) = if msg.starts_with("🔑")
                    || msg.contains("Code:2506")
                    || msg.contains("当前用户不存在")
                {
                    (
                        "login",
                        "请在小鹅通网站登录账号后，点击“打开登录窗口”复用登录态。",
                    )
                } else {
                    (
                        "course",
                        "该 URL 不属于小鹅通课程，或网络问题。请检查 URL 后用主流程下载单视频。",
                    )
                };
                state.write().error_display = Some(ErrorDisplay::new(
                    error_type,
                    msg.trim_start_matches(|c: char| {
                        !c.is_ascii_alphanumeric() && c != '：' && c != ':'
                    })
                    .to_string(),
                    suggestion.to_string(),
                ));
                state.write().status_message = "课程池分析失败".to_string();
                // 非登录错误 → 自动回退单视频流程(原 trigger_course 仅做课程分析,这里补全回退)
                if error_type == "course" {
                    let url = state.read().url.clone();
                    if !url.is_empty() {
                        fetch_action.call(url);
                    }
                }
            }
            None => {}
        }
    });
}
