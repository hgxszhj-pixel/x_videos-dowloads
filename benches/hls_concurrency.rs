//! Benchmark: find optimal `max_concurrent` for HLS segment downloads.
//!
//! In-process mock HLS server (32 segments × 50 ms latency), times end-to-end
//! download across {1, 2, 4, 8, 16, 32} concurrency. Plain `Instant` loop —
//! one sample per config. Run: `cargo bench --bench hls_concurrency`.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

// Inline-include hls sources (crate is binary-only; this bench is its own crate).
#[path = "../src/hls/mod.rs"]
#[allow(dead_code)]
mod hls;

// Stub for `crate::types::DownloadProgress` used by HlsDownloader::download sig.
#[allow(dead_code)]
pub mod types {
    #[derive(Debug, Clone, Default)]
    pub struct DownloadProgress {
        pub percent: f32,
        pub downloaded_bytes: u64,
        pub total_bytes: u64,
        pub speed_bps: u64,
        pub eta_secs: u64,
    }
}

use hls::HlsDownloader;

const SEGMENT_COUNT: usize = 32;
const SEGMENT_LATENCY_MS: u64 = 50;
const SEGMENT_PAYLOAD: &[u8] = b"bench-segment-payload-12345678";

async fn run_once(max_concurrent: usize) -> (Duration, usize) {
    let reqs = Arc::new(AtomicUsize::new(0));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let req_counter = reqs.clone();

    let server = tokio::spawn(async move {
        loop {
            let (mut socket, _) = match listener.accept().await { Ok(p) => p, Err(_) => break };
            let reqs = req_counter.clone();
            tokio::spawn(async move {
                let mut buf = vec![0u8; 4096];
                let n = match socket.read(&mut buf).await { Ok(n) if n > 0 => n, _ => return };
                let req = String::from_utf8_lossy(&buf[..n]).into_owned();
                reqs.fetch_add(1, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(SEGMENT_LATENCY_MS)).await;
                let (ct, body) = if req.starts_with("GET /manifest") {
                    let mut m = String::from("#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-TARGETDURATION:10\n#EXT-X-MEDIA-SEQUENCE:0\n");
                    for i in 0..SEGMENT_COUNT { m.push_str(&format!("#EXTINF:5.0,\nseg_{:04}.ts\n", i)); }
                    m.push_str("#EXT-X-ENDLIST\n");
                    ("application/vnd.apple.mpegurl", m)
                } else {
                    ("video/mp2t", String::from_utf8_lossy(SEGMENT_PAYLOAD).into_owned())
                };
                let resp = format!("HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", ct, body.len());
                let _ = socket.write_all(resp.as_bytes()).await;
                let _ = socket.write_all(body.as_bytes()).await;
                let _ = socket.shutdown().await;
            });
        }
    });

    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("out.ts");
    let dl = HlsDownloader::new(reqwest::Client::new()).max_concurrent(max_concurrent).retry_count(1);
    let url = format!("http://127.0.0.1:{}/manifest.m3u8", port);
    let start = Instant::now();
    let _ = dl.download(&url, &out, None).await;
    server.abort();
    (start.elapsed(), reqs.load(Ordering::SeqCst))
}

#[tokio::main(flavor = "multi_thread", worker_threads = 8)]
async fn main() {
    let configs = [1usize, 2, 4, 8, 16, 32];
    println!("{:<14} | {:>13} | {:>13}", "max_concurrent", "wall_ms", "requests");
    println!("{}-+-{}-+-{}", "-".repeat(14), "-".repeat(15), "-".repeat(15));
    for &c in &configs {
        let (dt, total) = run_once(c).await;
        println!("{:<14} | {:>13} | {:>13}", c, dt.as_millis(), total);
    }
}
