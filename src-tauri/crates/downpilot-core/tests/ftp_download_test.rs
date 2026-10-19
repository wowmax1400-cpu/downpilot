//! End-to-end verification of the FTP path: a minimal in-process FTP server
//! (control channel + PASV data connection) serves a known payload, and the
//! engine's `run_ftp` must fetch it byte-exactly — including a resume where
//! the client starts mid-file via `REST`.
//!
//! The server is deliberately dumb: it implements only the commands
//! `ftp_connect`/`run_ftp` actually issue (USER, PASS, TYPE, CWD, SIZE,
//! PASV, REST, RETR, QUIT) so the test fails loudly if our client starts
//! depending on something the fixtures don't provide.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

use downpilot_core::ftp::{parse_ftp_url, run_ftp};
use downpilot_core::models::{DownloadTask, EngineEvent, Status};

/// Recorded facts about what the client actually asked for, so the test can
/// assert on the wire behaviour (a REST was sent, the right path was used).
#[derive(Default)]
struct ServerLog {
    user: std::sync::Mutex<Option<String>>,
    pass: std::sync::Mutex<Option<String>>,
    cwd: std::sync::Mutex<Option<String>>,
    rest_offsets: std::sync::Mutex<Vec<u64>>,
    retr_names: std::sync::Mutex<Vec<String>>,
    quit_seen: AtomicBool,
}

/// Spawn a one-shot FTP server on an ephemeral port serving `payload` as
/// `/{dir}/{name}`. Returns (port, log, join handle).
fn spawn_ftp_server(
    dir: &str,
    name: &str,
    payload: Vec<u8>,
    expect_user: &str,
    expect_pass: &str,
) -> (u16, Arc<ServerLog>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ftp control");
    let port = listener.local_addr().unwrap().port();
    let log = Arc::new(ServerLog::default());
    let log2 = log.clone();
    let dir = dir.to_string();
    let name = name.to_string();
    let expect_user = expect_user.to_string();
    let expect_pass = expect_pass.to_string();

    let handle = thread::spawn(move || {
        // One control connection per accepted socket; the client reconnects
        // on resume, so accept in a loop until the payload is fully served
        // (bounded to a handful of connections to avoid hanging a failure).
        for _ in 0..4 {
            let Ok((ctrl, _)) = listener.accept() else {
                return;
            };
            let _ = ctrl.set_read_timeout(Some(std::time::Duration::from_secs(10)));
            let mut reader = BufReader::new(ctrl.try_clone().unwrap());
            let reply = |s: &TcpStream, code: &str, msg: &str| {
                let mut s = s.try_clone().unwrap();
                let _ = write!(s, "{code} {msg}\r\n");
            };
            reply(&ctrl, "220", "downpilot test ftp ready");

            let mut rest_offset: u64 = 0;
            let mut cwd: Option<String> = None;
            let mut data_listener: Option<TcpListener> = None;
            let mut logged_in = false;
            let mut line = String::new();
            loop {
                line.clear();
                if reader.read_line(&mut line).unwrap_or(0) == 0 {
                    break;
                }
                let cmd_line = line.trim_end_matches(['\r', '\n']).to_string();
                let (verb, arg) = match cmd_line.split_once(' ') {
                    Some((v, a)) => (v.to_ascii_uppercase(), a.to_string()),
                    None => (cmd_line.to_ascii_uppercase(), String::new()),
                };
                match verb.as_str() {
                    "USER" => {
                        *log2.user.lock().unwrap() = Some(arg.clone());
                        if arg == expect_user {
                            reply(&ctrl, "331", "password required");
                        } else {
                            reply(&ctrl, "530", "unknown user");
                        }
                    }
                    "PASS" => {
                        *log2.pass.lock().unwrap() = Some(arg.clone());
                        if arg == expect_pass {
                            logged_in = true;
                            reply(&ctrl, "230", "logged in");
                        } else {
                            reply(&ctrl, "530", "bad password");
                        }
                    }
                    "TYPE" => reply(&ctrl, "200", "type set"),
                    "CWD" => {
                        *log2.cwd.lock().unwrap() = Some(arg.clone());
                        cwd = Some(arg.clone());
                        reply(&ctrl, "250", "cwd ok");
                    }
                    "SIZE" => {
                        if logged_in && cwd.as_deref() == Some(dir.as_str()) && arg == name {
                            reply(&ctrl, "213", &payload.len().to_string());
                        } else {
                            reply(&ctrl, "550", "no such file");
                        }
                    }
                    "PASV" => {
                        let dl = TcpListener::bind("127.0.0.1:0").expect("bind data");
                        let p = dl.local_addr().unwrap().port();
                        data_listener = Some(dl);
                        reply(
                            &ctrl,
                            "227",
                            &format!("Entering Passive Mode (127,0,0,1,{},{}).", p / 256, p % 256),
                        );
                    }
                    "REST" => {
                        let off: u64 = arg.parse().unwrap_or(0);
                        rest_offset = off;
                        log2.rest_offsets.lock().unwrap().push(off);
                        reply(&ctrl, "350", "restart at offset");
                    }
                    "RETR" => {
                        log2.retr_names.lock().unwrap().push(arg.clone());
                        if !logged_in || cwd.as_deref() != Some(dir.as_str()) || arg != name {
                            reply(&ctrl, "550", "no such file");
                            continue;
                        }
                        let Some(dl) = data_listener.take() else {
                            reply(&ctrl, "425", "no data connection");
                            continue;
                        };
                        reply(&ctrl, "150", "opening data connection");
                        let (mut data, _) = dl.accept().expect("accept data");
                        let start = rest_offset as usize;
                        if start < payload.len() {
                            let _ = data.write_all(&payload[start..]);
                        }
                        let _ = data.flush();
                        drop(data);
                        rest_offset = 0;
                        reply(&ctrl, "226", "transfer complete");
                    }
                    "QUIT" => {
                        log2.quit_seen.store(true, Ordering::SeqCst);
                        reply(&ctrl, "221", "bye");
                        return; // client done — thread exits, `join()` returns
                    }
                    _ => reply(&ctrl, "502", "not implemented"),
                }
            }
        }
    });
    (port, log, handle)
}

fn task_for(port: u16, url_path: &str, save_dir: &str, total: i64) -> DownloadTask {
    let mut t = DownloadTask::default_for_probe(
        &format!("ftp://127.0.0.1:{port}{url_path}"),
        "",
    );
    t.id = 1;
    t.save_dir = save_dir.to_string();
    t.status = Status::Downloading;
    t.total_size = total;
    t
}

#[tokio::test]
async fn ftp_download_fetches_file_byte_exact() {
    let payload: Vec<u8> = (0..64_000u32).map(|i| (i % 251) as u8).collect();
    let (port, log, handle) = spawn_ftp_server(
        "/pub",
        "data.bin",
        payload.clone(),
        "ftpuser@downpilot.local",
        "anonymous",
    );
    let tmp = tempfile::tempdir().unwrap();
    let save_dir = tmp.path().to_string_lossy().to_string();

    let task = task_for(port, "/pub/data.bin", &save_dir, payload.len() as i64);
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<EngineEvent>();
    let got = run_ftp(task, Arc::new(AtomicBool::new(false)), tx)
        .await
        .expect("ftp download should succeed");

    assert_eq!(got, payload.len() as u64, "byte count reported");
    let on_disk = std::fs::read(tmp.path().join("data.bin")).unwrap();
    assert_eq!(on_disk, payload, "downloaded bytes must match exactly");

    // The client must have used anonymous login (RFC 1635 pair) and CWD'd
    // to the file's directory before RETR.
    assert_eq!(log.user.lock().unwrap().as_deref(), Some("ftpuser@downpilot.local"));
    assert_eq!(log.pass.lock().unwrap().as_deref(), Some("anonymous"));
    assert_eq!(log.cwd.lock().unwrap().as_deref(), Some("/pub"));
    assert_eq!(log.retr_names.lock().unwrap().as_slice(), ["data.bin"]);
    // Fresh download: no REST issued.
    assert!(log.rest_offsets.lock().unwrap().is_empty(), "no REST on fresh start");
    assert!(log.quit_seen.load(Ordering::SeqCst), "client should QUIT");

    // Progress events must have reached the channel.
    let mut progress = 0;
    while let Ok(ev) = rx.try_recv() {
        if matches!(ev, EngineEvent::Progress { .. }) {
            progress += 1;
        }
    }
    assert!(progress > 0, "at least one progress event expected");

    handle.join().unwrap();
}

#[tokio::test]
async fn ftp_resume_uses_rest_from_existing_bytes() {
    let payload: Vec<u8> = (0..80_000u32).map(|i| (i % 97) as u8).collect();
    let (port, log, handle) = spawn_ftp_server(
        "/files",
        "resume.bin",
        payload.clone(),
        "ftpuser@downpilot.local",
        "anonymous",
    );
    let tmp = tempfile::tempdir().unwrap();
    let save_dir = tmp.path().to_string_lossy().to_string();

    // Simulate a half-finished earlier run: first 30_000 bytes on disk.
    let split = 30_000usize;
    std::fs::write(tmp.path().join("resume.bin"), &payload[..split]).unwrap();

    let task = task_for(port, "/files/resume.bin", &save_dir, payload.len() as i64);
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel::<EngineEvent>();
    let got = run_ftp(task, Arc::new(AtomicBool::new(false)), tx)
        .await
        .expect("resumed ftp download should succeed");

    assert_eq!(got, payload.len() as u64, "final size == full payload");
    let on_disk = std::fs::read(tmp.path().join("resume.bin")).unwrap();
    assert_eq!(on_disk, payload, "resumed file must not be corrupt");

    // The resume must have asked the server to restart at exactly the
    // number of bytes already on disk.
    let offsets = log.rest_offsets.lock().unwrap().clone();
    assert_eq!(offsets, vec![split as u64], "REST offset == existing bytes");

    handle.join().unwrap();
}

#[tokio::test]
async fn ftp_url_with_credentials_is_honoured() {
    let payload = b"credentialed payload".to_vec();
    let (port, log, handle) =
        spawn_ftp_server("/priv", "secret.txt", payload.clone(), "bob", "s3cret");
    let tmp = tempfile::tempdir().unwrap();
    let save_dir = tmp.path().to_string_lossy().to_string();

    let mut task = DownloadTask::default_for_probe(
        &format!("ftp://bob:s3cret@127.0.0.1:{port}/priv/secret.txt"),
        "",
    );
    task.id = 3;
    task.save_dir = save_dir.clone();
    task.status = Status::Downloading;

    // Sanity: our own parser agrees with the URL.
    let parsed = parse_ftp_url(&task.url).unwrap();
    assert_eq!(parsed.user.as_deref(), Some("bob"));
    assert_eq!(parsed.password.as_deref(), Some("s3cret"));

    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel::<EngineEvent>();
    let got = run_ftp(task, Arc::new(AtomicBool::new(false)), tx)
        .await
        .expect("credentialed ftp download should succeed");
    assert_eq!(got, payload.len() as u64);
    assert_eq!(std::fs::read(tmp.path().join("secret.txt")).unwrap(), payload);
    assert_eq!(log.user.lock().unwrap().as_deref(), Some("bob"));

    handle.join().unwrap();
}

#[tokio::test]
async fn ftp_bad_port_is_a_clean_error_not_a_hang() {
    // Nothing is listening on this port; connect must fail, not hang.
    let mut task = DownloadTask::default_for_probe("ftp://127.0.0.1:1/nope.bin", "");
    task.id = 4;
    task.save_dir = PathBuf::from("/tmp").to_string_lossy().to_string();
    task.status = Status::Downloading;

    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel::<EngineEvent>();
    let res = run_ftp(task, Arc::new(AtomicBool::new(false)), tx).await;
    assert!(res.is_err(), "connection to a dead port must error");
}
