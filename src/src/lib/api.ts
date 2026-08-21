// Bridge between the UI and the DownPilot engine.
//
// In the Tauri app this calls the Rust backend through `invoke`.
// When opened in a plain browser (`vite dev` without Tauri) it falls back
// to a small local mock so the UI + theme engine are previewable.

import { invoke } from "@tauri-apps/api/core";
import type {
  AppSettings,
  DownloadTask,
  GrabbedLink,
  MediaInfo,
  NewDownload,
} from "../types";

export const isTauri = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

// ---------------------------------------------------------------------------
// Mock backend (browser preview only)
// ---------------------------------------------------------------------------

const MOCK_KEY = "downpilot.mock.db";
let mockSeq = 1;
let mockTimer: number | null = null;

function loadMock(): DownloadTask[] {
  try {
    const raw = localStorage.getItem(MOCK_KEY);
    return raw ? (JSON.parse(raw) as DownloadTask[]) : [];
  } catch {
    return [];
  }
}

function saveMock(tasks: DownloadTask[]) {
  localStorage.setItem(MOCK_KEY, JSON.stringify(tasks));
}

function seedMock(): DownloadTask[] {
  const now = Math.floor(Date.now() / 1000);
  const mk = (p: Partial<DownloadTask>): DownloadTask => ({
    id: mockSeq++,
    url: "https://example.com/file.bin",
    filename: "placeholder.bin",
    save_dir: "/home/amir/Downloads",
    status: "queued",
    total_size: 0,
    downloaded: 0,
    speed_bps: 0,
    threads: 8,
    format_id: null,
    webpage_url: null,
    referer: null,
    user_agent: null,
    cookies: null,
    category: "other",
    expected_sha256: null,
    error: null,
    created_at: now,
    updated_at: now,
    ...p,
  });
  return [
    mk({ filename: "ubuntu-26.04-desktop-amd64.iso", category: "programs", status: "downloading", total_size: 5.4 * 1024 ** 3, downloaded: 2.1 * 1024 ** 3, speed_bps: 9.2 * 1024 ** 2 }),
    mk({ filename: "project-files.zip", category: "compressed", status: "paused", total_size: 850 * 1024 ** 2, downloaded: 340 * 1024 ** 2 }),
    mk({ filename: "presentation.pdf", category: "documents", status: "completed", total_size: 12.4 * 1024 ** 2, downloaded: 12.4 * 1024 ** 2 }),
    mk({ filename: "video-lecture-04.mp4", category: "video", status: "completed", total_size: 1.2 * 1024 ** 3, downloaded: 1.2 * 1024 ** 3 }),
    mk({ filename: "soundtrack.flac", category: "music", status: "queued", total_size: 320 * 1024 ** 2 }),
  ];
}

function startMockProgress() {
  if (mockTimer != null) return; // already running
  mockTimer = window.setInterval(() => {
    // Tab in the background: don't wake the page up. This is what caused
    // the freeze when switching to another browser/app.
    if (document.hidden) return;
    const cur = loadMock();
    let dirty = false;
    for (const t of cur) {
      if (t.status === "downloading" && t.total_size > 0) {
        t.downloaded = Math.min(t.total_size, t.downloaded + 512 * 1024);
        t.speed_bps = 512 * 1024;
        if (t.downloaded >= t.total_size) t.status = "completed";
        dirty = true;
      }
    }
    if (dirty) saveMock(cur);
    // Nothing left to simulate -> stop the timer entirely.
    if (!cur.some((t) => t.status === "downloading")) {
      window.clearInterval(mockTimer!);
      mockTimer = null;
    }
  }, 1000);
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

export async function listDownloads(): Promise<DownloadTask[]> {
  if (!isTauri()) {
    let tasks = loadMock();
    if (tasks.length === 0) {
      tasks = seedMock();
      saveMock(tasks);
      startMockProgress();
    }
    return tasks;
  }
  return invoke<DownloadTask[]>("list_downloads");
}

export async function addDownload(nd: NewDownload): Promise<number> {
  if (!isTauri()) {
    const tasks = loadMock();
    const now = Math.floor(Date.now() / 1000);
    const name = nd.filename || nd.url.split("/").pop() || "download.bin";
    const task: DownloadTask = {
      id: mockSeq++,
      url: nd.url,
      filename: name,
      save_dir: nd.save_dir || "/home/amir/Downloads",
      status: "queued",
      total_size: 0,
      downloaded: 0,
      speed_bps: 0,
      threads: nd.threads || 8,
      format_id: nd.format_id || null,
      webpage_url: nd.webpage_url || null,
      referer: nd.referer || null,
      user_agent: nd.user_agent || null,
      cookies: nd.cookies || null,
      category: nd.filename ? guessCat(nd.filename) : "other",
      expected_sha256: nd.expected_sha256 || null,
      error: null,
      created_at: now,
      updated_at: now,
    };
    tasks.push(task);
    saveMock(tasks);
    return task.id;
  }
  return invoke<number>("add_download", { nd });
}

import { guessCategory } from "./format";
const guessCat = guessCategory;

export async function pauseDownload(id: number): Promise<void> {
  if (!isTauri()) {
    const tasks = loadMock().map((t) =>
      t.id === id && t.status === "downloading" ? { ...t, status: "paused" as const } : t
    );
    saveMock(tasks);
    return;
  }
  await invoke("pause_download", { id });
}

export async function resumeDownload(id: number): Promise<void> {
  if (!isTauri()) {
    const tasks = loadMock().map((t) =>
      t.id === id && t.status === "paused" ? { ...t, status: "downloading" as const } : t
    );
    saveMock(tasks);
    return;
  }
  await invoke("resume_download", { id });
}

export async function cancelDownload(id: number): Promise<void> {
  if (!isTauri()) {
    const tasks = loadMock().map((t) =>
      t.id === id && (t.status === "downloading" || t.status === "queued" || t.status === "paused")
        ? { ...t, status: "cancelled" as const }
        : t
    );
    saveMock(tasks);
    return;
  }
  await invoke("cancel_download", { id });
}

export async function removeDownload(id: number): Promise<void> {
  if (!isTauri()) {
    saveMock(loadMock().filter((t) => t.id !== id));
    return;
  }
  await invoke("remove_download", { id });
}

export async function analyzeUrl(
  url: string,
  cookies?: string | null,
  referer?: string | null,
  userAgent?: string | null
): Promise<MediaInfo> {
  if (!isTauri()) {
    return {
      title: "Sample Video Title",
      webpage_url: url,
      thumbnail: null,
      duration: 642,
      uploader: "Some Channel",
      formats: [],
      best_formats: [
        { format_id: "bv*+ba/b", ext: "mp4", resolution: "best", fps: null, vcodec: "avc1", acodec: "mp4a", filesize: 90 * 1024 ** 2, note: "Best quality", url: null, protocol: "https" },
        { format_id: "137+140", ext: "mp4", resolution: "1920x1080", fps: 30, vcodec: "avc1", acodec: "mp4a", filesize: 74 * 1024 ** 2, note: "1080p", url: null, protocol: "https" },
        { format_id: "136+140", ext: "mp4", resolution: "1280x720", fps: 30, vcodec: "avc1", acodec: "mp4a", filesize: 40 * 1024 ** 2, note: "720p", url: null, protocol: "https" },
        { format_id: "140", ext: "m4a", resolution: "audio only", fps: null, vcodec: "none", acodec: "mp4a", filesize: 4 * 1024 ** 2, note: "Audio only", url: null, protocol: "https" },
      ],
      default_filename: "Sample Video Title.mp4",
    };
  }
  return invoke<MediaInfo>("analyze_url", { url, cookies, referer, userAgent });
}

export async function getSettings(): Promise<AppSettings> {
  if (!isTauri()) {
    return {
      save_dir: "/home/amir/Downloads",
      threads: 8,
      speed_limit_kbps: 0,
      theme: localStorage.getItem("dp.theme") || "deep-midnight",
      accent: localStorage.getItem("dp.accent") || "",
      auto_intercept: false,
      proxy_url: null,
      schedule_enabled: false,
      schedule_start: "23:00",
      schedule_end: "06:00",
    };
  }
  return invoke<AppSettings>("get_settings");
}

export async function setSetting(key: string, value: string): Promise<void> {
  if (!isTauri()) {
    if (key === "theme") localStorage.setItem("dp.theme", value);
    if (key === "accent") localStorage.setItem("dp.accent", value);
    return;
  }
  await invoke("set_setting", { key, value });
}

export async function openInFileManager(path: string): Promise<void> {
  if (!isTauri()) return;
  await invoke("open_in_file_manager", { path });
}

export async function grabSite(
  url: string,
  limit?: number
): Promise<GrabbedLink[]> {
  if (!isTauri()) {
    // Mock: return a few sample links
    await new Promise((r) => setTimeout(r, 600));
    return [
      { url: url + "/video.mp4", filename: "video.mp4", ext: "mp4" },
      { url: url + "/archive.zip", filename: "archive.zip", ext: "zip" },
      { url: url + "/song.mp3", filename: "song.mp3", ext: "mp3" },
      { url: url + "/doc.pdf", filename: "doc.pdf", ext: "pdf" },
      { url: url + "/movie.mkv", filename: "movie.mkv", ext: "mkv" },
    ];
  }
  return invoke<GrabbedLink[]>("grab_site", { url, limit });
}

export function subscribeToClipboard(
  cb: (url: string) => void
): () => void {
  if (!isTauri()) return () => {};
  import("@tauri-apps/api/event").then(({ listen }) => {
    void listen("downloads://clipboard", (e) => cb(e.payload as string));
  });
  return () => {}; // unlisten not implemented for simplicity
}

export function subscribeToEvents(cb: (ev: unknown) => void): () => void {
  if (!isTauri()) return () => {};
  // Lazy import keeps the browser preview free of tauri event machinery
  import("@tauri-apps/api/event").then(({ listen }) => {
    void listen("downloads://event", (e) => cb(e.payload));
  });
  return () => {};
}
