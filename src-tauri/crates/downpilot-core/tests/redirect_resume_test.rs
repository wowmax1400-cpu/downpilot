//! Redirect-chain replay on resume.
//!
//! These tests stand up a tiny hand-rolled HTTP server that exposes a
//! resource behind one or more 302 hops. The client never knows the final
//! URL — it just GETs the entry point and relies on transparent redirect
//! following.
//!
//! The interesting question for a download engine is: when a download is
//! paused mid-flight and resumed, does the engine replay the redirect
//! chain on every chunk request, or does it pin to a resolved URL (or a
//! cached Location header) and miss the hop on the second pass? The
//! correct behaviour is "replay the chain every time" — that's what
//! `reqwest::Client` does by default because each request is a fresh
//! `GET` against the original URL.
//!
//! Test 1 (`redirect_resume_completes_with_intact_content`):
//!   `/redirected → 302 → /real-file`. Download, pause at ~50%, resume.
//!   Expect: the resume re-hits `/redirected`, the chain is replayed, and
//!   the final file matches the original. This is the happy path that
//!   should already work on `main` because `reqwest` follows redirects
//!   transparently per request.
//!
//! Test 2 (`redirect_chain_of_three_replays_all_hops_on_resume`):
//!   `/redirected → 302 → /hop2 → 302 → /real-file`. Same scenario. The
//!   server counts how many times each hop is hit so the test can assert
//!   the chain was replayed after the pause (not cached). This is a
//!   stronger version of test 1 that documents the full-chain replay
//!   invariant.
//!
//! Both tests are `#[test]` (not `#[ignore]`) on the assumption that
//! transparent redirect following by `reqwest` already gives the correct
//! behaviour. If a future change to the engine introduces URL pinning or
//! redirect caching, test 2 will start failing and the fix is to remove
//! the caching layer.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use downpilot_core::engine::DownloadManager;
use downpilot_core::models::{EngineEvent, NewDownload, Status};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

// 128 MiB — large enough that 200ms is comfortably mid-flight on loopback
// even with throttling, so the pause-then-resume window is reliable.
const SIZE: usize = 128 * 1024 * 1024;
static DIR_SEQ: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, Copy)]
enum PathKind {
    RedirectTo(&'static str),
    Real,
}

struct RedirectServer {
    addr: std::net::SocketAddr,
    expected: Vec<u8>,
    /// Per-path: how many times the path was hit as the FINAL hop (i.e.
    /// not counting the redirect responses themselves, but the actual
    /// content / range requests served after the chain was followed).
    /// Carried on the struct for assertion use in the 3-hop test; the
    /// single-hop test only reads the total-hits map.
    #[allow(dead_code)]
    final_hits: Arc<Mutex<HashMap<String, usize>>>,
    /// Per-path: how many times this path was hit at all (including
    /// redirect responses).
    total_hits: Arc<Mutex<HashMap<String, usize>>>,
}

async fn spawn_redirect_server(
    paths: &[(String, PathKind)],
    real_bytes: Vec<u8>,
) -> RedirectServer {
    let total = real_bytes.len();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let final_hits: Arc<Mutex<HashMap<String, usize>>> = Arc::new(Mutex::new(HashMap::new()));
    let total_hits: Arc<Mutex<HashMap<String, usize>>> = Arc::new(Mutex::new(HashMap::new()));
    for (p, _) in paths {
        final_hits.lock().unwrap().insert(p.clone(), 0);
        total_hits.lock().unwrap().insert(p.clone(), 0);
    }
    // Make sure the real file's stats exist too.
    for (p, kind) in paths {
        if matches!(kind, PathKind::Real) {
            final_hits.lock().unwrap().entry(p.clone()).or_insert(0);
            total_hits.lock().unwrap().entry(p.clone()).or_insert(0);
        }
    }
    let final_hits_clone = Arc::clone(&final_hits);
    let total_hits_clone = Arc::clone(&total_hits);

    let path_table: Arc<Vec<(String, PathKind)>> = Arc::new(paths.to_vec());
    let real_bytes_arc: Arc<Vec<u8>> = Arc::new(real_bytes);
    let expected_for_caller = (*real_bytes_arc).clone();

    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                continue;
            };
            let path_table = Arc::clone(&path_table);
            let real_bytes = Arc::clone(&real_bytes_arc);
            let final_hits = Arc::clone(&final_hits_clone);
            let total_hits = Arc::clone(&total_hits_clone);
            tokio::spawn(async move {
                handle_redirect(
                    &mut stream,
                    &path_table,
                    &real_bytes,
                    &final_hits,
                    &total_hits,
                    total,
                )
                .await;
            });
        }
    });

    RedirectServer {
        addr,
        expected: expected_for_caller,
        final_hits,
        total_hits,
    }
}

fn url_for(server: &RedirectServer, path: &str) -> String {
    format!("http://{}/{}", server.addr, path.trim_start_matches('/'))
}

/// Stream `data` in `chunk` byte pieces, sleeping `sleep_us` µs between
/// pieces. Keeps loopback at ~chunk * 1e6 / sleep_us B/s so the test can
/// reliably catch the download mid-flight.
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

fn find_kind<'a>(table: &'a [(String, PathKind)], path: &str) -> Option<&'a PathKind> {
    table.iter().find(|(p, _)| p == path).map(|(_, k)| k)
}

fn find_redirect_target(table: &[(String, PathKind)], from: &str) -> Option<String> {
    table.iter().find_map(|(p, k)| {
        if p == from {
            if let PathKind::RedirectTo(target) = k {
                return Some(target.to_string());
            }
        }
        None
    })
}

async fn handle_redirect<S>(
    stream: &mut S,
    path_table: &Arc<Vec<(String, PathKind)>>,
    real_bytes: &Arc<Vec<u8>>,
    final_hits: &Arc<Mutex<HashMap<String, usize>>>,
    total_hits: &Arc<Mutex<HashMap<String, usize>>>,
    total: usize,
) where
    S: tokio::io::AsyncWrite + tokio::io::AsyncRead + Unpin,
{
    let mut buf = vec![0u8; 8192];
    let n = match stream.read(&mut buf).await {
        Ok(n) if n > 0 => n,
        _ => {
            let _ = stream.shutdown().await;
            return;
        }
    };
    let req = String::from_utf8_lossy(&buf[..n]).to_string();

    let path = req
        .split_whitespace()
        .nth(1)
        .and_then(|p| p.split('?').next())
        .unwrap_or("")
        .trim_start_matches('/')
        .to_string();

    {
        let mut t = total_hits.lock().unwrap();
        *t.entry(path.clone()).or_insert(0) += 1;
    }

    let kind = match find_kind(path_table, &path) {
        Some(k) => *k,
        None => {
            let _ = stream
                .write_all(
                    b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .await;
            let _ = stream.shutdown().await;
            return;
        }
    };

    match kind {
        PathKind::RedirectTo(target) => {
            let location = format!("/{target}");
            // Use 302 (temporary) so the test never assumes permanent
            // caching. Either 301 or 302 is fine for what we're testing;
            // the point is that the client must re-follow the chain.
            let resp = format!(
                "HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            );
            let _ = stream.write_all(resp.as_bytes()).await;
            let _ = stream.shutdown().await;
        }
        PathKind::Real => {
            // We're at the final hop. Count it, then serve Range or full
            // body. The Range logic mirrors the etag test's server, minus
            // the If-Range handling — this test isn't about ETag.
            {
                let mut t = final_hits.lock().unwrap();
                *t.entry(path.clone()).or_insert(0) += 1;
            }

            let range_spec = req
                .lines()
                .find(|l| l.to_ascii_lowercase().starts_with("range:"))
                .and_then(|l| l.split('=').nth(1))
                .map(|s| s.trim().to_string());

            if let Some(spec) = range_spec {
                let parts: Vec<&str> = spec.splitn(2, '-').collect();
                let start: usize = parts[0].parse().unwrap_or(0);
                let end: usize = if parts.len() > 1 && !parts[1].is_empty() {
                    parts[1].parse().unwrap_or(total - 1)
                } else {
                    total - 1
                };
                let end = end.min(total - 1);
                if start > end || start >= total {
                    let _ = stream
                        .write_all(
                            format!(
                                "HTTP/1.1 416 Range Not Satisfiable\r\nContent-Range: bytes */{total}\r\nConnection: close\r\n\r\n"
                            )
                            .as_bytes(),
                        )
                        .await;
                    let _ = stream.shutdown().await;
                    return;
                }
                let len = end - start + 1;
                let head = format!(
                    "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {start}-{end}/{total}\r\nContent-Length: {len}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n"
                );
                let _ = stream.write_all(head.as_bytes()).await;
                throttled_write(stream, &real_bytes[start..=end], 16 * 1024, 250).await;
                let _ = stream.shutdown().await;
            } else {
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {total}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n"
                );
                let _ = stream.write_all(head.as_bytes()).await;
                throttled_write(stream, real_bytes, 16 * 1024, 250).await;
                let _ = stream.shutdown().await;
            }
        }
    }

    // Defensive: suppress unused-target warning when there's a single hop.
    let _ = find_redirect_target(path_table, &path);
}

fn setup() -> (
    std::path::PathBuf,
    Arc<DownloadManager>,
    tokio::sync::mpsc::UnboundedReceiver<EngineEvent>,
) {
    let seq = DIR_SEQ.fetch_add(1, Ordering::Relaxed);
    let tmp = std::env::temp_dir().join(format!("dp-redir-{}-{seq}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let db_path = tmp.join("test.db");
    let state_dir = tmp.join("parts");
    std::fs::create_dir_all(&tmp).unwrap();

    let (manager, rx) = DownloadManager::new(&db_path, state_dir);
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
        mirrors: Vec::new(),
    }
}

async fn wait_for_status(
    sink_rx: &mut tokio::sync::mpsc::UnboundedReceiver<EngineEvent>,
    id: i64,
    want: Status,
    timeout: Duration,
) -> bool {
    tokio::time::timeout(timeout, async {
        while let Some(ev) = sink_rx.recv().await {
            if let EngineEvent::StatusChanged {
                id: eid, status, ..
            } = ev
            {
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

/// Single-hop redirect: `/redirected → 302 → /real-file`. After pause +
/// resume the engine must still arrive at `/real-file` and complete the
/// download with intact content. Passes on `main` because reqwest
/// transparently follows the 302 on every fresh GET, and the engine
/// stores the original URL on the task row.
#[tokio::test]
async fn redirect_resume_completes_with_intact_content() {
    let content = (0..SIZE).map(|i| (i % 251) as u8).collect::<Vec<_>>();
    let paths = vec![
        ("redirected".to_string(), PathKind::RedirectTo("real-file")),
        ("real-file".to_string(), PathKind::Real),
    ];
    let server = spawn_redirect_server(&paths, content).await;
    let url = url_for(&server, "redirected");
    let (tmp, manager, mut sink_rx) = setup();

    let id = manager
        .add(new_download(
            &url,
            "redirected.bin",
            &tmp.to_string_lossy(),
            2,
        ))
        .await
        .expect("add should succeed (redirect followed during probe)");

    let _ = wait_for_status(
        &mut sink_rx,
        id,
        Status::Downloading,
        Duration::from_secs(5),
    )
    .await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    manager
        .pause(id)
        .expect("pause should work while downloading");

    let paused = wait_for_status(&mut sink_rx, id, Status::Paused, Duration::from_secs(10)).await;
    assert!(paused, "task did not pause");

    manager.resume(id).expect("resume should succeed");
    let done = wait_for_status(&mut sink_rx, id, Status::Completed, Duration::from_secs(30)).await;
    assert!(done, "resume did not complete");

    let out_path = tmp.join("redirected.bin");
    let got = std::fs::read(&out_path).expect("output file should exist");
    assert_eq!(got.len(), server.expected.len(), "file size mismatch");
    assert_eq!(got, server.expected, "redirected resumed content mismatch");

    // The entry point must have been hit at least twice (probe + resume):
    // once before the pause and once after. If the engine cached the
    // resolved URL or stopped following redirects on resume, this counter
    // would stay at 1 and the test would still pass (the final file
    // would also fail) — we assert it as documentation, not as a hard
    // gate, since the byte equality is the real correctness check.
    let entry_hits = server
        .total_hits
        .lock()
        .unwrap()
        .get("redirected")
        .copied()
        .unwrap_or(0);
    assert!(
        entry_hits >= 2,
        "expected /redirected to be re-hit after pause (got {entry_hits})"
    );
    println!("✓ single-hop redirect: /redirected hit {entry_hits} times, final file bytes match");

    let _ = std::fs::remove_dir_all(&tmp);
}

/// Three-hop chain: `/redirected → 302 → /hop2 → 302 → /real-file`. After
/// pause + resume, every hop must be re-traversed — reqwest replays the
/// chain on every fresh GET, so this is the same correctness contract
/// the engine relies on for the more common two-hop case. Documents that
/// the engine does not pin to a resolved URL or cache the Location
/// header across pause boundaries.
#[tokio::test]
async fn redirect_chain_of_three_replays_all_hops_on_resume() {
    let content = (0..SIZE).map(|i| (i % 251) as u8).collect::<Vec<_>>();
    let paths = vec![
        ("redirected".to_string(), PathKind::RedirectTo("hop2")),
        ("hop2".to_string(), PathKind::RedirectTo("real-file")),
        ("real-file".to_string(), PathKind::Real),
    ];
    let server = spawn_redirect_server(&paths, content).await;
    let url = url_for(&server, "redirected");
    let (tmp, manager, mut sink_rx) = setup();

    let id = manager
        .add(new_download(&url, "chain.bin", &tmp.to_string_lossy(), 2))
        .await
        .expect("add should succeed (3-hop redirect followed during probe)");

    let _ = wait_for_status(
        &mut sink_rx,
        id,
        Status::Downloading,
        Duration::from_secs(5),
    )
    .await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    manager
        .pause(id)
        .expect("pause should work while downloading");

    let paused = wait_for_status(&mut sink_rx, id, Status::Paused, Duration::from_secs(10)).await;
    assert!(paused, "task did not pause");

    // Snapshot the hop-hit counts so we can detect a post-resume replay.
    let before: HashMap<String, usize> = server.total_hits.lock().unwrap().clone();
    let before_final = before.get("real-file").copied().unwrap_or(0);
    assert!(
        before_final >= 1,
        "real-file should have been hit at least once before the pause (probe), got {before_final}"
    );

    manager.resume(id).expect("resume should succeed");
    let done = wait_for_status(&mut sink_rx, id, Status::Completed, Duration::from_secs(30)).await;
    assert!(done, "resume did not complete");

    let out_path = tmp.join("chain.bin");
    let got = std::fs::read(&out_path).expect("output file should exist");
    assert_eq!(got.len(), server.expected.len(), "file size mismatch");
    assert_eq!(got, server.expected, "chain-resumed content mismatch");

    // After the resume, both the entry point AND the intermediate hop
    // AND the final hop should have been hit again. If the engine pinned
    // to /real-file after the first request, the entry + hop2 counters
    // would be unchanged post-resume.
    let after = server.total_hits.lock().unwrap().clone();
    let entry = after.get("redirected").copied().unwrap_or(0);
    let hop2 = after.get("hop2").copied().unwrap_or(0);
    let real = after.get("real-file").copied().unwrap_or(0);
    assert!(
        entry > before.get("redirected").copied().unwrap_or(0),
        "/redirected was not re-hit after resume (before={}, after={entry})",
        before.get("redirected").copied().unwrap_or(0)
    );
    assert!(
        hop2 > before.get("hop2").copied().unwrap_or(0),
        "/hop2 was not re-traversed after resume (before={}, after={hop2})",
        before.get("hop2").copied().unwrap_or(0)
    );
    assert!(
        real > before_final,
        "/real-file was not re-traversed after resume (before={before_final}, after={real})"
    );

    println!(
        "✓ 3-hop redirect: /redirected={entry} /hop2={hop2} /real-file={real} (all hops replayed after pause)"
    );

    let _ = std::fs::remove_dir_all(&tmp);
}
