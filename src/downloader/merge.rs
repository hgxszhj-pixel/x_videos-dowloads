//! 分段合并逻辑
//!
//! 按分段 offset 排序后顺序写入，消除所有 seek；在 spawn_blocking 中执行文件 I/O。

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use tracing::{debug, info, warn};

use super::VideoDownloader;
use crate::core::constants::SEGMENT_VALIDATION_RATIO;
use crate::types::validate_output_path;

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

        // Validate output path to prevent path traversal
        validate_output_path(output_path, temp_dir_path)
            .map_err(|e| anyhow!("Path traversal detected: {}", e))
            .context("output path validation failed")?;

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
            // 打开输出文件
            let mut output = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(false)
                .open(&output_path_owned)?;

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
