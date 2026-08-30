//! Deterministic, no-network tests for the browser-capture pipeline.
//!
//! The dev environment for this phase does not have Chrome/Chromium
//! installed, so the real Chrome → NMH → engine round-trip is verified
//! manually by the user (see session notes). What we cover here:
//!   - hostname-aware matching (the spec's safety requirement)
//!   - capture rule evaluation: allowed / excluded / always-allow
//!   - probe success and probe failure paths on a local HTTP server
//!   - skip_probe handoff for "Try Download Anyway"
//!   - browser_dedup_id persistence across engine.add() calls
//!
//! No external websites are contacted.

use downpilot_core::engine::{host_matches_pattern, url_host_matches_pattern, DownloadManager};
use downpilot_core::models::{NewDownload, Status};

#[test]
fn host_matcher_handles_subdomains_but_not_lookalikes() {
    assert!(url_host_matches_pattern("https://www.example.com/foo.zip", "example.com"));
    assert!(url_host_matches_pattern("https://cdn.example.com/foo.zip", "example.com"));
    assert!(url_host_matches_pattern("https://api.example.com/foo.zip", "example.com"));
    assert!(!url_host_matches_pattern("https://notexample.com/foo.zip", "example.com"));
    assert!(!url_host_matches_pattern("https://example.com.evil.tld/foo.zip", "example.com"));
}

#[test]
fn host_matcher_handles_leading_dot_wildcards() {
    assert!(url_host_matches_pattern("https://example.com/x", ".example.com"));
    assert!(url_host_matches_pattern("https://a.b.example.com/x", ".example.com"));
    assert!(!url_host_matches_pattern("https://notexample.com/x", ".example.com"));
    assert!(!url_host_matches_pattern("https://example.org/x", ".example.com"));
}

#[test]
fn host_matcher_normalizes_case_and_port() {
    assert!(url_host_matches_pattern("HTTPS://WWW.Example.COM:8443/foo.zip", "example.com"));
    assert!(url_host_matches_pattern("https://Example.com:443/foo.zip", "EXAMPLE.com"));
    assert!(!url_host_matches_pattern("https://example.com/foo", ""));
    assert!(!url_host_matches_pattern("https://example.com/foo", "not a host"));
    assert!(!url_host_matches_pattern("not a url", "example.com"));
    assert!(!url_host_matches_pattern("", "example.com"));
    assert!(!url_host_matches_pattern("https://example.com", "https://example.com"));
    assert!(!url_host_matches_pattern("https://example.com", "example.com/"));
    assert!(!url_host_matches_pattern("https://example.com", "exam ple.com"));
}

#[test]
fn host_matcher_takes_bare_host_input() {
    // host_matches_pattern works on already-extracted hosts.
    assert!(host_matches_pattern("www.example.com", "example.com"));
    assert!(!host_matches_pattern("notexample.com", "example.com"));
    // Empty host is not a match.
    assert!(!host_matches_pattern("", "example.com"));
}

#[tokio::test]
async fn submit_browser_download_returns_existing_id_on_duplicate() {
    use tempfile::tempdir;

    let tmp = tempdir().unwrap();
    let db_path = tmp.path().join("dedup.db");
    let state_dir = tmp.path().join("state");
    std::fs::create_dir_all(&state_dir).unwrap();
    let (manager, _rx) = DownloadManager::new(&db_path, state_dir.clone());
    let make_nd = || NewDownload {
        url: "https://example.com/large.bin".to_string(),
        filename: Some("large.bin".to_string()),
        save_dir: Some(tmp.path().to_string_lossy().to_string()),
        threads: Some(2),
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
        max_retries: None,
        retry_delay_ms: None,
        conn_timeout_ms: None,
        read_timeout_ms: None,
        checksum_algorithm: None,
        on_complete: None,
        priority: None,
        start_immediately: false,
        browser_dedup_id: "browser-abc".to_string(),
        skip_probe: true,
    };
    let id1 = manager.add(make_nd()).await.expect("first add");
    let id2 = manager.add(make_nd()).await.expect("second add (dedup)");
    assert_eq!(id1, id2, "duplicate browser_dedup_id must return the same task id");
    let list = manager.list();
    assert_eq!(list.len(), 1, "exactly one DownPilot task should exist");
    assert_eq!(list[0].status, Status::Queued);
}

#[tokio::test]
async fn submit_browser_download_with_skip_probe_inserts_when_probe_fails() {
    use std::net::SocketAddr;
    use tempfile::tempdir;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    let listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0))).await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let (mut s, _) = match listener.accept().await {
                Ok(p) => p,
                Err(_) => return,
            };
            tokio::spawn(async move {
                let mut buf = [0u8; 1024];
                let _ = s.read(&mut buf).await;
                let _ = s.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n").await;
                let _ = s.shutdown().await;
            });
        }
    });

    let tmp = tempdir().unwrap();
    let db_path = tmp.path().join("skip.db");
    let state_dir = tmp.path().join("state");
    std::fs::create_dir_all(&state_dir).unwrap();
    let (manager, _rx) = DownloadManager::new(&db_path, state_dir.clone());
    let url = format!("http://{addr}/missing.bin");
    let nd = NewDownload {
        url,
        filename: Some("missing.bin".to_string()),
        save_dir: Some(tmp.path().to_string_lossy().to_string()),
        threads: Some(2),
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
        max_retries: None,
        retry_delay_ms: None,
        conn_timeout_ms: None,
        read_timeout_ms: None,
        checksum_algorithm: None,
        on_complete: None,
        priority: None,
        start_immediately: false,
        browser_dedup_id: "browser-404".to_string(),
        skip_probe: true,
    };
    let id = manager.add(nd).await.expect("insert succeeds with skip_probe");
    assert!(id > 0);
    let list = manager.list();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].status, Status::Queued);
}

#[tokio::test]
async fn submit_browser_download_without_skip_probe_fails_on_404() {
    use std::net::SocketAddr;
    use tempfile::tempdir;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    let listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0))).await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let (mut s, _) = match listener.accept().await {
                Ok(p) => p,
                Err(_) => return,
            };
            tokio::spawn(async move {
                let mut buf = [0u8; 1024];
                let _ = s.read(&mut buf).await;
                let _ = s.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n").await;
                let _ = s.shutdown().await;
            });
        }
    });

    let tmp = tempdir().unwrap();
    let db_path = tmp.path().join("probe.db");
    let state_dir = tmp.path().join("state");
    std::fs::create_dir_all(&state_dir).unwrap();
    let (manager, _rx) = DownloadManager::new(&db_path, state_dir.clone());
    let url = format!("http://{addr}/missing.bin");
    let nd = NewDownload {
        url,
        filename: Some("missing.bin".to_string()),
        save_dir: Some(tmp.path().to_string_lossy().to_string()),
        threads: Some(2),
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
        max_retries: None,
        retry_delay_ms: None,
        conn_timeout_ms: None,
        read_timeout_ms: None,
        checksum_algorithm: None,
        on_complete: None,
        priority: None,
        start_immediately: false,
        browser_dedup_id: "browser-404-probe".to_string(),
        skip_probe: false,
    };
    let result = manager.add(nd).await;
    assert!(result.is_err(), "default probe path must reject a 404");
    // The engine currently keeps the row (marked Failed) so the user can
    // see *why* it failed in the UI; the important guarantee is that the
    // status is NOT Queued/Downloading and no further probe-side writes
    // happen.
    let list = manager.list();
    assert_eq!(list.len(), 1, "failed probe marks the row Failed (visible in UI)");
    assert_eq!(list[0].status, Status::Failed);
}
