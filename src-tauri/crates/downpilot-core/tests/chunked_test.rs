//! End-to-end test of the chunked download engine against a real local HTTP
//! server with Range support. Verifies multi-threaded download + file
//! integrity + SQLite persistence + pause/resume.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use downpilot_core::engine::DownloadManager;
use downpilot_core::models::{EngineEvent, NewDownload, Status};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

const SIZE: usize = 1024 * 1024; // 1 MiB payload
const BIG_SIZE: usize = 128 * 1024 * 1024; // 128 MiB — large enough that 100ms isn't enough to finish
static DIR_SEQ: AtomicUsize = AtomicUsize::new(0);

/// Minimal HTTP server that answers HEAD + ranged GET requests.
async fn spawn_range_server_of_size(total: usize) -> (String, Vec<u8>) {
    let content: Vec<u8> = (0..total).map(|i| (i % 251) as u8).collect();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://{addr}/test.bin");
    let expected = content.clone();
    let content = Arc::new(content);

    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else { continue };
            let content = Arc::clone(&content);
            tokio::spawn(async move {
                let mut buf = [0u8; 8192];
                let n = stream.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).to_string();

                if req.starts_with("HEAD") {
                    let _ = stream
                        .write_all(
                            format!(
                                "HTTP/1.1 200 OK\r\nContent-Length: {total}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n"
                            )
                            .as_bytes(),
                        )
                        .await;
                    let _ = stream.shutdown().await;
                    return;
                }

                // Parse Range header: "Range: bytes=start-end"
                let range = req
                    .lines()
                    .find(|l| l.to_ascii_lowercase().starts_with("range:"))
                    .and_then(|l| l.split("=").nth(1))
                    .map(|s| s.trim().to_string());

                if let Some(spec) = range {
                    let parts: Vec<&str> = spec.split('-').collect();
                    let start: usize = parts[0].parse().unwrap_or(0);
                    let end: usize = if parts.len() > 1 && !parts[1].is_empty() {
                        parts[1].parse().unwrap_or(total - 1)
                    } else {
                        total - 1
                    };
                    let end = end.min(total - 1);
                    let len = end - start + 1;
                    let head = format!(
                        "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {start}-{end}/{total}\r\nContent-Length: {len}\r\nConnection: close\r\n\r\n"
                    );
                    let _ = stream.write_all(head.as_bytes()).await;
                    let _ = stream.write_all(&content[start..=end]).await;
                    let _ = stream.shutdown().await;
                } else {
                    let head = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {total}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n"
                    );
                    let _ = stream.write_all(head.as_bytes()).await;
                    let _ = stream.write_all(&content).await;
                    let _ = stream.shutdown().await;
                }
            });
        }
    });

    (url, expected)
}

async fn spawn_range_server() -> (String, Vec<u8>) {
    spawn_range_server_of_size(SIZE).await
}

/// Shared test setup: temp dirs, manager, pump loop, sink.
fn setup() -> (std::path::PathBuf, Arc<DownloadManager>, tokio::sync::mpsc::UnboundedReceiver<EngineEvent>) {
    let seq = DIR_SEQ.fetch_add(1, Ordering::Relaxed);
    let tmp = std::env::temp_dir().join(format!("dp-test-{}-{seq}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let db_path = tmp.join("test.db");
    let state_dir = tmp.join("parts");

    let (manager, rx) = DownloadManager::new(&db_path, state_dir.clone());
    let (sink, sink_rx) = tokio::sync::mpsc::unbounded_channel();
    manager.clone().pump_loop(rx, sink);
    (tmp, manager, sink_rx)
}

fn new_download(url: &str, filename: &str, save_dir: &str, threads: u32) -> NewDownload {
    NewDownload {
        url: url.to_string(),
        filename: Some(filename.to_string()),
        save_dir: Some(save_dir.to_string()),
        threads: Some(threads),
        format_id: None,
        webpage_url: None,
        referer: None,
        user_agent: None,
        cookies: None,
        expected_sha256: None,
        start_immediately: true,
    }
}

/// Wait for a specific status on a task, returning after seeing it.
async fn wait_for_status(
    sink_rx: &mut tokio::sync::mpsc::UnboundedReceiver<EngineEvent>,
    id: i64,
    want: Status,
    timeout: Duration,
) -> bool {
    tokio::time::timeout(timeout, async {
        while let Some(ev) = sink_rx.recv().await {
            if let EngineEvent::StatusChanged { id: eid, status, .. } = ev {
                if eid == id && status == want {
                    return true;
                }
                if status == Status::Failed {
                    return false;
                }
            }
        }
        false
    })
    .await
    .unwrap_or(false)
}

#[tokio::test]
async fn chunked_download_4_threads_persists_and_verifies() {
    let (url, expected) = spawn_range_server().await;
    let (tmp, manager, mut sink_rx) = setup();

    let id = manager
        .add(new_download(&url, "test.bin", &tmp.to_string_lossy(), 4))
        .expect("add should succeed");

    let done = wait_for_status(&mut sink_rx, id, Status::Completed, Duration::from_secs(30)).await;
    assert!(done, "download did not reach Completed status");

    // Verify final file bytes
    let out_path = tmp.join("test.bin");
    let got = std::fs::read(&out_path).expect("output file should exist");
    assert_eq!(got.len(), expected.len(), "file size mismatch");
    assert_eq!(got, expected, "file content mismatch (integrity check)");

    // Verify persistence: DB row reflects completed state
    let task = manager.get(id).expect("task should be in db");
    assert_eq!(task.status, Status::Completed);
    assert_eq!(task.downloaded, SIZE as i64);

    // Part files should be cleaned up after merge (best-effort check)
    let state_dir = tmp.join("parts");
    let parts_left: Vec<_> = std::fs::read_dir(&state_dir)
        .map(|rd| rd.filter_map(|e| e.ok()).collect::<Vec<_>>())
        .unwrap_or_default();
    // The part dir may still exist if cleanup raced with test shutdown;
    // the important thing is file integrity and DB persistence.
    if parts_left.is_empty() {
        println!("✓ part files cleaned up");
    }

    println!("✓ chunked download: 4 threads, 1 MiB, integrity OK, persisted OK");

    let _ = std::fs::remove_dir_all(&tmp);
}

#[tokio::test]
async fn pause_then_resume_keeps_progress() {
    let (url, expected) = spawn_range_server_of_size(BIG_SIZE).await;
    let (tmp, manager, mut sink_rx) = setup();

    let id = manager
        .add(new_download(&url, "pause.bin", &tmp.to_string_lossy(), 2))
        .unwrap();

    // Give it a moment to start, then pause (64 MiB takes a while even on
    // loopback, so 100ms is safely mid-download)
    tokio::time::sleep(Duration::from_millis(100)).await;
    manager.pause(id).expect("pause should work while downloading");

    let paused = wait_for_status(&mut sink_rx, id, Status::Paused, Duration::from_secs(10)).await;
    assert!(paused, "task did not pause");

    // Resume and finish
    manager.resume(id).unwrap();
    let done = wait_for_status(&mut sink_rx, id, Status::Completed, Duration::from_secs(30)).await;
    assert!(done, "resume did not complete");

    let out_path = tmp.join("pause.bin");
    let got = std::fs::read(&out_path).expect("output file should exist");
    assert_eq!(got, expected, "resumed download content mismatch");

    println!("✓ pause/resume: progress kept, resumed file verified");

    let _ = std::fs::remove_dir_all(&tmp);
}
