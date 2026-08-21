// Size / speed / ETA formatting helpers (IDM-style compact)

export function formatBytes(n: number | null | undefined): string {
  if (!n || n <= 0) return "—";
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
  if (!bps || bps <= 0) return "—";
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

export function formatDate(ts: number): string {
  const d = new Date(ts * 1000);
  return d.toLocaleDateString(undefined, { month: "short", day: "numeric" });
}

export function formatDuration(secs: number | null): string {
  if (!secs || secs <= 0) return "";
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  const s = Math.round(secs % 60);
  const mm = m.toString().padStart(2, "0");
  const ss = s.toString().padStart(2, "0");
  return h > 0 ? `${h}:${mm}:${ss}` : `${m}:${ss}`;
}

/** Guess a media category from a filename (matches Rust `guess_category`). */
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
