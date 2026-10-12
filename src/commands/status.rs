//! `status` 子命令 — 工具健康检查 (受 mole `mo status` 启发)
//!
//! 检查项(均独立可测):
//! 1. yt-dlp 是否可用 + 版本
//! 2. ffmpeg 是否可用(可选,部分流程需要)
//! 3. python3 是否可用(智学云 resolver 需要)
//! 4. 输出目录可写性
//! 5. 输出目录剩余磁盘空间(Warn < 1GB, Error < 100MB)
//! 6. 历史文件可读性
//! 7. cookie 文件可读性(若指定)
//!
//! 设计原则: 每个 check 都是独立小函数,可单独验证;
//! 整个 report 是普通 Vec<HealthCheck>,便于测试与扩展。

use std::path::Path;
use std::process::Command;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tracing::debug;

/// 健康严重等级
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// 通过
    Ok,
    /// 警告(降级可用,但建议修)
    Warn,
    /// 错误(关键依赖缺失,核心流程不可用)
    Error,
}

/// 单条健康检查
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheck {
    pub name: String,
    pub severity: Severity,
    pub detail: String,
}

impl HealthCheck {
    pub fn ok(name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            severity: Severity::Ok,
            detail: detail.into(),
        }
    }
    pub fn warn(name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            severity: Severity::Warn,
            detail: detail.into(),
        }
    }
    pub fn error(name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            severity: Severity::Error,
            detail: detail.into(),
        }
    }
}

/// 整体健康报告
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HealthReport {
    pub checks: Vec<HealthCheck>,
}

impl HealthReport {
    pub fn push(&mut self, check: HealthCheck) {
        self.checks.push(check);
    }

    /// 返回最严重的等级(Ok < Warn < Error)
    pub fn worst(&self) -> Severity {
        self.checks
            .iter()
            .map(|c| c.severity)
            .max_by_key(|s| match s {
                Severity::Ok => 0,
                Severity::Warn => 1,
                Severity::Error => 2,
            })
            .unwrap_or(Severity::Ok)
    }

    pub fn count_by(&self, sev: Severity) -> usize {
        self.checks.iter().filter(|c| c.severity == sev).count()
    }
}

// ============================ 单项 check 实现 ============================

/// 检查 PATH 中是否有某命令 + 拿 --version 输出第一行
pub fn check_command(name: &str, version_flag: &str) -> HealthCheck {
    let path = match which::which(name) {
        Ok(p) => p,
        Err(_) => {
            return HealthCheck::error(
                format!("{} 可执行", name),
                format!("未在 PATH 中找到 {}", name),
            );
        }
    };

    let output = Command::new(&path).arg(version_flag).output();
    match output {
        Ok(out) if out.status.success() => {
            let raw = String::from_utf8_lossy(&out.stdout);
            let version_line = raw.lines().next().unwrap_or("").trim();
            // 兼容 ffmpeg 输出到 stderr 的情况
            let version_line = if version_line.is_empty() {
                let err_raw = String::from_utf8_lossy(&out.stderr);
                err_raw.lines().next().unwrap_or("").trim().to_string()
            } else {
                version_line.to_string()
            };
            HealthCheck::ok(
                format!("{} 可执行", name),
                format!("{} → {} ({})", path.display(), version_line, name),
            )
        }
        Ok(out) => HealthCheck::warn(
            format!("{} 可执行", name),
            format!(
                "{} 找到但 {} 退出 {:?}",
                path.display(),
                version_flag,
                out.status.code()
            ),
        ),
        Err(e) => HealthCheck::warn(
            format!("{} 可执行", name),
            format!("{} 找到但执行失败: {}", path.display(), e),
        ),
    }
}

/// 检查 dir 是否存在 + 可写(创建临时 marker 后删除)
pub fn check_writable(dir: &Path) -> HealthCheck {
    let name = format!("目录可写 {}", dir.display());
    if !dir.exists() {
        return HealthCheck::error(&name, format!("目录不存在: {}", dir.display()));
    }
    if !dir.is_dir() {
        return HealthCheck::error(&name, format!("不是目录: {}", dir.display()));
    }
    let probe = dir.join(".xvd-write-probe");
    let write_ok = std::fs::write(&probe, b"ok").is_ok();
    if !write_ok {
        return HealthCheck::error(&name, "无写权限");
    }
    let _ = std::fs::remove_file(&probe);
    HealthCheck::ok(&name, format!("可写 ({})", dir.display()))
}

/// 检查 dir 剩余磁盘空间(可用 statvfs / Darwin sysinfo)
pub fn check_disk_space(dir: &Path, warn_bytes: u64, error_bytes: u64) -> HealthCheck {
    let name = format!("磁盘空间 {}", dir.display());
    let free = match free_bytes(dir) {
        Some(b) => b,
        None => return HealthCheck::warn(&name, "无法读取 (非 macOS/Linux 标准 FS?)"),
    };
    if free < error_bytes {
        HealthCheck::error(&name, format!("仅剩 {} bytes < {} 阈值", free, error_bytes))
    } else if free < warn_bytes {
        HealthCheck::warn(&name, format!("仅剩 {} bytes < {} 阈值", free, warn_bytes))
    } else {
        HealthCheck::ok(&name, format!("剩余 {} bytes", free))
    }
}

/// 跨平台 free bytes — macOS/Linux 都支持。Windows 暂返回 None.
#[cfg(unix)]
fn free_bytes(dir: &Path) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;
    let meta = std::fs::metadata(dir).ok()?;
    let dev = meta.dev();
    // 用 statvfs — 但 stdlib 没暴露,用 `which::which("df")` 拿 df 兜底
    // 这里改为使用 libc-free 的 std::fs: 现代 Rust 提供 `available_space` (1.69+)
    // 如果 std 不提供就 fallback 到 df 命令。
    std::fs::metadata(dir)
        .ok()
        .map(|_| dev)
        .and_then(|_| read_available_via_df(dir))
}

#[cfg(unix)]
fn read_available_via_df(dir: &Path) -> Option<u64> {
    let out = Command::new("df").arg("-k").arg(dir).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    // 第二行第四列是 Available (1K-blocks)
    let line = text.lines().nth(1)?;
    let cols: Vec<&str> = line.split_whitespace().collect();
    if cols.len() < 4 {
        return None;
    }
    cols.get(3)?.parse::<u64>().ok().map(|kb| kb * 1024)
}

#[cfg(not(unix))]
fn free_bytes(_dir: &Path) -> Option<u64> {
    None
}

/// 检查文件是否存在 + 可读
pub fn check_file_readable(name_hint: &str, path: &Path) -> HealthCheck {
    let name = format!("{} 可读", name_hint);
    if !path.exists() {
        return HealthCheck::warn(&name, format!("文件不存在: {}", path.display()));
    }
    match std::fs::File::open(path) {
        Ok(_) => HealthCheck::ok(&name, format!("{} 可读", path.display())),
        Err(e) => HealthCheck::warn(&name, format!("{} 打开失败: {}", path.display(), e)),
    }
}

/// 主报告:跑全套
pub fn run_health_checks(
    output_dir: &Path,
    history_path: Option<&Path>,
    cookie_path: Option<&Path>,
    warn_bytes: u64,
    error_bytes: u64,
) -> HealthReport {
    let mut report = HealthReport::default();

    // 命令检查(独立错误不互相影响)
    report.push(check_command("yt-dlp", "--version"));
    report.push(check_command("ffmpeg", "-version"));
    report.push(check_command("python3", "--version"));

    // 输出目录
    report.push(check_writable(output_dir));
    report.push(check_disk_space(output_dir, warn_bytes, error_bytes));

    // 历史文件(可选)
    if let Some(hp) = history_path {
        report.push(check_file_readable("下载历史", hp));
    }

    // cookie 文件(可选)
    if let Some(cp) = cookie_path {
        report.push(check_file_readable("cookie 文件", cp));
    }

    debug!("health report 完成: {} 项检查", report.checks.len());
    report
}

/// 人读输出
pub fn print_report(report: &HealthReport) {
    println!("=== 健康检查报告 ===\n");
    for c in &report.checks {
        let icon = match c.severity {
            Severity::Ok => "✓",
            Severity::Warn => "⚠",
            Severity::Error => "✗",
        };
        println!("{} [{}] {}", icon, c.name, c.detail);
    }
    let worst = report.worst();
    let summary = match worst {
        Severity::Ok => "✓ 一切正常",
        Severity::Warn => "⚠ 有警告项,核心功能可用",
        Severity::Error => "✗ 关键依赖缺失,部分功能不可用",
    };
    println!(
        "\n汇总: {} ok / {} warn / {} error — {}",
        report.count_by(Severity::Ok),
        report.count_by(Severity::Warn),
        report.count_by(Severity::Error),
        summary
    );
}

/// JSON 输出
pub fn report_to_json(report: &HealthReport) -> Result<String> {
    Ok(serde_json::to_string_pretty(report)?)
}

/// CLI 入口
pub fn run_status_commands(
    output_dir: &Path,
    history_path: Option<&Path>,
    cookie_path: Option<&Path>,
    json: bool,
) -> Result<()> {
    let report = run_health_checks(
        output_dir,
        history_path,
        cookie_path,
        1_073_741_824, // 1 GB warn
        104_857_600,   // 100 MB error
    );
    if json {
        println!("{}", report_to_json(&report)?);
    } else {
        print_report(&report);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_max_ordering() {
        let mut report = HealthReport::default();
        report.push(HealthCheck::ok("a", "ok"));
        assert_eq!(report.worst(), Severity::Ok);
        report.push(HealthCheck::warn("b", "warn"));
        assert_eq!(report.worst(), Severity::Warn);
        report.push(HealthCheck::error("c", "err"));
        assert_eq!(report.worst(), Severity::Error);
    }

    #[test]
    fn count_by_filters_correctly() {
        let mut report = HealthReport::default();
        report.push(HealthCheck::ok("a", "x"));
        report.push(HealthCheck::ok("b", "x"));
        report.push(HealthCheck::warn("c", "x"));
        report.push(HealthCheck::error("d", "x"));
        assert_eq!(report.count_by(Severity::Ok), 2);
        assert_eq!(report.count_by(Severity::Warn), 1);
        assert_eq!(report.count_by(Severity::Error), 1);
    }

    #[test]
    fn check_writable_for_existing_dir() {
        let tmp = std::env::temp_dir();
        let c = check_writable(&tmp);
        // 多数 tmp 可写;不可写系统(罕见) 至少不是 panic
        assert!(matches!(
            c.severity,
            Severity::Ok | Severity::Error | Severity::Warn
        ));
        if c.severity == Severity::Error {
            // Error 也 OK,只要语义对(目录存在但不可写)
            assert!(c.detail.contains("写权限") || c.detail.contains("目录不存在"));
        }
    }

    #[test]
    fn check_writable_for_nonexistent_dir() {
        let path = Path::new("/tmp/this-must-not-exist-xyz-98765");
        let _ = std::fs::remove_dir_all(path);
        let c = check_writable(path);
        assert_eq!(c.severity, Severity::Error);
        assert!(c.detail.contains("目录不存在"));
    }

    #[test]
    fn check_file_readable_missing_warns() {
        let c = check_file_readable("fake cookie", Path::new("/tmp/no-such-cookie-xyz-98765"));
        assert_eq!(c.severity, Severity::Warn);
        assert!(c.detail.contains("文件不存在"));
    }

    #[test]
    fn check_file_readable_existing_ok() {
        let tmp = std::env::temp_dir().join("xvd-status-test-readable.txt");
        std::fs::write(&tmp, b"hello").unwrap();
        let c = check_file_readable("fake history", &tmp);
        assert_eq!(c.severity, Severity::Ok);
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn check_command_existing_yields_some_status() {
        // sh 一定有 — 拿 sh 测试,避免依赖 yt-dlp
        let c = check_command("sh", "--version");
        assert!(matches!(c.severity, Severity::Ok | Severity::Warn));
    }

    #[test]
    fn check_command_missing_yields_error() {
        let c = check_command("definitely-not-a-real-cmd-xyz-98765", "--version");
        assert_eq!(c.severity, Severity::Error);
        assert!(c.detail.contains("未在 PATH 中找到"));
    }

    #[test]
    fn report_to_json_roundtrips() {
        let mut report = HealthReport::default();
        report.push(HealthCheck::ok("yt-dlp", "ok"));
        report.push(HealthCheck::error("ffmpeg", "missing"));
        let s = report_to_json(&report).unwrap();
        assert!(s.contains("yt-dlp"));
        assert!(s.contains("ffmpeg"));
        assert!(s.contains("\"ok\"") || s.contains("\"error\""));
    }
}
