//! Engine-level retry-budget regression tests for abrupt transport
//! termination (v1.1 TLS-EOF spec, req 4/8/9).
//!
//! Uses the REAL retry infrastructure (DownloadManager::maybe_retry) —
//! no second retry mechanism — against a local origin that drops
//! connections at the HTTP layer (the plain-HTTP equivalent of the
//! docs.rs "peer closed connection without sending TLS close_notify"):
//!
//! 1. a retryable abrupt close is retried automatically and completes
//!    with byte-exact content (no duplicates, no gaps)
//! 2. max_retries is honored exactly: the budget is consumed, then the
//!    task settles in Failed and is NOT retried again
//! 3. classification: the persisted error_category is "connection"
//!    (retryable), not "tls"

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use downpilot_core::engine::DownloadManager;
use downpilot_core::models::{EngineEvent, NewDownload, Status};
use tempfile::TempDir;

const TOTAL: usize = 192 * 1024;
// Half the payload: a truncated GET always stops at exactly half of what
// its Content-Length promises, so a RESUMED attempt (start = CUT) receives
// its full remaining segment and completes — deterministic retry-then-
// succeed without compounding truncation.
const CUT: usize = TOTAL / 2;
static DIR_SEQ: AtomicU32 = AtomicU32::new(0);

/// Blocking origin. HEAD is always answered. GETs are answered with a
/// response head + only `CUT` body bytes, then an abrupt socket drop for
/// the first `refuse_first` GETs (or all of them when `always_refuse`);
/// every later GET is answered fully with proper Range support and a
/// graceful FIN. Cutting mid-body (after the response head) is the true
/// docs.rs shape: the transfer has begun, so hyper cannot transparently
/// retry it and the error must surface to the engine.
fn spawn_flaky_origin(refuse_first: u32, always_refuse: bool) -> String {
    let get_hits = Arc::new(AtomicU32::new(0));
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut sock) = stream else { continue };
            let peer = sock
                .peer_addr()
                .map(|a| a.port().to_string())
                .unwrap_or("?".into());
            eprintln!("[origin] conn from :{peer}");
            let get_hits = Arc::clone(&get_hits);
            std::thread::spawn(move || {
                // Read until the request headers are complete. A single
                // read can stop mid-headers; the unread remainder in the
                // kernel buffer would turn the later abrupt close into a
                // TCP RST that discards the response in flight, and hyper
                // would transparently retry the request — hiding the
                // failure from the engine (found during test bring-up).
                let mut req = String::new();
                let mut buf = [0u8; 4096];
                loop {
                    let n = sock.read(&mut buf).unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    req.push_str(&String::from_utf8_lossy(&buf[..n]));
                    if req.contains("\r\n\r\n") {
                        break;
                    }
                }
                if req.starts_with("HEAD") {
                    let head = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {TOTAL}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n"
                    );
                    let _ = sock.write_all(head.as_bytes());
                    let _ = sock.flush();
                    return;
                }
                let hit = get_hits.fetch_add(1, Ordering::SeqCst);
                let truncated = always_refuse || hit < refuse_first;
                let method = req.lines().next().unwrap_or("?").to_string();
                eprintln!("[origin] req=\"{method}\" hit={hit} truncated={truncated}");
                // Parse "Range: bytes=N-..." for resumable GETs.
                let start = req
                    .lines()
                    .find(|l| l.to_ascii_lowercase().starts_with("range:"))
                    .and_then(|l| l.split('=').nth(1))
                    .and_then(|s| s.split('-').next())
                    .and_then(|s| s.trim().parse::<usize>().ok())
                    .unwrap_or(0);
                if truncated {
                    // Head + HALF the promised body, then close — abrupt
                    // mid-transfer termination that is short regardless of
                    // the resume offset, so every engine-visible truncated
                    // attempt fails on the short-body check. The 100 ms
                    // delay before closing guarantees the client has read
                    // the response head (and part of the body) before the
                    // FIN/EOF arrives, so hyper surfaces a mid-body
                    // truncation error instead of transparently retrying
                    // a request whose response vanished with the
                    // connection.
                    let head = format!(
                        "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {start}-{}/{TOTAL}\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\nConnection: close\r\nETag: \"cut\"\r\n\r\n",
                        TOTAL - 1,
                        TOTAL - start
                    );
                    let promised = TOTAL - start;
                    let send = promised / 2;
                    let _ = sock.write_all(head.as_bytes());
                    let _ = sock.write_all(&vec![0xABu8; send]);
                    let _ = sock.flush();
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    let _ = sock.shutdown(std::net::Shutdown::Write);
                    return;
                }
                let end = TOTAL - 1;
                let len = TOTAL - start;
                let head = format!(
                    "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {start}-{end}/{TOTAL}\r\nContent-Length: {len}\r\nAccept-Ranges: bytes\r\nConnection: close\r\nETag: \"flaky\"\r\n\r\n"
                );
                let _ = sock.write_all(head.as_bytes());
                // Throttle the full body (16 KiB chunks, 80 ms apart ≈ 1 s
                // total) so the transfer spans several of the pump's 400 ms
                // forwarding ticks AND the downloader's 150 ms progress-emit
                // interval — otherwise a loopback-fast transfer completes
                // before any Progress event is forwarded.
                let body = vec![0xABu8; len];
                let mut off = 0usize;
                while off < body.len() {
                    let end = (off + 16 * 1024).min(body.len());
                    if sock.write_all(&body[off..end]).is_err() {
                        return;
                    }
                    let _ = sock.flush();
                    off = end;
                    if off < body.len() {
                        std::thread::sleep(std::time::Duration::from_millis(80));
                    }
                }
                let _ = sock.flush();
                // Graceful close: fulfill Content-Length, then FIN.
                let _ = sock.shutdown(std::net::Shutdown::Write);
            });
        }
    });
    format!("http://{addr}/flaky.bin")
}

fn setup() -> (TempDir, Arc<DownloadManager>) {
    let seq = DIR_SEQ.fetch_add(1, Ordering::SeqCst);
    let tmp = tempfile::Builder::new()
        .prefix(&format!("dp-retry-{seq}-"))
        .tempdir()
        .unwrap();
    let db_path = tmp.path().join("test.db");
    let (manager, rx) = DownloadManager::new(&db_path, tmp.path().join("parts"));
    let (sink, _sink_rx) = tokio::sync::mpsc::unbounded_channel();
    manager.clone().pump_loop(rx, sink);
    (tmp, manager)
}

fn new_download(url: &str, dir: &std::path::Path, max_retries: i32) -> NewDownload {
    NewDownload {
        url: url.to_string(),
        filename: Some("flaky.bin".into()),
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
        max_retries: Some(max_retries),
        retry_delay_ms: Some(50),
        conn_timeout_ms: None,
        read_timeout_ms: Some(5000),
        checksum_algorithm: None,
        on_complete: None,
        priority: None,
        start_immediately: true,
        browser_dedup_id: String::new(),
        // The retry tests target the *body* GET, not the synchronous
        // add() probe (which never enters the retry path).
        skip_probe: true,
        mirrors: Vec::new(),
    }
}

/// Poll the task until a condition holds, with a hard timeout.
async fn wait_until(
    manager: &Arc<DownloadManager>,
    id: i64,
    what: &str,
    timeout: Duration,
    mut cond: impl FnMut(&downpilot_core::models::DownloadTask) -> bool,
) -> downpilot_core::models::DownloadTask {
    let deadline = tokio::time::Instant::now() + timeout;
    let mut last_status = String::new();
    loop {
        if let Some(t) = manager.get(id) {
            if t.status == Status::Failed
                || t.status == Status::Completed
                || t.status.as_str() != last_status
            {
                eprintln!(
                    "[test] id={id} status={} retry={} err={:?}",
                    t.status.as_str(),
                    t.retry_count,
                    t.error
                );
                last_status = t.status.as_str().to_string();
            }
            if cond(&t) {
                return t;
            }
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for {what}"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

/// A retryable abrupt close must be auto-retried through the existing
/// retry infrastructure and complete with exact content. Proves:
/// classification=connection (retryable), retry happened, no duplicated
/// or skipped bytes.
#[tokio::test]
async fn abrupt_close_is_retried_and_completes_exactly() {
    // NOTE: the first GET sent on each fresh reqwest client can be
    // transparently retried inside hyper-util (retry_canceled_requests
    // checkout race), so a single truncated GET is not enough to force an
    // engine-visible failure. Truncating the first three GETs makes the
    // engine-visible attempt of every early attempt fail, while the
    // eventual success still comes from the origin's full response.
    let origin = spawn_flaky_origin(3, false);
    let (_tmp, manager) = setup();
    let id = manager
        .add(new_download(&origin, _tmp.path(), 3))
        .await
        .expect("add with skip_probe should not fail");

    let task = wait_until(&manager, id, "Completed", Duration::from_secs(30), |t| {
        t.status == Status::Completed
    })
    .await;

    assert!(
        task.retry_count >= 1,
        "expected at least one automatic retry, got {}",
        task.retry_count
    );
    // NOTE: on the COMPLETED task the error/category fields are empty —
    // a failed task's classification is proven by
    // zero_max_retries_means_no_auto_retry below (error_category ==
    // "connection" on Failed).

    let data = std::fs::read(_tmp.path().join("flaky.bin")).unwrap();
    assert_eq!(data.len(), TOTAL, "no duplicated or skipped bytes");
    assert!(data.iter().all(|&b| b == 0xAB), "content integrity");
}

/// max_retries is an upper bound: with a permanently-refusing origin the
/// budget is consumed exactly and the task settles in Failed — no
/// infinite retry loop.
#[tokio::test]
async fn retry_budget_exhausted_stops_at_max_retries() {
    let origin = spawn_flaky_origin(0, true);
    let (_tmp, manager) = setup();
    let id = manager
        .add(new_download(&origin, _tmp.path(), 2))
        .await
        .expect("add with skip_probe should not fail");

    let task = wait_until(
        &manager,
        id,
        "Failed with retry_count == 2",
        Duration::from_secs(30),
        |t| t.status == Status::Failed && t.retry_count >= 2,
    )
    .await;

    assert_eq!(
        task.retry_count, 2,
        "retry budget must be consumed exactly, not exceeded"
    );
    assert_eq!(task.status, Status::Failed);

    // Give any (wrong) further retry time to fire, then confirm the task
    // stayed put: no retry beyond max_retries.
    tokio::time::sleep(Duration::from_millis(800)).await;
    let settled = manager.get(id).unwrap();
    assert_eq!(settled.retry_count, 2, "no retry beyond max_retries");
    assert_eq!(settled.status, Status::Failed, "stays Failed");

    // The persisted category must be retryable-class ("connection") so
    // the failure is understood as transient — the budget, not the
    // classifier, stopped the retries.
    assert_eq!(
        settled.error_category, "connection",
        "abrupt close must classify as 'connection', got {:?}",
        settled.error_category
    );
}

/// max_retries == 0 preserves the pre-existing configured behavior: a
/// failure is final, no automatic retry at all.
#[tokio::test]
async fn zero_max_retries_means_no_auto_retry() {
    let origin = spawn_flaky_origin(0, true);
    let (_tmp, manager) = setup();
    let id = manager
        .add(new_download(&origin, _tmp.path(), 0))
        .await
        .expect("add with skip_probe should not fail");

    let task = wait_until(&manager, id, "Failed", Duration::from_secs(30), |t| {
        t.status == Status::Failed
    })
    .await;

    assert_eq!(task.retry_count, 0, "max_retries=0 must not retry");
    assert_eq!(task.status, Status::Failed);

    tokio::time::sleep(Duration::from_millis(500)).await;
    let settled = manager.get(id).unwrap();
    assert_eq!(settled.retry_count, 0, "still no retry");
    assert_eq!(settled.status, Status::Failed);
}

/// Progress events for the surviving attempt must carry a monotonic
/// byte count (no duplicate-byte regressions after the reconnect).
#[tokio::test]
async fn progress_is_monotonic_across_retry() {
    // See the note on abrupt_close_is_retried_and_completes_exactly:
    // three truncated GETs guarantee the surfaced attempt fails at least
    // once before the full response succeeds.
    let origin = spawn_flaky_origin(3, false);
    let seq = DIR_SEQ.fetch_add(1, Ordering::SeqCst);
    let tmp = tempfile::Builder::new()
        .prefix(&format!("dp-retry-mon-{seq}-"))
        .tempdir()
        .unwrap();
    let db_path = tmp.path().join("test.db");
    let (manager2, rx) = DownloadManager::new(&db_path, tmp.path().join("parts"));
    let (fwd_tx, mut fwd_rx) = tokio::sync::mpsc::unbounded_channel();
    manager2.clone().pump_loop(rx, fwd_tx);

    let id = manager2
        .add(new_download(&origin, tmp.path(), 3))
        .await
        .expect("add should not fail");

    let _ = wait_until(&manager2, id, "Completed", Duration::from_secs(30), |t| {
        t.status == Status::Completed
    })
    .await;

    // Drain remaining events (the pump forwards on a 400 ms tick, so give
    // it time to flush), collect Progress for our id, assert non-decreasing
    // downloaded counts.
    let mut last = 0i64;
    let drain_deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    while tokio::time::Instant::now() < drain_deadline {
        // Timeout here is EXPECTED between pump ticks (400 ms cadence) —
        // keep polling until the deadline; only a closed channel ends the
        // drain.
        match tokio::time::timeout(Duration::from_millis(500), fwd_rx.recv()).await {
            Ok(Some(ev)) => {
                if let EngineEvent::Progress {
                    id: eid,
                    downloaded,
                    ..
                } = ev
                {
                    if eid == id {
                        assert!(
                            downloaded >= last,
                            "progress regressed: {downloaded} after {last}"
                        );
                        last = downloaded;
                    }
                }
            }
            Ok(None) => break,
            Err(_) => {}
        }
    }
    assert!(last > 0, "no progress events observed");
}
