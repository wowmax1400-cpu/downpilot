// Modern dark status bar. Shows:
//   - Total / paused / active counts
//   - Combined download speed (sum of every active task)
//   - Bandwidth cap if set ("↓ 12.3 MB/s (cap 10 MB/s)")
//   - Total remaining bytes across active downloads
//   - Active bandwidth-profile indicator (when a schedule is currently
//     overriding one or more queues). The chip shows the profile name
//     + a list of queues whose effective speed differs from their cap.
//   - Version
//
// All numbers are computed from the live store.

import { useEffect } from "react";
import type { DownloadsStore } from "../store/useDownloads";
import type { ActiveProfile } from "../types";
import { formatBytes, formatSpeed } from "../lib/format";

function formatKbps(kbps: number): string {
  if (kbps === 0) return "∞";
  if (kbps >= 1024) return Math.round(kbps / 1024) + " MB/s";
  return kbps + " kB/s";
}

export function StatusBar({ store }: { store: DownloadsStore }) {
  const c = store.counts;
  const s = store.stats;
  const cap = (store.settings?.speed_limit_kbps ?? 0) > 0
    ? (store.settings!.speed_limit_kbps as number) * 1024
    : 0;
  const pane = "px-2 text-[11px] text-[var(--text-dim)] h-[22px] flex items-center select-none bg-[var(--surface)] border border-[var(--line)] rounded-[3px]";
  const num = "ml-1 font-mono tabular-nums text-[var(--text)] font-medium";

  return (
    <div className="flex h-[28px] shrink-0 items-center border-t border-[var(--line)] bg-[var(--bg-toolbar)] px-2 text-[11px] gap-2 select-none relative">
      {/* Glowing neon progress line across top of status bar */}
      <div className="absolute top-0 left-0 right-0 h-[2px] bg-gradient-to-r from-cyan-400 via-blue-500 to-purple-500 opacity-80" />

      {/* Pane 1: Total / Active / Paused */}
      <div className={`${pane} min-w-[210px]`} title="Download state counters">
        <span>Total:</span>
        <span className={num}>{c.all ?? 0}</span>
        <span className="mx-1.5 text-gray-500">|</span>
        <span>Active:</span>
        <span className={num}>{s.activeCount}</span>
        <span className="mx-1.5 text-gray-500">|</span>
        <span>Paused:</span>
        <span className={num}>{c.paused ?? 0}</span>
      </div>

      {/* Pane 2: Remaining bytes */}
      <div className={`${pane} min-w-[150px]`} title="Total remaining bytes">
        <span className="font-mono">{formatBytes(s.remaining)}</span>
        <span className="text-gray-400 ml-1">remaining</span>
      </div>

      {/* Spacer to push right panes */}
      <div className="flex-1" />

      {/* Pane 3: Speed & Limit Cap */}
      <div className={`${pane} min-w-[170px]`} title="Download Speed and Bandwidth Limit">
        <span>Speed:</span>
        <span className={num}>{formatSpeed(s.activeSpeed)}</span>
        <span className="mx-1.5 text-gray-500">|</span>
        <span>Cap:</span>
        <span className={num}>{cap > 0 ? formatSpeed(cap) : "Off"}</span>
      </div>

      {/* Pane 4: App version */}
      <div className={`${pane} min-w-[110px] justify-center`}>
        <span className="font-mono text-cyan-400 font-semibold">DownPilot v2.0.0</span>
      </div>
    </div>
  );
}

function NotificationToast({
  level,
  title,
  body,
  onDismiss,
}: {
  level: string;
  title: string;
  body: string;
  onDismiss: () => void;
}) {
  // Auto-dismiss after 6 s.
  useEffect(() => {
    const t = setTimeout(onDismiss, 6000);
    return () => clearTimeout(t);
  }, [onDismiss]);
  const colors =
    level === "error"
      ? "border-red-500/30 bg-red-500/10 text-red-200"
      : level === "warning"
      ? "border-amber-500/30 bg-amber-500/10 text-amber-200"
      : "border-emerald-500/30 bg-emerald-500/10 text-emerald-200";
  return (
    <div
      role="status"
      className={
        "mx-2 flex max-w-md items-center gap-2 rounded-[2px] border px-2 py-1 text-[11px] " +
        colors
      }
    >
      <div className="min-w-0 flex-1">
        <div className="truncate font-semibold">{title || "(no title)"}</div>
        {body && <div className="truncate text-txt-dim">{body}</div>}
      </div>
      <button
        type="button"
        className="shrink-0 text-txt-dim hover:text-txt"
        onClick={onDismiss}
        aria-label="Dismiss notification"
      >
        ×
      </button>
    </div>
  );
}
