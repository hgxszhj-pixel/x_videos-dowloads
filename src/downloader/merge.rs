//! 分段合并逻辑
//!
//! 按分段 offset 排序后顺序写入，消除所有 seek；在 spawn_blocking 中执行文件 I/O。

use std::path::{Path, PathBuf};

use anyhow::Result;
use tracing::{debug, info, warn};

use super::VideoDownloader;
use crate::core::constants::SEGMENT_VALIDATION_RATIO;

/// 给路径添加 ".tmp" 后缀,例如 `foo.mp4` → `foo.mp4.tmp`。
/// 用于 merge 时先写到临时路径,全部段合并完后 atomic rename 到 final。
/// 父目录不变 → 与 final 在同一 FS,POSIX `rename(2)` 保证原子性。
fn append_tmp_suffix(p: &Path) -> PathBuf {
    let mut s = p.as_os_str().to_owned();
    s.push(".tmp");
    PathBuf::from(s)
}

impl VideoDownloader {
    /// 合并分段文件 - 优化版本: 按 offset 排序后顺序写入，消除所有 seek
    /// 使用 spawn_blocking 避免阻塞 async runtime
    pub(super) async fn merge_segments(
        &self,
        output_path: &Path,
        temp_dir_path: &Path,
        segments: u32,
        total_size: u64,
        chunk_size: u64,
    ) -> Result<u64> {
        use std::io::{Read, Seek, SeekFrom, Write};

        // 根据文件大小自适应缓冲区策略
        let copy_buffer_size = if total_size < 100 * 1024 * 1024 {
            64 * 1024
        } else if total_size < 1024 * 1024 * 1024 {
            512 * 1024
        } else {
            1024 * 1024
        };

        // 收集分段信息用于在 blocking 任务中处理
        let mut segment_info: Vec<(u64, PathBuf)> = Vec::with_capacity(segments as usize);
        for i in 0..segments {
            let temp_path = temp_dir_path.join(format!("part_{}.tmp", i));
            let expected_offset = i as u64 * chunk_size;
            let expected_segment_size = if i == segments - 1 {
                total_size - expected_offset
            } else {
                chunk_size
            };

            if let Ok(metadata) = std::fs::metadata(&temp_path) {
                let actual_size = metadata.len();
                if actual_size >= expected_segment_size * SEGMENT_VALIDATION_RATIO / 100 {
                    segment_info.push((expected_offset, temp_path));
                }
            }
        }

        // 按 offset 排序
        segment_info.sort_by_key(|(offset, _)| *offset);
        let total_segments = segment_info.len() as u32;

        // 在 blocking 线程中执行所有文件 I/O
        // 转换为 PathBuf 以确保所有权转移
        let output_path_owned = output_path.to_path_buf();
        let final_size = tokio::task::spawn_blocking(move || {
            // 写到 "<output>.tmp",全部段合并完后 atomic rename 到 <output>。
            // 这样中途被杀 → "<output>.tmp" 留在用户目录(清楚标记为 partial),
            // 不会有半截的 <output>(避免污染)。
            let tmp_output = append_tmp_suffix(&output_path_owned);
            let mut output = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&tmp_output)?;

            let mut final_size = 0u64;
            let mut buffer = vec![0u8; copy_buffer_size];

            info!("开始合并 {} 个分段 (缓冲区: {}KB)...", total_segments, copy_buffer_size / 1024);

            for (segment_idx, (expected_offset, temp_path)) in segment_info.into_iter().enumerate() {
                let mut input = std::fs::File::open(&temp_path)?;

                // 验证分段从正确的位置开始写入
                let current_pos = output.stream_position()?;
                if current_pos != expected_offset {
                    output.seek(SeekFrom::Start(expected_offset))?;
                }

                let mut segment_total = 0u64;
                loop {
                    let bytes_read = match input.read(&mut buffer) {
                        Ok(0) => break,
                        Ok(n) => n,
                        Err(e) => {
                            warn!("读取分段 {} 时出错: {:?}", segment_idx, e);
                            break;
                        }
                    };

                    if let Err(e) = output.write_all(&buffer[..bytes_read]) {
                        warn!("写入分段 {} 时出错: {:?}", segment_idx, e);
                        break;
                    }

                    segment_total += bytes_read as u64;
                    final_size += bytes_read as u64;

                    if segment_total.is_multiple_of(50 * 1024 * 1024) {
                        let seg_idx_u32 = segment_idx as u32;
                        let last_segment = seg_idx_u32 == total_segments - 1;
                        let denominator = if last_segment {
                            total_size - expected_offset
                        } else {
                            chunk_size
                        } as f64;
                        info!("合并进度: 分段 {}/{}, {:.1}%",
                            seg_idx_u32 + 1, total_segments, segment_total as f64 / denominator * 100.0);
                    }
                }

                debug!("合并分段 {}/{}: {} bytes", segment_idx + 1, total_segments, segment_total);
            }

            // 全部段合并成功,atomic rename "<output>.tmp" → "<output>"。
            // POSIX rename(2) 在同一 FS 上是原子操作。
            // 失败 → 留下 <output>.tmp partial 文件,原 <output> 不动(干净)。
            std::fs::rename(&tmp_output, &output_path_owned)?;

            Ok::<u64, std::io::Error>(final_size)
        }).await??;

        // 阶段3: 清理临时文件 (使用 tokio::fs，异步)
        for i in 0..segments {
            let temp_path = temp_dir_path.join(format!("part_{}.tmp", i));
            if let Err(e) = tokio::fs::remove_file(&temp_path).await {
                debug!("删除临时文件 {} 失败: {:?}", temp_path.display(), e);
            }
        }

        Ok(final_size)
    }
}

#[cfg(test)]
#[allow(unused_imports)]
mod tests {
    use super::*;

    #[test]
    fn append_tmp_suffix_adds_dot_tmp_suffix() {
        // foo.mp4 → foo.mp4.tmp(父目录不变)
        let p = std::path::Path::new("/tmp/test/foo.mp4");
        let r = append_tmp_suffix(p);
        assert_eq!(r, std::path::PathBuf::from("/tmp/test/foo.mp4.tmp"));

        // 无扩展名也照常加 .tmp
        let p2 = std::path::Path::new("/var/folders/abc/video");
        let r2 = append_tmp_suffix(p2);
        assert_eq!(r2, std::path::PathBuf::from("/var/folders/abc/video.tmp"));
    }

    /// 验证 merge_segments 写到 `<output>.tmp`,完成后 atomic rename 到 `<output>`:
    /// - merge 结束后 `<output>` 存在(被 rename)
    /// - merge 结束后 `<output>.tmp` **不存在**(rename 走了)
    /// - 输出内容是 4 个 part 顺序连接
    #[tokio::test]
    async fn merge_segments_writes_to_tmp_then_atomic_renames() {
        // output dir(模拟 ~/Movies/),放 foo.mp4 作为最终文件路径
        let output_dir = tempfile::TempDir::new().expect("create output TempDir");
        let output_path = output_dir.path().join("foo.mp4");
        let tmp_path = append_tmp_suffix(&output_path);

        // part 文件放独立的临时目录(模拟 OS temp,在用户目录之外)
        let parts_dir = tempfile::TempDir::new().expect("create parts TempDir");
        let segments: u32 = 4;
        let chunk_size: u64 = 100;
        let total_size: u64 = chunk_size * segments as u64;

        // 创建 4 个 part 文件,内容可识别 (part_i 全是字节 0x0(i+1))
        for i in 0..segments {
            let p = parts_dir.path().join(format!("part_{}.tmp", i));
            let byte = (i as u8) + 1;
            std::fs::write(&p, vec![byte; chunk_size as usize])
                .expect("write part file");
        }

        // 调 merge_segments
        let downloader = VideoDownloader::new().expect("create VideoDownloader");
        let merged_size = downloader
            .merge_segments(&output_path, parts_dir.path(), segments, total_size, chunk_size)
            .await
            .expect("merge should succeed");

        // 验证 1:返回的 merged_size == total_size
        assert_eq!(merged_size, total_size, "merged size should equal total");

        // 验证 2:output_path 存在(被 rename 过来了)
        assert!(
            output_path.exists(),
            "final output path must exist (rename moved it from .tmp)"
        );

        // 验证 3:tmp_path 不存在(被 rename 走了,而不是双份)
        assert!(
            !tmp_path.exists(),
            "tmp file must NOT exist after successful merge (rename consumed it)"
        );

        // 验证 4:内容是 4 段顺序拼接
        let merged = std::fs::read(&output_path).expect("read merged output");
        assert_eq!(merged.len() as u64, total_size);
        for i in 0..segments as usize {
            let byte = (i as u8) + 1;
            let start = i * chunk_size as usize;
            let end = start + chunk_size as usize;
            assert!(
                merged[start..end].iter().all(|&b| b == byte),
                "part {i} bytes mismatch"
            );
        }

        // TempDir 在 drop 时清理;不需要显式清理 part 文件。
    }
}
