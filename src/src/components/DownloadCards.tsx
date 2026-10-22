// Card/panel view of the download list.
//
// This is the "cards" half of the table/cards split. Each download is
// a self-contained panel: icon + name on the header line, a progress
// bar, then a metric row (size, speed, ETA, remaining). Actions sit
// in the top-right corner of the panel and appear on hover.
//
// The card list reuses the same store data and the same status fusion
// as the table view, so switching between the two never changes what
// the user sees about a download.

import { useMemo } from "react";
import type { DownloadTask } from "../types";
import { fileIconFor } from "./icons";
import {
  IconDelete,
  IconOpenFile,
  IconPause,
  IconRedownload,
  IconResume,
} from "./icons";
import { formatBytes, formatSpeed, formatEta } from "../lib/format";
import { deriveTaskMetrics } from "../lib/metrics";
import {
  uiStatus,
  uiPct,
  uiStatusLabel,
  statusGlyph,
  statusTextClass,
  emptyState,
} from "../lib/taskUi";

export interface DownloadCardsProps {
  tasks: DownloadTask[];
  grouped: { key: string; items: DownloadTask[] }[];
  selected: Set<number>;
  onSelect: (id: number, ev: React.MouseEvent) => void;
  onRetry: (id: number) => void;
  onContextMenu: (id: number, x: number, y: number) => void;
  onRowActivate?: (id: number) => void;
  onPause: (id: number) => void;
  onResume: (id: number) => void;
  onRemove: (id: number) => void;
  /** Active sidebar filter — used for the contextual empty state. */
  sidebarKey?: { kind: string; value?: string | number };
  /** Current search query — distinguishes "no downloads" from
   *  "no downloads match this search". */
  search?: string;
  speedHistory?: (id: number) => number[];
}

export function DownloadCards({
  tasks,
  grouped,
  selected,
  onSelect,
  onRetry,
  onContextMenu,
  onRowActivate,
  onPause,
  onResume,
  onRemove,
  sidebarKey,
  search,
  speedHistory,
}: DownloadCardsProps) {
  if (tasks.length === 0) {
    const empty = emptyState(sidebarKey, search);
    return (
      <div className="flex min-h-0 flex-1 flex-col items-center justify-center text-center">
        <div className="text-sm text-txt-dim">{empty.title}</div>
        <div className="mt-1 text-xs text-txt-faint">{empty.hint}</div>
      </div>
    );
  }

  return (
    <div className="cards-scroll min-h-0 flex-1 overflow-y-auto bg-[var(--table-bg)] p-3">
      {grouped.map((g) => (
        <section key={g.key || "all"} className="mb-4 last:mb-0">
          {g.key && (
            <h2 className="cards-group">
              {g.key}
              <span className="cards-group__count">{g.items.length}</span>
            </h2>
          )}
          <div className="cards-grid">
            {g.items.map((t) => (
              <Card
                key={t.id}
                t={t}
                selected={selected.has(t.id)}
                onSelect={onSelect}
                onRetry={onRetry}
                onContextMenu={onContextMenu}
                onRowActivate={onRowActivate}
                onPause={onPause}
                onResume={onResume}
                onRemove={onRemove}
                speedHistory={speedHistory}
              />
            ))}
          </div>
        </section>
      ))}
    </div>
  );
}

function Card({
  t,
  selected,
  onSelect,
  onRetry,
  onContextMenu,
  onRowActivate,
  onPause,
  onResume,
  onRemove,
  speedHistory,
}: {
  t: DownloadTask;
  selected: boolean;
  onSelect: (id: number, ev: React.MouseEvent) => void;
  onRetry: (id: number) => void;
  onContextMenu: (id: number, x: number, y: number) => void;
  onRowActivate?: (id: number) => void;
  onPause: (id: number) => void;
  onResume: (id: number) => void;
  onRemove: (id: number) => void;
  speedHistory?: (id: number) => number[];
}) {
  const iconResult = useMemo(
    () => fileIconFor(t.filename, t.category),
    [t.filename, t.category],
  );
  const Icon = iconResult.Icon;
  const iconColor = iconResult.color;

  const total = t.total_size || 0;
  const done = t.downloaded || 0;
  const pct = uiPct(t);
  const st = uiStatus(t);
  const metrics = deriveTaskMetrics(t, speedHistory?.(t.id) ?? []);

  const isActive = st === "downloading";
  const isDone = st === "completed";
  const isFailed = st === "failed";

  const speedText = isActive
    ? metrics.speedBps > 0
      ? formatSpeed(metrics.speedBps)
      : "—"
    : metrics.avgSpeedBps > 0
    ? formatSpeed(metrics.avgSpeedBps)
    : "—";

  const etaText =
    isActive && metrics.etaSecs != null && metrics.remaining > 0
      ? formatEta(metrics.remaining, metrics.speedBps)
      : "—";

  const remainingText =
    total > 0 && done < total ? formatBytes(total - done) : "—";

  const progressFill = isFailed
    ? "bg-red-500"
    : st === "paused"
    ? "bg-amber-500"
    : isDone
    ? "bg-emerald-500"
    : "bg-gradient-to-r from-cyan-400 to-blue-500";

  return (
    <article
      className={`card group ${selected ? "card--selected" : ""}`}
      data-download-id={t.id}
      aria-selected={selected}
      tabIndex={0}
      onPointerDown={(e) => {
        if (
          e.pointerType !== "mouse" ||
          (e.button === 0 && !e.shiftKey && !e.ctrlKey && !e.metaKey)
        ) {
          onSelect(t.id, e);
        }
      }}
      onClick={(e) => {
        if (!(e.shiftKey || e.ctrlKey || e.metaKey)) onSelect(t.id, e);
      }}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onSelect(t.id, e as unknown as React.MouseEvent);
        }
      }}
      onContextMenu={(e) => {
        e.preventDefault();
        onContextMenu(t.id, e.clientX, e.clientY);
      }}
      onDoubleClick={() => onRowActivate?.(t.id)}
    >
      {/* ---- header: check, icon, name, status, actions ---- */}
      <header className="card__head">
        <button
          type="button"
          aria-label={selected ? `Deselect ${t.filename}` : `Select ${t.filename}`}
          aria-pressed={selected}
          className={`card__check ${selected ? "card__check--on" : ""}`}
          onClick={(e) => {
            e.stopPropagation();
            onSelect(t.id, e);
          }}
        >
          {selected ? "✓" : ""}
        </button>

        <span className={`card__icon ${iconColor}`}>
          <Icon size={18} />
        </span>

        <div className="card__ident">
          <div className="card__name" title={t.filename || t.url}>
            {t.filename || t.url || "—"}
          </div>
          <div className="card__sub">
            <span className={`card__status ${statusTextClass(st)}`}>
              <span aria-hidden="true">{statusGlyph(st)}</span>
              {uiStatusLabel(t, pct)}
            </span>
            {t.queue && t.queue !== "general" && (
              <span className="card__tag" title={`Queue: ${t.queue}`}>
                {t.queue}
              </span>
            )}
            {t.category && t.category !== "other" && (
              <span className="card__tag" title={`Category: ${t.category}`}>
                {t.category}
              </span>
            )}
          </div>
        </div>

        <div className="card__actions">
          {isFailed && (
            <button
              type="button"
              className="card__act card__act--danger"
              title="Retry download"
              aria-label={`Retry ${t.filename}`}
              onClick={(e) => {
                e.stopPropagation();
                onRetry(t.id);
              }}
            >
              <IconRedownload size={14} />
            </button>
          )}
          {isActive ? (
            <button
              type="button"
              className="card__act"
              title="Pause download"
              aria-label={`Pause ${t.filename}`}
              onClick={(e) => {
                e.stopPropagation();
                onPause(t.id);
              }}
            >
              <IconPause size={14} />
            </button>
          ) : (
            !isDone && (
              <button
                type="button"
                className="card__act"
                title="Resume download"
                aria-label={`Resume ${t.filename}`}
                onClick={(e) => {
                  e.stopPropagation();
                  onResume(t.id);
                }}
              >
                <IconResume size={14} />
              </button>
            )
          )}
          {isDone && (
            <button
              type="button"
              className="card__act"
              title="Open file"
              aria-label={`Open ${t.filename}`}
              onClick={(e) => {
                e.stopPropagation();
                onRowActivate?.(t.id);
              }}
            >
              <IconOpenFile size={14} />
            </button>
          )}
          <button
            type="button"
            className="card__act card__act--danger"
            title="Remove download"
            aria-label={`Remove ${t.filename}`}
            onClick={(e) => {
              e.stopPropagation();
              onRemove(t.id);
            }}
          >
            <IconDelete size={14} />
          </button>
        </div>
      </header>

      {/* ---- progress line ---- */}
      <div className="card__progress">
        <div className="card__track">
          <div
            className={`card__fill ${progressFill}`}
            style={{ width: `${pct}%` }}
          />
        </div>
        <span className="card__pct">{pct}%</span>
      </div>

      {/* ---- metrics ---- */}
      <footer className="card__metrics">
        <Metric label="Size" value={formatBytes(total)} />
        <Metric
          label="Done"
          value={total > 0 ? `${formatBytes(done)}` : "—"}
        />
        <Metric label="Speed" value={speedText} />
        <Metric label="ETA" value={etaText} />
      </footer>

      {isFailed && t.error && (
        <p className="card__error" title={t.error}>
          {cleanError(t.error)}
        </p>
      )}
    </article>
  );
}

function cleanError(err: string): string {
  // Drop the generic Rust/reqwest prelude so the user sees only the
  // actionable part — the host, path, or status code.
  // e.g. "error sending request for url (https://...): connection refused"
  //   -> "connection refused"
  const stripped = err
    .replace(/^error sending request for url\s*\(\s*https?:\/\/[^)]+\)\s*:\s*/i, "")
    .replace(/^error sending request for url\s*\(\s*https?:\/\/[^)]+\)\s*/i, "")
    .replace(/^error sending request for url\s*/i, "")
    .replace(/^hyper error:\s*/i, "")
    .replace(/^request error \(cause:\s*[^)]+\):\s*/i, "")
    .trim();
  return stripped.length > 8 ? stripped : err;
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div className="card__metric">
      <span className="card__metric-label">{label}</span>
      <span className="card__metric-value">{value}</span>
    </div>
  );
}
