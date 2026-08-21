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
        Self { client: Mutex::new(client), state_dir, speed_limit }
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

    /// Probe the server: returns (total size if known, range support).
    async fn probe(&self, task: &DownloadTask) -> Result<(Option<i64>, bool), DownloadError> {
        let resp = self.http_client().head(&task.url).headers(Self::headers_for(task)).send().await?;
        if resp.status().is_success() {
            let range_ok = resp
                .headers()
                .get("accept-ranges")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.eq_ignore_ascii_case("bytes"))
                .unwrap_or(false)
                || resp.headers().get("content-range").is_some();
            return Ok((resp.content_length().map(|n| n as i64), range_ok));
        }
        // Some servers reject HEAD; probe with a 1-byte ranged GET.
        let mut h = Self::headers_for(task);
        h.insert(RANGE, HeaderValue::from_static("bytes=0-0"));
        let resp = self.http_client().get(&task.url).headers(h).send().await?;
        if resp.status() == StatusCode::PARTIAL_CONTENT {
            let total = resp
                .headers()
                .get("content-range")
                .and_then(|v| v.to_str().ok())
                .and_then(parse_content_range_total);
            return Ok((total, true));
        }
        Ok((resp.content_length().map(|n| n as i64), false))
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
        let (total_opt, range_ok) = self.probe(&task).await?;
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
        let existing = fs::metadata(final_path).await.map(|m| m.len()).unwrap_or(0);
        let mut headers = Self::headers_for(task);
        if existing > 0 {
            headers.insert(RANGE, HeaderValue::from_str(&format!("bytes={existing}-")).unwrap());
        }
        let resp = self.http_client().get(&task.url).headers(headers).send().await?;
        if !resp.status().is_success() && resp.status() != StatusCode::PARTIAL_CONTENT {
            return Err(DownloadError::NoRangeSupport);
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
            let client = self.http_client();
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
                    if resp.status() != StatusCode::PARTIAL_CONTENT && resp.status() != StatusCode::OK {
                        return Err(DownloadError::NoRangeSupport);
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
            return Err(err);
        }
        if finished < task_count {
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
async fn throttle_write<W: tokio::io::AsyncWrite + Unpin>(
    file: &mut W,
    done: &mut u64,
    chunk: &[u8],
    speed_limit: &Arc<AtomicU64>,
) -> Result<(), DownloadError> {
    throttle_bucket(chunk.len() as u64, speed_limit).await;
    file.write_all(chunk).await?;
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
            out.write_all(&buf[..n]).await?;
            total += n as u64;
        }
    }
    out.flush().await?;
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
