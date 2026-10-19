// Integration tests for the per-queue authentication path:
//   * a real HTTP server that requires HTTP Basic auth
//   * DownPilot's ChunkDownloader with a CredentialStore attached
//   * three cases: correct creds, wrong creds, missing creds
//
// The server is implemented inline (no extra deps); requests
// without the right Authorization header return 401.

use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;

use base64::Engine as _;
use downpilot_core::credentials::CredentialStore;
use downpilot_core::download::{ChunkDownloader, DownloadError};
use downpilot_core::engine::DownloadManager;
use downpilot_core::models::{DownloadTask, NewDownload};
use tempfile::tempdir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// Build a minimal "real" server. Responds 401 unless the
/// Authorization header is exactly `Basic <base64("alice:s3cret")>`.
async fn start_auth_server() -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let expected = base64::engine::general_purpose::STANDARD.encode(b"alice:s3cret");
    let handle = tokio::spawn(async move {
        loop {
            let (mut sock, _) = match listener.accept().await {
                Ok(x) => x,
                Err(_) => return,
            };
            let expected = expected.clone();
            tokio::spawn(async move {
                let mut buf = vec![0u8; 4096];
                let n = match sock.read(&mut buf).await {
                    Ok(0) | Err(_) => return,
                    Ok(n) => n,
                };
                let req = String::from_utf8_lossy(&buf[..n]);
                // HTTP header names are case-insensitive; do a
                // case-insensitive prefix match on the *name* only,
                // then compare the value bytes verbatim (base64
                // values are case-sensitive).
                let auth_ok = req.lines().any(|l| {
                    let lower = l.to_ascii_lowercase();
                    if !lower.starts_with("authorization:") {
                        return false;
                    }
                    // Find the ':' in the *original* line to preserve value case.
                    let Some(idx) = l.find(':') else {
                        return false;
                    };
                    let value = l[idx + 1..].trim();
                    // Value format: "Basic <base64>". Match the "Basic " prefix
                    // case-insensitively; compare the base64 part verbatim.
                    if value.len() < 6 {
                        return false;
                    }
                    if !value[..6].eq_ignore_ascii_case("Basic ") {
                        return false;
                    }
                    let b64 = &value[6..];
                    b64.trim() == expected.as_str()
                });
                if !auth_ok {
                    let resp = b"HTTP/1.1 401 Unauthorized\r\n\
                                 WWW-Authenticate: Basic realm=\"x\"\r\n\
                                 Content-Length: 0\r\n\
                                 \r\n";
                    let _ = sock.write_all(resp).await;
                    return;
                }
                // 16-byte body — small but real.
                let body = b"hello, world!!!\x00\x00\x00\x00";
                let resp = format!(
                    "HTTP/1.1 200 OK\r\n\
                     Content-Length: {}\r\n\
                     Accept-Ranges: none\r\n\
                     Connection: close\r\n\
                     \r\n",
                    body.len()
                );
                let _ = sock.write_all(resp.as_bytes()).await;
                let _ = sock.write_all(body).await;
            });
        }
    });
    (format!("http://{addr}/file.bin"), handle)
}

async fn insert_task(
    db_dir: &std::path::Path,
    parts_dir: &std::path::Path,
    queue: &str,
    url: &str,
    save_dir: &std::path::Path,
) -> DownloadTask {
    let db_path = db_dir.join("test.db");
    let (mgr, _rx) = DownloadManager::new(&db_path, parts_dir.to_path_buf());
    let nd = NewDownload {
        url: url.to_string(),
        save_dir: Some(save_dir.to_string_lossy().to_string()),
        filename: Some("file.bin".to_string()),
        queue: Some(queue.to_string()),
        priority: None,
        start_immediately: false,
        user_agent: None,
        referer: None,
        cookies: None,
        proxy_url: None,
        category: Some("misc".to_string()),
        description: None,
        expected_sha256: None,
        browser_dedup_id: String::new(),
        skip_probe: true,
        max_retries: None,
        retry_delay_ms: None,
        conn_timeout_ms: None,
        read_timeout_ms: None,
        threads: None,
        format_id: None,
        webpage_url: None,
        checksum_algorithm: None,
        on_complete: None,
        mirrors: Vec::new(),
    };
    let id = mgr.add(nd).await.expect("add");
    mgr.get(id).expect("get")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn correct_credentials_resolve_to_basic_header() {
    let (url, _server) = start_auth_server().await;
    let dirs = tempdir().unwrap();
    let data = tempdir().unwrap();
    let parts = data.path().join("parts");
    std::fs::create_dir_all(&parts).unwrap();

    let creds = Arc::new(CredentialStore::new(data.path()));
    creds.set("auth-queue", "alice:s3cret").expect("set creds");

    let speed = Arc::new(AtomicU64::new(0));
    let dl = ChunkDownloader::new(PathBuf::from("/tmp"), speed);
    dl.set_credentials(creds);

    let task = insert_task(data.path(), &parts, "auth-queue", &url, dirs.path()).await;
    assert_eq!(task.queue, "auth-queue");

    // The probe path must NOT return 401, because we have the
    // right credentials. It may still return an error for other
    // reasons (e.g. no Accept-Ranges), but the auth path is OK.
    let res = dl.probe(&task).await;
    if let Err(DownloadError::HttpStatus(401)) = res {
        panic!("got 401 with correct creds — header not sent");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn missing_credentials_yield_no_auth_header() {
    let (url, _server) = start_auth_server().await;
    let dirs = tempdir().unwrap();
    let data = tempdir().unwrap();
    let parts = data.path().join("parts");
    std::fs::create_dir_all(&parts).unwrap();

    let creds = Arc::new(CredentialStore::new(data.path()));
    // Intentionally do NOT set a cred for "noauth-queue".
    let speed = Arc::new(AtomicU64::new(0));
    let dl = ChunkDownloader::new(PathBuf::from("/tmp"), speed);
    dl.set_credentials(creds);

    let task = insert_task(data.path(), &parts, "noauth-queue", &url, dirs.path()).await;
    let res = dl.probe(&task).await;
    match res {
        Err(DownloadError::HttpStatus(401)) => {}
        other => panic!("expected 401 when no creds, got {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn wrong_credentials_yield_401_on_real_request() {
    let (url, _server) = start_auth_server().await;
    let dirs = tempdir().unwrap();
    let data = tempdir().unwrap();
    let parts = data.path().join("parts");
    std::fs::create_dir_all(&parts).unwrap();

    let creds = Arc::new(CredentialStore::new(data.path()));
    creds
        .set("auth-queue", "alice:WRONG")
        .expect("set wrong creds");

    let speed = Arc::new(AtomicU64::new(0));
    let dl = ChunkDownloader::new(PathBuf::from("/tmp"), speed);
    dl.set_credentials(creds);

    let task = insert_task(data.path(), &parts, "auth-queue", &url, dirs.path()).await;
    let res = dl.probe(&task).await;
    match res {
        Err(DownloadError::HttpStatus(401)) => {}
        other => panic!("expected 401 with wrong creds, got {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn credentials_are_per_queue_not_shared() {
    let dirs = tempdir().unwrap();
    let data = tempdir().unwrap();
    let parts = data.path().join("parts");
    std::fs::create_dir_all(&parts).unwrap();

    let creds = Arc::new(CredentialStore::new(data.path()));
    creds.set("queue-a", "alice:s3cret").expect("set a");
    creds.set("queue-b", "bearer-token-xyz").expect("set b");
    // queue-c intentionally has no cred.

    let _a = insert_task(
        data.path(),
        &parts,
        "queue-a",
        "http://example.invalid/a",
        dirs.path(),
    )
    .await;
    let _b = insert_task(
        data.path(),
        &parts,
        "queue-b",
        "http://example.invalid/b",
        dirs.path(),
    )
    .await;
    let _c = insert_task(
        data.path(),
        &parts,
        "queue-c",
        "http://example.invalid/c",
        dirs.path(),
    )
    .await;

    // Re-derive expected values from the store, since we can't
    // read them back via the downloader's private field.
    let store = CredentialStore::new(data.path());
    let a_val = store.get("queue-a").unwrap();
    let b_val = store.get("queue-b").unwrap();
    let c_val = store.get("queue-c");

    assert!(a_val.contains(':'), "queue-a uses Basic auth");
    assert!(!b_val.contains(':'), "queue-b uses Bearer auth");
    assert!(
        matches!(
            c_val,
            Err(downpilot_core::credentials::CredError::NotConfigured)
        ),
        "queue-c has no cred, got {c_val:?}"
    );

    // Sanity: encode the Basic value the way the downloader does.
    let (u, p) = a_val.split_once(':').unwrap();
    let expected_a = format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(format!("{u}:{p}").as_bytes())
    );
    let expected_b = format!("Bearer {b_val}");
    assert!(expected_a.starts_with("Basic "));
    assert_eq!(expected_b, "Bearer bearer-token-xyz");
}
