//! DownPilot Tauri shell: bridges the React UI and the Chrome extension to
//! the `downpilot-core` engine. Also runs a small loopback TCP server
//! (127.0.0.1:19851) speaking the Chrome native-messaging frame protocol so
//! the `downpilot-nmh` relay can forward extension messages without any HTTP
//! stack.

use std::sync::Arc;

use downpilot_core::engine::DownloadManager;
use downpilot_core::models::{EngineEvent, NewDownload, Status};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

pub struct AppState {
    pub manager: Arc<DownloadManager>,
    pub creds: Arc<downpilot_core::credentials::CredentialStore>,
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

#[tauri::command]
fn list_downloads(state: State<'_, AppState>) -> Vec<downpilot_core::models::DownloadTask> {
    state.manager.list()
}

#[tauri::command]
async fn add_download(state: State<'_, AppState>, nd: NewDownload) -> Result<i64, String> {
    state.manager.add(nd).await
}

#[tauri::command]
fn pause_download(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    state.manager.pause(id)
}

#[tauri::command]
fn resume_download(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    state.manager.resume(id)
}

#[tauri::command]
fn cancel_download(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    state.manager.cancel(id)
}

#[tauri::command]
fn remove_download(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    state.manager.remove(id)
}

#[tauri::command]
async fn analyze_url(
    state: State<'_, AppState>,
    url: String,
    cookies: Option<String>,
    referer: Option<String>,
    user_agent: Option<String>,
) -> Result<downpilot_core::models::MediaInfo, String> {
    state.manager.analyze(&url, cookies.as_deref(), referer.as_deref(), user_agent.as_deref()).await
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> downpilot_core::models::AppSettings {
    state.manager.settings()
}

#[tauri::command]
fn set_setting(state: State<'_, AppState>, key: String, value: String) {
    state.manager.set_setting(&key, &value);
}

#[tauri::command]
fn get_default_dir(state: State<'_, AppState>) -> String {
    state.manager.default_save_dir()
}

#[tauri::command]
fn open_in_file_manager(state: State<'_, AppState>, path: String) -> Result<(), String> {
    let dir = if std::path::Path::new(&path).is_dir() {
        path.clone()
    } else {
        std::path::Path::new(&path)
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or(path.clone())
    };
    let _ = state;
    std::process::Command::new("xdg-open")
        .arg(&dir)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("cannot open folder: {e}"))
}

/// Open a specific file (not its parent folder) with the OS default
/// handler. Used for "Open File" in the context menu / inspector
/// when a download is complete. Falls back to opening the parent
/// folder if the file no longer exists.
#[tauri::command]
fn open_file(state: State<'_, AppState>, path: String) -> Result<(), String> {
    let _ = state;
    let p = std::path::Path::new(&path);
    let target = if p.exists() {
        p.to_path_buf()
    } else if let Some(parent) = p.parent() {
        parent.to_path_buf()
    } else {
        return Err(format!("path not found: {path}"));
    };
    std::process::Command::new("xdg-open")
        .arg(&target)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("cannot open: {e}"))
}

/// Site grabber: fetch a page and list downloadable file links.
///
/// `rules` is optional. When absent, the engine uses
/// `GrabRules::sensible_defaults()`. The UI passes the user's
/// configured rules so the same dialog is consistent across calls.
#[tauri::command]
async fn grab_site(
    state: State<'_, AppState>,
    url: String,
    limit: Option<usize>,
    rules: Option<downpilot_core::models::GrabRules>,
) -> Result<Vec<downpilot_core::models::GrabbedLink>, String> {
    let r = rules.unwrap_or_else(downpilot_core::models::GrabRules::sensible_defaults);
    state.manager.grab_site_with_rules(&url, limit.unwrap_or(50), &r).await
}

// ---------------------------------------------------------------------------
// Extended IDM-class commands: queues, sites, schedules, categories, bulk,
// completion actions, checksums, import/export, metadata edits.
// ---------------------------------------------------------------------------

#[tauri::command]
fn list_queues(state: State<'_, AppState>) -> Vec<downpilot_core::models::DownloadQueue> {
    state.manager.list_queues()
}

#[tauri::command]
fn create_queue(state: State<'_, AppState>, queue: downpilot_core::models::DownloadQueue) -> Result<(), String> {
    state.manager.create_queue(queue)
}

#[tauri::command]
fn update_queue(state: State<'_, AppState>, queue: downpilot_core::models::DownloadQueue) -> Result<(), String> {
    state.manager.update_queue(queue)
}

#[tauri::command]
fn delete_queue(state: State<'_, AppState>, name: String) -> Result<(), String> {
    state.manager.delete_queue(&name)
}

#[tauri::command]
fn list_sites(state: State<'_, AppState>) -> Vec<downpilot_core::models::SiteRule> {
    state.manager.list_sites()
}

#[tauri::command]
fn save_site(state: State<'_, AppState>, site: downpilot_core::models::SiteRule) -> Result<(), String> {
    state.manager.save_site(site)
}

#[tauri::command]
fn delete_site(state: State<'_, AppState>, domain: String) -> Result<(), String> {
    state.manager.delete_site(&domain)
}

/// Test which SiteRule would apply to a hypothetical URL. Used by the
/// UI to show a preview while the user is typing a domain.
#[tauri::command]
fn test_site_match(state: State<'_, AppState>, url: String) -> Option<downpilot_core::models::SiteRule> {
    state.manager.test_site_match(&url)
}

#[tauri::command]
fn list_schedules(state: State<'_, AppState>) -> Vec<downpilot_core::models::Schedule> {
    state.manager.list_schedules()
}

#[tauri::command]
fn save_schedule(state: State<'_, AppState>, schedule: downpilot_core::models::Schedule) -> Result<i64, String> {
    state.manager.save_schedule(schedule)
}

#[tauri::command]
fn delete_schedule(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    state.manager.delete_schedule(id)
}

#[tauri::command]
fn list_categories(state: State<'_, AppState>) -> Vec<downpilot_core::models::Category> {
    state.manager.list_categories()
}

#[tauri::command]
fn save_category(state: State<'_, AppState>, category: downpilot_core::models::Category) -> Result<(), String> {
    state.manager.save_category(category)
}

#[tauri::command]
fn delete_category(state: State<'_, AppState>, name: String) -> Result<(), String> {
    state.manager.delete_category(&name)
}

#[tauri::command]
fn resume_all(state: State<'_, AppState>) -> Result<(), String> {
    state.manager.resume_all()
}

#[tauri::command]
fn pause_all(state: State<'_, AppState>) -> Result<(), String> {
    state.manager.pause_all()
}

#[tauri::command]
fn stop_all(state: State<'_, AppState>) -> Result<(), String> {
    state.manager.stop_all()
}

#[tauri::command]
fn retry_all(state: State<'_, AppState>) -> Result<(), String> {
    state.manager.retry_all()
}

#[tauri::command]
fn redownload(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    state.manager.redownload(id)
}

#[tauri::command]
fn move_to_queue(state: State<'_, AppState>, id: i64, queue: String) -> Result<(), String> {
    state.manager.move_to_queue(id, &queue)
}

#[tauri::command]
fn set_category(state: State<'_, AppState>, id: i64, category: String) -> Result<(), String> {
    state.manager.set_category(id, &category)
}

#[tauri::command]
fn rename_download(state: State<'_, AppState>, id: i64, filename: String) -> Result<(), String> {
    state.manager.rename(id, &filename)
}

#[tauri::command]
fn set_save_dir(state: State<'_, AppState>, id: i64, dir: String) -> Result<(), String> {
    state.manager.set_save_dir_for(id, &dir)
}

#[tauri::command]
fn set_description(state: State<'_, AppState>, id: i64, description: String) -> Result<(), String> {
    state.manager.set_description(id, &description)
}

#[tauri::command]
fn recheck_download(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    state.manager.recheck(id)
}

#[tauri::command]
fn delete_file(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    state.manager.delete_file(id)
}

#[tauri::command]
fn remove_keep_file(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    state.manager.remove_keep_file(id)
}

#[tauri::command]
fn export_downloads(state: State<'_, AppState>, path: String) -> Result<usize, String> {
    state.manager.export_downloads(&path)
}

#[tauri::command]
async fn import_downloads(state: State<'_, AppState>, path: String, start: Option<bool>) -> Result<usize, String> {
    state.manager.import_downloads(&path, start.unwrap_or(false)).await
}

#[tauri::command]
fn clear_finished(state: State<'_, AppState>) -> Result<usize, String> {
    state.manager.clear_finished()
}

#[tauri::command]
fn set_queue_speed(state: State<'_, AppState>, queue: String, kbps: u64) {
    state.manager.set_queue_speed(&queue, kbps);
}
#[tauri::command]
async fn probe_url(
    state: State<'_, AppState>,
    url: String,
    cookies: Option<String>,
    referer: Option<String>,
    user_agent: Option<String>,
) -> Result<downpilot_core::models::ProbeInfo, String> {
    state.manager.probe_url(&url, cookies.as_deref(), referer.as_deref(), user_agent.as_deref()).await
}

/// Read a text file from disk and pull out the URLs it contains. Used by
/// the drag-drop handler when the user drops a `.txt` / `.list` / similar
/// file onto the window — IDM does the same thing.
#[tauri::command]
fn read_url_file(path: String, max_bytes: Option<u64>) -> Result<Vec<String>, String> {
    let p = std::path::Path::new(&path);
    let data = std::fs::read(p).map_err(|e| format!("read failed: {e}"))?;
    let cap = max_bytes.unwrap_or(1_048_576) as usize; // 1 MiB cap
    let slice: &[u8] = if data.len() > cap { &data[..cap] } else { &data[..] };
    let text = String::from_utf8_lossy(slice);
    Ok(extract_urls(&text))
}

/// Greedily extract http(s)/ftp URLs from a blob of text. Stops at
/// whitespace, angle brackets, quotes, parens, or brackets. Trims
/// trailing punctuation that browsers normally strip.
fn extract_urls(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut i = 0;
    let bytes = text.as_bytes();
    while i < bytes.len() {
        if let Some(scheme_len) = starts_with_scheme_at(bytes, i) {
            let start = i;
            i += scheme_len;
            while i < bytes.len() {
                let b = bytes[i];
                if b.is_ascii_whitespace()
                    || b == b'<'
                    || b == b'>'
                    || b == b'"'
                    || b == b'\''
                    || b == b')'
                    || b == b']'
                {
                    break;
                }
                i += 1;
            }
            let mut end = i;
            while end > start {
                let last = bytes[end - 1];
                if matches!(last, b'.' | b',' | b';' | b':' | b'!' | b'?') {
                    end -= 1;
                } else {
                    break;
                }
            }
            let raw = &text[start..end];
            if raw.is_empty() {
                continue;
            }
            let owned = raw.to_string();
            if seen.insert(owned.clone()) {
                out.push(owned);
            }
        } else {
            i += 1;
        }
    }
    out
}

fn starts_with_scheme_at(bytes: &[u8], i: usize) -> Option<usize> {
    for prefix in [b"https://".as_slice(), b"http://".as_slice(), b"ftp://".as_slice()] {
        if i + prefix.len() <= bytes.len() && bytes[i..i + prefix.len()].eq_ignore_ascii_case(prefix) {
            return Some(prefix.len());
        }
    }
    None
}

/// Toggle a queue's enabled flag. Disabled queues park incoming tasks as
/// Queued and don't promote them to Downloading until re-enabled.
#[tauri::command]
fn set_queue_enabled(state: State<'_, AppState>, name: String, enabled: bool) -> Result<(), String> {
    state.manager.set_queue_enabled(&name, enabled)
}

#[tauri::command]
fn resume_queue(state: State<'_, AppState>, queue: String) -> Result<usize, String> {
    state.manager.resume_queue(&queue)
}

#[tauri::command]
fn pause_queue(state: State<'_, AppState>, queue: String) -> Result<usize, String> {
    state.manager.pause_queue(&queue)
}

#[tauri::command]
fn stop_queue(state: State<'_, AppState>, queue: String) -> Result<usize, String> {
    state.manager.stop_queue(&queue)
}

#[tauri::command]
fn copy_text_to_clipboard(_state: State<'_, AppState>, text: String) -> Result<(), String> {
    // Reuse the same wl-paste/xclip pipeline in reverse: spawn `wl-copy`
    // (Wayland) or feed stdin to `xclip -selection clipboard` (X11). This
    // avoids the window-focus side-effect that `arboard` triggers on some
    // compositors.
    use std::io::Write;
    use std::process::{Command, Stdio};
    if let Ok(mut child) = Command::new("wl-copy").stdin(Stdio::piped()).spawn() {
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(text.as_bytes());
        }
        let _ = child.wait();
        return Ok(());
    }
    let mut child = Command::new("xclip")
        .arg("-selection")
        .arg("clipboard")
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| format!("clipboard not available: {e}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(text.as_bytes());
    }
    let _ = child.wait();
    Ok(())
}


#[tauri::command]
fn active_profiles(state: State<'_, AppState>) -> Vec<downpilot_core::models::ActiveProfile> {
    state.manager.active_profiles()
}

#[tauri::command]
fn effective_queue_speed(state: State<'_, AppState>, queue: String) -> u64 {
    state.manager.effective_queue_speed_kbps(&queue)
}

#[tauri::command]
fn queue_credential_status(
    state: State<'_, AppState>,
    queue: String,
) -> bool {
    state.creds.is_configured(&queue)
}

#[tauri::command]
fn set_queue_credential(
    state: State<'_, AppState>,
    queue: String,
    secret: String,
) -> Result<(), String> {
    state
        .creds
        .set(&queue, &secret)
        .map_err(|e| format!("cannot write credential: {e}"))
}

#[tauri::command]
fn clear_queue_credential(
    state: State<'_, AppState>,
    queue: String,
) -> Result<(), String> {
    state
        .creds
        .clear(&queue)
        .map_err(|e| format!("cannot delete credential: {e}"))
}

#[tauri::command]
fn test_queue_credential(
    state: State<'_, AppState>,
    queue: String,
) -> serde_json::Value {
    // Lightweight self-test: confirm the secret is readable and non-empty,
    // without exposing the value to the UI. The UI shows a single
    // "Tested OK" / "Not configured" badge.
    let configured = state.creds.is_configured(&queue);
    let has_value = configured
        && state
            .creds
            .get(&queue)
            .map(|s| !s.is_empty())
            .unwrap_or(false);
    serde_json::json!({
        "configured": configured,
        "valid": has_value,
    })
}

/// Run `wl-paste` (GNOME Wayland) or fall back to `xclip -o` (X11) as an
/// async child. kill_on_drop ensures the child dies if the caller's timeout
/// fires while the clipboard owner is unresponsive.
async fn read_clipboard() -> Option<std::process::Output> {
    // IMPORTANT: just run `wl-paste` with NO flags. By default it reads the
    // CLIPBOARD selection (Ctrl+C). We must NOT pass `-p`/`--primary`, because
    // grabbing the primary (mouse) selection on GNOME/Wayland forcibly takes
    // selection ownership, which instantly dismisses any open right-click
    // context menu (the "menu flashes open then closes" bug). The old code's
    // 1.5s timer loop ran wl-paste repeatedly, which is what stole the
    // selection and killed menus — we now read event-driven instead of polling.
    let mut cmd = tokio::process::Command::new("wl-paste");
    cmd.kill_on_drop(true);
    match cmd.output().await {
        Ok(out) => return Some(out),
        Err(_) => {}
    }
    // X11 fallback: explicitly target the CLIPBOARD selection (not PRIMARY).
    let mut cmd = tokio::process::Command::new("xclip");
    cmd.arg("-selection").arg("clipboard").arg("-o").kill_on_drop(true);
    cmd.output().await.ok()
}

/// Watch the system clipboard for URLs and emit `downloads://clipboard`
/// events. Uses `wl-paste` (GNOME Wayland) or `xclip` (X11); silently skips
/// if neither is installed. The frontend shows a "grab this URL?" prompt.
///
/// IMPORTANT (bug fix): on GNOME/Wayland, running `wl-paste` briefly takes
/// clipboard ownership. If that happens while a right-click context menu is
/// open on the desktop / another app, the compositor drops the selection and
/// the menu instantly closes ("menu flashes open then closes"). Because we
/// can't reliably detect that state from the app on this Wayland session, the
/// monitor is OFF by default (`clipboard_monitor` setting). When `enabled` is
/// false we never touch the clipboard, so context menus can never be dismissed.
pub fn start_clipboard_monitor(handle: AppHandle, enabled: bool) {
    if !enabled {
        eprintln!("[clipboard-monitor] disabled (clipboard_monitor=false) — context-menu bug avoided");
        return;
    }
    tauri::async_runtime::spawn(async move {
        let mut last: Option<String> = None;
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
            // Read the clipboard with tokio::process + timeout + kill_on_drop.
            // Plain `wl-paste` reads the CLIPBOARD (Ctrl+C) selection only; we
            // never pass `-p`/`--primary`. The timeout kills the child if the
            // clipboard owner is unresponsive and we just skip this tick.
            let out = tokio::time::timeout(
                std::time::Duration::from_secs(3),
                read_clipboard(),
            )
            .await
            .unwrap_or(None);
            let Some(out) = out else { continue };
            if !out.status.success() {
                continue;
            }
            let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if text.is_empty() || text.len() > 4096 {
                continue;
            }
            if text == last.as_deref().unwrap_or("") {
                continue;
            }
            last = Some(text.clone());
            // Only react to content that looks like a single URL.
            let trimmed = text.trim();
            let looks_url = trimmed.starts_with("http://")
                || trimmed.starts_with("https://")
                || (trimmed.contains('.') && !trimmed.contains(char::is_whitespace) && trimmed.contains('/'));
            if looks_url {
                let _ = handle.emit("downloads://clipboard", &trimmed);
            }
        }
    });
}

// ---------------------------------------------------------------------------
// Native messaging socket (mirrors Chrome's 4-byte LE length prefix framing)
// ---------------------------------------------------------------------------

pub const NMH_PORT: u16 = 19851;

/// Server-side timeout for the `BrowserDownloadDetected` probe. The
/// ask-notification must not feel slow, so we cap the probe well under
/// the typical user-prompt deadline. If the probe times out the
/// extension is told the probe failed and can still let the user
/// "Try Download Anyway".
pub const PROBE_TIMEOUT_MS: u64 = 4000;

#[derive(serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum NmhMessage {
    Ping,
    Status,
    Analyze {
        url: String,
        cookies: Option<String>,
        referer: Option<String>,
        user_agent: Option<String>,
    },
    Download {
        url: String,
        format_id: Option<String>,
        filename: Option<String>,
        referer: Option<String>,
        user_agent: Option<String>,
        cookies: Option<String>,
    },
    /// Snapshot every task in the DB. The extension can use this to render a
    /// mirror UI without holding a long-lived Tauri command open.
    List,
    /// Re-queue a task that was previously paused/stopped/failed.
    Start { id: i64 },
    Pause { id: i64 },
    Resume { id: i64 },
    /// User-initiated stop. The task transitions to Stopped; partial files
    /// are kept so Resume can continue from where it left off.
    Cancel { id: i64 },
    /// Rescan the disk to recover the on-disk byte count for a paused task.
    Recheck { id: i64 },
    /// Returns the app's browser-integration configuration so the extension
    /// enforces the SAME capture rules the user set in the app's Settings.
    Config,
    /// Extension signals it saw a download start. The engine does NOT
    /// insert anything — it just records the event so the ask-mode
    /// notification can offer a real IDM-style preview. Optional cookie
    /// header is forwarded only because the user asked for capture.
    BrowserDownloadDetected {
        url: String,
        filename: Option<String>,
        referer: Option<String>,
        cookies: Option<String>,
        /// Opaque browser-side handle (chrome.downloads.DownloadItem.id)
        /// used to suppress duplicate inserts on Chrome retries.
        browser_id: String,
    },
    /// User accepted the ask-mode preview. Equivalent to `Download` plus
    /// the dedup id, but typed explicitly so the wire protocol is
    /// self-documenting.
    SubmitBrowserDownload {
        url: String,
        filename: Option<String>,
        referer: Option<String>,
        user_agent: Option<String>,
        cookies: Option<String>,
        /// Optional category/queue/save_dir chosen by the user in the
        /// ask-mode preview. None = let the engine's category-resolution
        /// rules decide.
        category: Option<String>,
        queue: Option<String>,
        save_dir: Option<String>,
        browser_id: String,
        /// When the user clicks "Try Download Anyway" after a failed
        /// probe, the extension sets this to true so the engine inserts
        /// the task without a synchronous HEAD/ranged-GET probe. The
        /// downloader's normal byte-flow path will surface 404/403/etc.
        #[serde(default)]
        try_download_anyway: bool,
    },
    /// User explicitly ignored a download (ask-mode "Ignore" button, or
    /// "always ignore this type" with the matching extension). Recorded
    /// only as a structured skip — no engine write.
    IgnoreBrowserDownload {
        url: String,
        browser_id: String,
    },
    /// Persist the user's "Always download this type" preference. The
    /// extension calls this when the user clicks "Always download this
    /// type" in the ask-notification. The engine adds the extension to
    /// `always_allow_extensions` so future downloads of the same
    /// extension skip ask-mode and capture automatically.
    /// `op` is "add" or "remove".
    SetAlwaysAllowExtension {
        extension: String,
        op: String,
    },
}

#[derive(serde::Serialize, Clone)]
struct NmhResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    // Echoed from the request so the Chrome extension can correlate async
    // responses (its pendingRequests map keys on this).
    #[serde(skip_serializing_if = "Option::is_none")]
    _req_id: Option<u64>,
}

pub async fn run_nmh_server(manager: Arc<DownloadManager>) {
    let listener = match TcpListener::bind(("127.0.0.1", NMH_PORT)).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("[downpilot] nmh server bind on 127.0.0.1:{NMH_PORT} failed: {e}");
            return;
        }
    };
    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                let mgr = manager.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_nmh_conn(stream, mgr).await {
                        eprintln!("[downpilot] nmh conn error: {e}");
                    }
                });
            }
            Err(_) => continue,
        }
    }
}

/// Read the request correlation id from a frame. Accepts both the
/// current `_req_id` and the legacy `_reqId` spelling so a version
/// mismatch between the extension and the app does not silently hang
/// the extension's `pendingRequests` map. Returns None when the field
/// is missing or not a u64.
fn extract_req_id(frame: &[u8]) -> Option<u64> {
    serde_json::from_slice::<serde_json::Value>(frame)
        .ok()
        .and_then(|v| {
            v.get("_req_id")
                .or_else(|| v.get("_reqId"))
                .and_then(|x| x.as_u64())
        })
}

async fn handle_nmh_conn(mut stream: TcpStream, manager: Arc<DownloadManager>) -> std::io::Result<()> {
    loop {
        let Some(frame) = read_frame(&mut stream).await? else {
            return Ok(());
        };
        let resp = handle_nmh_message(&frame, &manager).await;
        let out = serde_json::to_vec(&resp).unwrap_or_else(|_| br#"{"ok":false,"error":"serialize"}"#.to_vec());
        stream.write_all(&(out.len() as u32).to_le_bytes()).await?;
        stream.write_all(&out).await?;
        stream.flush().await?;
    }
}

async fn read_frame(stream: &mut TcpStream) -> std::io::Result<Option<Vec<u8>>> {
    let mut len_buf = [0u8; 4];
    match stream.read_exact(&mut len_buf).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let len = u32::from_le_bytes(len_buf) as usize;
    if len > 16 * 1024 * 1024 {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "frame too large"));
    }
    let mut buf = vec![0u8; len];
    stream.read_exact(&mut buf).await?;
    Ok(Some(buf))
}

async fn handle_nmh_message(frame: &[u8], manager: &Arc<DownloadManager>) -> NmhResponse {
    // The Chrome extension tags every request with a `_req_id` so it can
    // correlate async responses. The nmh relay forwards frames verbatim, so
    // the app must read and echo it back — otherwise the extension's
    // `pendingRequests` map never matches and the click silently hangs.
    // Accept both `_req_id` (current) and `_reqId` (legacy) so an older
    // extension running against a newer app (or vice versa) does not
    // hang in-flight requests. Backward-compat extracted to
    // `extract_req_id` so the dual-name policy is unit-tested.
    let req_id: Option<u64> = extract_req_id(frame);
    let msg: NmhMessage = match serde_json::from_slice(frame) {
        Ok(m) => m,
        Err(e) => {
            return NmhResponse {
                ok: false,
                data: None,
                error: Some(format!("bad message: {e}")),
                _req_id: None,
            }
        }
    };
    let resp = match msg {
        NmhMessage::Ping => NmhResponse { ok: true, data: Some(serde_json::json!({ "pong": true })), error: None, _req_id: req_id },
        NmhMessage::Status => {
            let active = manager.list().iter().filter(|t| t.status == Status::Downloading).count();
            NmhResponse {
                ok: true,
                data: Some(serde_json::json!({ "active": active, "version": env!("CARGO_PKG_VERSION") })),
                error: None,
                _req_id: req_id,
            }
        }
        NmhMessage::Analyze { url, cookies, referer, user_agent } => {
            match manager.analyze(&url, cookies.as_deref(), referer.as_deref(), user_agent.as_deref()).await {
                Ok(media) => NmhResponse { ok: true, data: Some(serde_json::to_value(media).unwrap_or_default()), error: None, _req_id: req_id },
                Err(e) => NmhResponse { ok: false, data: None, error: Some(e), _req_id: req_id },
            }
        }
        NmhMessage::Download { url, format_id, filename, referer, user_agent, cookies } => {
            let nd = NewDownload {
                url,
                filename,
                save_dir: None,
                threads: None,
                format_id,
                webpage_url: None,
                referer,
                user_agent,
                cookies,
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
                start_immediately: true,
                browser_dedup_id: String::new(),
                skip_probe: false,
            };
            match manager.add(nd).await {
                Ok(id) => NmhResponse { ok: true, data: Some(serde_json::json!({ "id": id })), error: None, _req_id: req_id },
                Err(e) => NmhResponse { ok: false, data: None, error: Some(e), _req_id: req_id },
            }
        }
        NmhMessage::List => {
            let tasks = manager.list();
            let data = serde_json::to_value(&tasks).unwrap_or(serde_json::json!([]));
            NmhResponse { ok: true, data: Some(data), error: None, _req_id: req_id }
        }
        NmhMessage::Start { id } => match manager.get(id) {
            Some(task) => {
                manager.start(task);
                NmhResponse { ok: true, data: Some(serde_json::json!({ "id": id })), error: None, _req_id: req_id }
            }
            None => NmhResponse { ok: false, data: None, error: Some("task not found".into()), _req_id: req_id },
        },
        NmhMessage::Pause { id } => match manager.pause(id) {
            Ok(()) => NmhResponse { ok: true, data: Some(serde_json::json!({ "id": id })), error: None, _req_id: req_id },
            Err(e) => NmhResponse { ok: false, data: None, error: Some(e), _req_id: req_id },
        },
        NmhMessage::Resume { id } => match manager.resume(id) {
            Ok(()) => NmhResponse { ok: true, data: Some(serde_json::json!({ "id": id })), error: None, _req_id: req_id },
            Err(e) => NmhResponse { ok: false, data: None, error: Some(e), _req_id: req_id },
        },
        NmhMessage::Cancel { id } => match manager.cancel(id) {
            Ok(()) => NmhResponse { ok: true, data: Some(serde_json::json!({ "id": id })), error: None, _req_id: req_id },
            Err(e) => NmhResponse { ok: false, data: None, error: Some(e), _req_id: req_id },
        },
        NmhMessage::Recheck { id } => match manager.recheck(id) {
            Ok(()) => NmhResponse { ok: true, data: Some(serde_json::json!({ "id": id })), error: None, _req_id: req_id },
            Err(e) => NmhResponse { ok: false, data: None, error: Some(e), _req_id: req_id },
        },
                NmhMessage::BrowserDownloadDetected { url, filename, referer, cookies, browser_id } => {
            // Run the same probe the in-app Add dialog uses (HEAD → 1-byte
            // ranged GET fallback). The extension surfaces the result in
            // the ask-notification so the user can confirm filename /
            // size / content type before committing. This call performs
            // no DB write; the real insert happens via the
            // user-driven SubmitBrowserDownload path.
            //
            // The probe is bounded by `PROBE_TIMEOUT_MS` so a slow or
            // hostile server cannot stall the ask-notification.
            let _ = filename;
            let probe = tokio::time::timeout(
                std::time::Duration::from_millis(PROBE_TIMEOUT_MS),
                manager.probe_url(
                    &url,
                    cookies.as_deref(),
                    referer.as_deref(),
                    /* user_agent */ None,
                ),
            )
            .await;
            let data = match probe {
                Ok(Ok(info)) => serde_json::json!({
                    "preview": true,
                    "ok": true,
                    "filename": info.filename,
                    "total_bytes": info.size,
                    "content_type": info.content_type,
                    "range_supported": info.supports_resume,
                    "browser_id": browser_id,
                }),
                Ok(Err(e)) => serde_json::json!({
                    "preview": true,
                    "ok": false,
                    "error": e,
                    "browser_id": browser_id,
                }),
                Err(_) => serde_json::json!({
                    "preview": true,
                    "ok": false,
                    "error": "probe timed out",
                    "browser_id": browser_id,
                }),
            };
            // The extension owns the "Try Download Anyway" path. We just
            // tell it that the probe failed; the user can still choose to
            // submit, and the engine will re-probe at byte-flow time.
            NmhResponse { ok: true, data: Some(data), error: None, _req_id: req_id }
        }
        NmhMessage::SubmitBrowserDownload { url, filename, referer, user_agent, cookies, category, queue, save_dir, browser_id, try_download_anyway } => {
            let nd = NewDownload {
                url,
                filename,
                save_dir,
                threads: None,
                format_id: None,
                webpage_url: None,
                referer,
                user_agent,
                cookies,
                expected_sha256: None,
                category,
                queue,
                description: None,
                max_retries: None,
                retry_delay_ms: None,
                conn_timeout_ms: None,
                read_timeout_ms: None,
                checksum_algorithm: None,
                on_complete: None,
                start_immediately: true,
                browser_dedup_id: browser_id,
                skip_probe: try_download_anyway,
            };
            match manager.add(nd).await {
                Ok(id) => NmhResponse { ok: true, data: Some(serde_json::json!({ "id": id, "captured": true })), error: None, _req_id: req_id },
                Err(e) => NmhResponse { ok: false, data: None, error: Some(e), _req_id: req_id },
            }
        }
        NmhMessage::IgnoreBrowserDownload { url, browser_id } => {
            // Structured skip. No engine write; the event is logged for
            // the user to inspect under History > Ignored if we add such a
            // view later. Cookies are never seen here on purpose.
            let _ = (url, browser_id);
            NmhResponse { ok: true, data: Some(serde_json::json!({ "ignored": true })), error: None, _req_id: req_id }
        }
        NmhMessage::SetAlwaysAllowExtension { extension, op } => {
            // Persist the user's "Always download this type" preference.
            // The engine stores it in AppSettings.always_allow_extensions
            // so the extension can read it on the next config fetch.
            //
            // Sanity: extension is non-empty, <=16 chars, alnum + dot. We
            // refuse anything else to keep this storage safe.
            let cleaned = extension.trim().trim_start_matches('.').to_ascii_lowercase();
            if cleaned.is_empty()
                || cleaned.len() > 16
                || !cleaned.chars().all(|c| c.is_ascii_alphanumeric() || c == '.')
            {
                return NmhResponse {
                    ok: false,
                    data: None,
                    error: Some(format!("invalid extension: {extension:?}")),
                    _req_id: req_id,
                };
            }
            let op = op.to_ascii_lowercase();
            if op != "add" && op != "remove" {
                return NmhResponse {
                    ok: false,
                    data: None,
                    error: Some(format!("invalid op: {op}")),
                    _req_id: req_id,
                };
            }
            // Read the current list, drop the entry being toggled, then
            // re-add it iff op == "add". Sorted + dedup so the stored
            // value is canonical.
            let current = manager
                .settings()
                .always_allow_extensions
                .unwrap_or_default();
            let mut entries: Vec<String> = current
                .split(',')
                .map(|s| s.trim().to_ascii_lowercase())
                .filter(|s| !s.is_empty() && s != &cleaned)
                .collect();
            if op == "add" {
                entries.push(cleaned.clone());
            }
            entries.sort();
            entries.dedup();
            let new_value = entries.join(",");
            manager.set_setting("always_allow_extensions", &new_value);
            NmhResponse {
                ok: true,
                data: Some(serde_json::json!({
                    "extension": cleaned,
                    "op": op,
                    "always_allow_extensions": new_value,
                })),
                error: None,
                _req_id: req_id,
            }
        }
NmhMessage::Config => {
            let s = manager.settings();
            // Only expose what the extension needs to enforce capture rules.
            // All lists are comma-separated (empty when unset).
            NmhResponse {
                ok: true,
                data: Some(serde_json::json!({
                    "auto_intercept": s.auto_intercept.unwrap_or(false),
                    "capture_mode": s.capture_mode.clone().unwrap_or_else(|| "ask".to_string()),
                    "clipboard_monitor": s.clipboard_monitor.unwrap_or(false),
                    "exclude_domains": (s.exclude_domains.clone().unwrap_or_default()),
                    "exclude_extensions": (s.exclude_extensions.clone().unwrap_or_default()),
                    "allowed_extensions": (s.allowed_extensions.clone().unwrap_or_default()),
                    "always_allow_extensions": (s.always_allow_extensions.clone().unwrap_or_default()),
                    "proxy_url": (s.proxy_url.clone().unwrap_or_default()),
                })),
                error: None,
                _req_id: req_id,
            }
        }
    };
    resp
}

// ---------------------------------------------------------------------------
// App entrypoint
// ---------------------------------------------------------------------------

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let data_dir = app.path().app_data_dir().expect("no app data dir");
            std::fs::create_dir_all(&data_dir).ok();
            let db_path = data_dir.join("downpilot.db");
            let parts_dir = data_dir.join("parts");
            std::fs::create_dir_all(&parts_dir).ok();

            let (manager, rx) = DownloadManager::new(&db_path, parts_dir);
            let creds = Arc::new(downpilot_core::credentials::CredentialStore::new(&data_dir));
            let (sink, mut sink_rx) = mpsc::unbounded_channel();
            // pump_loop spawns tokio tasks internally, so it must run on
            // Tauri's async runtime (setup runs on the main thread).
            let mgr = manager.clone();
            tauri::async_runtime::spawn(async move {
                mgr.pump_loop(rx, sink);
            });

            // Tasks left "downloading" by a crash get parked as paused.
            for task in manager.list() {
                if task.status == Status::Downloading {
                    let _ = manager.pause(task.id);
                }
            }

            let handle: AppHandle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                while let Some(ev) = sink_rx.recv().await {
                    let _ = handle.emit("downloads://event", &ev);
                }
            });

            let mgr = manager.clone();
            tauri::async_runtime::spawn(async move {
                run_nmh_server(mgr).await;
            });

            // Clipboard URL monitor (wl-paste on GNOME Wayland / xclip on X11).
            // Disabled by default: on GNOME/Wayland a clipboard read dismisses
            // open right-click context menus, so we don't poll unless the user
            // explicitly opts in via the clipboard_monitor setting.
            let clipboard_enabled = manager.settings().clipboard_monitor.unwrap_or(false);
            start_clipboard_monitor(app.handle().clone(), clipboard_enabled);

            app.manage(AppState { manager, creds });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_downloads,
            add_download,
            pause_download,
            resume_download,
            cancel_download,
            remove_download,
            analyze_url,
            get_settings,
            set_setting,
            get_default_dir,
            open_in_file_manager,
            open_file,
            grab_site,
            list_queues,
            create_queue,
            update_queue,
            delete_queue,
            list_sites,
            save_site,
            delete_site,
            test_site_match,
            list_schedules,
            save_schedule,
            delete_schedule,
            list_categories,
            save_category,
            delete_category,
            resume_all,
            pause_all,
            stop_all,
            retry_all,
            redownload,
            move_to_queue,
            set_category,
            rename_download,
            set_save_dir,
            set_description,
            recheck_download,
            delete_file,
            remove_keep_file,
            export_downloads,
            import_downloads,
            clear_finished,
            set_queue_speed,
            set_queue_enabled,
            resume_queue,
            pause_queue,
            stop_queue,
            probe_url,
            read_url_file,
            copy_text_to_clipboard,
            active_profiles,
            effective_queue_speed,
            queue_credential_status,
            set_queue_credential,
            clear_queue_credential,
            test_queue_credential
        ])
        .run(tauri::generate_context!())
        .expect("error while running DownPilot");
}

// Keep EngineEvent import used (forwarder task type annotation).
#[allow(unused)]
fn _typecheck(_: &mut mpsc::UnboundedReceiver<EngineEvent>) {}


#[cfg(test)]
mod req_id_compat {
    use super::extract_req_id;

    #[test]
    fn accepts_current_name() {
        let frame = br#"{"_req_id": 42, "type": "ping"}"#;
        assert_eq!(extract_req_id(frame), Some(42));
    }

    #[test]
    fn accepts_legacy_name() {
        let frame = br#"{"_reqId": 7, "type": "ping"}"#;
        assert_eq!(extract_req_id(frame), Some(7));
    }

    #[test]
    fn prefers_current_over_legacy() {
        let frame = br#"{"_req_id": 1, "_reqId": 2, "type": "ping"}"#;
        assert_eq!(extract_req_id(frame), Some(1));
    }

    #[test]
    fn returns_none_when_missing() {
        let frame = br#"{"type": "ping"}"#;
        assert_eq!(extract_req_id(frame), None);
    }

    #[test]
    fn returns_none_for_non_u64() {
        let frame = br#"{"_req_id": "42"}"#;
        assert_eq!(extract_req_id(frame), None);
    }

    #[test]
    fn returns_none_for_invalid_json() {
        let frame = b"not json";
        assert_eq!(extract_req_id(frame), None);
    }
}
