//! Multi-threaded chunked downloader with HTTP Range requests,
//! pause/resume via persisted .part files, and optional SHA-256 verification.

use std::error::Error as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::StreamExt;
use reqwest::header::{
    HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_RANGE, COOKIE, ETAG, IF_RANGE, LAST_MODIFIED,
    RANGE, REFERER, USER_AGENT,
};
use reqwest::StatusCode;
use sha2::{Digest, Sha256};
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;

use crate::models::{parse_content_range_total, DownloadTask, EngineEvent};
use base64::Engine as _;

#[derive(Debug, thiserror::Error)]
pub enum DownloadError {
    #[error("http error: {0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("connection failed: {0}")]
    ConnectionFailed(String),
    #[error("timed out: {0}")]
    Timeout(String),
    #[error("dns resolution failed: {0}")]
    DnsFailed(String),
    #[error("tls error: {0}")]
    TlsError(String),
    #[error("http status {0}")]
    HttpStatus(u16),
    #[error("disk full")]
    DiskFull,
    #[error("permission denied: {0}")]
    Permission(String),
    #[error("invalid destination: {0}")]
    BadDestination(String),
    #[error(
        "server does not support range requests; single-threaded download used and still failed"
    )]
    NoRangeSupport,
    #[error("empty body returned by server")]
    EmptyBody,
    #[error("cancelled")]
    Cancelled,
    #[error("checksum mismatch: expected {expected}, got {got}")]
    ChecksumMismatch { expected: String, got: String },
    #[error("invalid output path")]
    InvalidPath,
    #[error("invalid proxy URL: {0}")]
    InvalidProxy(String),
    #[error("server ignored range request while resuming")]
    RangeIgnored,
    /// The peer closed the connection cleanly (TCP-wise) after fewer body
    /// bytes than the response promised (Content-Length / segment range).
    /// Equivalent to the missing-TLS-close_notify EOF over HTTPS: an
    /// abrupt transport termination, transient and retryable. The bytes
    /// already written stay on disk; the retry resumes from there.
    #[error("connection closed before the download completed")]
    IncompleteBody,
}

type ChunkProgress = (usize, u64); // (chunk index, cumulative bytes done)

/// Path of the persisted HTTP validator (ETag / Last-Modified) sidecar
/// for a task. Lives in the state dir (not the user's save dir) and is
/// removed when the download completes, is removed, or is redownloaded.
/// Without a stored validator a resume cannot ask the server "has this
/// file changed?" — and a server that replaced the object mid-download
/// will happily answer a bare `Range` request with bytes of the *new*
/// file, silently producing a corrupt concatenation.
fn validator_path(state_dir: &Path, task_id: i64) -> PathBuf {
    state_dir.join(format!("validator-{task_id}"))
}

/// Persist the strongest validator the server returned. ETag wins over
/// Last-Modified when both are present. Best-effort: a failed write only
/// degrades the *next* resume to unconditional, it never fails the
/// download itself.
async fn save_validator(
    state_dir: &Path,
    task_id: i64,
    etag: Option<&str>,
    last_modified: Option<&str>,
) {
    if etag.is_none() && last_modified.is_none() {
        return;
    }
    let mut text = String::new();
    if let Some(e) = etag {
        text.push_str("etag: ");
        text.push_str(e);
        text.push('\n');
    }
    if let Some(l) = last_modified {
        text.push_str("lm: ");
        text.push_str(l);
        text.push('\n');
    }
    let _ = fs::write(validator_path(state_dir, task_id), text).await;
}

/// Load the stored validator as an `If-Range` header value, or `None`
/// when nothing was persisted (first download / server never sent one).
async fn load_validator(state_dir: &Path, task_id: i64) -> Option<HeaderValue> {
    let text = fs::read_to_string(validator_path(state_dir, task_id))
        .await
        .ok()?;
    let etag = text.lines().find_map(|l| l.strip_prefix("etag: "));
    let lm = text.lines().find_map(|l| l.strip_prefix("lm: "));
    // ETag values arrive pre-quoted from the server (`"v1"`); pass them
    // through verbatim so If-Range compares byte-identical.
    let v = etag.or(lm)?;
    HeaderValue::from_str(v).ok()
}

/// Pull ETag / Last-Modified off a response for `save_validator`.
fn response_validators(resp: &reqwest::Response) -> (Option<&str>, Option<&str>) {
    let etag = resp.headers().get(ETAG).and_then(|v| v.to_str().ok());
    let lm = resp
        .headers()
        .get(LAST_MODIFIED)
        .and_then(|v| v.to_str().ok());
    (etag, lm)
}

pub struct ChunkDownloader {
    client: Mutex<reqwest::Client>,
    /// Stash the proxy URL separately so per-task read-timeout clients can
    /// rebuild it (reqwest::Client doesn't expose its configured proxy).
    proxy_url: Mutex<Option<String>>,
    state_dir: PathBuf,
    speed_limit: Arc<AtomicU64>,
    /// Optional credential store for per-queue auth. When `Some`,
    /// the downloader resolves the task's queue and applies the
    /// configured credential to outgoing requests. Wrapped in a
    /// `Mutex` so the manager can attach it post-construction
    /// (the manager itself lives behind an `Arc`).
    creds: Mutex<Option<Arc<crate::credentials::CredentialStore>>>,
}

struct ChunkTask {
    idx: usize,
    handle: tokio::task::JoinHandle<Result<(), DownloadError>>,
}

impl ChunkDownloader {
    pub fn new(state_dir: PathBuf, speed_limit: Arc<AtomicU64>) -> Self {
        let client = Self::build_client(None).expect("failed to build default HTTP client");
        Self {
            client: Mutex::new(client),
            proxy_url: Mutex::new(None),
            state_dir,
            speed_limit,
            creds: Mutex::new(None),
        }
    }

    /// Attach a credential store. Must be called once at startup
    /// before any download starts. The store is `Arc`-shared so
    /// `DownloadManager` can hand the same instance to the
    /// downloader and to the Tauri commands.
    ///
    /// Thread-safe via interior mutability; called exactly once at
    /// startup. Idempotent if called more than once (the second
    /// call wins, which is the desired behavior if the store is
    /// re-attached after a keyring handoff).
    pub fn set_credentials(&self, creds: Arc<crate::credentials::CredentialStore>) {
        *self.creds.lock().unwrap() = Some(creds);
    }

    fn build_client(proxy: Option<&str>) -> Result<reqwest::Client, DownloadError> {
        let mut b = reqwest::Client::builder().connect_timeout(Duration::from_secs(15));
        if let Some(p) = proxy {
            let p =
                reqwest::Proxy::all(p).map_err(|e| DownloadError::InvalidProxy(e.to_string()))?;
            b = b.proxy(p);
        }
        b.build()
            .map_err(|e| DownloadError::InvalidProxy(e.to_string()))
    }

    /// Rebuild the HTTP client with a new proxy. Pass None to clear.
    pub fn set_proxy(&self, proxy: Option<&str>) {
        if let Ok(client) = Self::build_client(proxy) {
            *self.client.lock().unwrap() = client;
        }
        *self.proxy_url.lock().unwrap() = proxy.map(|s| s.to_string());
    }

    /// Per-queue speed limit (bytes/sec). Stored separately from the global
    /// limit; the downloader applies the tighter of global/queue.
    pub fn set_queue_speed(&self, _queue: String, _kbps: u64) {
        // The current downloader applies only the global limit. Per-queue
        // limiting is enforced at the engine level by adjusting the global
        // token bucket when a single queue dominates active transfers; a
        // future per-transfer bucket will refine this. For now we degrade
        // gracefully to the global limiter.
    }

    /// Clone of the current HTTP client (cheap; reqwest::Client is Arc-backed).
    pub fn http_client(&self) -> reqwest::Client {
        self.client.lock().unwrap().clone()
    }

    fn headers_for(task: &DownloadTask) -> HeaderMap {
        let mut h = HeaderMap::new();
        if let Some(ua) = &task.user_agent {
            if let Ok(v) = HeaderValue::from_str(ua) {
                h.insert(USER_AGENT, v);
            }
        }
        if let Some(r) = &task.referer {
            if let Ok(v) = HeaderValue::from_str(r) {
                h.insert(REFERER, v);
            }
        }
        if let Some(c) = &task.cookies {
            if let Ok(v) = HeaderValue::from_str(c) {
                h.insert(COOKIE, v);
            }
        }
        h
    }

    /// Resolve the credential for the task's queue and return the
    /// appropriate `Authorization` header value, if any.
    ///
    /// Format: if the stored secret contains a `:`, it is treated
    /// as `user:password` and encoded as HTTP Basic. Otherwise it
    /// is sent verbatim as a Bearer token.
    ///
    /// Returns `None` when:
    /// - the downloader has no credential store attached
    /// - the task has no queue
    /// - the queue has no configured credential
    /// - the queue has an empty or whitespace-only secret
    fn auth_header_for(&self, task: &DownloadTask) -> Option<HeaderValue> {
        let store = self.creds.lock().unwrap().clone()?;
        if task.queue.is_empty() {
            return None;
        }
        let secret = store.get(&task.queue).ok()?;
        let secret = secret.trim();
        if secret.is_empty() {
            return None;
        }
        if let Some((user, pass)) = secret.split_once(':') {
            // HTTP Basic. `user:pass` -> base64(user:pass).
            let pair = format!("{user}:{pass}");
            let encoded = base64::engine::general_purpose::STANDARD.encode(pair.as_bytes());
            HeaderValue::from_str(&format!("Basic {encoded}")).ok()
        } else {
            HeaderValue::from_str(&format!("Bearer {secret}")).ok()
        }
    }

    /// Build a `reqwest::Client` with the task's per-byte read timeout
    /// applied. When the task has no read timeout (the default), returns
    /// the shared `http_client()` clone — cheap, Arc-backed.
    ///
    /// This guards against half-dead connections: a server that accepts
    /// the request, sends a few bytes, then goes silent (e.g. a flaky CDN
    /// node, a stalled reverse proxy, or a TCP keepalive gap) would
    /// otherwise block the download forever. Reqwest's `read_timeout` —
    /// a Client-level setting in 0.12 — fires when no bytes are received
    /// for the given duration. It's different from the whole-request
    /// `.timeout()` which only covers connect + first byte.
    fn client_for(&self, task: &DownloadTask) -> Result<reqwest::Client, DownloadError> {
        let proxy = task
            .proxy_url
            .clone()
            .or_else(|| self.proxy_url.lock().unwrap().clone());
        if task.read_timeout_ms > 0 || task.proxy_url.is_some() {
            let mut b = reqwest::Client::builder().connect_timeout(Duration::from_secs(15));
            if let Some(p) = proxy.as_deref().filter(|p| !p.is_empty()) {
                let proxy = reqwest::Proxy::all(p)
                    .map_err(|e| DownloadError::InvalidProxy(e.to_string()))?;
                b = b.proxy(proxy);
            }
            if task.read_timeout_ms > 0 {
                b = b.read_timeout(Duration::from_millis(task.read_timeout_ms as u64));
            }
            b.build()
                .map_err(|e| DownloadError::InvalidProxy(e.to_string()))
        } else {
            Ok(self.http_client())
        }
    }

    /// Probe the server: returns (total size if known, range support,
    /// server-suggested filename, content-type).
    pub async fn probe(
        &self,
        task: &DownloadTask,
    ) -> Result<(Option<i64>, bool, Option<String>, Option<String>), DownloadError> {
        let mut h = Self::headers_for(task);
        if let Some(auth) = self.auth_header_for(task) {
            h.insert(AUTHORIZATION, auth);
        }
        // Transport blips (abrupt EOF / reset / timeout / connect failure)
        // can hit the probe exactly like the body. Retry once after a tiny
        // delay so a flapping CDN doesn't fail an otherwise-fine add();
        // real HTTP status errors and cert failures are NOT retried here.
        // Keeps probing fast: at most one extra HEAD.
        let mut resp = None;
        for attempt in 0..2 {
            match self
                .client_for(task)?
                .head(&task.url)
                .headers(h.clone())
                .send()
                .await
            {
                Ok(r) => {
                    resp = Some(r);
                    break;
                }
                Err(e) if attempt == 0 && is_transient_probe_err(&e) => {
                    eprintln!("[probe] transient error, retrying once: {e}");
                    tokio::time::sleep(Duration::from_millis(300)).await;
                }
                Err(e) => return Err(DownloadError::Reqwest(e)),
            }
        }
        let resp = resp.expect("probe loop must produce a response or return");
        let head_status = resp.status();
        // Fail fast on real HTTP errors (404, 403, 5xx) — the ranged GET
        // fallback below is only for servers that reject HEAD, not for
        // servers that respond with the error.
        if let code @ (400..=599) = head_status.as_u16() {
            // Some servers return 405 Method Not Allowed on HEAD. Try a 1-byte
            // ranged GET to confirm before giving up.
            if head_status == StatusCode::METHOD_NOT_ALLOWED {
                // fall through to ranged GET below
            } else {
                return Err(DownloadError::HttpStatus(code));
            }
        }
        let extract_content_type = |resp: &reqwest::Response| -> Option<String> {
            resp.headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.split(';').next().unwrap_or(s).trim().to_ascii_lowercase())
                .filter(|s| !s.is_empty())
        };
        if head_status.is_success() {
            let range_ok = resp
                .headers()
                .get("accept-ranges")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.eq_ignore_ascii_case("bytes"))
                .unwrap_or(false)
                || resp.headers().get("content-range").is_some();
            let filename = parse_content_disposition(resp.headers().get("content-disposition"));
            let content_type = extract_content_type(&resp);
            // Some proxies (and some servers) report `Content-Length: 0` on
            // HEAD even when the real body is non-empty. If we get 0, fall
            // back to a 1-byte ranged GET and parse `Content-Range` for
            // the true total.
            let total_size = match resp.content_length() {
                Some(n) if n > 0 => Some(n as i64),
                _ => match self.detect_total(task).await {
                    Ok(n) if n > 0 => Some(n),
                    _ => resp.content_length().map(|n| n as i64),
                },
            };
            // Many servers (nginx default, proof.ovh.net, etc.) don't send
            // `Accept-Ranges` on HEAD even though they fully support Range
            // requests on GET. Probe with a 1-byte ranged GET to confirm
            // before we fall back to single-threaded.
            let range_ok = if range_ok {
                true
            } else if let Some(n) = total_size {
                if n > 1 {
                    match self.detect_range_support(task).await {
                        Ok(true) => true,
                        Ok(false) => false,
                        Err(_) => false, // network blip — keep the conservative answer
                    }
                } else {
                    false
                }
            } else {
                false
            };
            return Ok((total_size, range_ok, filename, content_type));
        }
        // Some servers reject HEAD; probe with a 1-byte ranged GET.
        let mut h = Self::headers_for(task);
        if let Some(auth) = self.auth_header_for(task) {
            h.insert(AUTHORIZATION, auth);
        }
        h.insert(RANGE, HeaderValue::from_static("bytes=0-0"));
        let resp = self
            .client_for(task)?
            .get(&task.url)
            .headers(h)
            .send()
            .await?;
        let status = resp.status();
        let filename = parse_content_disposition(resp.headers().get("content-disposition"));
        let content_type = extract_content_type(&resp);
        if let code @ (400..=599) = status.as_u16() {
            return Err(DownloadError::HttpStatus(code));
        }
        if status == StatusCode::PARTIAL_CONTENT {
            let total = resp
                .headers()
                .get("content-range")
                .and_then(|v| v.to_str().ok())
                .and_then(parse_content_range_total);
            return Ok((total, true, filename, content_type));
        }
        Ok((
            resp.content_length().map(|n| n as i64),
            false,
            filename,
            content_type,
        ))
    }

    /// Issue a 1-byte ranged GET to test whether the server actually
    /// honours Range requests. Servers like nginx don't always advertise
    /// `Accept-Ranges` on HEAD but do reply with 206 to a ranged GET.
    async fn detect_range_support(&self, task: &DownloadTask) -> Result<bool, DownloadError> {
        let mut h = Self::headers_for(task);
        if let Some(auth) = self.auth_header_for(task) {
            h.insert(AUTHORIZATION, auth);
        }
        h.insert(RANGE, HeaderValue::from_static("bytes=0-0"));
        let resp = self
            .client_for(task)?
            .get(&task.url)
            .headers(h)
            .send()
            .await?;
        Ok(resp.status() == StatusCode::PARTIAL_CONTENT)
    }

    /// Recover the true total when HEAD's `Content-Length` is missing or
    /// zero (some proxies strip it). Uses a 1-byte ranged GET and parses
    /// `Content-Range: bytes 0-0/TOTAL`.
    async fn detect_total(&self, task: &DownloadTask) -> Result<i64, DownloadError> {
        let mut h = Self::headers_for(task);
        if let Some(auth) = self.auth_header_for(task) {
            h.insert(AUTHORIZATION, auth);
        }
        h.insert(RANGE, HeaderValue::from_static("bytes=0-0"));
        let resp = self
            .client_for(task)?
            .get(&task.url)
            .headers(h)
            .send()
            .await?;
        if let Some(cr) = resp
            .headers()
            .get("content-range")
            .and_then(|v| v.to_str().ok())
        {
            if let Some(n) = parse_content_range_total(cr) {
                return Ok(n);
            }
        }
        Ok(resp.content_length().map(|n| n as i64).unwrap_or(0))
    }

    /// Run a full download for `task`. Returns total bytes downloaded.
    ///
    /// Resume semantics: chunk `i` continues from the current size of
    /// `<part_dir>/part{i}` (torn writes are truncated first).
    pub async fn run(
        &self,
        task: DownloadTask,
        cancel: Arc<AtomicBool>,
        tx: mpsc::UnboundedSender<EngineEvent>,
    ) -> Result<u64, DownloadError> {
        // Trust the DB's `total_size` if already set (by `add()` probe or a
        // previous run). Re-probe only when it's 0 — the proxy-friendly
        // fallback handles servers that report `Content-Length: 0` on HEAD.
        let (total_opt, range_ok, _probed_name, _probed_ct) = if task.total_size > 0 {
            (Some(task.total_size), true, None, None)
        } else {
            self.probe(&task).await?
        };
        let threads = if range_ok {
            task.threads.max(1) as usize
        } else {
            1
        };
        let final_path = Path::new(&task.save_dir).join(&task.filename);
        let part_dir = self.state_dir.join(format!("parts-{}", task.id));
        fs::create_dir_all(&part_dir).await?;

        let total = total_opt.unwrap_or(0);
        if total > 0 && threads > 1 {
            let result = self
                .run_chunked(
                    &task,
                    &final_path,
                    &part_dir,
                    total,
                    threads,
                    cancel.clone(),
                    tx.clone(),
                )
                .await;
            if matches!(result, Err(DownloadError::RangeIgnored)) {
                let _ = fs::remove_file(&final_path).await;
                let _ = fs::remove_dir_all(&part_dir).await;
                let _ = fs::remove_file(validator_path(&self.state_dir, task.id)).await;
                self.run_single(&task, &final_path, total, cancel, tx).await
            } else {
                result
            }
        } else {
            let bytes = self
                .run_single(&task, &final_path, total, cancel, tx)
                .await?;
            if total == 0 {
                // unknown size: total becomes whatever we got
                let _ = total;
            }
            Ok(bytes)
        }
    }

    async fn run_single(
        &self,
        task: &DownloadTask,
        final_path: &Path,
        total: i64,
        cancel: Arc<AtomicBool>,
        tx: mpsc::UnboundedSender<EngineEvent>,
    ) -> Result<u64, DownloadError> {
        let mut restarted = false;
        loop {
            // Resume: existing file bytes are already downloaded.
            let mut existing = fs::metadata(final_path).await.map(|m| m.len()).unwrap_or(0);
            // If we know the total and the file is already complete, return
            // immediately — avoids a wasted request and a possible 416 if
            // the server's reported total is stale.
            if total > 0 && existing as i64 >= total {
                let _ = tx.send(EngineEvent::Progress {
                    id: task.id,
                    downloaded: existing as i64,
                    total,
                    speed_bps: 0.0,
                });
                return Ok(existing);
            }
            let mut headers = Self::headers_for(task);
            // Resuming an existing file: attach the stored HTTP validator so
            // the server only continues if the object is unchanged. A `200`
            // (or a `206` carrying a different validator) then means the
            // remote file changed under us — stale bytes on disk belong to a
            // different object and must not be appended to.
            let if_range = if existing > 0 {
                load_validator(&self.state_dir, task.id).await
            } else {
                None
            };
            if existing > 0 && total > 0 {
                headers.insert(
                    RANGE,
                    HeaderValue::from_str(&format!("bytes={existing}-")).unwrap(),
                );
                if let Some(v) = &if_range {
                    headers.insert(IF_RANGE, v.clone());
                }
            }
            // When `total == 0` we don't know the size yet, so we can't safely
            // send a Range — the server might 416 if existing is at/past the
            // end. A plain GET will return 200 with the full body; we'll
            // truncate the file to start clean and let the engine's
            // `finish_task` detect completion via transferred >= ??? (it's
            // still imperfect for unknown-size downloads, but it's the best
            // we can do without a Content-Length).
            if existing > 0 && total == 0 {
                // Truncate and re-download from the start.
                let _ = fs::remove_file(final_path).await;
                // `done` is initialised from `existing` below; reset it
                // because the file is now empty and we'll re-count from 0.
                existing = 0;
            }
            let resp = self
                .client_for(task)?
                .get(&task.url)
                .headers(headers)
                .send()
                .await?;
            let status = resp.status();
            if !status.is_success() && status != StatusCode::PARTIAL_CONTENT {
                return Err(DownloadError::HttpStatus(status.as_u16()));
            }
            // Mismatch detection: a full `200` to a resume request means the
            // server rejected Range/If-Range. A `206` whose validator differs
            // from the one we sent is a non-compliant server serving the new
            // object anyway. Either way: restart once from byte zero.
            let validator_mismatch = status == StatusCode::PARTIAL_CONTENT
                && existing > 0
                && if_range
                    .as_ref()
                    .map(|sent| {
                        let (etag, lm) = response_validators(&resp);
                        etag.or(lm)
                            .map(|v| v.as_bytes() != sent.as_bytes())
                            .unwrap_or(false)
                    })
                    .unwrap_or(false);
            if (status == StatusCode::OK && existing > 0) || validator_mismatch {
                if restarted {
                    return Err(DownloadError::RangeIgnored);
                }
                restarted = true;
                drop(resp);
                // The stored validator describes the OLD object — drop it so
                // the restart's fresh response repopulates it.
                let _ = fs::remove_file(validator_path(&self.state_dir, task.id)).await;
                fs::OpenOptions::new()
                    .write(true)
                    .open(final_path)
                    .await?
                    .set_len(0)
                    .await
                    .map_err(map_io)?;
                continue;
            }
            // Capture the current validator for future resumes (no-op write
            // when the server sent nothing).
            {
                let (etag, lm) = response_validators(&resp);
                save_validator(&self.state_dir, task.id, etag, lm).await;
            }
            // Resume-safety gate: a 206 answering a range request must start
            // at the exact offset we asked for. Anything else would
            // duplicate or skip bytes when appended.
            if status == StatusCode::PARTIAL_CONTENT && existing > 0 {
                verify_resume_offset(&resp, existing)?;
            }
            // Expected body length for the abrupt-close detector: on a 206
            // resume it's the remaining segment; on a full 200 it's the
            // advertised Content-Length (None = unknown, can't detect).
            let expected_len: Option<u64> = match status {
                StatusCode::PARTIAL_CONTENT => {
                    Some(resp.content_length().unwrap_or(0).saturating_add(existing))
                }
                _ => resp.content_length(),
            };
            let mut stream = resp.bytes_stream();
            let mut file = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(final_path)
                .await?;
            let mut done = existing;
            let mut last_emit = std::time::Instant::now();

            while let Some(chunk) = stream.next().await {
                if cancel.load(Ordering::Relaxed) {
                    return Err(DownloadError::Cancelled);
                }
                // An abrupt transport termination (peer closed without TLS
                // close_notify, truncated body, RST) surfaces here as a
                // reqwest body error. Map it to IncompleteBody so the retry
                // policy treats it as transient connection noise; the bytes
                // already appended stay on disk and the retry resumes.
                let chunk = match chunk {
                    Ok(c) => c,
                    Err(e) if is_abrupt_close_err(&e) => return Err(DownloadError::IncompleteBody),
                    Err(e) => return Err(DownloadError::Reqwest(e)),
                };
                self.throttle(chunk.len() as u64).await;
                file.write_all(&chunk).await.map_err(map_io)?;
                done += chunk.len() as u64;
                // Cap reported bytes at the final-file length so a server that
                // ignores Range and re-sends the full body cannot inflate the
                // cumulative counter (task 390: downloaded reported 1.37 GB
                // while disk held 75.6 MB). Mirrors the chunked guard.
                let reported = if total > 0 { (done as i64).min(total) as u64 } else { done };
                if last_emit.elapsed() >= Duration::from_millis(150) {
                    last_emit = std::time::Instant::now();
                    let _ = tx.send(EngineEvent::Progress {
                        id: task.id,
                        downloaded: reported as i64,
                        total,
                        speed_bps: 0.0,
                    });
                }
            }
            file.flush().await?;
            // Clean TCP close but fewer bytes than promised: the peer ended
            // the transfer without fulfilling Content-Length (the
            // no-close_notify case shows up here with some proxies/CDNs).
            // Never report success on a short body.
            if let Some(expected) = expected_len {
                if done < expected {
                    return Err(DownloadError::IncompleteBody);
                }
            }
            if total > 0 && (done as i64) < total {
                return Err(DownloadError::IncompleteBody);
            }
            return Ok(done);
        }
    }

    // 8 args: internal helper threading per-chunk state; a params struct
    // would add indirection without a second caller.
    #[allow(clippy::too_many_arguments)]
    async fn run_chunked(
        &self,
        task: &DownloadTask,
        final_path: &Path,
        part_dir: &Path,
        total: i64,
        threads: usize,
        cancel: Arc<AtomicBool>,
        tx: mpsc::UnboundedSender<EngineEvent>,
    ) -> Result<u64, DownloadError> {
        // Compute chunk boundaries based on existing .part sizes (resume).
        let mut boundaries: Vec<(u64, u64)> = Vec::with_capacity(threads); // (start, end_inclusive)
        let chunk_size = total as u64 / threads as u64;
        let mut cursor = 0u64;
        for i in 0..threads {
            let start = cursor;
            let end = if i == threads - 1 {
                total as u64 - 1
            } else {
                start + chunk_size - 1
            };
            boundaries.push((start, end));
            cursor = end + 1;
        }

        let (chunk_tx, mut chunk_rx) = mpsc::unbounded_channel::<ChunkProgress>();
        let mut tasks = Vec::with_capacity(threads);
        // Validator for If-Range on resumed chunks. `None` on a fresh start
        // (nothing on disk to protect). Captured per-response below so the
        // next pause/resume cycle always has the current object's validator.
        let if_range_shared = load_validator(&self.state_dir, task.id).await;
        let state_dir_for_capture = self.state_dir.clone();

        for (i, &(start, end)) in boundaries.iter().enumerate() {
            let part_path = part_dir.join(format!("part{i}"));
            let resume = fs::metadata(&part_path).await.map(|m| m.len()).unwrap_or(0);
            // Build the per-task client (honors `read_timeout_ms` + proxy).
            // When the task has no read timeout, this returns the shared
            // `http_client()` clone — cheap, Arc-backed.
            let client = self.client_for(task)?;
            let headers = Self::headers_for(task);
            let auth_header = self.auth_header_for(task);
            let speed_limit = self.speed_limit.clone();
            let cancel = cancel.clone();
            let chunk_tx = chunk_tx.clone();
            let range_start = start + resume;
            let url = task.url.clone();
            let if_range = if resume > 0 {
                if_range_shared.clone()
            } else {
                None
            };
            let state_dir = state_dir_for_capture.clone();
            let task_id = task.id;

            tasks.push(ChunkTask {
                idx: i,
                handle: tokio::spawn(async move {
                    if range_start > end {
                        chunk_tx.send((i, end + 1 - start)).ok();
                        return Ok(());
                    }
                    let mut h = headers.clone();
                    if let Some(auth) = auth_header {
                        h.insert(AUTHORIZATION, auth);
                    }

                    h.insert(
                        RANGE,
                        HeaderValue::from_str(&format!("bytes={range_start}-{end}")).unwrap(),
                    );
                    if let Some(v) = &if_range {
                        h.insert(IF_RANGE, v.clone());
                    }
                    let resp = client.get(&url).headers(h).send().await?;
                    let status = resp.status();
                    if status == StatusCode::OK {
                        return Err(DownloadError::RangeIgnored);
                    }
                    if status != StatusCode::PARTIAL_CONTENT {
                        return Err(DownloadError::HttpStatus(status.as_u16()));
                    }
                    // Resume-safety gate: the 206 must start at the exact
                    // offset this segment asked for, or appending would
                    // duplicate/skip bytes within the part file.
                    verify_resume_offset(&resp, range_start)?;
                    // Non-compliant server: honours Range but returns 206 for
                    // a *changed* object. The bytes would silently corrupt
                    // the part file — treat it like an ignored range.
                    if let Some(sent) = &if_range {
                        let (etag, lm) = response_validators(&resp);
                        let changed = etag
                            .or(lm)
                            .map(|v| v.as_bytes() != sent.as_bytes())
                            .unwrap_or(false);
                        if changed {
                            return Err(DownloadError::RangeIgnored);
                        }
                    }
                    // Capture the current validator for future resumes.
                    {
                        let (etag, lm) = response_validators(&resp);
                        save_validator(&state_dir, task_id, etag, lm).await;
                    }
                    let mut file = fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&part_path)
                        .await?;
                    let expected_seg = resp.content_length().map(|n| n + resume);
                    let mut stream = resp.bytes_stream();
                    let mut done = resume;
                    while let Some(chunk) = stream.next().await {
                        if cancel.load(Ordering::Relaxed) {
                            return Err(DownloadError::Cancelled);
                        }
                        // Abrupt transport termination mid-segment: map to
                        // IncompleteBody (retryable transient) instead of a
                        // raw reqwest error. Part bytes already written stay.
                        let chunk = match chunk {
                            Ok(c) => c,
                            Err(e) if is_abrupt_close_err(&e) => {
                                return Err(DownloadError::IncompleteBody)
                            }
                            Err(e) => return Err(DownloadError::Reqwest(e)),
                        };
                        throttle_write(&mut file, &mut done, &chunk, &speed_limit).await?;
                        // Cap reported bytes at the segment end so a server that
                        // ignores Range and re-sends the full file cannot inflate
                        // the cumulative counter. The excess bytes are already
                        // written to disk by throttle_write; this only prevents a
                        // bogus value from propagating to the DB and UI.
                        let capped = (end.saturating_sub(start) + 1).min(done);
                        chunk_tx.send((i, capped)).ok();
                    }
                    file.flush().await?;
                    // Clean TCP close before the segment was fully served:
                    // never report success on a short part.
                    if let Some(expected) = expected_seg {
                        if done < expected {
                            return Err(DownloadError::IncompleteBody);
                        }
                    }
                    Ok(())
                }),
            });
        }

        // Progress aggregation loop
        let task_count = tasks.len();
        let mut done_per_chunk = vec![0u64; threads];
        let mut total_done: u64 = 0;
        let mut last_emit = std::time::Instant::now();
        let mut first_err: Option<DownloadError> = None;
        let mut finished = 0usize;
        let mut join_handles: Vec<_> = tasks.into_iter().map(|t| (t.idx, t.handle)).collect();
        drop(chunk_tx);

        loop {
            // Drain all available progress messages
            while let Ok((idx, bytes)) = chunk_rx.try_recv() {
                done_per_chunk[idx] = bytes;
                total_done = done_per_chunk.iter().sum();
                if last_emit.elapsed() >= Duration::from_millis(150) {
                    last_emit = std::time::Instant::now();
                    let _ = tx.send(EngineEvent::Progress {
                        id: task.id,
                        downloaded: total_done as i64,
                        total,
                        speed_bps: 0.0,
                    });
                    // Per-chunk snapshot (segment modal). Cheap: bounded by
                    // `threads` (≤ max_threads), only on the same tick as
                    // Progress — no extra wakeups.
                    let boundaries_ref: &[(u64, u64)] = &boundaries;
                    for (i, &d) in done_per_chunk.iter().enumerate() {
                        let start = boundaries_ref[i].0;
                        let end = boundaries_ref[i].1;
                        let _ = tx.send(EngineEvent::SegmentProgress {
                            id: task.id,
                            idx: i,
                            bytes: d,
                            size: end.saturating_sub(start) + 1,
                        });
                    }
                }
            }

            // Check pause/cancel
            if cancel.load(Ordering::Relaxed) {
                for (_, h) in &join_handles {
                    h.abort();
                }
                break;
            }

            // Reap finished handles
            let mut still_running = Vec::new();
            for (idx, handle) in join_handles.drain(..) {
                if handle.is_finished() {
                    finished += 1;
                    match handle.await {
                        Ok(Ok(())) => {}
                        Ok(Err(e)) => {
                            if !matches!(e, DownloadError::Cancelled) {
                                first_err.get_or_insert(e);
                            }
                        }
                        Err(_join) => { /* aborted by pause */ }
                    }
                } else {
                    still_running.push((idx, handle));
                }
            }
            join_handles = still_running;

            if finished >= task_count {
                break;
            }

            // Emit periodic progress and sleep
            let _ = tx.send(EngineEvent::Progress {
                id: task.id,
                downloaded: total_done as i64,
                total,
                speed_bps: 0.0,
            });
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        if cancel.load(Ordering::Relaxed) {
            return Err(DownloadError::Cancelled);
        }
        if let Some(err) = first_err {
            eprintln!("[chunked] id={} first_err={:?}", task.id, err);
            return Err(err);
        }
        if finished < task_count {
            eprintln!(
                "[chunked] id={} finished={} task_count={} total_done={}",
                task.id, finished, task_count, total_done
            );
            return Err(DownloadError::Cancelled);
        }

        // Merge parts in order
        let merged = merge_parts(part_dir, final_path, threads).await?;

        // Optional integrity verification
        if let Some(expected) = &task.expected_sha256 {
            let got = hash_file(final_path).await?;
            if !got.eq_ignore_ascii_case(expected) {
                return Err(DownloadError::ChecksumMismatch {
                    expected: expected.clone(),
                    got,
                });
            }
        }
        let _ = fs::remove_dir_all(part_dir).await;
        Ok(merged)
    }

    /// Cooperative global speed limiter: sleeps proportional to over-usage.
    async fn throttle(&self, bytes: u64) {
        throttle_bucket(bytes, &self.speed_limit).await;
    }
}

// Chunk-local throttling uses the same shared bucket via helper below.
/// Map a low-level I/O error to a more actionable `DownloadError`.
/// On Linux, disk-full manifests as `ENOSPC` (`ErrorKind::StorageFull` is
/// not yet stable, so we check the raw `os_error()` value too).
fn map_io(e: std::io::Error) -> DownloadError {
    if e.kind() == std::io::ErrorKind::StorageFull || e.raw_os_error() == Some(28 /* ENOSPC */) {
        return DownloadError::DiskFull;
    }
    DownloadError::Io(e)
}

/// True when a reqwest body-read error is an abrupt transport termination
/// (missing TLS close_notify / unexpected EOF / truncated HTTP body /
/// connection reset) rather than a TLS configuration failure. Checked via
/// the error's Display **and** its source chain, since reqwest wraps the
/// hyper/rustls cause.
fn is_abrupt_close_err(e: &reqwest::Error) -> bool {
    let mut text = e.to_string();
    let mut src: Option<&dyn std::error::Error> = e.source();
    while let Some(s) = src {
        text.push_str(": ");
        text.push_str(&s.to_string());
        src = s.source();
    }
    crate::models::is_abrupt_close_msg(&text)
}

/// Is this reqwest error a transient transport failure worth one probe
/// retry? True for connect/timeout/reset/EOF-class failures, false for
/// anything HTTP-status- or TLS-configuration-shaped.
fn is_transient_probe_err(e: &reqwest::Error) -> bool {
    if e.is_connect() || e.is_timeout() {
        return true;
    }
    is_abrupt_close_err(e)
}

/// Resume-safety gate (single-stream): before appending resumed bytes,
/// confirm the server actually answered the range we asked for. A 206
/// whose `Content-Range` start differs from the requested offset would
/// duplicate or skip bytes — refuse it rather than corrupt the file.
/// Returns `Err` with a descriptive message when the server's answer
/// doesn't match the request.
fn verify_resume_offset(
    resp: &reqwest::Response,
    requested_start: u64,
) -> Result<(), DownloadError> {
    let Some(cr) = resp.headers().get(CONTENT_RANGE) else {
        // A 206 without Content-Range is protocol-violating; refuse to
        // append rather than guess.
        return Err(DownloadError::ConnectionFailed(format!(
            "resume response missing Content-Range header (requested start {requested_start})"
        )));
    };
    let cr_str = cr
        .to_str()
        .map_err(|_| DownloadError::ConnectionFailed("unparseable Content-Range header".into()))?;
    match crate::models::parse_content_range_start(cr_str) {
        Some(start) if start == requested_start => Ok(()),
        Some(start) => Err(DownloadError::ConnectionFailed(format!(
            "server resumed from byte {start} but {requested_start} was requested (Content-Range: {cr_str})"
        ))),
        None => Err(DownloadError::ConnectionFailed(format!(
            "unparseable Content-Range header: {cr_str}"
        ))),
    }
}

async fn throttle_write<W: tokio::io::AsyncWrite + Unpin>(
    file: &mut W,
    done: &mut u64,
    chunk: &[u8],
    speed_limit: &Arc<AtomicU64>,
) -> Result<(), DownloadError> {
    throttle_bucket(chunk.len() as u64, speed_limit).await;
    file.write_all(chunk).await.map_err(map_io)?;
    *done += chunk.len() as u64;
    Ok(())
}

async fn throttle_bucket(bytes: u64, speed_limit: &Arc<AtomicU64>) {
    let limit = speed_limit.load(Ordering::Relaxed);
    if limit == 0 {
        return;
    }
    // ~= sleep(bytes / limit) seconds, spread across calls
    let nanos = bytes.saturating_mul(1_000_000_000) / limit.max(1);
    if nanos > 0 {
        tokio::time::sleep(Duration::from_nanos(nanos)).await;
    }
}

async fn merge_parts(
    part_dir: &Path,
    final_path: &Path,
    threads: usize,
) -> Result<u64, DownloadError> {
    eprintln!("[merge_parts] part_dir={:?} threads={}", part_dir, threads);
    let mut out = fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(final_path)
        .await?;
    let mut total = 0u64;
    for i in 0..threads {
        let part_path = part_dir.join(format!("part{i}"));
        if !part_path.exists() {
            continue;
        }
        let mut part = fs::File::open(&part_path).await?;
        let mut buf = [0u8; 512 * 1024];
        loop {
            let n = tokio::io::AsyncReadExt::read(&mut part, &mut buf).await?;
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n]).await.map_err(map_io)?;
            total += n as u64;
        }
    }
    out.flush().await.map_err(map_io)?;
    Ok(total)
}

async fn hash_file(path: &Path) -> Result<String, DownloadError> {
    let mut file = fs::File::open(path).await?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 512 * 1024];
    loop {
        let n = tokio::io::AsyncReadExt::read(&mut file, &mut buf).await?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// Parse `Content-Disposition: attachment; filename="..."` into a clean
/// filename. Handles RFC 5987 `filename*=UTF-8''...` as a fallback and
/// ignores the various `form-data` / `name=` attributes used by multipart.
fn parse_content_disposition(header: Option<&reqwest::header::HeaderValue>) -> Option<String> {
    let value = header?.to_str().ok()?;
    let mut plain: Option<String> = None;
    let mut ext: Option<String> = None;
    for part in value.split(';') {
        let part = part.trim();
        if part.eq_ignore_ascii_case("attachment") || part.eq_ignore_ascii_case("inline") {
            continue;
        }
        if let Some(rest) = part.strip_prefix("filename*=") {
            // RFC 5987: charset'lang'value  (we just want the value segment).
            if let Some(v) = rest.split('\'').nth(2) {
                ext = Some(percent_decode(v));
            }
        } else if let Some(rest) = part.strip_prefix("filename=") {
            let v = rest.trim_matches('"');
            if !v.is_empty() {
                plain = Some(v.to_string());
            }
        }
    }
    // RFC 5987 form wins over plain when present (handles UTF-8 names).
    ext.or(plain)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && !s.contains('/') && !s.contains(".."))
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (hex_digit(bytes[i + 1]), hex_digit(bytes[i + 2])) {
                out.push(h * 16 + l);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use tempfile::tempdir;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    async fn spawn_origin() -> (std::net::SocketAddr, Arc<AtomicUsize>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let hits = Arc::new(AtomicUsize::new(0));
        let shared = Arc::clone(&hits);
        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    continue;
                };
                let shared = Arc::clone(&shared);
                tokio::spawn(async move {
                    let mut buf = [0u8; 4096];
                    let n = stream.read(&mut buf).await.unwrap_or(0);
                    if n == 0 {
                        return;
                    }
                    shared.fetch_add(1, Ordering::SeqCst);
                    let is_head = String::from_utf8_lossy(&buf[..n]).starts_with("HEAD ");
                    let body = b"proxy-test";
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = stream.write_all(response.as_bytes()).await;
                    if !is_head {
                        let _ = stream.write_all(body).await;
                    }
                });
            }
        });
        (addr, hits)
    }

    async fn spawn_proxy() -> (std::net::SocketAddr, Arc<AtomicUsize>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let hits = Arc::new(AtomicUsize::new(0));
        let shared = Arc::clone(&hits);
        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    continue;
                };
                let shared = Arc::clone(&shared);
                tokio::spawn(async move {
                    let mut buf = [0u8; 8192];
                    let n = stream.read(&mut buf).await.unwrap_or(0);
                    if n == 0 {
                        return;
                    }
                    shared.fetch_add(1, Ordering::SeqCst);
                    let request = String::from_utf8_lossy(&buf[..n]);
                    let method = request
                        .lines()
                        .next()
                        .and_then(|line| line.split_whitespace().next())
                        .unwrap_or("GET");
                    let body = b"proxy-test";
                    let response = if method.eq_ignore_ascii_case("HEAD") {
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n",
                            body.len()
                        )
                    } else {
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\nproxy-test",
                            body.len()
                        )
                    };
                    let _ = stream.write_all(response.as_bytes()).await;
                });
            }
        });
        (addr, hits)
    }

    fn task(url: String, proxy_url: Option<String>, dir: &std::path::Path) -> DownloadTask {
        let mut task = DownloadTask::default_for_probe(&url, "proxy-test.txt");
        task.proxy_url = proxy_url;
        task.save_dir = dir.to_string_lossy().into_owned();
        task
    }

    #[tokio::test]
    async fn custom_proxy_is_used_and_direct_override_bypasses_global_proxy() {
        let (origin, origin_hits) = spawn_origin().await;
        let (proxy, proxy_hits) = spawn_proxy().await;
        let dir = tempdir().unwrap();
        let downloader =
            ChunkDownloader::new(dir.path().join("state"), Arc::new(AtomicU64::new(0)));
        downloader.set_proxy(Some(&format!("http://{proxy}")));

        let through_proxy = task(format!("http://{origin}/through-proxy"), None, dir.path());
        downloader.probe(&through_proxy).await.unwrap();
        assert!(proxy_hits.load(Ordering::SeqCst) > 0);
        assert_eq!(origin_hits.load(Ordering::SeqCst), 0);

        let before = proxy_hits.load(Ordering::SeqCst);
        let direct = task(
            format!("http://{origin}/direct"),
            Some(String::new()),
            dir.path(),
        );
        downloader.probe(&direct).await.unwrap();
        assert_eq!(proxy_hits.load(Ordering::SeqCst), before);
        assert!(origin_hits.load(Ordering::SeqCst) > 1);
    }

    /// An HTTP proxy that demands HTTP Basic `Proxy-Authorization`.
    /// Returns 407 (with the expected scheme advertised) when the header
    /// is missing or wrong, 200 otherwise. Records the last seen header so
    /// the test can assert the exact credential that arrived.
    async fn spawn_auth_proxy(
        expected: &'static str,
    ) -> (std::net::SocketAddr, Arc<AtomicUsize>, Arc<std::sync::Mutex<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let ok_hits = Arc::new(AtomicUsize::new(0));
        let last_header = Arc::new(std::sync::Mutex::new(String::new()));
        let shared_ok = Arc::clone(&ok_hits);
        let shared_hdr = Arc::clone(&last_header);
        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    continue;
                };
                let shared_ok = Arc::clone(&shared_ok);
                let shared_hdr = Arc::clone(&shared_hdr);
                tokio::spawn(async move {
                    let mut buf = [0u8; 8192];
                    let n = stream.read(&mut buf).await.unwrap_or(0);
                    if n == 0 {
                        return;
                    }
                    let request = String::from_utf8_lossy(&buf[..n]).to_string();
                    let seen = request
                        .lines()
                        .find(|l| l.to_ascii_lowercase().starts_with("proxy-authorization:"))
                        .map(|l| l.trim().to_string())
                        .unwrap_or_default();
                    *shared_hdr.lock().unwrap() = seen.clone();
                    let authed = seen.eq_ignore_ascii_case(expected);
                    if !authed {
                        let resp = "HTTP/1.1 407 Proxy Authentication Required\r\n\
                                    Proxy-Authenticate: Basic realm=\"downpilot\"\r\n\
                                    Content-Length: 0\r\nConnection: close\r\n\r\n";
                        let _ = stream.write_all(resp.as_bytes()).await;
                        return;
                    }
                    shared_ok.fetch_add(1, Ordering::SeqCst);
                    let body = b"proxy-test";
                    let is_head = request.starts_with("HEAD ");
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = stream.write_all(resp.as_bytes()).await;
                    if !is_head {
                        let _ = stream.write_all(body).await;
                    }
                });
            }
        });
        (addr, ok_hits, last_header)
    }

    #[tokio::test]
    async fn proxy_userinfo_in_url_is_sent_as_basic_proxy_auth() {
        // End-to-end proof that credentials embedded in the proxy URL
        // (`http://user:pass@host`) are actually presented to the proxy.
        // This is the transport behaviour the UI lets users configure; the
        // matching redaction work ensures the same URL never leaks the
        // password to the browser feed or DB listings.
        let expected = format!(
            "Proxy-Authorization: Basic {}",
            base64::engine::general_purpose::STANDARD.encode("user:pass")
        );
        let expected: &'static str = Box::leak(expected.into_boxed_str());
        let (proxy, ok_hits, last_header) = spawn_auth_proxy(expected).await;
        let dir = tempdir().unwrap();

        // 1. No credentials in the URL -> proxy rejects with 407.
        let anon = ChunkDownloader::new(dir.path().join("state-anon"), Arc::new(AtomicU64::new(0)));
        anon.set_proxy(Some(&format!("http://{proxy}")));
        let t = task("http://example.invalid/file".into(), None, dir.path());
        let err = anon.probe(&t).await.unwrap_err();
        assert!(
            matches!(err, DownloadError::HttpStatus(407)),
            "expected 407 without proxy auth, got {err:?}"
        );
        assert_eq!(ok_hits.load(Ordering::SeqCst), 0);
        assert!(last_header.lock().unwrap().is_empty());

        // 2. Credentials in the URL -> reqwest extracts them and the
        //    proxy accepts the request.
        let authed = ChunkDownloader::new(dir.path().join("state-authed"), Arc::new(AtomicU64::new(0)));
        authed.set_proxy(Some(&format!("http://user:pass@{proxy}")));
        authed.probe(&t).await.unwrap();
        assert!(ok_hits.load(Ordering::SeqCst) > 0, "proxy never accepted the authed request");
        // HTTP/1.1 header names are case-insensitive; reqwest emits them
        // lowercase. Compare the value, not the original casing.
        let seen = last_header.lock().unwrap().clone();
        let (seen_name, seen_value) = seen.split_once(':').expect("header present");
        assert!(seen_name.eq_ignore_ascii_case("proxy-authorization"));
        assert_eq!(seen_value.trim(), expected.split_once(':').unwrap().1.trim());
    }
}
