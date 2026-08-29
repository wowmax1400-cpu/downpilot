//! Multi-threaded chunked downloader with HTTP Range requests,
//! pause/resume via persisted .part files, and optional SHA-256 verification.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::StreamExt;
use reqwest::header::{HeaderMap, HeaderValue, COOKIE, RANGE, REFERER, USER_AGENT};
use reqwest::StatusCode;
use sha2::{Digest, Sha256};
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;

use crate::models::{parse_content_range_total, DownloadTask, EngineEvent};

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
    #[error("server does not support range requests; single-threaded download used and still failed")]
    NoRangeSupport,
    #[error("empty body returned by server")]
    EmptyBody,
    #[error("cancelled")]
    Cancelled,
    #[error("checksum mismatch: expected {expected}, got {got}")]
    ChecksumMismatch { expected: String, got: String },
    #[error("invalid output path")]
    InvalidPath,
}

type ChunkProgress = (usize, u64); // (chunk index, cumulative bytes done)

pub struct ChunkDownloader {
    client: Mutex<reqwest::Client>,
    /// Stash the proxy URL separately so per-task read-timeout clients can
    /// rebuild it (reqwest::Client doesn't expose its configured proxy).
    proxy_url: Mutex<Option<String>>,
    state_dir: PathBuf,
    speed_limit: Arc<AtomicU64>,
}

struct ChunkTask {
    idx: usize,
    handle: tokio::task::JoinHandle<Result<(), DownloadError>>,
}

impl ChunkDownloader {
    pub fn new(state_dir: PathBuf, speed_limit: Arc<AtomicU64>) -> Self {
        let client = Self::build_client(None);
        Self {
            client: Mutex::new(client),
            proxy_url: Mutex::new(None),
            state_dir,
            speed_limit,
        }
    }

    fn build_client(proxy: Option<&str>) -> reqwest::Client {
        let mut b = reqwest::Client::builder().connect_timeout(Duration::from_secs(15));
        if let Some(p) = proxy {
            if let Ok(p) = reqwest::Proxy::all(p) {
                b = b.proxy(p);
            }
        }
        b.build().expect("failed to build http client")
    }

    /// Rebuild the HTTP client with a new proxy. Pass None to clear.
    pub fn set_proxy(&self, proxy: Option<&str>) {
        *self.client.lock().unwrap() = Self::build_client(proxy);
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
    fn client_for(&self, task: &DownloadTask) -> reqwest::Client {
        if task.read_timeout_ms > 0 {
            let proxy = self.proxy_url.lock().unwrap().clone();
            let mut b = reqwest::Client::builder();
            if let Some(p) = proxy.as_deref() {
                if let Ok(proxy) = reqwest::Proxy::all(p) {
                    b = b.proxy(proxy);
                }
            }
            b.read_timeout(Duration::from_millis(task.read_timeout_ms as u64))
                .build()
                .unwrap_or_else(|_| self.http_client())
        } else {
            self.http_client()
        }
    }

    /// Probe the server: returns (total size if known, range support,
    /// server-suggested filename, content-type).
    pub async fn probe(
        &self,
        task: &DownloadTask,
    ) -> Result<(Option<i64>, bool, Option<String>, Option<String>), DownloadError> {
        let resp = self
            .client_for(task)
            .head(&task.url)
            .headers(Self::headers_for(task))
            .send()
            .await?;
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
        h.insert(RANGE, HeaderValue::from_static("bytes=0-0"));
        let resp = self
            .client_for(task)
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
        h.insert(RANGE, HeaderValue::from_static("bytes=0-0"));
        let resp = self
            .client_for(task)
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
        h.insert(RANGE, HeaderValue::from_static("bytes=0-0"));
        let resp = self
            .client_for(task)
            .get(&task.url)
            .headers(h)
            .send()
            .await?;
        if let Some(cr) = resp.headers().get("content-range").and_then(|v| v.to_str().ok()) {
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
        let threads = if range_ok { task.threads.max(1) as usize } else { 1 };
        let final_path = Path::new(&task.save_dir).join(&task.filename);
        let part_dir = self.state_dir.join(format!("parts-{}", task.id));
        fs::create_dir_all(&part_dir).await?;

        let total = total_opt.unwrap_or(0);
        if total > 0 && threads > 1 {
            self.run_chunked(&task, &final_path, &part_dir, total, threads, cancel, tx).await
        } else {
            let bytes = self.run_single(&task, &final_path, total, cancel, tx).await?;
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
        if existing > 0 && total > 0 {
            headers.insert(RANGE, HeaderValue::from_str(&format!("bytes={existing}-")).unwrap());
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
            .client_for(task)
            .get(&task.url)
            .headers(headers)
            .send()
            .await?;
        let status = resp.status();
        if !status.is_success() && status != StatusCode::PARTIAL_CONTENT {
            return Err(DownloadError::HttpStatus(status.as_u16()));
        }
        let mut stream = resp.bytes_stream();
        let mut file = fs::OpenOptions::new().create(true).append(true).open(final_path).await?;
        let mut done = existing;
        let mut last_emit = std::time::Instant::now();

        while let Some(chunk) = stream.next().await {
            if cancel.load(Ordering::Relaxed) {
                return Err(DownloadError::Cancelled);
            }
            let chunk = chunk?;
            self.throttle(chunk.len() as u64).await;
            file.write_all(&chunk).await?;
            done += chunk.len() as u64;
            if last_emit.elapsed() >= Duration::from_millis(150) {
                last_emit = std::time::Instant::now();
                let _ = tx.send(EngineEvent::Progress {
                    id: task.id,
                    downloaded: done as i64,
                    total,
                    speed_bps: 0.0,
                });
            }
        }
        file.flush().await?;
        Ok(done)
    }

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
            let end = if i == threads - 1 { total as u64 - 1 } else { start + chunk_size - 1 };
            boundaries.push((start, end));
            cursor = end + 1;
        }

        let (chunk_tx, mut chunk_rx) = mpsc::unbounded_channel::<ChunkProgress>();
        let mut tasks = Vec::with_capacity(threads);

        for i in 0..threads {
            let (start, end) = boundaries[i];
            let part_path = part_dir.join(format!("part{i}"));
            let resume = fs::metadata(&part_path).await.map(|m| m.len()).unwrap_or(0);
            // Build the per-task client (honors `read_timeout_ms` + proxy).
            // When the task has no read timeout, this returns the shared
            // `http_client()` clone — cheap, Arc-backed.
            let client = self.client_for(task);
            let headers = Self::headers_for(task);
            let speed_limit = self.speed_limit.clone();
            let cancel = cancel.clone();
            let chunk_tx = chunk_tx.clone();
            let range_start = start + resume;
            let url = task.url.clone();
            tasks.push(ChunkTask {
                idx: i,
                handle: tokio::spawn(async move {
                    if range_start > end {
                        chunk_tx.send((i, end + 1 - start)).ok();
                        return Ok(());
                    }
                    let mut h = headers.clone();
                    h.insert(
                        RANGE,
                        HeaderValue::from_str(&format!("bytes={range_start}-{end}")).unwrap(),
                    );
                    let resp = client.get(&url).headers(h).send().await?;
                    let status = resp.status();
                    if status != StatusCode::PARTIAL_CONTENT && status != StatusCode::OK {
                        return Err(DownloadError::HttpStatus(status.as_u16()));
                    }
                    let mut file = fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&part_path)
                        .await?;
                    let mut stream = resp.bytes_stream();
                    let mut done = resume;
                    while let Some(chunk) = stream.next().await {
                        if cancel.load(Ordering::Relaxed) {
                            return Err(DownloadError::Cancelled);
                        }
                        let chunk = chunk?;
                        throttle_write(&mut file, &mut done, &chunk, &speed_limit).await?;
                        chunk_tx.send((i, done)).ok();
                    }
                    file.flush().await?;
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
            eprintln!("[chunked] id={} finished={} task_count={} total_done={}", task.id, finished, task_count, total_done);
            return Err(DownloadError::Cancelled);
        }

        // Merge parts in order
        let merged = merge_parts(part_dir, final_path, threads).await?;

        // Optional integrity verification
        if let Some(expected) = &task.expected_sha256 {
            let got = hash_file(final_path).await?;
            if !got.eq_ignore_ascii_case(expected) {
                return Err(DownloadError::ChecksumMismatch { expected: expected.clone(), got });
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
    if e.kind() == std::io::ErrorKind::StorageFull
        || e.raw_os_error() == Some(28 /* ENOSPC */)
    {
        return DownloadError::DiskFull;
    }
    DownloadError::Io(e)
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

async fn merge_parts(part_dir: &Path, final_path: &Path, threads: usize) -> Result<u64, DownloadError> {
    eprintln!("[merge_parts] part_dir={:?} threads={}", part_dir, threads);
    let mut out = fs::OpenOptions::new().create(true).truncate(true).write(true).open(final_path).await?;
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
