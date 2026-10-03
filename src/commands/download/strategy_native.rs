//! Strategy: 智学云平台原生下载（reqwest + Authorization header）
//!
//! **P0-2 修复契约**：token 必须通过 HTTP header 传递，绝不能进入
//! `tokio::process::Command` 的 argv（否则 `ps aux` 会泄漏）。

use std::time::Instant;

use anyhow::Result;
use tracing::info;

use crate::cli::Cli;
use crate::commands::path;
use crate::http_client;
use crate::types::{sanitize_filename, DownloadResult, VideoInfo, DEFAULT_USER_AGENT};

/// 使用原生 reqwest 下载（智学云等需要自定义 Authorization 头的平台）
pub async fn download(info: &VideoInfo, cli: &Cli, _start_time: Instant) -> Result<DownloadResult> {
    // 确定输出路径
    let output_dir = if cli.output.as_os_str() == "." {
        path::get_default_output_dir()
    } else {
        cli.output.clone()
    };
    let output_dir = path::validate_output_dir(&output_dir)?;
    tokio::fs::create_dir_all(&output_dir).await?;

    // 确定文件名
    let filename = if let Some(ref name) = cli.filename {
        sanitize_filename(name)
    } else {
        sanitize_filename(&info.title)
    };

    let output_path = output_dir.join(format!("{}.mp4", filename));

    // 获取 token
    let token = std::env::var("ZHIXUEYUN_TOKEN")
        .or_else(|_| {
            Ok::<_, ()>(
                crate::platform::zhixueyun_token::load_zhixueyun_cookies().unwrap_or_default(),
            )
        })
        .unwrap_or_default();

    let url = &info.url;
    let user_agent = cli
        .user_agent
        .clone()
        .unwrap_or_else(|| DEFAULT_USER_AGENT.to_string());
    let auth_header = format_auth_header(&token);

    let client = (*http_client::shared()).clone();
    let response = client
        .get(url)
        .header(reqwest::header::USER_AGENT, user_agent)
        .header(reqwest::header::AUTHORIZATION, auth_header)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let err = format!("HTTP {}", status);
        info!("下载失败: {}", err);
        println!("\n下载失败: {}", err);
        return Ok(DownloadResult::failure(output_path, err));
    }

    let bytes = response.bytes().await?;
    tokio::fs::write(&output_path, &bytes).await?;
    let file_size = bytes.len() as u64;

    info!("下载完成: {} ({} bytes)", output_path.display(), file_size);
    println!(
        "\n下载完成: {} ({} bytes)",
        output_path.display(),
        file_size
    );
    Ok(DownloadResult::success(output_path, file_size, 0.0))
}

/// 格式化智学云平台的 Authorization 请求头
///
/// 格式：`Authorization: Bearer__{token}`
pub(crate) fn format_auth_header(token: &str) -> String {
    format!("Authorization: Bearer__{token}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// P0-2 合约:token 必须出现在 Authorization header 里(Bearer__ 前缀)
    /// — 防退化:有人改格式导致 token 不走 header → 改去 argv(泄漏)
    #[test]
    fn auth_header_contains_token_with_bearer_prefix() {
        let h = format_auth_header("tok_abc123");
        assert!(h.starts_with("Authorization: Bearer__"), "实得: {h}");
        assert!(h.ends_with("tok_abc123"), "实得: {h}");
    }

    /// 空 token 也必须按格式输出(让上层去判断 401,不静默丢)
    #[test]
    fn auth_header_empty_token_still_well_formed() {
        let h = format_auth_header("");
        assert_eq!(h, "Authorization: Bearer__");
    }
}
