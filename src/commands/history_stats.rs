//! 下载历史统计分析 (受 mole 的 `mo history` 启发)
//!
//! 纯函数 `compute_stats` 接收 history 切片 + 当前时间,产出 `HistoryStats`,
//! 包含:成功率 / 累计下载字节 / 平台分布 / Top-N 最大文件 / 最近 7d 计数。

use crate::history::{DownloadStatus, HistoryEntry, Platform};
use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;

/// 单个平台统计
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlatformStat {
    pub platform: String,
    pub total: usize,
    pub completed: usize,
    pub failed: usize,
    pub total_bytes: u64,
}

/// Top-N 最大文件(不含失败)
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LargestEntry {
    pub url: String,
    pub title: Option<String>,
    pub size_bytes: u64,
    pub platform: String,
}

/// 整体统计快照
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HistoryStats {
    pub total: usize,
    pub completed: usize,
    pub failed: usize,
    /// 0.0..=1.0,total=0 时为 0.0
    pub success_rate: f64,
    pub total_bytes: u64,
    pub avg_duration_secs: Option<f64>,
    /// 按平台统计,key 是 platform 的 lowercase 名(便于测试断言 + 稳定序列化)
    pub by_platform: Vec<PlatformStat>,
    /// 按大小降序
    pub largest: Vec<LargestEntry>,
    pub recent_7d: usize,
    pub first_seen: Option<DateTime<Utc>>,
    pub last_seen: Option<DateTime<Utc>>,
}

/// 计算历史统计。`now` 参数注入便于测试。
pub fn compute_stats(entries: &[HistoryEntry], now: DateTime<Utc>) -> HistoryStats {
    let total = entries.len();
    let mut completed = 0usize;
    let mut failed = 0usize;
    let mut total_bytes: u64 = 0;
    let mut duration_sum = 0.0f64;
    let mut duration_count = 0usize;
    let mut platform_map: BTreeMap<String, PlatformStat> = BTreeMap::new();
    let mut largest: Vec<(u64, &HistoryEntry)> = Vec::new();
    let mut recent_7d = 0usize;
    let cutoff = now - Duration::days(7);
    let mut first_seen: Option<DateTime<Utc>> = None;
    let mut last_seen: Option<DateTime<Utc>> = None;

    for entry in entries {
        // 时间窗口
        if entry.downloaded_at >= cutoff {
            recent_7d += 1;
        }
        first_seen = Some(first_seen.map_or(entry.downloaded_at, |p| p.min(entry.downloaded_at)));
        last_seen = Some(last_seen.map_or(entry.downloaded_at, |p| p.max(entry.downloaded_at)));

        // 状态
        match entry.status {
            DownloadStatus::Completed => completed += 1,
            DownloadStatus::Failed => failed += 1,
        }

        // 字节 + largest
        if let Some(size) = entry.file_size {
            if entry.status == DownloadStatus::Completed {
                total_bytes = total_bytes.saturating_add(size);
                largest.push((size, entry));
            }
        }

        // 时长
        if let Some(d) = entry.duration_secs {
            if d.is_finite() && d >= 0.0 {
                duration_sum += d;
                duration_count += 1;
            }
        }

        // 平台聚合
        let key = platform_key(&entry.platform);
        let stat = platform_map.entry(key).or_insert_with(|| PlatformStat {
            platform: platform_label(&entry.platform),
            total: 0,
            completed: 0,
            failed: 0,
            total_bytes: 0,
        });
        stat.total += 1;
        match entry.status {
            DownloadStatus::Completed => stat.completed += 1,
            DownloadStatus::Failed => stat.failed += 1,
        }
        if entry.status == DownloadStatus::Completed {
            if let Some(size) = entry.file_size {
                stat.total_bytes = stat.total_bytes.saturating_add(size);
            }
        }
    }

    // 排序 largest(降序)+ 取 Top-10
    largest.sort_by_key(|(size, _)| std::cmp::Reverse(*size));
    let largest: Vec<LargestEntry> = largest
        .into_iter()
        .take(10)
        .map(|(size, e)| LargestEntry {
            url: e.url.clone(),
            title: e.title.clone(),
            size_bytes: size,
            platform: platform_label(&e.platform),
        })
        .collect();

    let success_rate = if total == 0 {
        0.0
    } else {
        completed as f64 / total as f64
    };

    let avg_duration_secs = if duration_count == 0 {
        None
    } else {
        Some(duration_sum / duration_count as f64)
    };

    let by_platform: Vec<PlatformStat> = platform_map.into_values().collect();

    HistoryStats {
        total,
        completed,
        failed,
        success_rate,
        total_bytes,
        avg_duration_secs,
        by_platform,
        largest,
        recent_7d,
        first_seen,
        last_seen,
    }
}

/// 平台判别 key(lowercase 字符串)
fn platform_key(p: &Platform) -> String {
    serde_json::to_value(p)
        .ok()
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| platform_label(p).to_lowercase())
}

/// 平台显示名
fn platform_label(p: &Platform) -> String {
    serde_json::to_value(p)
        .ok()
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "unknown".to_string())
}

/// 人类可读摘要
pub fn format_summary(stats: &HistoryStats) -> String {
    use std::fmt::Write;
    let mut s = String::new();
    let _ = writeln!(s, "下载历史统计");
    let _ = writeln!(s, "================");
    let _ = writeln!(s, "总条目:        {}", stats.total);
    let _ = writeln!(s, "完成:          {}", stats.completed);
    let _ = writeln!(s, "失败:          {}", stats.failed);
    let _ = writeln!(s, "成功率:        {:.1}%", stats.success_rate * 100.0);
    let _ = writeln!(s, "累计下载:      {}", human_bytes(stats.total_bytes));
    match stats.avg_duration_secs {
        Some(d) => {
            let _ = writeln!(s, "平均耗时:      {:.1}s", d);
        }
        None => {
            let _ = writeln!(s, "平均耗时:      n/a");
        }
    }
    let _ = writeln!(s, "近 7 天:       {}", stats.recent_7d);
    match (stats.first_seen, stats.last_seen) {
        (Some(f), Some(l)) => {
            let _ = writeln!(s, "首次记录:      {}", f.format("%Y-%m-%d %H:%M"));
            let _ = writeln!(s, "最近记录:      {}", l.format("%Y-%m-%d %H:%M"));
        }
        _ => {
            let _ = writeln!(s, "首次/最近:     n/a");
        }
    }

    if !stats.by_platform.is_empty() {
        let _ = writeln!(s, "\n按平台分布:");
        let _ = writeln!(
            s,
            "  {:<12} {:>6} {:>6} {:>6} {:>12}",
            "platform", "total", "ok", "fail", "bytes"
        );
        for p in &stats.by_platform {
            let _ = writeln!(
                s,
                "  {:<12} {:>6} {:>6} {:>6} {:>12}",
                truncate(&p.platform, 12),
                p.total,
                p.completed,
                p.failed,
                human_bytes(p.total_bytes)
            );
        }
    }

    if !stats.largest.is_empty() {
        let _ = writeln!(s, "\nTop 文件(按大小):");
        for (i, l) in stats.largest.iter().enumerate() {
            let title = l.title.as_deref().unwrap_or("(no title)");
            let _ = writeln!(
                s,
                "  {:>2}. {} [{}] {}",
                i + 1,
                human_bytes(l.size_bytes),
                truncate(&l.platform, 10),
                truncate(title, 50)
            );
        }
    }

    s
}

fn human_bytes(n: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    if n == 0 {
        return "0 B".to_string();
    }
    let mut size = n as f64;
    let mut idx = 0;
    while size >= 1024.0 && idx < UNITS.len() - 1 {
        size /= 1024.0;
        idx += 1;
    }
    if idx == 0 {
        format!("{} {}", n, UNITS[0])
    } else {
        format!("{:.2} {}", size, UNITS[idx])
    }
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

/// CLI 入口:扫描历史文件 + 输出
pub fn run_history_stats(history_path: &Path, json: bool) -> anyhow::Result<()> {
    use anyhow::{anyhow, Context};

    if !history_path.exists() {
        return Err(anyhow!(
            "history file not found: {}",
            history_path.display()
        ));
    }
    let content = std::fs::read_to_string(history_path)
        .with_context(|| format!("read {}", history_path.display()))?;
    let entries: Vec<HistoryEntry> = serde_json::from_str(&content)
        .with_context(|| format!("parse {}", history_path.display()))?;

    let stats = compute_stats(&entries, Utc::now());

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&stats).map_err(|e| anyhow!("json: {}", e))?
        );
    } else {
        print!("{}", format_summary(&stats));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use std::path::PathBuf;

    fn entry_at(
        url: &str,
        days_ago: i64,
        status: DownloadStatus,
        size: Option<u64>,
    ) -> HistoryEntry {
        let url = url.to_string();
        let ts = Utc::now() - Duration::days(days_ago);
        HistoryEntry {
            id: uuid::Uuid::new_v4(),
            url: url.clone(),
            title: Some(format!("Title for {}", url)),
            file_path: Some(PathBuf::from(format!("/tmp/{}", url.replace('/', "_")))),
            file_size: size,
            downloaded_at: ts,
            duration_secs: Some(10.0),
            status,
            platform: crate::utils::url_matcher::detect_platform(&url),
            error_message: None,
        }
    }

    #[test]
    fn stats_empty() {
        let now = Utc.with_ymd_and_hms(2026, 10, 4, 12, 0, 0).unwrap();
        let s = compute_stats(&[], now);
        assert_eq!(s.total, 0);
        assert_eq!(s.completed, 0);
        assert_eq!(s.failed, 0);
        assert_eq!(s.success_rate, 0.0);
        assert_eq!(s.total_bytes, 0);
        assert!(s.avg_duration_secs.is_none());
        assert!(s.by_platform.is_empty());
        assert!(s.largest.is_empty());
        assert_eq!(s.recent_7d, 0);
        assert!(s.first_seen.is_none());
        assert!(s.last_seen.is_none());
    }

    #[test]
    fn stats_all_completed() {
        let now = Utc.with_ymd_and_hms(2026, 10, 4, 12, 0, 0).unwrap();
        let entries = vec![
            entry_at(
                "https://www.youtube.com/watch?v=a",
                0,
                DownloadStatus::Completed,
                Some(1024),
            ),
            entry_at(
                "https://www.bilibili.com/video/b",
                1,
                DownloadStatus::Completed,
                Some(2048),
            ),
        ];
        let s = compute_stats(&entries, now);
        assert_eq!(s.total, 2);
        assert_eq!(s.completed, 2);
        assert_eq!(s.failed, 0);
        assert_eq!(s.success_rate, 1.0);
        assert_eq!(s.total_bytes, 3072);
        assert_eq!(s.recent_7d, 2);
    }

    #[test]
    fn stats_mixed_success_rate() {
        let now = Utc.with_ymd_and_hms(2026, 10, 4, 12, 0, 0).unwrap();
        let entries = vec![
            entry_at(
                "https://www.youtube.com/watch?v=u1",
                0,
                DownloadStatus::Completed,
                Some(100),
            ),
            entry_at(
                "https://www.youtube.com/watch?v=u2",
                0,
                DownloadStatus::Failed,
                None,
            ),
            entry_at(
                "https://www.youtube.com/watch?v=u3",
                0,
                DownloadStatus::Completed,
                Some(200),
            ),
            entry_at(
                "https://www.youtube.com/watch?v=u4",
                0,
                DownloadStatus::Failed,
                None,
            ),
        ];
        let s = compute_stats(&entries, now);
        assert_eq!(s.total, 4);
        assert_eq!(s.completed, 2);
        assert_eq!(s.failed, 2);
        assert!((s.success_rate - 0.5).abs() < 1e-9);
    }

    #[test]
    fn stats_platform_breakdown() {
        let now = Utc.with_ymd_and_hms(2026, 10, 4, 12, 0, 0).unwrap();
        let entries = vec![
            entry_at(
                "https://www.youtube.com/watch?v=yt1",
                0,
                DownloadStatus::Completed,
                Some(1000),
            ),
            entry_at(
                "https://www.youtube.com/watch?v=yt2",
                0,
                DownloadStatus::Failed,
                None,
            ),
            entry_at(
                "https://www.bilibili.com/video/b1",
                0,
                DownloadStatus::Completed,
                Some(2000),
            ),
        ];
        let s = compute_stats(&entries, now);
        assert_eq!(s.by_platform.len(), 2);
        // 找到 youtube 那条
        let yt = s
            .by_platform
            .iter()
            .find(|p| p.platform.to_lowercase().contains("youtube"))
            .expect("yt stat");
        assert_eq!(yt.total, 2);
        assert_eq!(yt.completed, 1);
        assert_eq!(yt.failed, 1);
        assert_eq!(yt.total_bytes, 1000);
    }

    #[test]
    fn stats_largest_top_10() {
        let now = Utc.with_ymd_and_hms(2026, 10, 4, 12, 0, 0).unwrap();
        let mut entries = Vec::new();
        for i in 0..15 {
            entries.push(entry_at(
                &format!("https://www.youtube.com/watch?v=l{}", i),
                0,
                DownloadStatus::Completed,
                Some(1000 + i as u64 * 100),
            ));
        }
        let s = compute_stats(&entries, now);
        assert_eq!(s.largest.len(), 10); // 上限 10
        assert!(s.largest[0].size_bytes >= s.largest[1].size_bytes); // 降序
        assert_eq!(s.largest[0].size_bytes, 1000 + 14 * 100);
    }

    #[test]
    fn stats_recent_7d_window() {
        // 用 Utc::now() 与 entry_at 锚定同一时间，避免 hardcoded now 与
        // entry_at 的 Utc::now() 不一致导致 day 8 偏移落到窗口内
        let now = Utc::now();
        let entries = vec![
            entry_at(
                "https://www.youtube.com/watch?v=r1",
                0,
                DownloadStatus::Completed,
                Some(100),
            ),
            entry_at(
                "https://www.youtube.com/watch?v=r2",
                5,
                DownloadStatus::Completed,
                Some(100),
            ),
            entry_at(
                "https://www.youtube.com/watch?v=r3",
                8,
                DownloadStatus::Completed,
                Some(100),
            ),
        ];
        let s = compute_stats(&entries, now);
        assert_eq!(s.recent_7d, 2); // day 0 和 day 5 在窗口内,day 8 之外
    }

    #[test]
    fn stats_first_last_seen() {
        let now = Utc.with_ymd_and_hms(2026, 10, 4, 12, 0, 0).unwrap();
        let entries = vec![
            entry_at(
                "https://www.youtube.com/watch?v=f1",
                30,
                DownloadStatus::Completed,
                Some(100),
            ),
            entry_at(
                "https://www.youtube.com/watch?v=f2",
                1,
                DownloadStatus::Completed,
                Some(100),
            ),
            entry_at(
                "https://www.youtube.com/watch?v=f3",
                10,
                DownloadStatus::Completed,
                Some(100),
            ),
        ];
        let s = compute_stats(&entries, now);
        assert!(s.first_seen.is_some());
        assert!(s.last_seen.is_some());
        assert!(s.first_seen.unwrap() <= s.last_seen.unwrap());
    }

    #[test]
    fn stats_failed_excluded_from_bytes_and_largest() {
        let now = Utc.with_ymd_and_hms(2026, 10, 4, 12, 0, 0).unwrap();
        let entries = vec![entry_at(
            "https://www.youtube.com/watch?v=x1",
            0,
            DownloadStatus::Failed,
            Some(999_999),
        )];
        let s = compute_stats(&entries, now);
        assert_eq!(s.total_bytes, 0);
        assert!(s.largest.is_empty());
        assert_eq!(s.failed, 1);
    }

    #[test]
    fn stats_avg_duration_ignores_invalid() {
        let now = Utc.with_ymd_and_hms(2026, 10, 4, 12, 0, 0).unwrap();
        let mut e1 = entry_at(
            "https://www.youtube.com/watch?v=a",
            0,
            DownloadStatus::Completed,
            Some(100),
        );
        e1.duration_secs = Some(10.0);
        let mut e2 = entry_at(
            "https://www.youtube.com/watch?v=b",
            0,
            DownloadStatus::Completed,
            Some(100),
        );
        e2.duration_secs = Some(20.0);
        let mut e3 = entry_at(
            "https://www.youtube.com/watch?v=c",
            0,
            DownloadStatus::Completed,
            Some(100),
        );
        e3.duration_secs = Some(f64::NAN);
        let s = compute_stats(&[e1, e2, e3], now);
        assert!(s.avg_duration_secs.is_some());
        assert!((s.avg_duration_secs.unwrap() - 15.0).abs() < 1e-9);
    }

    #[test]
    fn human_bytes_units() {
        assert_eq!(human_bytes(0), "0 B");
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(1024), "1.00 KB");
        assert_eq!(human_bytes(1024 * 1024), "1.00 MB");
        assert_eq!(human_bytes(1024u64.pow(3)), "1.00 GB");
        assert_eq!(human_bytes(1024u64.pow(4)), "1.00 TB");
        assert_eq!(human_bytes(1024u64.pow(5)), "1024.00 TB"); // 溢出上界
    }

    #[test]
    fn format_summary_non_empty() {
        let now = Utc.with_ymd_and_hms(2026, 10, 4, 12, 0, 0).unwrap();
        let entries = vec![entry_at(
            "https://www.youtube.com/watch?v=fs",
            0,
            DownloadStatus::Completed,
            Some(2048),
        )];
        let s = compute_stats(&entries, now);
        let out = format_summary(&s);
        assert!(out.contains("下载历史统计"));
        assert!(out.contains("总条目"));
        assert!(out.contains("2.00 KB"));
    }
}
