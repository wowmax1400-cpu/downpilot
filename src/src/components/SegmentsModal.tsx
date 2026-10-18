// IDM-style transfer progress dialog.
//
// Shows the overall bar plus one bar per engine segment (multi-threaded
// range download). Data comes from the ephemeral `segment_progress`
// engine events collected by `useDownloads`; nothing is persisted —
// when the engine isn't chunked-splitting (single-thread download or
// completed task) there are no segments and the dialog still works,
// showing only the overall bar.
//
// Toolbar buttons: Pause / Resume / Stop and the global Limiter, like
// IDM's task window.

import { Dialog, Btn } from "./Dialog";
import type { DownloadsStore, SegmentSnapshot } from "../store/useDownloads";
import { formatBytes, formatSpeed } from "../lib/format";
import { computeSpeedBps } from "../lib/metrics";

export function IDMSegmentsDialog({
  store,
  id,
  onClose,
}: {
  store: DownloadsStore;
  id: number;
  onClose: () => void;
}) {
  // Re-render on segment deltas: reading the version counter keeps the
  // subscription hot even though the underlying map is a ref.
  void store.segmentVersion;
  const task = store.tasks.find((t) => t.id === id);

  if (!task) {
    return (
      <Dialog title="Download progress" onClose={onClose} width={640}>
        <div className="p-4 text-sm text-txt-dim">Download no longer exists.</div>
      </Dialog>
    );
  }

  const segments = store.getSegments(id);
  const total = task.total_size || 0;
  const done = task.downloaded || 0;
  const pct = total > 0 ? Math.min(100, Math.round((done / total) * 100)) : 0;
  const speed =
    task.status === "downloading" ? computeSpeedBps(store.getSpeedHistory(id)) : 0;
  const limiterActive = (store.settings?.speed_limit_kbps ?? 0) > 0;
  const finished = segments.filter((s) => s.size > 0 && s.bytes >= s.size).length;
  const threads = task.threads || segments.length || 1;

  return (
    <Dialog title="Download progress" onClose={onClose} width={680}>
      {/* Header block: File Name / Total Size / Source URL — IDM order */}
      <div className="space-y-1 border-b border-[var(--line-soft)] px-4 py-3 text-sm">
        <div className="flex gap-2">
          <span className="min-w-[92px] shrink-0 text-txt-faint">File Name:</span>
          <span className="truncate font-medium text-txt" title={task.filename || task.url}>
            {task.filename || task.url}
          </span>
        </div>
        <div className="flex gap-2">
          <span className="min-w-[92px] shrink-0 text-txt-faint">Total Size:</span>
          <span className="font-mono tabular-nums text-txt-dim">
            {total > 0 ? formatBytes(total) : "unknown"}
          </span>
        </div>
        <div className="flex gap-2">
          <span className="min-w-[92px] shrink-0 text-txt-faint">Source URL:</span>
          <span className="min-w-0 flex-1 truncate text-txt-dim" title={task.url}>
            {task.url}
          </span>
        </div>
      </div>

      {/* Overall progress bar */}
      <div className="space-y-2 px-4 pt-3">
        <div className="flex items-baseline justify-between text-xs">
          <span className="text-txt-dim">
            {formatBytes(done)} <span className="text-txt-faint">of</span>{" "}
            {total > 0 ? formatBytes(total) : "?"}
          </span>
          <span className="font-mono tabular-nums text-txt">
            {pct}%
            {speed > 0 && (
              <span className="ml-2 text-emerald-300">{formatSpeed(speed)}</span>
            )}
          </span>
        </div>
        <div className="h-2 w-full overflow-hidden rounded-[1px] bg-[var(--progress-track)]">
          <div
            className={"h-full rounded-[1px] transition-[width] duration-300 " + overallFill(task.status)}
            style={{ width: `${pct}%` }}
          />
        </div>
      </div>

      {/* Per-segment bars */}
      {segments.length > 0 ? (
        <div className="px-4 pt-3">
          <div className="pb-1 text-[10.5px] font-semibold uppercase tracking-wider text-txt-faint">
            Segments ({finished} of {segments.length} complete)
          </div>
          <div className="max-h-56 space-y-1.5 overflow-y-auto">
            {segments.map((s) => (
              <SegmentRow key={s.idx} s={s} />
            ))}
          </div>
        </div>
      ) : (
        <div className="px-4 pt-3 text-xs text-txt-faint">
          No segment data — this transfer is running as a single connection
          ({threads} thread{threads === 1 ? "" : "s"}).
        </div>
      )}

      {/* Actions: Pause / Resume / Stop + global Limiter */}
      <div className="flex items-center gap-2 border-t border-[var(--line-soft)] px-4 py-3">
        <Btn
          size="sm"
          disabled={task.status !== "downloading"}
          onClick={() => void store.pause(id)}
        >
          Pause
        </Btn>
        <Btn
          size="sm"
          disabled={task.status !== "paused"}
          onClick={() => void store.resume(id)}
        >
          Resume
        </Btn>
        <Btn
          size="sm"
          variant="danger"
          disabled={task.status === "completed" || task.status === "cancelled"}
          onClick={() => {
            void store.cancel(id);
            onClose();
          }}
        >
          Stop
        </Btn>
        <div className="flex-1" />
        <span
          title={
            limiterActive
              ? `Global cap is ${store.settings?.speed_limit_kbps} kB/s — click to disable`
              : "No global cap — click to set 512 kB/s"
          }
        >
          <Btn
            size="sm"
            variant={limiterActive ? "default" : "ghost"}
            onClick={() => void store.setBandwidth(limiterActive ? 0 : 512)}
          >
            Limiter: {limiterActive ? `${store.settings?.speed_limit_kbps} kB/s` : "Off"}
          </Btn>
        </span>
      </div>
    </Dialog>
  );
}

function SegmentRow({ s }: { s: SegmentSnapshot }) {
  const pct = s.size > 0 ? Math.max(0, Math.min(100, (s.bytes / s.size) * 100)) : 0;
  const complete = s.size > 0 && s.bytes >= s.size;
  return (
    <div className="flex items-center gap-2 text-xs">
      <span className="w-14 shrink-0 font-mono tabular-nums text-txt-faint">
        #{s.idx + 1}
      </span>
      <div className="h-2 min-w-0 flex-1 overflow-hidden rounded-[1px] bg-[var(--progress-track)]">
        <div
          className={
            "h-full rounded-[1px] transition-[width] duration-300 " +
            (complete ? "bg-emerald-500/80" : "bg-indigo-400/80")
          }
          style={{ width: `${pct}%` }}
        />
      </div>
      <span className="w-40 shrink-0 text-right font-mono text-[11px] tabular-nums text-txt-dim">
        {formatBytes(s.bytes)} / {s.size > 0 ? formatBytes(s.size) : "?"}
        {complete && <span className="text-emerald-400"> ✓</span>}
      </span>
    </div>
  );
}

// Fill color by status, matching the table's ProgressBar.
function overallFill(status: string): string {
  switch (status) {
    case "completed":
      return "bg-emerald-500/80";
    case "failed":
      return "bg-red-500/80";
    case "paused":
      return "bg-amber-500/70";
    default:
      return "bg-indigo-400/80";
  }
}
