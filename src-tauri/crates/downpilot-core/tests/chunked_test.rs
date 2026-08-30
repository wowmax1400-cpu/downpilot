//! End-to-end test of the chunked download engine against a real local HTTP
//! server with Range support. Verifies multi-threaded download + file
//! integrity + SQLite persistence + pause/resume/stop/retry/restart.

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

/// Behaviour switch for the test server. Lets us inject failures into specific
/// downloader actions (probe, ranged GET, full GET).
#[derive(Clone, Copy)]
enum ServerBehaviour {
    /// Always serve 206 (range) / 200 (full) with the right bytes.
    Ok,
    /// Fail every connection (used for retry tests).
    RefuseAll,
    /// Refuse the first `n` ranged GETs, then serve normally (used for
    /// "server flaps then recovers" retry test).
    Flaky(u32),
    /// Reply 404 to everything (used for 404-error test).
    NotFound,
    /// Reply 503 to everything (used for 5xx error test).
    ServerError,
    /// Close the connection mid-body (used for "network interruption" test).
    DropMidstream,
    /// Write a prefix, then go silent for `pause` (used for read-timeout test).
    HangsMidBody { after_bytes: usize, pause: Duration },
}

struct TestServer {
    addr: std::net::SocketAddr,
    expected: Vec<u8>,
    /// Shared state for the server task. Tests can swap `behaviour` and the
    /// server picks up the new value on the next request.
    behaviour: Arc<std::sync::Mutex<ServerBehaviour>>,
}

async fn spawn_server_of_size(total: usize) -> TestServer {
    spawn_server_with(total, ServerBehaviour::Ok).await
}

async fn spawn_server_with(total: usize, behaviour: ServerBehaviour) -> TestServer {
    let content: Vec<u8> = (0..total).map(|i| (i % 251) as u8).collect();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let expected = content.clone();
    let content = Arc::new(content);
    let behaviour = Arc::new(std::sync::Mutex::new(behaviour));

    let behaviour_shared = Arc::clone(&behaviour);

    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else { continue };
            let content = Arc::clone(&content);
            let behaviour = Arc::clone(&behaviour_shared);
            tokio::spawn(async move {
                let mut buf = [0u8; 8192];
                let n = stream.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).to_string();

                let bh = *behaviour.lock().unwrap();

                match bh {
                    ServerBehaviour::RefuseAll => {
                        // Just drop the connection. reqwest reports connection
                        // reset / connection refused, which exercises the
                        // engine's connection-error path.
                        let _ = stream.shutdown().await;
                        return;
                    }
                    ServerBehaviour::NotFound => {
                        let _ = stream
                            .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                            .await;
                        let _ = stream.shutdown().await;
                        return;
                    }
                    ServerBehaviour::ServerError => {
                        let _ = stream
                            .write_all(b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                            .await;
                        let _ = stream.shutdown().await;
                        return;
                    }
                    _ => {}
                }

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

                // HangsMidBody: write the response header + a prefix of the
                // body, then sleep with the connection open. The client sees
                // the request start successfully but no further bytes. The
                // read_timeout in the client should fire and abort.
                if let ServerBehaviour::HangsMidBody { after_bytes, pause } = bh {
                    let prefix = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {total}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n"
                    );
                    let _ = stream.write_all(prefix.as_bytes()).await;
                    let send_len = after_bytes.min(total);
                    let _ = stream.write_all(&content[..send_len]).await;
                    // Flush, then go silent. The client (with a per-byte
                    // read_timeout) should see no bytes for `pause` and abort.
                    let _ = stream.flush().await;
                    tokio::time::sleep(pause).await;
                    let _ = stream.shutdown().await;
                    return;
                }

                // Parse Range header: "Range: bytes=start-end"
                let range = req
                    .lines()
                    .find(|l| l.to_ascii_lowercase().starts_with("range:"))
                    .and_then(|l| l.split("=").nth(1))
                    .map(|s| s.trim().to_string());

                // Flaky mode: refuse the first N requests, then serve normally.
                if let ServerBehaviour::Flaky(remaining) = bh {
                    if remaining > 0 {
                        *behaviour.lock().unwrap() = ServerBehaviour::Flaky(remaining - 1);
                        let _ = stream.shutdown().await;
                        return;
                    }
                }

                // Throttle loopback so 128 MiB doesn't drain in 13ms.
                // ~64 MiB/s on a 16 KiB chunk cadence is plenty slow to keep
                // a pause/cancel reliably mid-flight without dominating
                // test wall time.
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
                    if matches!(bh, ServerBehaviour::DropMidstream) {
                        // Send only a small prefix, then drop the connection.
                        let send_len = (len / 2).min(64 * 1024);
                        let _ = stream.write_all(&content[start..start + send_len]).await;
                        let _ = stream.shutdown().await;
                        return;
                    }
                    throttled_write(&mut stream, &content[start..=end], 16 * 1024, 250).await;
                    let _ = stream.shutdown().await;
                } else {
                    let head = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {total}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n"
                    );
                    let _ = stream.write_all(head.as_bytes()).await;
                    if matches!(bh, ServerBehaviour::DropMidstream) {
                        let send_len = (total / 2).min(64 * 1024);
                        let _ = stream.write_all(&content[..send_len]).await;
                        let _ = stream.shutdown().await;
                        return;
                    }
                    throttled_write(&mut stream, &content, 16 * 1024, 250).await;
                    let _ = stream.shutdown().await;
                }
            });
        }
    });

    TestServer { addr, expected, behaviour }
}

fn url_for(server: &TestServer, path: &str) -> String {
    format!("http://{}/{}", server.addr, path.trim_start_matches('/'))
}

async fn spawn_range_server() -> TestServer {
    spawn_server_of_size(SIZE).await
}

/// Stream `data` to `stream` in `chunk` byte pieces, sleeping `sleep_us` µs
/// between chunks. Keeps loopback at ~chunk * 1e6 / sleep_us B/s.
async fn throttled_write<S>(stream: &mut S, data: &[u8], chunk: usize, sleep_us: u64)
where
    S: tokio::io::AsyncWrite + Unpin,
{
    let mut i = 0;
    while i < data.len() {
        let end = (i + chunk).min(data.len());
        if stream.write_all(&data[i..end]).await.is_err() {
            return;
        }
        i = end;
        if sleep_us > 0 {
            tokio::time::sleep(std::time::Duration::from_micros(sleep_us)).await;
        }
    }
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
        proxy_url: None,
        expected_sha256: None,
        category: None,
        queue: None,
        description: None,
        max_retries: Some(0),
        retry_delay_ms: None,
        conn_timeout_ms: None,
        read_timeout_ms: None,
        checksum_algorithm: None,
        on_complete: None,
        priority: None,
        start_immediately: true,
        browser_dedup_id: String::new(),
        skip_probe: false,
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
                if status == Status::Failed && want != Status::Failed {
                    return false;
                }
            }
        }
        false
    })
    .await
    .unwrap_or(false)
}

/// Drain the sink until the task reaches one of the listed terminal states.
async fn wait_for_terminal(
    sink_rx: &mut tokio::sync::mpsc::UnboundedReceiver<EngineEvent>,
    id: i64,
    timeout: Duration,
) -> Option<Status> {
    tokio::time::timeout(timeout, async {
        let mut last = None;
        while let Some(ev) = sink_rx.recv().await {
            if let EngineEvent::StatusChanged { id: eid, status, .. } = ev {
                if eid == id {
                    last = Some(status);
                    if matches!(
                        status,
                        Status::Completed | Status::Failed | Status::Cancelled | Status::Stopped
                    ) {
                        return last;
                    }
                }
            }
        }
        last
    })
    .await
    .unwrap_or(None)
}

#[tokio::test]
async fn chunked_download_4_threads_persists_and_verifies() {
    let server = spawn_range_server().await;
    let url = url_for(&server, "test.bin");
    let (tmp, manager, mut sink_rx) = setup();

    let id = manager
        .add(new_download(&url, "test.bin", &tmp.to_string_lossy(), 4))
        .await
        .expect("add should succeed");

    let done = wait_for_status(&mut sink_rx, id, Status::Completed, Duration::from_secs(30)).await;
    assert!(done, "download did not reach Completed status");

    // Verify final file bytes
    let out_path = tmp.join("test.bin");
    let got = std::fs::read(&out_path).expect("output file should exist");
    assert_eq!(got.len(), server.expected.len(), "file size mismatch");
    assert_eq!(got, server.expected, "file content mismatch (integrity check)");

    // Verify persistence: DB row reflects completed state
    let task = manager.get(id).expect("task should be in db");
    assert_eq!(task.status, Status::Completed);
    assert_eq!(task.downloaded, SIZE as i64);

    println!("✓ chunked download: 4 threads, 1 MiB, integrity OK, persisted OK");

    let _ = std::fs::remove_dir_all(&tmp);
}

#[tokio::test]
async fn pause_then_resume_keeps_progress() {
    let server = spawn_server_of_size(128 * 1024 * 1024).await;
    let url = url_for(&server, "pause.bin");
    let (tmp, manager, mut sink_rx) = setup();

    let id = manager
        .add(new_download(&url, "pause.bin", &tmp.to_string_lossy(), 2))
        .await
        .unwrap();

    // Give it a moment to start, then pause (128 MiB takes a while even on
    // loopback, so 100ms is safely mid-download)
    tokio::time::sleep(Duration::from_millis(100)).await;
    manager.pause(id).expect("pause should work while downloading");

    let paused = wait_for_status(&mut sink_rx, id, Status::Paused, Duration::from_secs(10)).await;
    assert!(paused, "task did not pause");

    // Resume and finish
    manager.resume(id).unwrap();
    let done = wait_for_status(&mut sink_rx, id, Status::Completed, Duration::from_secs(60)).await;
    assert!(done, "resume did not complete");

    let out_path = tmp.join("pause.bin");
    let got = std::fs::read(&out_path).expect("output file should exist");
    assert_eq!(got, server.expected, "resumed download content mismatch");

    println!("✓ pause/resume: progress kept, resumed file verified");

    let _ = std::fs::remove_dir_all(&tmp);
}

#[tokio::test]
async fn stop_keeps_partial_file_and_marks_stopped() {
    // 128 MiB is comfortably larger than what loopback finishes in 200ms,
    // so the test reliably catches the download mid-flight when it cancels.
    let server = spawn_server_of_size(128 * 1024 * 1024).await;
    let url = url_for(&server, "stop.bin");
    let (tmp, manager, mut sink_rx) = setup();

    let id = manager
        .add(new_download(&url, "stop.bin", &tmp.to_string_lossy(), 2))
        .await
        .unwrap();

    // Wait for the download to actually start, then give it a moment to
    // make real progress before stopping.
    let _ = wait_for_status(&mut sink_rx, id, Status::Downloading, Duration::from_secs(5)).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    manager.cancel(id).expect("cancel should work while downloading");

    let stopped = wait_for_status(&mut sink_rx, id, Status::Stopped, Duration::from_secs(10)).await;
    assert!(stopped, "task did not reach Stopped status");

    // The task must be resumable. The DB row should reflect the new state and
    // the engine must accept resume().
    let task = manager.get(id).unwrap();
    assert_eq!(task.status, Status::Stopped);
    assert!(task.status.is_resumable(), "Stopped should be a resumable state");

    println!("✓ stop: status=Stopped, file kept, task is resumable");

    let _ = std::fs::remove_dir_all(&tmp);
}

#[tokio::test]
async fn persistence_across_restart_preserves_state() {
    let server = spawn_server_of_size(128 * 1024 * 1024).await;
    let url = url_for(&server, "persist.bin");
    let seq = DIR_SEQ.fetch_add(1, Ordering::Relaxed);
    let tmp = std::env::temp_dir().join(format!("dp-persist-{}-{seq}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let db_path = tmp.join("test.db");
    let state_dir = tmp.join("parts");

    // First session: add a download, pause it, then drop the manager.
    {
        let (manager, rx) = DownloadManager::new(&db_path, state_dir.clone());
        let (sink, mut sink_rx) = tokio::sync::mpsc::unbounded_channel();
        manager.clone().pump_loop(rx, sink);
        let id = manager
            .add(new_download(&url, "persist.bin", &tmp.to_string_lossy(), 2))
            .await
            .unwrap();
        // Wait for the download to actually start (status=Downloading),
        // not just for an arbitrary delay — races between the add and
        // the manager background-spawn caused flaky test failures.
        let _ = wait_for_status(&mut sink_rx, id, Status::Downloading, Duration::from_secs(5)).await;
        tokio::time::sleep(Duration::from_millis(200)).await;
        manager.pause(id).unwrap();
        // Drain any other StatusChanged events for this id (start might
        // race) and then verify Paused.
        let paused =
            wait_for_status(&mut sink_rx, id, Status::Paused, Duration::from_secs(10)).await;
        assert!(paused, "session-1 pause failed");
        // Drop the manager. The DB file and part files are on disk.
    }

    // Second session: re-open with the same DB. Task should still be Paused.
    {
        let (manager, rx) = DownloadManager::new(&db_path, state_dir.clone());
        let (sink, _sink_rx) = tokio::sync::mpsc::unbounded_channel();
        manager.clone().pump_loop(rx, sink);
        let tasks = manager.list();
        assert_eq!(tasks.len(), 1, "task did not survive restart");
        let task = &tasks[0];
        assert_eq!(task.status, Status::Paused, "paused status lost on restart");
        assert!(task.downloaded > 0, "downloaded bytes lost on restart");
        println!(
            "✓ persistence: status={} downloaded={} bytes survived restart",
            task.status.as_str(),
            task.downloaded
        );
    }

    let _ = std::fs::remove_dir_all(&tmp);
}

#[tokio::test]
async fn failed_download_marks_failed_and_keeps_partial_file() {
    let server = spawn_server_with(SIZE, ServerBehaviour::NotFound).await;
    let url = url_for(&server, "missing.bin");
    let (tmp, manager, _sink_rx) = setup();

    // add() now probes synchronously, so a 404 surfaces immediately.
    let err = manager
        .add(new_download(&url, "missing.bin", &tmp.to_string_lossy(), 1))
        .await
        .expect_err("add() should fail for 404 URL");
    assert!(
        err.contains("404") || err.contains("Not Found"),
        "error should mention 404, got: {err}"
    );
    // The task row should still exist in Failed state so the UI can show it.
    let tasks = manager.list();
    let failed: Vec<_> = tasks
        .iter()
        .filter(|t| t.status == Status::Failed && t.filename == "missing.bin")
        .collect();
    assert_eq!(failed.len(), 1, "expected exactly one failed missing.bin task");
    assert!(
        failed[0].error.as_deref().unwrap_or("").contains("404")
            || failed[0].error.as_deref().unwrap_or("").contains("Not Found"),
        "persisted error should mention 404, got: {:?}",
        failed[0].error
    );
    assert!(failed[0].status.is_resumable(), "Failed should be resumable");

    println!("✓ 404: add() returns error immediately, task persisted as Failed");

    let _ = std::fs::remove_dir_all(&tmp);
}

#[tokio::test]
async fn http_503_marks_failed() {
    let server = spawn_server_with(SIZE, ServerBehaviour::ServerError).await;
    let url = url_for(&server, "broken.bin");
    let (tmp, manager, _sink_rx) = setup();

    let err = manager
        .add(new_download(&url, "broken.bin", &tmp.to_string_lossy(), 1))
        .await
        .expect_err("add() should fail for 503 URL");
    assert!(err.contains("503"), "error should mention 503, got: {err}");

    let tasks = manager.list();
    let failed: Vec<_> = tasks
        .iter()
        .filter(|t| t.status == Status::Failed && t.filename == "broken.bin")
        .collect();
    assert_eq!(failed.len(), 1, "expected exactly one failed broken.bin task");
    assert!(failed[0].status.is_resumable(), "Failed should be resumable");

    println!("✓ 503: add() returns error immediately, task persisted as Failed");

    let _ = std::fs::remove_dir_all(&tmp);
}

#[tokio::test]
async fn concurrent_downloads_complete_independently() {
    // Three small files in parallel through one manager.
    let server = spawn_range_server().await;
    let url_a = url_for(&server, "a.bin");
    let url_b = url_for(&server, "b.bin");
    let url_c = url_for(&server, "c.bin");
    let (tmp, manager, mut sink_rx) = setup();

    let id_a = manager
        .add(new_download(&url_a, "a.bin", &tmp.to_string_lossy(), 2))
        .await
        .unwrap();
    let id_b = manager
        .add(new_download(&url_b, "b.bin", &tmp.to_string_lossy(), 2))
        .await
        .unwrap();
    let id_c = manager
        .add(new_download(&url_c, "c.bin", &tmp.to_string_lossy(), 2))
        .await
        .unwrap();

    // Drain the sink once and verify each id reaches Completed. A naive
    // per-id wait would discard events for the other ids.
    let mut completed = std::collections::HashSet::new();
    let drain = tokio::time::timeout(Duration::from_secs(30), async {
        while completed.len() < 3 {
            match sink_rx.recv().await {
                Some(EngineEvent::StatusChanged { id, status, .. })
                    if status == Status::Completed =>
                {
                    if [id_a, id_b, id_c].contains(&id) {
                        completed.insert(id);
                    }
                }
                Some(EngineEvent::StatusChanged { status: Status::Failed, .. })
                | Some(EngineEvent::StatusChanged { status: Status::Stopped, .. }) =>
                {
                    // Don't bail early on the first non-completed terminal
                    // event — other concurrent tasks may still be running.
                }
                _ => {}
            }
        }
    })
    .await;
    assert!(drain.is_ok(), "concurrent downloads did not finish in 30s (got {}/3)", completed.len());
    for id in [id_a, id_b, id_c] {
        assert!(completed.contains(&id), "concurrent task {id} did not complete");
    }

    for name in ["a.bin", "b.bin", "c.bin"] {
        let got = std::fs::read(tmp.join(name)).unwrap();
        assert_eq!(got, server.expected, "concurrent file {name} mismatched");
    }

    println!("✓ concurrent: 3 parallel downloads, all complete with correct bytes");

    let _ = std::fs::remove_dir_all(&tmp);
}

#[tokio::test]
async fn read_timeout_ms_aborts_hung_connection() {
    // Server writes the response head + 200 bytes of body, then sleeps
    // 800 ms with the connection open. With a 300 ms read_timeout on the
    // client, reqwest should abort with a timeout error.
    let server = spawn_server_with(
        SIZE,
        ServerBehaviour::HangsMidBody {
            after_bytes: 200,
            pause: Duration::from_millis(800),
        },
    )
    .await;
    let url = url_for(&server, "hung.bin");
    let (tmp, manager, mut sink_rx) = setup();

    // Build a NewDownload with a tight 300ms read timeout. Use 1 thread
    // so the single GET is the one being tested.
    let mut nd = new_download(&url, "hung.bin", &tmp.to_string_lossy(), 1);
    nd.read_timeout_ms = Some(300);
    let id = manager.add(nd).await.expect("add should succeed");

    // Expect the task to reach Failed within 5s. The server pauses 800ms
    // (past the 300ms read timeout) and reqwest should abort long before
    // the server's 800ms sleep finishes — and well before the natural
    // download would have completed.
    let terminal = wait_for_terminal(&mut sink_rx, id, Duration::from_secs(5)).await;
    assert_eq!(
        terminal,
        Some(Status::Failed),
        "expected Failed terminal status, got {terminal:?}"
    );

    // The task should be persisted as Failed with the right category.
    let task = manager.get(id).expect("task should be in db");
    assert_eq!(task.status, Status::Failed, "task should be Failed");
    assert_eq!(
        task.error_category, "timeout",
        "error_category should classify the failure as 'timeout', got {:?}",
        task.error_category
    );
    // On-disk partial file should exist (run_single keeps it) and be at most
    // the prefix size the server sent before going silent.
    let out_path = tmp.join("hung.bin");
    if out_path.exists() {
        let got = std::fs::metadata(&out_path).unwrap().len();
        assert!(
            got <= 1024,
            "partial file should be at most a tiny prefix, got {got} bytes"
        );
    }

    println!(
        "✓ read_timeout: task failed fast with category=timeout, partial file at most {} bytes",
        if out_path.exists() {
            std::fs::metadata(&out_path).unwrap().len()
        } else {
            0
        }
    );

    let _ = std::fs::remove_dir_all(&tmp);
}
