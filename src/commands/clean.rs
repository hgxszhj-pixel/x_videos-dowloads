//! `clean` 子命令 — 扫描下载目录找 stale / orphan / empty / duplicate 文件
//!
//! 受 mole `clean --dry-run --whitelist` 启发:
//! - empty:    大小为 0 的文件
//! - temp:     `.partial` / `.tmp` / `.crdownload` / `.part` 临时文件
//! - duplicate: 同尺寸 + 同 sha256 的批量文件(保留最早 mtime)
//!
//! 设计原则:
//! - [`scan`] 为 pure 函数(仅读 FS),可独立 unit test
//! - [`apply`] 与 [`scan`] 分离, dry-run 时只调用 scan
//! - 白名单文件即使匹配也跳过(用户主动保护)

use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

/// 清理目标类别
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CleanTarget {
    /// 仅空文件
    Empty,
    /// 仅临时/分片文件
    Temp,
    /// 仅重复文件
    Duplicate,
    /// 全部(默认)
    All,
}

impl CleanTarget {
    /// 解析 CLI 字符串(大小写不敏感)。空串或 "all" → All
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "empty" => Some(Self::Empty),
            "temp" | "tmp" | "partial" => Some(Self::Temp),
            "duplicate" | "dup" => Some(Self::Duplicate),
            "all" | "" => Some(Self::All),
            _ => None,
        }
    }
}

/// 单条清理候选
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanEntry {
    pub path: PathBuf,
    pub reason: String,
    pub size: u64,
}

/// 扫描报告
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CleanReport {
    pub entries: Vec<CleanEntry>,
    pub total_bytes: u64,
    pub scanned_files: usize,
}

impl CleanReport {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 按 reason 分组统计(用于人读输出)
    pub fn grouped(&self) -> HashMap<String, Vec<&CleanEntry>> {
        let mut map: HashMap<String, Vec<&CleanEntry>> = HashMap::new();
        for e in &self.entries {
            map.entry(e.reason.clone()).or_default().push(e);
        }
        map
    }
}

/// 判断扩展名是否属于临时/分片文件
fn is_temp_extension(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|s| s.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("partial" | "tmp" | "crdownload" | "part" | "download")
    )
}

/// `false` 表示文件应被跳过 — 命中白名单
fn is_whitelisted(path: &Path, whitelist: &[PathBuf]) -> bool {
    whitelist.iter().any(|w| w == path)
}

/// 读取文件 sha256 — 仅在 size 相同时调用(快路径)
fn sha256_of(path: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    let mut f = fs::File::open(path).with_context(|| format!("打开 {:?}", path))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// 主扫描:遍历 dir 收集符合 target 的清理候选
///
/// **pure 函数** — 不删除任何东西,只读 FS。dry-run 模式下也调用此函数。
pub fn scan(dir: &Path, target: CleanTarget, whitelist: &[PathBuf]) -> Result<CleanReport> {
    if !dir.exists() {
        anyhow::bail!("目录不存在: {:?}", dir);
    }
    if !dir.is_dir() {
        anyhow::bail!("路径不是目录: {:?}", dir);
    }

    let mut report = CleanReport::default();
    let include_empty = matches!(target, CleanTarget::Empty | CleanTarget::All);
    let include_temp = matches!(target, CleanTarget::Temp | CleanTarget::All);
    let include_dup = matches!(target, CleanTarget::Duplicate | CleanTarget::All);

    // 1) 先收集 (size, mtime, path) — 顺带 empty + temp
    let mut by_size: HashMap<u64, Vec<(PathBuf, SystemTime)>> = HashMap::new();

    for entry in fs::read_dir(dir).with_context(|| format!("读取目录 {:?}", dir))? {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                debug!("跳过不可读 entry: {}", e);
                continue;
            }
        };
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        report.scanned_files += 1;

        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(e) => {
                debug!("跳过不可读 metadata {:?}: {}", path, e);
                continue;
            }
        };
        let size = meta.len();
        let mtime = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);

        if is_whitelisted(&path, whitelist) {
            continue;
        }

        // 分类优先级:temp > empty。一个 .partial 空文件归 temp 更准确(用户更容易理解"下载中断的临时分片")。
        let temp_hit = include_temp && is_temp_extension(&path);

        // temp
        if temp_hit {
            report.entries.push(CleanEntry {
                path: path.clone(),
                reason: "temp".into(),
                size,
            });
            continue;
        }

        // empty
        if include_empty && size == 0 {
            report.entries.push(CleanEntry {
                path: path.clone(),
                reason: "empty".into(),
                size,
            });
            continue; // 已记录,不再走 size index
        }

        // duplicate 候选:仅当 size > 0 时参与(避免与 empty 重叠)
        if include_dup && size > 0 {
            by_size.entry(size).or_default().push((path.clone(), mtime));
        }
    }

    // 2) 对 size > 1 的桶做 sha256 二次聚合
    if include_dup {
        for (size, paths) in by_size {
            if paths.len() < 2 {
                continue;
            }
            let mut by_hash: HashMap<String, Vec<(PathBuf, SystemTime)>> = HashMap::new();
            for (p, m) in paths {
                match sha256_of(&p) {
                    Ok(h) => {
                        by_hash.entry(h).or_default().push((p, m));
                    }
                    Err(e) => {
                        debug!("sha256 失败 {:?}: {}", p, e);
                    }
                }
            }
            for (_hash, mut group) in by_hash {
                if group.len() < 2 {
                    continue;
                }
                // 按 mtime 升序:最早的保留,其余标记删除
                group.sort_by_key(|(_p, m)| *m);
                let (_, _) = group.remove(0); // 保留最早 — unused
                let freed = size * (group.len() as u64);
                for (p, _m) in group {
                    report.entries.push(CleanEntry {
                        path: p,
                        reason: "duplicate".into(),
                        size,
                    });
                }
                report.total_bytes += freed;
            }
        }
    }

    // 累加 total_bytes(空 + temp 部分)
    for e in &report.entries {
        if e.reason != "duplicate" {
            report.total_bytes += e.size;
        }
    }

    info!(
        "clean scan 完成: scanned={}, candidates={}, freed={} bytes",
        report.scanned_files,
        report.entries.len(),
        report.total_bytes
    );

    Ok(report)
}

/// 实际删除 entries,返回成功删除的字节数(失败项不计入但不抛错)
pub fn apply(entries: &[CleanEntry]) -> Result<u64> {
    let mut freed = 0u64;
    for e in entries {
        match fs::remove_file(&e.path) {
            Ok(()) => {
                info!("删除: {:?} ({} bytes, {})", e.path, e.size, e.reason);
                freed += e.size;
            }
            Err(err) => {
                debug!("删除失败 {:?}: {}", e.path, err);
            }
        }
    }
    Ok(freed)
}

/// 人读输出 report
pub fn print_report(report: &CleanReport, dry_run: bool) {
    let verb = if dry_run { "将删除" } else { "已删除" };
    if report.is_empty() {
        println!(
            "✓ 没有需要清理的内容 (扫描 {} 个文件)",
            report.scanned_files
        );
        return;
    }
    println!(
        "扫描: {} 个文件 | 候选: {} 个 | {}约 {} bytes",
        report.scanned_files,
        report.entries.len(),
        verb,
        report.total_bytes
    );
    let grouped = report.grouped();
    let mut reasons: Vec<&String> = grouped.keys().collect();
    reasons.sort();
    for reason in reasons {
        let list = &grouped[reason];
        println!("\n  [{}] ({} 个)", reason, list.len());
        for e in list.iter().take(10) {
            println!("    - {:?} ({} bytes)", e.path, e.size);
        }
        if list.len() > 10 {
            println!("    ... 还有 {} 个", list.len() - 10);
        }
    }
}

/// CLI 入口:从 cli flags 派发
pub fn run_clean_commands(
    target: CleanTarget,
    dry_run: bool,
    whitelist: &[PathBuf],
    scan_dir: &Path,
) -> Result<()> {
    let report = scan(scan_dir, target, whitelist)?;
    print_report(&report, dry_run);
    if !dry_run && !report.is_empty() {
        let freed = apply(&report.entries)?;
        println!("\n✓ 实际释放 {} bytes", freed);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn tmp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("xvd-clean-test-{}-{}", name, std::process::id()));
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
    fn clean_target_parse_known() {
        assert_eq!(CleanTarget::parse("empty"), Some(CleanTarget::Empty));
        assert_eq!(CleanTarget::parse("TEMP"), Some(CleanTarget::Temp));
        assert_eq!(CleanTarget::parse("dup"), Some(CleanTarget::Duplicate));
        assert_eq!(CleanTarget::parse("all"), Some(CleanTarget::All));
        assert_eq!(CleanTarget::parse(""), Some(CleanTarget::All));
        assert_eq!(CleanTarget::parse("garbage"), None);
    }

    #[test]
    fn scan_empty_target_finds_zero_size() {
        let dir = tmp_dir("empty");
        write_file(&dir, "real.mp4", b"hello");
        let p_empty = write_file(&dir, "blank.mp4", b"");
        let report = scan(&dir, CleanTarget::Empty, &[]).unwrap();
        assert_eq!(report.entries.len(), 1);
        assert_eq!(report.entries[0].path, p_empty);
        assert_eq!(report.entries[0].reason, "empty");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_temp_target_finds_partial_and_tmp() {
        let dir = tmp_dir("temp");
        write_file(&dir, "video.mp4", b"good");
        let p_partial = write_file(&dir, "x.mp4.partial", b"junk");
        let p_tmp = write_file(&dir, "y.tmp", b"junk");
        let p_cd = write_file(&dir, "z.crdownload", b"junk");
        let report = scan(&dir, CleanTarget::Temp, &[]).unwrap();
        let paths: Vec<_> = report.entries.iter().map(|e| e.path.clone()).collect();
        assert!(paths.contains(&p_partial));
        assert!(paths.contains(&p_tmp));
        assert!(paths.contains(&p_cd));
        assert_eq!(report.entries.len(), 3);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_duplicate_target_keeps_oldest() {
        let dir = tmp_dir("dup");
        let payload = b"identical-content-12345";
        let p1 = write_file(&dir, "a.mp4", payload);
        std::thread::sleep(std::time::Duration::from_millis(50));
        let p2 = write_file(&dir, "b.mp4", payload);
        std::thread::sleep(std::time::Duration::from_millis(50));
        let p3 = write_file(&dir, "c.mp4", payload);
        let report = scan(&dir, CleanTarget::Duplicate, &[]).unwrap();
        assert_eq!(report.entries.len(), 2, "保留最早 1 个,删 2 个");
        let paths: Vec<_> = report.entries.iter().map(|e| &e.path).collect();
        assert!(!paths.contains(&&p1), "最早 mtime 应保留");
        assert!(paths.contains(&&p2));
        assert!(paths.contains(&&p3));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_respects_whitelist() {
        let dir = tmp_dir("wl");
        let protected = write_file(&dir, "blank.mp4", b"");
        let report = scan(&dir, CleanTarget::Empty, &[protected.clone()]).unwrap();
        assert!(report.entries.is_empty(), "白名单文件应被跳过");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_all_target_combines_categories() {
        let dir = tmp_dir("all");
        write_file(&dir, "good.mp4", b"normal");
        write_file(&dir, "blank.mp4", b"");
        write_file(&dir, "x.partial", b"junk");
        let p_dup1 = write_file(&dir, "d1.mp4", b"dup-content");
        let p_dup2 = write_file(&dir, "d2.mp4", b"dup-content");
        let report = scan(&dir, CleanTarget::All, &[]).unwrap();
        // 预期: 1 empty + 1 temp + 1 duplicate (d2 删,保留 d1)
        assert_eq!(report.entries.len(), 3);
        let paths: Vec<_> = report.entries.iter().map(|e| &e.path).collect();
        assert!(paths.contains(&&p_dup2));
        assert!(!paths.contains(&&p_dup1));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_nonexistent_dir_errors() {
        let res = scan(
            Path::new("/tmp/does-not-exist-xxxxx-9999"),
            CleanTarget::All,
            &[],
        );
        assert!(res.is_err());
    }

    #[test]
    fn apply_actually_removes_files() {
        let dir = tmp_dir("apply");
        let p1 = write_file(&dir, "blank.mp4", b"");
        let p2 = write_file(&dir, "x.partial", b"junk");
        let report = scan(&dir, CleanTarget::All, &[]).unwrap();
        assert_eq!(report.entries.len(), 2);
        let freed = apply(&report.entries).unwrap();
        assert!(freed > 0);
        assert!(!p1.exists());
        assert!(!p2.exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
