// Bridge between the UI and the DownPilot engine.
//
// In the Tauri app this calls the Rust backend through `invoke`.
// When opened in a plain browser (`vite dev` without Tauri) it falls back
// to a small local mock so the UI + theme engine are previewable.

import { invoke } from "@tauri-apps/api/core";
import type {
  AppSettings,
  ActiveProfile,
  DownloadTask,
  DownloadQueue,
  SiteRule,
  Schedule,
  Category,
  GrabbedLink,
  GrabRules,
  MediaInfo,
  NewDownload,
  EngineEvent,
} from "../types";

export const isTauri = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

// ---------------------------------------------------------------------------
// Small helper: call the backend, or fall back to a mock callback.
// ---------------------------------------------------------------------------
async function call<T>(
  cmd: string,
  args: Record<string, unknown>,
  mock: () => T | Promise<T>
): Promise<T> {
  if (!isTauri()) return mock();
  return invoke<T>(cmd, args);
}

// ---------------------------------------------------------------------------
// Downloads
// ---------------------------------------------------------------------------

export async function listDownloads(): Promise<DownloadTask[]> {
  return call("list_downloads", {}, () => []);
}

// The real backend returns the old `DownloadTask` shape (without the IDM
// extension fields). We fill defaults so the UI is tolerant.
export async function addDownload(nd: NewDownload): Promise<number> {
  return call("add_download", { nd }, () => Math.floor(Math.random() * 1e6));
}

export async function pauseDownload(id: number): Promise<void> {
  return call("pause_download", { id }, () => undefined);
}
export async function resumeDownload(id: number): Promise<void> {
  return call("resume_download", { id }, () => undefined);
}
export async function cancelDownload(id: number): Promise<void> {
  return call("cancel_download", { id }, () => undefined);
}
export async function removeDownload(id: number): Promise<void> {
  return call("remove_download", { id }, () => undefined);
}
export async function redownload(id: number): Promise<void> {
  return call("redownload", { id }, () => undefined);
}

export async function resumeAll(): Promise<void> {
  return call("resume_all", {}, () => undefined);
}
export async function pauseAll(): Promise<void> {
  return call("pause_all", {}, () => undefined);
}
export async function stopAll(): Promise<void> {
  return call("stop_all", {}, () => undefined);
}
export async function retryAll(): Promise<void> {
  return call("retry_all", {}, () => undefined);
}
export async function recheckDownload(id: number): Promise<void> {
  return call("recheck_download", { id }, () => undefined);
}
export async function deleteFile(id: number): Promise<void> {
  return call("delete_file", { id }, () => undefined);
}
export async function removeKeepFile(id: number): Promise<void> {
  return call("remove_keep_file", { id }, () => undefined);
}
export async function moveToQueue(id: number, queue: string): Promise<void> {
  return call("move_to_queue", { id, queue }, () => undefined);
}
export async function setCategory(id: number, category: string): Promise<void> {
  return call("set_category", { id, category }, () => undefined);
}
export async function renameDownload(id: number, filename: string): Promise<void> {
  return call("rename_download", { id, filename }, () => undefined);
}
export async function setSaveDir(id: number, dir: string): Promise<void> {
  return call("set_save_dir", { id, dir }, () => undefined);
}
export async function setDescription(id: number, description: string): Promise<void> {
  return call("set_description", { id, description }, () => undefined);
}
export async function clearFinished(): Promise<number> {
  return call<number>("clear_finished", {}, () => 0);
}

/**
 * Open a file with the OS default handler. The backend resolves the
 * file via `xdg-open` (or equivalent) and returns when the launcher
 * has accepted the request. Used for "Open File" in the inspector +
 * context menu.
 */
export async function openFile(path: string): Promise<void> {
  return call<void>("open_file", { path }, () => undefined);
}
export async function exportDownloads(path: string): Promise<number> {
  return call("export_downloads", { path }, () => 0);
}
export async function importDownloads(path: string, start = false): Promise<number> {
  return call("import_downloads", { path, start }, () => 0);
}

// ---------------------------------------------------------------------------
// Queues
// ---------------------------------------------------------------------------

export async function listQueues(): Promise<DownloadQueue[]> {
  return call("list_queues", {}, () => []);
}
export async function createQueue(queue: DownloadQueue): Promise<void> {
  return call("create_queue", { queue }, () => undefined);
}
export async function updateQueue(queue: DownloadQueue): Promise<void> {
  return call("update_queue", { queue }, () => undefined);
}
export async function deleteQueue(name: string): Promise<void> {
  return call("delete_queue", { name }, () => undefined);
}
export async function setQueueSpeed(queue: string, kbps: number): Promise<void> {
  return call("set_queue_speed", { queue, kbps }, () => undefined);
}
export async function setQueueEnabled(name: string, enabled: boolean): Promise<void> {
  return call("set_queue_enabled", { name, enabled }, () => undefined);
}
export async function resumeQueue(queue: string): Promise<number> {
  return call("resume_queue", { queue }, () => 0);
}
export async function pauseQueue(queue: string): Promise<number> {
  return call("pause_queue", { queue }, () => 0);
}
export async function stopQueue(queue: string): Promise<number> {
  return call("stop_queue", { queue }, () => 0);
}

export interface ProbeInfo {
  url: string;
  filename: string;
  size: number;
  content_type: string | null;
  supports_resume: boolean;
}

export async function probeUrl(
  url: string,
  cookies?: string | null,
  referer?: string | null,
  userAgent?: string | null
): Promise<ProbeInfo> {
  return call(
    "probe_url",
    { url, cookies, referer, userAgent },
    () => ({ url, filename: url.split("/").pop() ?? "download", size: 0, content_type: null, supports_resume: false })
  );
}

export async function readUrlFile(path: string, maxBytes?: number): Promise<string[]> {
  return call("read_url_file", { path, maxBytes }, () => []);
}

export async function copyTextToClipboard(text: string): Promise<void> {
  return call("copy_text_to_clipboard", { text }, () => undefined);
}

// ---------------------------------------------------------------------------
// Sites
// ---------------------------------------------------------------------------

export async function listSites(): Promise<SiteRule[]> {
  return call("list_sites", {}, () => []);
}
export async function saveSite(site: SiteRule): Promise<void> {
  return call("save_site", { site }, () => undefined);
}
export async function deleteSite(domain: string): Promise<void> {
  return call("delete_site", { domain }, () => undefined);
}

// ---------------------------------------------------------------------------
// Schedules
// ---------------------------------------------------------------------------

export async function listSchedules(): Promise<Schedule[]> {
  return call("list_schedules", {}, () => []);
}
export async function saveSchedule(schedule: Schedule): Promise<number> {
  return call("save_schedule", { schedule }, () => -1);
}
export async function deleteSchedule(id: number): Promise<void> {
  return call("delete_schedule", { id }, () => undefined);
}

// ---------------------------------------------------------------------------
// Categories
// ---------------------------------------------------------------------------

export async function listCategories(): Promise<Category[]> {
  return call("list_categories", {}, () => []);
}
export async function saveCategory(category: Category): Promise<void> {
  return call("save_category", { category }, () => undefined);
}
export async function deleteCategory(name: string): Promise<void> {
  return call("delete_category", { name }, () => undefined);
}

// ---------------------------------------------------------------------------
// Settings / analysis / misc
// ---------------------------------------------------------------------------

export async function getSettings(): Promise<AppSettings> {
  return call("get_settings", {}, () => ({
    download_dir: "/home/amir/Downloads",
    default_threads: 8,
    max_threads: 16,
    max_simultaneous: 3,
    speed_limit_kbps: 0,
    theme: "deep-midnight",
    accent: null,
    auto_intercept: false,
    use_ytdlp_for_media: true,
    clipboard_monitor: false,
    proxy_url: null,
    schedule_enabled: false,
    schedule_start: "23:00",
    schedule_end: "06:00",
    on_complete: null,
    max_retries: 5 as any,
    retry_delay_ms: 2000,
    conn_timeout_ms: 30000,
    read_timeout_ms: 30000,
    // Browser-capture defaults (the Chrome extension reads these via the
    // NMH `config` message and enforces the rules locally).
    exclude_extensions: null,
    exclude_domains: null,
    allowed_extensions: null,
    always_allow_extensions: null,
    capture_mode: "ask",
  }));
}

export async function setSetting(key: string, value: string): Promise<void> {
  if (!isTauri()) return;
  await invoke("set_setting", { key, value });
}

export async function getDefaultDir(): Promise<string> {
  return call("get_default_dir", {}, () => "/home/amir/Downloads");
}

export async function openInFileManager(path: string): Promise<void> {
  if (!isTauri()) return;
  await invoke("open_in_file_manager", { path });
}

export async function analyzeUrl(
  url: string,
  cookies?: string | null,
  referer?: string | null,
  userAgent?: string | null
): Promise<MediaInfo> {
  return call("analyze_url", { url, cookies, referer, userAgent }, () => ({
    title: "Sample Media",
    webpage_url: url,
    thumbnail: null,
    duration: 642,
    uploader: null,
    formats: [],
    best_formats: [],
    default_filename: "sample.mp4",
  }));
}

export async function grabSite(url: string, limit?: number): Promise<GrabbedLink[]> {
  return call("grab_site", { url, limit }, () => []);
}
export async function grabSiteWithRules(
  url: string,
  limit: number,
  rules: GrabRules,
): Promise<GrabbedLink[]> {
  return call("grab_site", { url, limit, rules }, () => []);
}
export async function testSiteMatch(url: string): Promise<SiteRule | null> {
  return call("test_site_match", { url }, () => null);
}

// ---------------------------------------------------------------------------
// Event subscriptions
// ---------------------------------------------------------------------------

export function subscribeToClipboard(cb: (url: string) => void): () => void {
  if (!isTauri()) return () => {};
  import("@tauri-apps/api/event").then(({ listen }) => {
    void listen("downloads://clipboard", (e) => cb(e.payload as string));
  });
  return () => {};
}

export function subscribeToEvents(cb: (ev: EngineEvent) => void): () => void {
  if (!isTauri()) return () => {};
  import("@tauri-apps/api/event").then(({ listen }) => {
    void listen("downloads://event", (e) => cb(e.payload as EngineEvent));
  });
  return () => {};
}


// ---------------------------------------------------------------------------
// Bandwidth profiles / schedules / effective speed
// ---------------------------------------------------------------------------

export async function activeProfiles(): Promise<ActiveProfile[]> {
  return call<ActiveProfile[]>("active_profiles", {}, () => []);
}

export async function effectiveQueueSpeedKbps(queue: string): Promise<number> {
  return call<number>("effective_queue_speed", { queue }, () => 0);
}

// ---------------------------------------------------------------------------
// Per-queue secure credential storage
// ---------------------------------------------------------------------------

/** True if a credential is currently configured for `queue`. The UI
 *  shows `[Configured ✓]` vs `[Not configured]` based on this flag.
 *  The value itself is never returned over the bridge. */
export async function queueCredentialStatus(queue: string): Promise<boolean> {
  return call<boolean>("queue_credential_status", { queue }, () => false);
}

/** Replace the credential for `queue`. The string is sent over the
 *  Tauri IPC channel and persisted to a 0600-permissioned file under
 *  the app's data directory. The UI never re-reads the value. */
export async function setQueueCredential(
  queue: string,
  secret: string
): Promise<void> {
  if (!isTauri()) return;
  await invoke("set_queue_credential", { queue, secret });
}

export async function clearQueueCredential(queue: string): Promise<void> {
  return call("clear_queue_credential", { queue }, () => undefined);
}

/** Lightweight liveness check. Returns { configured, valid } — never
 *  the credential itself. */
export async function testQueueCredential(
  queue: string
): Promise<{ configured: boolean; valid: boolean }> {
  return call(
    "test_queue_credential",
    { queue },
    () => ({ configured: false, valid: false })
  );
}

// Queue credential storage (passwords never land in SQLite).
export async function saveQueuePassword(queue: string, password: string): Promise<void> {
  return call("save_queue_password", { queue, password }, () => undefined);
}
export async function clearQueuePassword(queue: string): Promise<void> {
  return call("clear_queue_password", { queue }, () => undefined);
}
export async function testQueueAuth(queue: string): Promise<boolean> {
  return call<boolean>("test_queue_auth", { queue }, () => false);
}
