//! FTP (ftp:// and ftps:// noted below) download support.
//!
//! Sits beside the HTTP path: the engine dispatches `ftp://` URLs here from
//! `run_task`, and the Add dialog/probe path uses `probe_ftp` instead of the
//! HTTP HEAD when the scheme matches. Credential behaviour:
//!
//! - `ftp://user:pass@host/path` — creds from the URL (percent-decoded)
//! - `ftp://user@host/path` — same passwordless user
//! - otherwise anonymous `ftpuser@downpilot.local` / `anonymous`, the
//!   RFC-1635 convention most anonymous mirrors accept.
//!
//! Resume: the engine's byte-count reconciliation (finish_task) plus the
//! `REST` restart mean a timed-out/paused FTP task can continue from what is
//! already on disk. SuppaFTP's `resume_transfer(offset)` sets a REST command
//! before the next data transfer, so we re-open the control channel and
//! restart at the file's current size on resume. No Range-equivalent
//! validator exists in FTP, so we accept the server's word at the offset and
//! never append after a size mismatch (we restart then too).

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use suppaftp::tokio::AsyncFtpStream;
use suppaftp::types::FileType;
use suppaftp::FtpError;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;

use crate::models::{DownloadTask, EngineEvent};
use tokio::fs;

/// Host/port/user/pass parsed from an FTP URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FtpUrl {
    pub host: String,
    pub port: u16,
    pub user: Option<String>,
    pub password: Option<String>,
    /// Path INCLUDING the leading `/`, already percent-decoded.
    pub path: String,
}

impl FtpUrl {
    /// Path of the file inside its directory (for CWD + RETR).
    pub fn dir(&self) -> String {
        match self.path.as_str().rfind('/') {
            Some(0) => String::from("/"),
            Some(i) => self.path[..i].to_string(),
            None => String::from("/"),
        }
    }
    pub fn filename(&self) -> String {
        match self.path.rsplit('/').next() {
            Some(n) if !n.is_empty() => n.to_string(),
            _ => String::from("download"),
        }
    }
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Parse an `ftp://` URL. Anonymous (no userinfo) yields `None` user.
/// Port defaults to 21 per RFC 3986 when absent.
pub fn parse_ftp_url(url: &str) -> Result<FtpUrl, String> {
    let rest = url
        .strip_prefix("ftp://")
        .ok_or_else(|| "not an ftp:// URL".to_string())?;
    // Split off any path/query/fragment first: `userinfo@host[:port]/path`.
    let (authority_raw, path_raw) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let authority_raw = authority_raw.split(['?', '#']).next().unwrap_or(authority_raw);
    let (userinfo, hostport) = match authority_raw.rfind('@') {
        Some(i) => (Some(&authority_raw[..i]), &authority_raw[i + 1..]),
        None => (None, authority_raw),
    };
    if hostport.is_empty() {
        return Err("ftp URL has no host".to_string());
    }
    let (host, port) = match hostport.rsplit_once(':') {
        Some((h, p)) if !h.is_empty() => (
            h.to_string(),
            p.parse::<u16>().map_err(|_| "invalid ftp port".to_string())?,
        ),
        _ => (hostport.to_string(), 21u16),
    };
    let (user, password) = match userinfo.as_deref() {
        Some(ui) => {
            let (u, p) = match ui.split_once(':') {
                Some((u, p)) => (u, Some(p.to_string())),
                None => (ui, None),
            };
            (Some(percent_decode(u)), p.map(|p| percent_decode(&p)))
        }
        None => (None, None),
    };

    // Path: strip query/fragment, percent-decode.
    let path = percent_decode(
        path_raw
            .split(['?', '#'])
            .next()
            .unwrap_or(path_raw),
    );
    Ok(FtpUrl {
        host,
        port,
        user,
        password,
        path,
    })
}

/// Anonymous login per RFC 1635. Some servers literally want the string
/// "anonymous" as the username, others accept any email-looking value;
/// `ftpuser@downpilot.local` is the usual working pair. Both are sent here.
const ANON_USER: &str = "ftpuser@downpilot.local";
const ANON_PASS: &str = "anonymous";

async fn ftp_connect(task: &DownloadTask, f: &FtpUrl) -> Result<AsyncFtpStream, FtpError> {
    let mut s = AsyncFtpStream::connect((f.host.as_str(), f.port)).await?;
    let user = f.user.as_deref().unwrap_or(ANON_USER);
    let pass = f.password.as_deref().unwrap_or(ANON_PASS);
    s.login(user, pass).await?;
    s.transfer_type(FileType::Binary).await?;
    let _ = task; // cookies/referer/UA are HTTP-only concepts
    Ok(s)
}

fn map_ftp_err(e: FtpError) -> crate::download::DownloadError {
    use crate::download::DownloadError;
    match e {
        FtpError::ConnectionError(io) => map_io(io),
        FtpError::UnexpectedResponse(r) => DownloadError::ConnectionFailed(format!(
            "FTP server refused command ({} {})",
            r.status,
            String::from_utf8_lossy(&r.body).trim()
        )),
        e => DownloadError::ConnectionFailed(format!("FTP error: {e}")),
    }
}

fn map_io(e: std::io::Error) -> crate::download::DownloadError {
    use crate::download::DownloadError;
    match e.kind() {
        std::io::ErrorKind::PermissionDenied => {
            DownloadError::Permission(e.to_string())
        }
        std::io::ErrorKind::StorageFull => DownloadError::DiskFull,
        _ => {
            if matches!(
                e.kind(),
                std::io::ErrorKind::ConnectionRefused
                    | std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::ConnectionAborted
                    | std::io::ErrorKind::BrokenPipe
                    | std::io::ErrorKind::NotConnected
                    | std::io::ErrorKind::WouldBlock
                    | std::io::ErrorKind::TimedOut
                    | std::io::ErrorKind::UnexpectedEof
            ) {
                DownloadError::ConnectionFailed(e.to_string())
            } else {
                DownloadError::Io(e)
            }
        }
    }
}

// ---------------------------------------------------------------------
// Probe
// ---------------------------------------------------------------------

/// FTP probe: SIZE (with CWD) + filename, so the Add dialog shows the
/// remote filename and size without writing anything to disk.
pub async fn probe_ftp(
    url: &str,
    _save_dir: Option<&str>,
) -> Result<crate::models::ProbeInfo, String> {
    let f = parse_ftp_url(url)?;
    let dir = f.dir();
    let name = f.filename();

    let mut ftp = ftp_connect(&DownloadTask::default_for_probe(url, ""), &f)
        .await
        .map_err(map_ftp_err)
        .map_err(|e| e.to_string())?;
    ftp.cwd(&dir).await.map_err(map_ftp_err).map_err(|e| e.to_string())?;
    let size = ftp
        .size(&name)
        .await
        .map_err(map_ftp_err)
        .map_err(|e| e.to_string())?;
    let _ = ftp.quit().await;

    Ok(crate::models::ProbeInfo {
        url: url.to_string(),
        filename: name,
        size: size as i64,
        content_type: None,
        supports_resume: true, // REST is universal enough in practice
    })
}

// ---------------------------------------------------------------------
// Actual download (with pause and resume)
// ---------------------------------------------------------------------

/// Read the current size of `path` on disk (0 if absent/inaccessible).
async fn existing_bytes(path: &Path) -> u64 {
    fs::metadata(path).await.map(|m| m.len()).unwrap_or(0)
}

pub async fn run_ftp(
    task: DownloadTask,
    cancel: Arc<AtomicBool>,
    tx: mpsc::UnboundedSender<EngineEvent>,
) -> Result<u64, crate::download::DownloadError> {
    use crate::download::DownloadError;
    let f = parse_ftp_url(&task.url).map_err(|e| {
        DownloadError::ConnectionFailed(format!("invalid ftp URL: {e}"))
    })?;
    let dir = f.dir();
    let file_name = f.filename();
    let final_path = Path::new(&task.save_dir).join(&file_name);
    let total = task.total_size; // 0 = unknown until we ask the server

    'attempt: loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(DownloadError::Cancelled);
        }
        // Reconnect per attempt — suppaftp's control channel isn't
        // long-lived across a cancel/wait, so this keeps pause/resume simple.
        let mut ftp = ftp_connect(&task, &f).await.map_err(map_ftp_err)?;
        ftp.cwd(&dir).await.map_err(map_ftp_err)?;

        let remote_total: Option<u64> = match ftp.size(&file_name).await {
            Ok(s) => Some(s as u64),
            Err(_) => None, // SIZE unsupported (rare): fall back to full body
        };

        // Existing is either `nothing on disk yet` → restart, or
        // `partial file` → try REST at that offset. If the server reports a
        // size smaller than `existing`, our partial is stale → restart.
        let existing = existing_bytes(&final_path).await;
        if let Some(rt) = remote_total {
            // Empty/known-complete short-circuits (mirrors the HTTP path).
            if existing >= rt {
                let _ = tx.send(EngineEvent::Progress {
                    id: task.id,
                    downloaded: existing as i64,
                    total: rt as i64,
                    speed_bps: 0.0,
                });
                let _ = ftp.quit().await;
                return Ok(existing);
            }
            if existing > rt {
                let _ = fs::remove_file(&final_path).await.map_err(map_io)?;
                continue 'attempt;
            }
        }
        // Any resume attempt > 0 uses REST.
        let restart_at: u64 = existing;
        if restart_at > 0 {
            ftp.resume_transfer(restart_at as usize)
                .await
                .map_err(map_ftp_err)?;
        }

        let mut stream = ftp
            .retr_as_stream(&file_name)
            .await
            .map_err(map_ftp_err)?;
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&final_path)
            .await
            .map_err(map_io)?;
        let mut done: u64 = restart_at;
        let mut last_emit = Instant::now();
        let mut buf = vec![0u8; 16 * 1024];
        loop {
            if cancel.load(Ordering::Relaxed) {
                // Drop the data stream (its Drop impl closes the connection
                // cleanly) and bail — bytes stay on disk for resume.
                drop(stream);
                let _ = ftp.quit().await;
                return Err(DownloadError::Cancelled);
            }
            let n = stream.read(&mut buf).await.map_err(map_io)?;
            if n == 0 {
                break;
            }
            file.write_all(&buf[..n]).await.map_err(map_io)?;
            done += n as u64;
            if last_emit.elapsed() >= Duration::from_millis(150) {
                last_emit = Instant::now();
                let _ = tx.send(EngineEvent::Progress {
                    id: task.id,
                    downloaded: done as i64,
                    total: rt(total, remote_total),
                    speed_bps: 0.0,
                });
            }
        }
        file.flush().await.map_err(map_io)?;
        drop(stream);
        let _ = ftp.quit().await;
        // SIZE known and body short → transient cut, retryable.
        if let Some(rt2) = remote_total {
            if done < rt2 {
                return Err(DownloadError::IncompleteBody);
            }
            let _ = tx.send(EngineEvent::Progress {
                id: task.id,
                downloaded: done as i64,
                total: rt2 as i64,
                speed_bps: 0.0,
            });
            return Ok(done);
        }
        return Ok(done);
    }
}

#[inline]
fn rt(total: i64, remote_total: Option<u64>) -> i64 {
    if total > 0 {
        total
    } else {
        remote_total.unwrap_or(0) as i64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ftp_url_parses_basic() {
        let u = parse_ftp_url("ftp://mirror.example/pub/big.tar.gz").unwrap();
        assert_eq!(u.host, "mirror.example");
        assert_eq!(u.port, 21);
        assert_eq!(u.user, None);
        assert_eq!(u.path, "/pub/big.tar.gz");
        assert_eq!(u.dir(), "/pub");
        assert_eq!(u.filename(), "big.tar.gz");
    }

    #[test]
    fn ftp_url_parses_userinfo_and_port() {
        let u = parse_ftp_url("ftp://user:s%40cret@mirror.example:2121/pub f/file.zip")
            .unwrap();
        assert_eq!(u.host, "mirror.example");
        assert_eq!(u.port, 2121);
        assert_eq!(u.user.as_deref(), Some("user"));
        assert_eq!(u.password.as_deref(), Some("s@cret"));
        assert_eq!(u.dir(), "/pub f");
        assert_eq!(u.filename(), "file.zip");
    }

    #[test]
    fn ftp_url_rejects_non_ftp() {
        assert!(parse_ftp_url("http://example.com/f").is_err());
        assert!(parse_ftp_url("ftp://").is_err());
    }

    #[test]
    fn ftp_url_root_file() {
        let u = parse_ftp_url("ftp://host/file").unwrap();
        assert_eq!(u.dir(), "/");
        assert_eq!(u.filename(), "file");
    }
}
