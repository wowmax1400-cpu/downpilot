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
  const seg = "px-3 text-xs text-txt-dim";
  const num = "ml-1 font-mono tabular-nums text-txt";
  const active: ActiveProfile[] = store.activeProfiles ?? [];
  const firstProfile = active[0];
  return (
    <div className="flex h-7 shrink-0 items-center border-t border-white/5 bg-surface text-xs">
      <div className={seg}>
        <span className={num}>{c.all ?? 0}</span> downloads
      </div>
      <div className="h-3 w-px bg-white/5" />
      <div className={seg}>
        <span className={num}>{c.paused ?? 0}</span> paused
      </div>
      <div className="h-3 w-px bg-white/5" />
      <div className={seg}>
        <span className={num}>{s.activeCount}</span>{" "}
        <span className="text-indigo-400">active</span>
      </div>
      <div className="h-3 w-px bg-white/5" />
      <div className={seg} title={cap > 0 ? `Capped at ${formatSpeed(cap)}` : "Unlimited"}>
        ↓ <span className={num}>{formatSpeed(s.activeSpeed)}</span>
        {cap > 0 && <span className="ml-1 text-txt-faint">(cap {formatSpeed(cap)})</span>}
      </div>
      <div className="h-3 w-px bg-white/5" />
      <div className={seg}>
        <span className={num}>{formatBytes(s.remaining)}</span> remaining
      </div>
      {firstProfile && (
        <>
          <div className="h-3 w-px bg-white/5" />
          <div
            className={seg + " flex items-center gap-1.5"}
            title={
              "Schedule active: " +
              firstProfile.name +
              " · affecting " +
              firstProfile.affected_queues.join(", ")
            }
          >
            <span className="inline-block h-1.5 w-1.5 animate-pulse rounded-full bg-amber-300" />
            <span className="text-amber-200/90">⏱ {firstProfile.name}</span>
            {firstProfile.affected_queues.length > 0 && (
              <span className="text-txt-faint">·</span>
            )}
            {firstProfile.affected_queues.slice(0, 3).map((q, i) => (
              <span key={i} className="text-txt-faint tabular-nums">
                {q}={formatKbps(store.effectiveSpeed?.[q] ?? 0)}
              </span>
            ))}
            {firstProfile.affected_queues.length > 3 && (
              <span className="text-txt-faint">
                +{firstProfile.affected_queues.length - 3}
              </span>
            )}
            {active.length > 1 && (
              <span className="text-txt-faint">+{active.length - 1}</span>
            )}
          </div>
        </>
      )}
      <div className="flex-1" />
      <div className={seg + " text-txt-faint"}>
        DownPilot v{store.version}
      </div>
    </div>
  );
}
