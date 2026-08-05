//! 单视频信息卡片 — 提取自原 app.rs:562-609
//!
//! 抓取成功后展示 title / uploader / duration / 画质 select。
//! 渲染条件由 AppComponent 控制 (state.read().has_video_info),
//! 本组件内部不重复 guard。

use dioxus::prelude::*;

use super::components::{CardShadow, WhiteCard};
use super::theme::{BORDER_DEFAULT, TEXT_MUTED, TEXT_PRIMARY};
use super::types::{AppState, VideoInfoDisplay};

/// 单视频信息卡片
#[component]
pub(super) fn VideoInfoCard(
    state: Store<AppState>,
    video_info: Signal<VideoInfoDisplay>,
) -> Element {
    rsx! {
        WhiteCard {
            shadow: CardShadow::Soft,
            div {
                style: "display: flex; align-items: center; gap: 10px; margin-bottom: 14px; padding-bottom: 14px; border-bottom: 1px solid #eee;",
                div { style: "font-size: 24px;", "🎬" }
                div {
                    style: "font-size: 16px; font-weight: 700; color: {TEXT_PRIMARY}; flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;",
                    "{video_info.read().title}"
                }
            }
            div {
                style: "display: grid; grid-template-columns: repeat(3, 1fr); gap: 12px;",
                div {
                    style: "{state.read().theme.bg_card_subtle()} text-align: center; padding: 14px 8px;",
                    div { style: "font-size: 11px; color: {TEXT_MUTED}; margin-bottom: 6px;", "👤 上传者" }
                    div { style: "font-size: 12px; font-weight: 600; color: {TEXT_PRIMARY}; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;", "{video_info.read().uploader}" }
                }
                div {
                    style: "{state.read().theme.bg_card_subtle()} text-align: center; padding: 14px 8px;",
                    div { style: "font-size: 11px; color: {TEXT_MUTED}; margin-bottom: 6px;", "⏱️ 时长" }
                    div { style: "font-size: 12px; font-weight: 600; color: {TEXT_PRIMARY};", "{video_info.read().duration}" }
                }
                div {
                    style: "{state.read().theme.bg_card_subtle()} text-align: center; padding: 14px 8px;",
                    div { style: "font-size: 11px; color: {TEXT_MUTED}; margin-bottom: 6px;", "📁 画质" }
                    div {
                        style: "font-size: 12px; font-weight: 600; color: {TEXT_PRIMARY};",
                        select {
                            style: "width: 100%; padding: 6px 8px; border-radius: 8px; border: 1px solid {BORDER_DEFAULT}; background: white; font-size: 12px; cursor: pointer;",
                            onchange: move |evt| {
                                let format_id = evt.value();
                                if !format_id.is_empty() {
                                    state.write().selected_format_id = Some(format_id);
                                }
                            },
                            option { value: "", "默认最佳" }
                            for format in video_info.read().formats.iter() {
                                option { value: "{format.format_id}", "{format.resolution} ({format.ext}) - {format.filesize}" }
                            }
                        }
                    }
                }
            }
        }
    }
}
