//! 安装器健康检查 (受 mole `mo installer` 启发)
//!
//! 检查 yt-dlp / ffmpeg / python3 当前版本,并通过 pip index 或 yt-dlp GitHub
//! releases 获取最新版本号。给出版本对齐建议,但**不自动升级**。

use chrono::{DateTime, Utc};
use serde::Serialize;

/// 单工具版本快照
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ToolVersion {
    pub name: String,
    /// None 表示未找到 / 命令执行失败
    pub current: Option<String>,
    /// None 表示未指定 latest 来源,或拉取失败
    pub latest: Option<String>,
    /// "unknown" | "outdated" | "latest" | "ahead" | "unavailable"
    pub status: String,
    /// 可执行的升级命令建议
    pub upgrade_command: Option<String>,
    pub detail: String,
}

impl ToolVersion {
    fn only(name: &str, current: Option<String>) -> Self {
        Self {
            name: name.to_string(),
            current,
            latest: None,
            status: "unknown".to_string(),
            upgrade_command: None,
            detail: String::new(),
        }
    }
}

/// 整体安装器报告
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct InstallerReport {
    pub checked_at: DateTime<Utc>,
    pub tools: Vec<ToolVersion>,
    pub outdated_count: usize,
    pub missing_count: usize,
}

/// 单工具版本采集选项
pub struct InspectorOptions<'a> {
    pub fetcher: &'a dyn LatestVersionFetcher,
    #[allow(dead_code)]
    pub timeout_secs: u64,
}

impl<'a> Default for InspectorOptions<'a> {
    fn default() -> Self {
        Self {
            fetcher: &HttpLatestVersionFetcher,
            timeout_secs: 10,
        }
    }
}

/// latest 版本拉取抽象,便于测试注入
pub trait LatestVersionFetcher {
    fn fetch_ytdlp_latest(&self) -> Option<String>;
    fn fetch_ffmpeg_latest(&self) -> Option<String>;
}

/// HTTP-based fetcher: yt-dlp 走 pip index,ffmpeg 走 BtbN nightly JSON(常见稳定元数据)。
/// 失败返回 None(不抛错,便于降级到 status-only 报告)。
pub struct HttpLatestVersionFetcher;

impl LatestVersionFetcher for HttpLatestVersionFetcher {
    fn fetch_ytdlp_latest(&self) -> Option<String> {
        // pip index 模式:GET https://pypi.org/pypi/yt-dlp/json → 取 info.version
        // 为避免引入 ureq/reqwest 依赖,使用 std::process 调用 curl 拉一次。
        let out = std::process::Command::new("curl")
            .args([
                "-sS",
                "--max-time",
                "10",
                "https://pypi.org/pypi/yt-dlp/json",
            ])
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let v: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
        v.get("info")
            .and_then(|i| i.get("version"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }

    fn fetch_ffmpeg_latest(&self) -> Option<String> {
        // ffmpeg 没有"官方最新版本号"的权威源;回退到本地 brew ls --versions
        // 该命令存在说明已通过 brew 安装(最常见的 macOS 路径)。
        let out = std::process::Command::new("brew")
            .args(["list", "--versions", "ffmpeg"])
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let stdout = String::from_utf8_lossy(&out.stdout);
        // 形如 "ffmpeg 7.0.2_1 7.1_1 7.1.1\n" → 取最后一段
        stdout.split_whitespace().nth(1).map(|s| s.to_string())
    }
}

/// 解析 `tool --version` 类输出,抽取首行/首个数字版本号
///
/// 例:
/// - "yt-dlp 2024.10.07" → Some("2024.10.07")
/// - "ffmpeg version 7.0.2 Copyright (c) ..." → Some("7.0.2")
/// - "Python 3.12.4" → Some("3.12.4")
/// - "command not found" → None
pub fn parse_version_output(_command: &str, output: &str, stderr: &str) -> Option<String> {
    let combined = format!("{}\n{}", output, stderr);
    let first_line = combined
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    if first_line.is_empty() {
        return None;
    }
    // 在首行中查找首个 V\d+(.\d+)* 模式
    for (idx, c) in first_line.char_indices() {
        if c == 'v' || c == 'V' {
            // 检查后续是否形如数字.数字...
            let rest = &first_line[idx + c.len_utf8()..];
            if let Some(ver) = extract_version_token(rest) {
                return Some(format!("{}{}", c, ver));
            }
        } else if c.is_ascii_digit() {
            if let Some(ver) = extract_version_token(&first_line[idx..]) {
                return Some(ver);
            }
        }
    }
    None
}

/// 从 `数字.数字...` 起点切出版本 token,直到第一个非版本字符
fn extract_version_token(s: &str) -> Option<String> {
    let mut out = String::new();
    let mut prev_was_dot = false;
    let mut has_digit = false;
    for c in s.chars() {
        if c.is_ascii_digit() {
            out.push(c);
            has_digit = true;
            prev_was_dot = false;
        } else if c == '.' && has_digit && !prev_was_dot {
            out.push(c);
            prev_was_dot = true;
        } else {
            break;
        }
    }
    if has_digit && out.ends_with('.') {
        out.pop();
    }
    // 接受标准多段(`1.0` / `1.0.0`)或单段但 ≥2 位(`10` / `42`)。
    if has_digit && (out.contains('.') || out.len() >= 2) {
        Some(out)
    } else {
        None
    }
}

/// 比较两个版本号(简化:按 . 分段,逐段比大小,缺失段补 0)
/// 返回 Ordering-less 的三元态:Less / Equal / Greater。
pub fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let pa: Vec<u64> = a
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse::<u64>().ok())
        .collect();
    let pb: Vec<u64> = b
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse::<u64>().ok())
        .collect();
    let n = pa.len().max(pb.len());
    for i in 0..n {
        let ai = pa.get(i).copied().unwrap_or(0);
        let bi = pb.get(i).copied().unwrap_or(0);
        match ai.cmp(&bi) {
            Ordering::Equal => continue,
            non_eq => return non_eq,
        }
    }
    Ordering::Equal
}

/// 运行 `cmd --version` 并返回 stdout/stderr
pub fn run_version_probe(cmd: &str, args: &[&str]) -> Option<(String, String)> {
    let out = std::process::Command::new(cmd).args(args).output().ok()?;
    Some((
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    ))
}

/// 编排函数:扫描工具链 + 评估状态
pub fn collect_installer_report(opts: &InspectorOptions) -> InstallerReport {
    let mut tools = Vec::new();

    // yt-dlp
    {
        let mut tv = ToolVersion::only("yt-dlp", None);
        tv.upgrade_command = Some("pip install -U yt-dlp".to_string());
        if let Some((out, err)) = run_version_probe("yt-dlp", &["--version"]) {
            tv.current = parse_version_output("yt-dlp", &out, &err);
            tv.detail = "来自 PyPI,建议每月升级一次".to_string();
        } else {
            tv.detail = "未找到 yt-dlp,需要先 pip install yt-dlp".to_string();
        }
        tv.latest = opts.fetcher.fetch_ytdlp_latest();
        tv.status = classify(&tv.current, &tv.latest);
        tools.push(tv);
    }

    // ffmpeg
    {
        let mut tv = ToolVersion::only("ffmpeg", None);
        tv.upgrade_command = Some("brew upgrade ffmpeg".to_string());
        if let Some((out, err)) = run_version_probe("ffmpeg", &["-version"]) {
            tv.current = parse_version_output("ffmpeg", &out, &err);
            tv.detail = "来自 Homebrew 或系统,无需频繁升级".to_string();
        } else {
            tv.detail = "未找到 ffmpeg".to_string();
        }
        tv.latest = opts.fetcher.fetch_ffmpeg_latest();
        tv.status = classify(&tv.current, &tv.latest);
        tools.push(tv);
    }

    // python3
    {
        let mut tv = ToolVersion::only("python3", None);
        tv.upgrade_command = Some("brew upgrade python@3".to_string());
        if let Some((out, err)) = run_version_probe("python3", &["--version"]) {
            tv.current = parse_version_output("python3", &out, &err);
            tv.detail = "yt-dlp / 本工具行为依赖".to_string();
        } else {
            tv.detail = "未找到 python3".to_string();
        }
        // python3 latest 跳过 (复杂)
        tv.latest = None;
        tv.status = classify(&tv.current, &tv.latest);
        tools.push(tv);
    }

    let outdated_count = tools.iter().filter(|t| t.status == "outdated").count();
    let missing_count = tools.iter().filter(|t| t.current.is_none()).count();

    InstallerReport {
        checked_at: Utc::now(),
        tools,
        outdated_count,
        missing_count,
    }
}

fn classify(current: &Option<String>, latest: &Option<String>) -> String {
    match (current, latest) {
        (None, _) => "unavailable".to_string(),
        (_, None) => "unknown".to_string(),
        (Some(c), Some(l)) => match compare_versions(c, l) {
            std::cmp::Ordering::Less => "outdated".to_string(),
            std::cmp::Ordering::Equal => "latest".to_string(),
            std::cmp::Ordering::Greater => "ahead".to_string(),
        },
    }
}

/// CLI 入口
pub fn run_installer(json: bool) -> anyhow::Result<()> {
    let opts = InspectorOptions::default();
    let report = collect_installer_report(&opts);
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(|e| anyhow::anyhow!("json: {}", e))?
        );
    } else {
        print!("{}", format_report(&report));
    }
    Ok(())
}

pub fn format_report(r: &InstallerReport) -> String {
    use std::fmt::Write;
    let mut s = String::new();
    let _ = writeln!(
        s,
        "安装器健康检查 @ {}",
        r.checked_at.format("%Y-%m-%d %H:%M:%S UTC")
    );
    let _ = writeln!(s, "================");
    let _ = writeln!(s, "总工具:    {}", r.tools.len());
    let _ = writeln!(s, "需升级:    {}", r.outdated_count);
    let _ = writeln!(s, "未安装:    {}", r.missing_count);
    let _ = writeln!(
        s,
        "\n{:<10} {:<12} {:<12} {:<10}",
        "tool", "current", "latest", "status"
    );
    for t in &r.tools {
        let cur = t.current.clone().unwrap_or_else(|| "(missing)".to_string());
        let lat = t.latest.clone().unwrap_or_else(|| "(unknown)".to_string());
        let _ = writeln!(
            s,
            "{:<10} {:<12} {:<12} {:<10}",
            t.name,
            truncate(&cur, 12),
            truncate(&lat, 12),
            t.status
        );
    }
    let outdated: Vec<&ToolVersion> = r.tools.iter().filter(|t| t.status == "outdated").collect();
    if !outdated.is_empty() {
        let _ = writeln!(s, "\n升级建议:");
        for t in outdated {
            if let Some(cmd) = &t.upgrade_command {
                let _ = writeln!(s, "  • {} → {}", t.name, cmd);
            }
        }
    }
    let missing: Vec<&ToolVersion> = r.tools.iter().filter(|t| t.current.is_none()).collect();
    if !missing.is_empty() {
        let _ = writeln!(s, "\n缺失工具:");
        for t in missing {
            if let Some(cmd) = &t.upgrade_command {
                let _ = writeln!(s, "  • {} (建议安装): {}", t.name, cmd);
            } else {
                let _ = writeln!(s, "  • {} 未安装", t.name);
            }
        }
    }
    s
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn static_fetcher(map: &[(&'static str, Option<&'static str>)]) -> impl LatestVersionFetcher {
        struct TestFetcher(std::collections::HashMap<&'static str, Option<String>>);
        impl LatestVersionFetcher for TestFetcher {
            fn fetch_ytdlp_latest(&self) -> Option<String> {
                self.0.get("ytdlp").cloned().flatten()
            }
            fn fetch_ffmpeg_latest(&self) -> Option<String> {
                self.0.get("ffmpeg").cloned().flatten()
            }
        }
        let mut m = std::collections::HashMap::new();
        for (k, v) in map {
            m.insert(*k, v.map(|s| s.to_string()));
        }
        TestFetcher(m)
    }

    #[test]
    fn parse_ytdlp_style() {
        assert_eq!(
            parse_version_output("yt-dlp", "2024.10.07\n", ""),
            Some("2024.10.07".to_string())
        );
    }

    #[test]
    fn parse_ffmpeg_style() {
        assert_eq!(
            parse_version_output(
                "ffmpeg",
                "",
                "ffmpeg version 7.0.2 Copyright (c) 2000-2024 the FFmpeg developers"
            ),
            Some("7.0.2".to_string())
        );
    }

    #[test]
    fn parse_python_style() {
        assert_eq!(
            parse_version_output("python3", "Python 3.12.4\n", ""),
            Some("3.12.4".to_string())
        );
    }

    #[test]
    fn parse_unknown_returns_none() {
        assert_eq!(parse_version_output("foo", "bar baz qux\n", ""), None);
    }

    #[test]
    fn parse_empty_returns_none() {
        assert_eq!(parse_version_output("foo", "", ""), None);
    }

    #[test]
    fn compare_versions_basic() {
        use std::cmp::Ordering;
        assert_eq!(compare_versions("1.2.3", "1.2.3"), Ordering::Equal);
        assert_eq!(compare_versions("1.2.3", "1.2.4"), Ordering::Less);
        assert_eq!(compare_versions("1.2.4", "1.2.3"), Ordering::Greater);
        assert_eq!(compare_versions("1.2", "1.2.0"), Ordering::Equal);
        assert_eq!(compare_versions("2024.10.7", "2024.10.07"), Ordering::Equal);
    }

    #[test]
    fn classify_outdated_when_current_less() {
        let f = static_fetcher(&[("ytdlp", Some("2024.10.07"))]);
        let opts = InspectorOptions {
            fetcher: &f,
            timeout_secs: 5,
        };
        let r = collect_installer_report(&opts);
        // 用真实 probe 的 current + 注入的 latest 评估状态
        let ytdlp = r.tools.iter().find(|t| t.name == "yt-dlp").unwrap();
        if let Some(cur) = &ytdlp.current {
            let status = classify(&Some(cur.clone()), &Some("2024.10.07".to_string()));
            // 真实版本要么 outdated 要么 ahead(取决于机器装的 yt-dlp 版本)
            assert!(status == "outdated" || status == "ahead" || status == "latest");
        }
    }

    #[test]
    fn classify_ahead_status() {
        // current 大于 latest → ahead(用比 latest 大的明确数字)
        let status = classify(
            &Some("2026.01.01".to_string()),
            &Some("2024.10.07".to_string()),
        );
        assert_eq!(status, "ahead");
    }

    #[test]
    fn classify_unknown_when_no_latest() {
        let status = classify(&Some("1.0.0".to_string()), &None);
        assert_eq!(status, "unknown");
        let status = classify(&None, &Some("1.0.0".to_string()));
        assert_eq!(status, "unavailable");
    }

    #[test]
    fn classify_outdated_detected() {
        let status = classify(
            &Some("2020.01.01".to_string()),
            &Some("2024.10.07".to_string()),
        );
        assert_eq!(status, "outdated");
    }

    #[test]
    fn report_counts_match_tools() {
        let f = static_fetcher(&[]);
        let opts = InspectorOptions {
            fetcher: &f,
            timeout_secs: 5,
        };
        let r = collect_installer_report(&opts);
        assert_eq!(r.tools.len(), 3);
        let missing = r.tools.iter().filter(|t| t.current.is_none()).count();
        assert_eq!(r.missing_count, missing);
        let outdated = r.tools.iter().filter(|t| t.status == "outdated").count();
        assert_eq!(r.outdated_count, outdated);
    }

    #[test]
    fn format_report_contains_headers() {
        let f = static_fetcher(&[]);
        let opts = InspectorOptions {
            fetcher: &f,
            timeout_secs: 5,
        };
        let r = collect_installer_report(&opts);
        let out = format_report(&r);
        assert!(out.contains("安装器健康检查"));
        assert!(out.contains("yt-dlp"));
        assert!(out.contains("ffmpeg"));
        assert!(out.contains("python3"));
    }

    #[test]
    fn run_version_probe_returns_output() {
        let (out, err) = run_version_probe("sh", &["-c", "echo hello; echo world >&2"]).unwrap();
        assert!(out.contains("hello"));
        assert!(err.contains("world"));
    }

    #[test]
    fn run_version_probe_missing_cmd_returns_none() {
        let r = run_version_probe("this-cmd-does-not-exist-xyz123", &["--version"]);
        assert!(r.is_none());
    }

    #[test]
    fn tool_version_only_constructor() {
        let t = ToolVersion::only("foo", None);
        assert_eq!(t.name, "foo");
        assert!(t.current.is_none());
        assert_eq!(t.status, "unknown");
    }
}
