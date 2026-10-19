//! Regression tests for abrupt transport termination (docs.rs field report).
//!
//! Simulates a peer that ends a transfer without a clean shutdown — the
//! HTTP/1.1 body is truncated mid-stream (reqwest surfaces this as
//! "error reading a body from connection: end of file before message
//! length reached"; over TLS it is the close_notify EOF) — and proves:
//! - the truncated transfer FAILS (never falsely completes)
//! - partial progress is preserved on disk
//! - a resume run completes the file with exact content and no
//!   duplicated bytes
//!
//! The classification assertions (close_notify / truncated-body strings
//! -> retryable "connection") live in `engine::retry_classification_tests`.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use downpilot_core::download::ChunkDownloader;
use downpilot_core::models::DownloadTask;
use tempfile::TempDir;
use tokio::sync::mpsc;

const TOTAL: usize = 256 * 1024;
const CUT: usize = 64 * 1024;

fn task_for(url: &str, dir: &std::path::Path, threads: u32) -> DownloadTask {
    let mut t = DownloadTask::default_for_probe(url, "cut.bin");
    t.save_dir = dir.to_string_lossy().into_owned();
    t.threads = threads;
    t.filename = "cut.bin".into();
    t.max_retries = 3;
    // A normal add() probes first (consuming a request) and stores the
    // total; mirror that so `run()` goes straight to the byte-flow path
    // and the FIRST body request is the truncated one.
    t.total_size = TOTAL as i64;
    t
}

/// Blocking origin serving `TOTAL` bytes of 0xAB. The FIRST request is
/// answered with only `CUT` bytes followed by an abrupt socket close
/// (no graceful Content-Length fulfilment, no TLS close_notify
/// equivalent); every later request gets the full body.
fn spawn_cut_origin() -> String {
    let hits = Arc::new(AtomicUsize::new(0));
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut sock) = stream else { continue };
            let n = hits.fetch_add(1, Ordering::SeqCst);
            // Consume the request head (single small read is enough for
            // reqwest's GET — headers fit in one segment).
            let mut buf = [0u8; 4096];
            let _ = sock.read(&mut buf);
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {TOTAL}\r\nAccept-Ranges: bytes\r\nETag: \"cut\"\r\n\r\n"
            );
            let _ = sock.write_all(head.as_bytes());
            let serve = if n == 0 { CUT } else { TOTAL };
            let _ = sock.write_all(&vec![0xABu8; serve]);
            let _ = sock.flush();
            // Drop without shutdown(): from the client this is an abrupt
            // EOF mid-body on the first hit — the transport-termination
            // class reported from docs.rs (missing close_notify over TLS).
            drop(sock);
        }
    });
    format!("http://{addr}/cut")
}

#[tokio::test]
async fn truncated_transfer_fails_not_completes_and_resume_finishes_exactly() {
    let tmp = TempDir::new().unwrap();
    let state = tmp.path().join("state");
    let out = tmp.path().join("out");
    std::fs::create_dir_all(&out).unwrap();
    let url = spawn_cut_origin();

    let dl = ChunkDownloader::new(
        state.clone(),
        Arc::new(std::sync::atomic::AtomicU64::new(0)),
    );
    let task = task_for(&url, &out, 1);
    let (tx, _rx) = mpsc::unbounded_channel();

    // Run 1: origin truncates mid-body -> must be Err, never Ok.
    let r1 = dl
        .run(
            task.clone(),
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
            tx.clone(),
        )
        .await;
    assert!(r1.is_err(), "truncated transfer must fail, got {r1:?}");

    // Partial data preserved on disk for the single-stream resume path.
    let partial = out.join("cut.bin");
    let len = std::fs::metadata(&partial).unwrap().len();
    assert_eq!(len as usize, CUT, "partial bytes must be preserved");

    // Run 2: origin now serves the full body; resume completes exactly.
    let r2 = dl
        .run(
            task,
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
            tx,
        )
        .await;
    let done = r2.expect("resume after transient cut must succeed");
    assert_eq!(done as usize, TOTAL);

    let data = std::fs::read(&partial).unwrap();
    assert_eq!(data.len(), TOTAL, "no duplicated or skipped bytes");
    assert!(data.iter().all(|&b| b == 0xAB), "content integrity");
}
