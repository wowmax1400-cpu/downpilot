// Size / speed / ETA / progress formatting helpers (IDM-style compact).

export function formatBytes(n: number | null | undefined): string {
  if (n == null || !isFinite(n) || n <= 0) return "—";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let i = 0;
  let v = n;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v >= 100 ? v.toFixed(0) : v >= 10 ? v.toFixed(1) : v.toFixed(2)} ${units[i]}`;
}

export function formatSpeed(bps: number | null | undefined): string {
  if (bps == null || !isFinite(bps) || bps <= 0) return "—";
  return `${formatBytes(bps)}/s`;
}

export function formatEta(remainingBytes: number, bps: number): string {
  if (!bps || bps <= 0 || remainingBytes <= 0) return "—";
  const secs = remainingBytes / bps;
  if (secs < 60) return `${Math.max(1, Math.round(secs))}s`;
  if (secs < 3600) return `${Math.floor(secs / 60)}m ${Math.round(secs % 60)}s`;
  return `${Math.floor(secs / 3600)}h ${Math.floor((secs % 3600) / 60)}m`;
}

export function formatPercent(done: number, total: number): string {
  if (!total || total <= 0) return "—";
  return `${Math.min(100, Math.round((done / total) * 100))}%`;
}

/** "03:41" / "01h 12m" / "—" */
export function formatTimeLeft(remaining: number, speed: number): string {
  return formatEta(remaining, speed);
}

/** mm:ss / hh:mm:ss elapsed. */
export function formatElapsed(secs: number | null | undefined): string {
  if (!secs || secs <= 0 || !isFinite(secs)) return "—";
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  const s = Math.round(secs % 60);
  const mm = m.toString().padStart(2, "0");
  const ss = s.toString().padStart(2, "0");
  return h > 0 ? `${h}:${mm}:${ss}` : `${m}:${ss}`;
}

/** Extract a display domain from a URL: "https://github.com/a/b/releases" -> "github.com" */
export function domainOf(url: string | null | undefined): string {
  if (!url) return "";
  try {
    return new URL(url).hostname.replace(/^www\./, "");
  } catch {
    return url.split("/")[0] || "";
  }
}

export function formatDate(ts: number): string {
  if (!ts || !isFinite(ts) || ts <= 0) return "—";
  const d = new Date(ts * 1000);
  return d.toLocaleDateString(undefined, { year: "numeric", month: "short", day: "2-digit" });
}

export function formatDateTime(ts: number): string {
  if (!ts || !isFinite(ts) || ts <= 0) return "—";
  const d = new Date(ts * 1000);
  return d.toLocaleString(undefined, {
    year: "numeric", month: "short", day: "2-digit",
    hour: "2-digit", minute: "2-digit",
  });
}

export function formatDuration(secs: number | null | undefined): string {
  return formatElapsed(secs);
}

/** Status -> human label (editable in one place). */
export function statusLabel(status: string): string {
  switch (status) {
    case "queued": return "Queued";
    case "scheduled": return "Scheduled";
    case "downloading": return "Downloading";
    case "paused": return "Paused";
    case "stopped": return "Stopped";
    case "completed": return "Complete";
    case "failed": return "Error";
    case "cancelled": return "Cancelled";
    default: return status;
  }
}

export function guessCategory(filename: string): string {
  const f = filename.toLowerCase();
  const exts = (list: string[]) => list.some((e) => f.endsWith(e));
  if (exts([".zip", ".rar", ".7z", ".tar", ".gz", ".bz2", ".xz", ".iso", ".zst", ".tar.gz"])) return "compressed";
  if (exts([".pdf", ".doc", ".docx", ".xls", ".xlsx", ".ppt", ".pptx", ".txt", ".md", ".odt", ".epub", ".csv"])) return "documents";
  if (exts([".mp3", ".flac", ".wav", ".aac", ".ogg", ".opus", ".m4a", ".wma"])) return "music";
  if (exts([".mp4", ".mkv", ".webm", ".avi", ".mov", ".wmv", ".flv", ".m4v", ".ts", ".mpg"])) return "video";
  if (exts([".exe", ".msi", ".deb", ".rpm", ".appimage", ".apk", ".dmg", ".run", ".sh", ".flatpak", ".flatpakref"])) return "programs";
  return "other";
}

/** Filename from a URL (last path segment, query stripped). */
export function filenameFromUrl(url: string): string {
  try {
    const u = new URL(url);
    const last = u.pathname.split("/").filter(Boolean).pop();
    return last || `${u.hostname}.bin`;
  } catch {
    return url.split("/").pop() || "download.bin";
  }
}

/** A short, single-line display name (for very long filenames). */
export function shortName(name: string, max = 64): string {
  if (!name) return "—";
  if (name.length <= max) return name;
  const head = name.slice(0, Math.max(8, Math.floor(max * 0.6)));
  const tail = name.slice(-Math.floor(max * 0.25));
  return `${head}…${tail}`;
}
