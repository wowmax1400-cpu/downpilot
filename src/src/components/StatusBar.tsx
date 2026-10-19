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
  const seg = "px-3 text-xs text-txt-dim";
  const num = "ml-1 font-mono tabular-nums text-txt";
  const active: ActiveProfile[] = store.activeProfiles ?? [];
  const firstProfile = active[0];
  return (
    <div className="flex h-7 shrink-0 items-center border-t border-white/5 bg-surface text-xs">
      {/* Left: counts */}
      <div className={seg} title="Total downloads (all states)">
        <span className="text-txt-faint">Total:</span>
        <span className={num}>{c.all ?? 0}</span>
      </div>
      <div className="h-3 w-px bg-white/5" />
      <div className={seg} title="Downloads transferring right now">
        <span className="text-txt-faint">Active:</span>
        <span className={num}>{s.activeCount}</span>
      </div>
      <div className="h-3 w-px bg-white/5" />
      <div className={seg} title="Downloads in paused or stopped state">
        <span className="text-txt-faint">Paused:</span>
        <span className={num}>{c.paused ?? 0}</span>
      </div>
      <div className="h-3 w-px bg-white/5" />
      <div className={seg} title="Downloads waiting for a queue slot">
        <span className="text-txt-faint">Queued:</span>
        <span className={num}>{c.queued ?? 0}</span>
      </div>
      <div className="h-3 w-px bg-white/5" />
      <div className={seg} title="Bytes still to download across active tasks">
        <span className={num}>{formatBytes(s.remaining)}</span>
        <span className="text-txt-faint"> remaining</span>
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
            <span className="inline-block h-1.5 w-1.5 animate-pulse rounded-[1px] bg-amber-300" />
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
      {store.schedulerStatus && (
        <>
          <div className="h-3 w-px bg-white/5" />
          <div
            className={seg}
            title={
              store.schedulerStatus.global_max_simultaneous > 0
                ? `Global concurrency: ${store.schedulerStatus.global_active} active of ${store.schedulerStatus.global_max_simultaneous}; ${store.schedulerStatus.global_queued} queued.`
                : `Global concurrency: ${store.schedulerStatus.global_active} active; ${store.schedulerStatus.global_queued} queued.`
            }
          >
            <span className="text-txt-faint">glb</span>{" "}
            <span className={num}>
              {store.schedulerStatus.global_active}
              {store.schedulerStatus.global_max_simultaneous > 0
                ? `/${store.schedulerStatus.global_max_simultaneous}`
                : ""}
            </span>
            <span className="text-txt-faint"> active</span>{" "}
            <span className={num}>{store.schedulerStatus.global_queued}</span>{" "}
            <span className="text-txt-faint">queued</span>
          </div>
        </>
      )}
      <div className="flex-1" />

      {/* Phase 3A: in-app toast for the most recent engine
          notification. Auto-dismisses after 6s; click × to clear. */}
      {store.lastNotification && (
        <NotificationToast
          key={store.lastNotification.at}
          level={store.lastNotification.level}
          title={store.lastNotification.title}
          body={store.lastNotification.body}
          onDismiss={store.dismissNotification}
        />
      )}

      {/* Right: aggregate speed + limit state (IDM footer layout) */}
      <div className={seg} title={cap > 0 ? `Global cap: ${formatSpeed(cap)}` : "No global bandwidth cap"}>
        <span className="text-txt-faint">Total Download Speed:</span>
        <span className={num + " text-emerald-300"}>{formatSpeed(s.activeSpeed)}</span>
      </div>
      <div className="h-3 w-px bg-white/5" />
      <div className={seg} title="Global bandwidth cap (change it from the toolbar)">
        <span className="text-txt-faint">Speed Limit:</span>
        <span className={"ml-1 font-mono tabular-nums " + (cap > 0 ? "text-amber-300" : "text-txt-faint")}>
          {cap > 0 ? formatSpeed(cap) : "Disabled"}
        </span>
      </div>
      <div className="h-3 w-px bg-white/5" />
      <div className={seg + " text-txt-faint"}>
        DownPilot v{store.version}
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
