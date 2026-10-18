import { useRef, useState } from "react";
import type { DownloadsStore } from "../store/useDownloads";
import type { DownloadTask } from "../types";
import { formatBytes, formatSpeed, formatEta, formatPercent, formatDate, domainOf } from "../lib/format";
import { deriveTaskMetrics } from "../lib/metrics";
import { IconStart, IconPause, IconStop, IconDelete, IconProperties, IconFolder, IconRedownload } from "./icons";

type Props = {
  store: DownloadsStore;
  onOpenProps: (id: number) => void;
  onClose: () => void;
};

export function DetailsPanel({ store, onOpenProps, onClose }: Props) {
  const sel = [...store.selected];
  const task: DownloadTask | null =
    sel.length === 1 ? store.tasks.find((t) => t.id === sel[0]) ?? null : null;

  // Drag-to-resize: track pointer movement on the left edge. Width is
  // persisted so the panel reopens at the user's preferred size.
  const [width, setWidth] = useState(() => {
    try {
      const w = parseInt(localStorage.getItem("downpilot.details.width") ?? "", 10);
      return Number.isFinite(w) && w >= 240 && w <= 520 ? w : 300;
    } catch {
      return 300;
    }
  });
  const dragRef = useRef<{ startX: number; startW: number } | null>(null);
  const [dragging, setDragging] = useState(false);
  const onResizeStart = (e: React.PointerEvent) => {
    e.preventDefault();
    dragRef.current = { startX: e.clientX, startW: width };
    setDragging(true);
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  };
  const onResizeMove = (e: React.PointerEvent) => {
    if (!dragRef.current) return;
    // Moving left widens the panel (panel sits on the right edge).
    const next = Math.round(dragRef.current.startW + (dragRef.current.startX - e.clientX));
    setWidth(Math.max(240, Math.min(520, next)));
  };
  const onResizeEnd = () => {
    if (!dragRef.current) return;
    dragRef.current = null;
    setDragging(false);
    try {
      localStorage.setItem("downpilot.details.width", String(width));
    } catch {
      /* ignore */
    }
  };

  return (
    <aside className={`details${dragging ? " details--dragging" : ""}`} style={{ width, flexBasis: width }}>
      <div
        className="details__resizer"
        role="separator"
        aria-orientation="vertical"
        aria-label="Resize details panel"
        onPointerDown={onResizeStart}
        onPointerMove={onResizeMove}
        onPointerUp={onResizeEnd}
        onPointerCancel={onResizeEnd}
      />
      <div className="details__head">
        <span className="details__title">Details</span>
        <div className="flex items-center gap-1">
          {task && (
            <button className="icon-btn" title="Properties" aria-label="Open properties" onClick={() => onOpenProps(task.id)}>
              <IconProperties size={14} />
            </button>
          )}
          <button className="icon-btn" title="Hide details" aria-label="Hide details" onClick={onClose}>×</button>
        </div>
      </div>

      {!task && (
        <div className="details__empty">
          <div className="details__empty-icon" aria-hidden="true">
            <IconProperties size={22} />
          </div>
          <strong>{sel.length > 1 ? `${sel.length} downloads selected` : "No download selected"}</strong>
          <span>
            {sel.length > 1
              ? "Select one download to inspect its details."
              : "Select a row to inspect status, progress, and actions."}
          </span>
        </div>
      )}

      {task && (
        <div className="details__body">
          <div className="details__name" title={task.filename || task.url}>
            {task.filename || task.url}
          </div>

          <div className="details__actions">
            {task.status === "downloading" ? (
              <button className="btn btn--default btn--sm" onClick={() => store.pause(task.id)}>
                <IconPause size={13} /> Pause
              </button>
            ) : (
              <button className="btn btn--primary btn--sm" onClick={() => store.resume(task.id)}>
                <IconStart size={13} /> {task.status === "paused" ? "Resume" : "Start"}
              </button>
            )}
            <button className="btn btn--default btn--sm" onClick={() => store.cancel(task.id)}>
              <IconStop size={13} /> Stop
            </button>
            <button className="btn btn--default btn--sm" onClick={() => store.redownload(task.id)}>
              <IconRedownload size={13} /> Redownload
            </button>
            <button className="btn btn--ghost btn--sm" onClick={() => store.openFolder(task)}>
              <IconFolder size={13} /> Open Folder
            </button>
            <button
              className="btn btn--ghost btn--sm"
              onClick={() => navigator.clipboard.writeText(task.url)}
              title="Copy download URL"
            >
              Copy URL
            </button>
            <button
              className="btn btn--danger btn--sm"
              onClick={() => store.removeKeepFile(task.id)}
              title="Remove from DownPilot without deleting the file"
            >
              <IconDelete size={13} /> Remove
            </button>
          </div>

          <dl className="kv">
            <Detail k="Status" v={task.status} />
            <Detail k="URL" v={task.url} />
            <Detail k="Size" v={formatBytes(task.total_size)} />
            <Detail k="Downloaded" v={formatBytes(task.downloaded)} />
            <Detail
              k="Progress"
              v={formatPercent(task.downloaded, task.total_size)}
            />
            <Detail k="Speed" v={(() => {
              const m = deriveTaskMetrics(task, store.getSpeedHistory(task.id));
              if (task.status === "downloading")
                return m.speedBps > 0 ? formatSpeed(m.speedBps) : "…";
              if (m.avgSpeedBps > 0) return formatSpeed(m.avgSpeedBps);
              return "—";
            })()} />
            <Detail
              k="Time Left"
              v={(() => {
                const m = deriveTaskMetrics(task, store.getSpeedHistory(task.id));
                if (task.status !== "downloading" || m.etaSecs == null) return "—";
                return formatEta(m.remaining, m.speedBps);
              })()}
            />
            <Detail
              k="Remaining"
              v={(() => {
                const m = deriveTaskMetrics(task, store.getSpeedHistory(task.id));
                return m.remaining >= 0 ? formatBytes(m.remaining) : "unknown";
              })()}
            />
            <Detail
              k="Average Speed"
              v={(() => {
                const m = deriveTaskMetrics(task, store.getSpeedHistory(task.id));
                return m.avgSpeedBps > 0 ? formatSpeed(m.avgSpeedBps) : "—";
              })()}
            />
            <Detail k="Category" v={task.category} />
            <Detail k="Queue" v={task.queue} />
            <Detail k="Connections" v={String(task.threads || 1)} />
            <Detail k="Added" v={formatDate(task.created_at)} />
            <Detail k="Domain" v={domainOf(task.url)} />
            <Detail k="Save To" v={task.save_dir} />
            <Detail k="Proxy" v={task.proxy_url || "Direct"} />
          </dl>

          {task.error && task.status === "failed" && (
            <div className="details__error">
              <strong>Error:</strong> {task.error}
            </div>
          )}
        </div>
      )}
    </aside>
  );
}

function Detail({ k, v }: { k: string; v: string }) {
  return (
    <div className="kv__row">
      <dt>{k}</dt>
      <dd title={v}>{v}</dd>
    </div>
  );
}
