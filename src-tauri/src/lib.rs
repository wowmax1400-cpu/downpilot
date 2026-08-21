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
    state.manager.add(nd)
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

/// Site grabber: fetch a page and list downloadable file links.
#[tauri::command]
async fn grab_site(
    state: State<'_, AppState>,
    url: String,
    limit: Option<usize>,
) -> Result<Vec<downpilot_core::models::GrabbedLink>, String> {
    state.manager.grab_site(&url, limit.unwrap_or(50)).await
}

/// Run `wl-paste` (GNOME Wayland) or fall back to `xclip -o` (X11) as an
/// async child. kill_on_drop ensures the child dies if the caller's timeout
/// fires while the clipboard owner is unresponsive.
async fn read_clipboard() -> Option<std::process::Output> {
    let mut cmd = tokio::process::Command::new("wl-paste");
    cmd.kill_on_drop(true);
    match cmd.output().await {
        Ok(out) => return Some(out),
        Err(_) => {}
    }
    let mut cmd = tokio::process::Command::new("xclip");
    cmd.arg("-o").kill_on_drop(true);
    cmd.output().await.ok()
}

/// Watch the system clipboard for URLs and emit `downloads://clipboard`
/// events. Uses `wl-paste` (GNOME Wayland) or `xclip` (X11); silently skips
/// if neither is installed. The frontend shows a "grab this URL?" prompt.
pub fn start_clipboard_monitor(handle: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut last: Option<String> = None;
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
            // Read the clipboard with tokio::process + timeout + kill_on_drop.
            // On Wayland, wl-paste can block indefinitely when the clipboard
            // owner doesn't answer the selection offer — a blocking
            // std::process::Command::output() here would freeze a runtime
            // worker (and with it NMH pings / UI commands). The timeout
            // kills the child and we just skip this tick.
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
}

#[derive(serde::Serialize, Clone)]
struct NmhResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
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
    let msg: NmhMessage = match serde_json::from_slice(frame) {
        Ok(m) => m,
        Err(e) => {
            return NmhResponse {
                ok: false,
                data: None,
                error: Some(format!("bad message: {e}")),
            }
        }
    };
    match msg {
        NmhMessage::Ping => NmhResponse { ok: true, data: Some(serde_json::json!({ "pong": true })), error: None },
        NmhMessage::Status => {
            let active = manager.list().iter().filter(|t| t.status == Status::Downloading).count();
            NmhResponse {
                ok: true,
                data: Some(serde_json::json!({ "active": active, "version": env!("CARGO_PKG_VERSION") })),
                error: None,
            }
        }
        NmhMessage::Analyze { url, cookies, referer, user_agent } => {
            match manager.analyze(&url, cookies.as_deref(), referer.as_deref(), user_agent.as_deref()).await {
                Ok(media) => NmhResponse { ok: true, data: Some(serde_json::to_value(media).unwrap_or_default()), error: None },
                Err(e) => NmhResponse { ok: false, data: None, error: Some(e) },
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
                start_immediately: true,
            };
            match manager.add(nd) {
                Ok(id) => NmhResponse { ok: true, data: Some(serde_json::json!({ "id": id })), error: None },
                Err(e) => NmhResponse { ok: false, data: None, error: Some(e) },
            }
        }
    }
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
            start_clipboard_monitor(app.handle().clone());

            app.manage(AppState { manager });
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
            grab_site
        ])
        .run(tauri::generate_context!())
        .expect("error while running DownPilot");
}

// Keep EngineEvent import used (forwarder task type annotation).
#[allow(unused)]
fn _typecheck(_: &mut mpsc::UnboundedReceiver<EngineEvent>) {}
