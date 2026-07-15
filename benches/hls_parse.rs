//! HLS 解析 micro-benchmark：当前实现 vs bumpalo 版本
//!
//! 目的：实测 bumpalo 是否能给 M3u8Parser::parse 带来显著加速
//! 测试场景：50/200/500 段 m3u8 + 加密 key + URL 解析
//!
//! 用法：`cargo bench --bench hls_parse`

use std::path::PathBuf;
use std::time::Instant;

// 与 src/main.rs 共享：include HLS parser 模块（bench 是独立 crate）
#[path = "../src/hls/parser.rs"]
mod hls_parser;
use hls_parser::M3u8Parser;

use bumpalo::Bump;

/// 生成真实感 m3u8 fixture（n 段，含加密 key + 每段标题）
fn make_playlist(segment_count: usize) -> String {
    let mut s = String::with_capacity(segment_count * 100);
    s.push_str("#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-TARGETDURATION:10\n#EXT-X-MEDIA-SEQUENCE:0\n");
    s.push_str("#EXT-X-KEY:METHOD=AES-128,URI=\"https://cdn.example.com/keys/seg.key\",IV=0x1234567890ABCDEF1234567890ABCDEF\n");
    for i in 0..segment_count {
        s.push_str(&format!(
            "#EXTINF:9.{:01},Segment {} - Introduction to the topic\n",
            i % 10,
            i
        ));
        s.push_str(&format!(
            "https://cdn.example.com/hls/seg{:05}.ts\n",
            i
        ));
    }
    s.push_str("#EXT-X-ENDLIST\n");
    s
}

/// Bumpalo 版本：把所有 temp String 走 arena（仅用于对比测量）
/// 注：模仿 parser 主逻辑，差异在 temp String 走 bump.alloc_str 而非 String
fn parse_with_bump<'b>(content: &str, _base_url: &str, bump: &'b Bump) -> Vec<(usize, String, f64)> {
    let mut segments = Vec::new();
    let mut current_duration = 0.0_f64;
    let mut seq = 0_usize;
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with("#EXTINF:") {
            let dur_str = line.trim_start_matches("#EXTINF:");
            current_duration = dur_str.parse().unwrap_or(0.0);
        } else if line.starts_with("#EXT-X-KEY:METHOD=AES-128") {
            // 模拟 key URL/IV 提取（bump.alloc_str）
            let _ = bump.alloc_str(line);
        } else if !line.starts_with('#') {
            // URL 走 bump arena
            let url = bump.alloc_str(line);
            segments.push((seq, url.to_string(), current_duration));
            seq += 1;
        }
    }
    segments
}

/// 多次运行取最小值（避免噪声）
fn min_nanos<F: FnMut()>(iters: usize, mut f: F) -> u128 {
    let mut min = u128::MAX;
    for _ in 0..iters {
        let start = Instant::now();
        f();
        let elapsed = start.elapsed().as_nanos();
        if elapsed < min {
            min = elapsed;
        }
    }
    min
}

fn main() {
    println!("=== HLS Parse micro-benchmark ===\n");

    for &n in &[50usize, 200, 500] {
        let content = make_playlist(n);
        let base = "https://cdn.example.com/hls/";
        println!("--- {} segments ---", n);

        // 当前实现
        let cur_min = min_nanos(100, || {
            let _ = M3u8Parser::parse(&content, base).unwrap();
        });
        println!("  current  : {:>10} ns", cur_min);

        // Bumpalo 版本（每次 reset）
        let mut bump = Bump::with_capacity(64 * 1024);
        let bump_min = min_nanos(100, || {
            bump.reset();
            let _ = parse_with_bump(&content, base, &bump);
        });
        println!("  bumpalo  : {:>10} ns", bump_min);

        let ratio = if cur_min > 0 {
            bump_min as f64 / cur_min as f64
        } else {
            0.0
        };
        let pct = (ratio - 1.0) * 100.0;
        println!(
            "  ratio    : {:.3}  ({}{}% vs current)",
            ratio,
            if pct > 0.0 { "+" } else { "" },
            pct as i32
        );
        println!();
    }
}