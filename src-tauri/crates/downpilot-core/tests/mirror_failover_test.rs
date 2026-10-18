//! Mirror/multi-source failover — engine-level end-to-end proof.
//!
//! Scenario 1 (`mirror_failover_completes_from_second_source`): a task is
//! enqueued with a dead primary URL (connection reset — retryable) and
//! one live mirror. The engine must fail the primary attempt, rotate to
//! the mirror, persist the new `mirror_index`, and finish byte-exact.
//!
//! Scenario 2 (`mirror_exhaustion_falls_through_to_failure`): primary and
//! every mirror are dead. The task must end in `Failed` (not loop
//! forever) and `mirror_index` must point at the last source.
//!
//! Scenario 3 (`non_retryable_error_does_not_failover`): the primary
//! answers 404 (authoritative). No rotation: `mirror_index` stays 0.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use downpilot_core::engine::DownloadManager;
use downpilot_core::models::{EngineEvent, NewDownload, Status};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

static DIR_SEQ: AtomicUsize = AtomicUsize::new(0);

const PAYLOAD: &[u8] = b"mirror-payload-0123456789-abcdefghijklmnopqrstuvwxyz";

/// Serve exactly one HTTP request from `PAYLOAD`, then close.
async fn spawn_static_server() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                continue;
            };
            let mut buf = vec![0u8; 4096];
            let _ = stream.read(&mut buf).await; // request head; GET has no body
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n",
                PAYLOAD.len()
            );
            let _ = stream.write_all(head.as_bytes()).await;
            let _ = stream.write_all(PAYLOAD).await;
            let _ = stream.shutdown().await;
        }
    });
    addr
}

/// Accept then immediately reset — client sees a connection-level
/// failure (retryable class) that must trigger mirror failover.
async fn spawn_dead_server() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            if let Ok((stream, _)) = listener.accept().await {
                drop(stream); // RST without a response
            }
        }
    });
    addr
}

async fn spawn_404_server() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                continue;
            };
            let mut buf = vec![0u8; 4096];
            let _ = stream.read(&mut buf).await;
            let _ = stream
                .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .await;
            let _ = stream.shutdown().await;
        }
    });
    addr
}

fn setup() -> (
    tempfile::TempDir,
    Arc<DownloadManager>,
    tokio::sync::mpsc::UnboundedReceiver<EngineEvent>,
) {
    let seq = DIR_SEQ.fetch_add(1, Ordering::SeqCst);
    let tmp = tempfile::Builder::new()
        .prefix(&format!("dp-mirror-{seq}-"))
        .tempdir()
        .unwrap();
    let db_path = tmp.path().join("test.db");
    let (manager, rx) = DownloadManager::new(&db_path, tmp.path().join("parts"));
    let (sink, sink_rx) = tokio::sync::mpsc::unbounded_channel();
    manager.clone().pump_loop(rx, sink);
    (tmp, manager, sink_rx)
}

fn new_download(url: &str, dir: &std::path::Path, mirrors: Vec<String>) -> NewDownload {
    NewDownload {
        url: url.to_string(),
        filename: Some("mirror.bin".into()),
        save_dir: Some(dir.to_string_lossy().into_owned()),
        threads: Some(1),
        format_id: None,
        webpage_url: None,
        referer: None,
        user_agent: None,
        cookies: None,
        proxy_url: None,
        expected_sha256: None,
        category: None,
        queue: None,
        description: None,
        max_retries: Some(0), // failover must not depend on retry budget
        retry_delay_ms: Some(20),
        conn_timeout_ms: Some(2000),
        read_timeout_ms: Some(5000),
        checksum_algorithm: None,
        on_complete: None,
        priority: None,
        start_immediately: true,
        browser_dedup_id: String::new(),
        skip_probe: true,
        mirrors,
    }
}

/// Poll `manager.get()` until `want` or timeout. Matches the house
/// pattern used by retry_budget_test.
async fn wait_for(
    manager: &Arc<DownloadManager>,
    id: i64,
    want: Status,
    timeout: Duration,
) -> bool {
    let deadline = tokio::time::Instant::now() + timeout;
    while tokio::time::Instant::now() < deadline {
        if let Some(t) = manager.get(id) {
            if t.status == want {
                return true;
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    false
}

#[tokio::test]
async fn mirror_failover_completes_from_second_source() {
    let (tmp, mgr, _mut_rx) = setup();
    let dead = spawn_dead_server().await;
    let live = spawn_static_server().await;

    let nd = new_download(
        &format!("http://{dead}/mirror.bin"),
        tmp.path(),
        vec![format!("http://{live}/mirror.bin")],
    );
    let id = mgr.add(nd).await.expect("add");

    assert!(
        wait_for(&mgr, id, Status::Completed, Duration::from_secs(20)).await,
        "task must complete via the mirror after primary failure"
    );

    let bytes = std::fs::read(tmp.path().join("mirror.bin")).unwrap();
    assert_eq!(bytes, PAYLOAD, "mirror bytes must be byte-exact");

    let task = mgr.get(id).expect("task row survives");
    assert_eq!(
        task.mirror_index, 1,
        "mirror_index must persist the failover (1 = first mirror)"
    );
    assert_eq!(task.mirrors.len(), 1);
}

#[tokio::test]
async fn mirror_exhaustion_falls_through_to_failure() {
    let (tmp, mgr, _rx) = setup();
    let dead_a = spawn_dead_server().await;
    let dead_b = spawn_dead_server().await;

    let nd = new_download(
        &format!("http://{dead_a}/mirror.bin"),
        tmp.path(),
        vec![format!("http://{dead_b}/mirror.bin")],
    );
    let id = mgr.add(nd).await.expect("add");

    assert!(
        wait_for(&mgr, id, Status::Failed, Duration::from_secs(30)).await,
        "task must fail once every source is exhausted"
    );

    let task = mgr.get(id).expect("task row survives");
    assert_eq!(
        task.mirror_index, 1,
        "rotation must stop at the last source, not loop"
    );
}

#[tokio::test]
async fn non_retryable_error_does_not_failover() {
    let (tmp, mgr, _rx) = setup();
    let not_found = spawn_404_server().await;
    let live = spawn_static_server().await;

    let nd = new_download(
        &format!("http://{not_found}/mirror.bin"),
        tmp.path(),
        vec![format!("http://{live}/mirror.bin")],
    );
    let id = mgr.add(nd).await.expect("add");

    assert!(
        wait_for(&mgr, id, Status::Failed, Duration::from_secs(15)).await,
        "404 is authoritative: task must fail, not rotate"
    );

    let task = mgr.get(id).expect("task row survives");
    assert_eq!(
        task.mirror_index, 0,
        "non-retryable failure must not consume a mirror"
    );
}
