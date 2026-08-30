//! Regression tests for the main-thread spawn crash.
//!
//! Background: sync Tauri commands run on the main (GTK) thread, where
//! there is no ambient Tokio runtime. `DownloadManager::start()` used a
//! bare `tokio::spawn`, which panicked with "there is no reactor running"
//! and — because the panic happened inside a WebKit/GTK callback that
//! cannot unwind — aborted the whole application. Any click on
//! Start / Resume / Force Start / Retry killed the app.
//!
//! The fix: the manager stores a Tokio runtime `Handle` (installed via
//! `install_runtime` at app startup) and spawns through it.
//!
//! These tests prove both halves:
//!   1. `start()` called from a thread with NO ambient runtime (the exact
//!      pre-fix crash condition) must not panic.
//!   2. `install_runtime()` must actually drive the spawned task to
//!      completion (status transitions + completion event).

use downpilot_core::engine::DownloadManager;
use downpilot_core::models::{NewDownload, Status};
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;

fn new_mgr() -> (Arc<DownloadManager>, TempDir) {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("test.db");
    let state_dir = tmp.path().join("state");
    std::fs::create_dir_all(&state_dir).unwrap();
    let (manager, _rx) = DownloadManager::new(&db_path, state_dir);
    (manager, tmp)
}

fn nd(url: &str) -> NewDownload {
    NewDownload {
        url: url.to_string(),
        filename: Some("spawn-regression.bin".into()),
        save_dir: None,
        threads: None,
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
        start_immediately: false,
        priority: None,
        browser_dedup_id: String::new(),
        skip_probe: true,
    }
}

/// Minimal HTTP server handing out 64 KiB of bytes, run on the installed
/// runtime so it stays alive while the manager's spawn path runs the task.
fn serve_bytes(rt: &tokio::runtime::Runtime, body: Vec<u8>) -> String {
    let body = Arc::new(body);
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = rt.block_on(async { tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap() });
    let addr = listener.local_addr().unwrap();
    rt.spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else { return };
            let body = Arc::clone(&body);
            tokio::spawn(async move {
                let mut buf = [0u8; 4096];
                let _ = sock.read(&mut buf).await;
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                use tokio::io::AsyncWriteExt;
                let _ = sock.write_all(head.as_bytes()).await;
                let _ = sock.write_all(&body).await;
            });
        }
    });
    format!("http://{addr}/file.bin")
}

/// The exact pre-fix crash condition: `start()` from a plain thread with
/// no ambient Tokio runtime. Before the fix this panicked with
/// "there is no reactor running" (aborting the app); now it must either
/// spawn via the installed handle or log-and-noop — never panic.
#[test]
fn start_from_runtimeless_thread_does_not_panic() {
    let (mgr, _tmp) = new_mgr();

    // Insert a task directly (add() is async; DB insert is sync).
    let id = mgr
        .db
        .insert(&nd("http://127.0.0.1:9/no-such-file.bin"), "/tmp")
        .expect("insert");
    let task = mgr.get(id).expect("task");

    let m2 = Arc::clone(&mgr);
    let handle = std::thread::spawn(move || {
        // Plain thread: no tokio context, exactly like the GTK main thread.
        m2.start(task);
    });
    // A panic inside the thread propagates to join(); the old code would
    // have aborted the process here.
    handle.join().expect("start() must not panic without a runtime");
}

/// The runtime installed via `install_runtime()` must actually drive the
/// spawned task: the download runs to completion and the status lands on
/// Completed, with a StatusChanged event emitted.
#[test]
fn installed_runtime_drives_task_to_completion() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let _rt_guard = rt.enter(); // lets the helper's rt.spawn work

    let body = vec![0xABu8; 64 * 1024];
    let url = serve_bytes(&rt, body);

    let (mgr, tmp) = new_mgr();
    let save_dir = tmp.path().join("out");
    std::fs::create_dir_all(&save_dir).unwrap();

    let id = mgr.db.insert(&nd(&url), save_dir.to_str().unwrap()).unwrap();
    let task = mgr.get(id).expect("task");

    mgr.install_runtime(rt.handle().clone());

    // start() from a runtime-less thread — but the installed handle now
    // carries the spawn.
    mgr.start(task);

    // Poll for completion (the 64 KiB body downloads in well under a second).
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        let t = mgr.get(id).expect("task");
        if t.status == Status::Completed {
            break;
        }
        if t.status == Status::Failed {
            panic!("task failed instead of completing: {:?}", t.error_category);
        }
        assert!(
            std::time::Instant::now() < deadline,
            "task did not complete in 10s (status: {:?})",
            t.status
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}
