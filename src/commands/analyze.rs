//! `analyze` 子命令 — 下载目录分析 (受 mole `mo analyze` 启发)
//!
//! 分析维度:
//! - 总字节数 + 文件数
//! - 按扩展名分组(bytes / count / avg)
//! - 按 mtime 分桶(recent / week / month / older)
//! - Top-N largest files
//!
//! 设计原则: [`analyze`] 为 pure 函数(只读 FS),便于 unit test;
//! 输出支持人读表格与 JSON 两种格式。

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

const DAY: Duration = Duration::from_secs(86_400);
const WEEK: Duration = Duration::from_secs(7 * 86_400);
const MONTH: Duration = Duration::from_secs(30 * 86_400);

/// 单个扩展名统计
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExtStats {
    pub count: usize,
    pub bytes: u64,
}

impl ExtStats {
    pub fn avg(&self) -> u64 {
        if self.count == 0 {
            0
        } else {
            self.bytes / self.count as u64
        }
    }
}

/// 年龄分桶
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgeBuckets {
    pub recent: usize, // < 1 天
    pub week: usize,   // < 7 天 (含 recent? 不,排他)
    pub month: usize,  // < 30 天
    pub older: usize,  // >= 30 天
}

impl AgeBuckets {
    #[expect(dead_code)] // 预留 utility: 总文件数=4 个 bucket 求和,format_report 时调用
    pub fn total(&self) -> usize {
        self.recent + self.week + self.month + self.older
    }
}

/// 大文件条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopFile {
    pub path: PathBuf,
    pub size: u64,
    pub mtime_secs_ago: u64,
}

/// 整体分析报告
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AnalysisReport {
    pub total_bytes: u64,
    pub total_files: usize,
    pub by_extension: HashMap<String, ExtStats>,
    pub age_buckets: AgeBuckets,
    pub top_files: Vec<TopFile>,
}

impl AnalysisReport {
    /// 总文件数(含空)
    #[expect(dead_code)] // 预留 utility: 按扩展名 count 求和,format_report 时调用
    pub fn grand_total(&self) -> usize {
        self.by_extension.values().map(|s| s.count).sum()
    }

    /// 扩展名按字节降序
    pub fn ext_sorted_by_bytes(&self) -> Vec<(&String, &ExtStats)> {
        let mut v: Vec<_> = self.by_extension.iter().collect();
        v.sort_by_key(|(_, stats)| std::cmp::Reverse(stats.bytes));
        v
    }
}

/// 主分析函数
pub fn analyze(dir: &Path) -> Result<AnalysisReport> {
    if !dir.exists() {
        anyhow::bail!("目录不存在: {:?}", dir);
    }
    if !dir.is_dir() {
        anyhow::bail!("路径不是目录: {:?}", dir);
    }

    let mut report = AnalysisReport::default();
    let now = SystemTime::now();

    // 用 Vec 暂存 (path, size, mtime) 以便最后取 Top-N
    let mut all_files: Vec<(PathBuf, u64, Duration)> = Vec::new();

    for entry in fs::read_dir(dir).with_context(|| format!("读取目录 {:?}", dir))? {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        let size = meta.len();
        let age = meta
            .modified()
            .ok()
            .and_then(|m| now.duration_since(m).ok())
            .unwrap_or(Duration::MAX);

        // 累加 total
        report.total_bytes += size;
        report.total_files += 1;

        // 扩展名
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .map(str::to_ascii_lowercase)
            .unwrap_or_else(|| "(no-ext)".to_string());
        let stats = report.by_extension.entry(ext).or_default();
        stats.count += 1;
        stats.bytes += size;

        // 年龄分桶
        if age < DAY {
            report.age_buckets.recent += 1;
        } else if age < WEEK {
            report.age_buckets.week += 1;
        } else if age < MONTH {
            report.age_buckets.month += 1;
        } else {
            report.age_buckets.older += 1;
        }

        all_files.push((path, size, age));
    }

    // Top-10 by size
    all_files.sort_by_key(|(_, size, _)| std::cmp::Reverse(*size));
    report.top_files = all_files
        .into_iter()
        .take(10)
        .map(|(p, s, age)| TopFile {
            path: p,
            size: s,
            mtime_secs_ago: age.as_secs(),
        })
        .collect();
    Ok(report)
}

/// 人读输出
pub fn print_report(report: &AnalysisReport, dir: &Path) {
    println!("=== 下载目录分析: {} ===\n", dir.display());
    println!(
        "总文件: {} | 总大小: {} ({} bytes)\n",
        report.total_files,
        humanize_bytes(report.total_bytes),
        report.total_bytes
    );

    // 按扩展名
    println!("-- 按扩展名 --");
    let sorted = report.ext_sorted_by_bytes();
    if sorted.is_empty() {
        println!("  (无)");
    } else {
        println!(
            "  {:<14} {:>8} {:>14} {:>14}",
            "扩展名", "文件数", "字节", "平均"
        );
        for (ext, s) in sorted {
            println!(
                "  {:<14} {:>8} {:>14} {:>14}",
                ext,
                s.count,
                humanize_bytes(s.bytes),
                humanize_bytes(s.avg())
            );
        }
    }

    // 年龄分桶
    println!("\n-- 按 mtime 年龄 --");
    let ab = &report.age_buckets;
    println!(
        "  < 1 天: {} | < 7 天: {} | < 30 天: {} | ≥ 30 天: {}",
        ab.recent, ab.week, ab.month, ab.older
    );

    // Top-10
    println!("\n-- Top 10 大文件 --");
    if report.top_files.is_empty() {
        println!("  (无)");
    } else {
        for (i, f) in report.top_files.iter().enumerate() {
            println!(
                "  {:>2}. {} ({}, {:.1} 天前)",
                i + 1,
                f.path.display(),
                humanize_bytes(f.size),
                f.mtime_secs_ago as f64 / 86_400.0
            );
        }
    }
}

pub fn report_to_json(report: &AnalysisReport) -> Result<String> {
    Ok(serde_json::to_string_pretty(report)?)
}

/// 简易字节人类化(KB / MB / GB)
fn humanize_bytes(b: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;
    if b >= GB {
        format!("{:.2} GB", b as f64 / GB as f64)
    } else if b >= MB {
        format!("{:.2} MB", b as f64 / MB as f64)
    } else if b >= KB {
        format!("{:.2} KB", b as f64 / KB as f64)
    } else {
        format!("{} B", b)
    }
}

/// CLI 入口
pub fn run_analyze_commands(dir: &Path, json: bool) -> Result<()> {
    let report = analyze(dir)?;
    if json {
        println!("{}", report_to_json(&report)?);
    } else {
        print_report(&report, dir);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn tmp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("xvd-analyze-test-{}-{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_file(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
        let p = dir.join(name);
        let mut f = fs::File::create(&p).unwrap();
        f.write_all(bytes).unwrap();
        p
    }

    #[test]
    fn humanize_bytes_thresholds() {
        assert_eq!(humanize_bytes(0), "0 B");
        assert_eq!(humanize_bytes(512), "512 B");
        assert_eq!(humanize_bytes(1024), "1.00 KB");
        assert_eq!(humanize_bytes(1024 * 1024), "1.00 MB");
        assert_eq!(humanize_bytes(1024u64.pow(3)), "1.00 GB");
    }

    #[test]
    fn ext_stats_avg_zero_when_empty() {
        let s = ExtStats::default();
        assert_eq!(s.avg(), 0);
    }

    #[test]
    fn age_buckets_total_sum() {
        let b = AgeBuckets {
            recent: 1,
            week: 2,
            month: 3,
            older: 4,
        };
        assert_eq!(b.total(), 10);
    }

    #[test]
    fn analyze_nonexistent_dir_errors() {
        let res = analyze(Path::new("/tmp/no-such-analyze-xyz-98765"));
        assert!(res.is_err());
    }

    #[test]
    fn analyze_aggregates_by_extension() {
        let dir = tmp_dir("ext");
        write_file(&dir, "a.mp4", b"hello-world");
        write_file(&dir, "b.mp4", b"hello");
        write_file(&dir, "c.m4a", b"!");
        write_file(&dir, "README", b"text");
        let report = analyze(&dir).unwrap();
        assert_eq!(report.total_files, 4);
        let mp4 = report.by_extension.get("mp4").unwrap();
        assert_eq!(mp4.count, 2);
        assert_eq!(mp4.bytes, 16);
        let m4a = report.by_extension.get("m4a").unwrap();
        assert_eq!(m4a.count, 1);
        assert!(report
            .by_extension
            .get("(no-ext)")
            .is_some_and(|s| s.count == 1));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn analyze_top_files_sorted_by_size_desc() {
        let dir = tmp_dir("top");
        write_file(&dir, "small.mp4", &vec![0u8; 10]);
        write_file(&dir, "big.mp4", &vec![0u8; 100]);
        write_file(&dir, "mid.mp4", &vec![0u8; 50]);
        let report = analyze(&dir).unwrap();
        assert!(report.top_files.len() >= 3);
        assert_eq!(report.top_files[0].size, 100);
        assert_eq!(report.top_files[1].size, 50);
        assert_eq!(report.top_files[2].size, 10);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn analyze_age_buckets_uses_modified_time() {
        let dir = tmp_dir("age");
        write_file(&dir, "fresh.mp4", b"x");
        // 通过 mtime 回拨 — 不易跨平台,这里只验证 recent >= 1
        let report = analyze(&dir).unwrap();
        assert!(report.age_buckets.recent >= 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn analyze_empty_dir() {
        let dir = tmp_dir("empty");
        let report = analyze(&dir).unwrap();
        assert_eq!(report.total_files, 0);
        assert_eq!(report.total_bytes, 0);
        assert!(report.by_extension.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn ext_sorted_by_bytes_desc() {
        let dir = tmp_dir("sort");
        write_file(&dir, "x.mp4", &vec![0u8; 100]);
        write_file(&dir, "y.m4a", &vec![0u8; 200]);
        let report = analyze(&dir).unwrap();
        let sorted = report.ext_sorted_by_bytes();
        assert_eq!(sorted[0].0, "m4a");
        assert_eq!(sorted[0].1.bytes, 200);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn report_to_json_roundtrips() {
        let report = AnalysisReport::default();
        let s = report_to_json(&report).unwrap();
        assert!(s.contains("total_files"));
        assert!(s.contains("age_buckets"));
    }
}
