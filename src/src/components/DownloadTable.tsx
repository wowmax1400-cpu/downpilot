// Modern dark download table with persistent column configuration.
//
// What this implements:
//   - Resizable columns (drag the right edge of a header)
//   - Hideable columns (right-click a header → Columns menu)
//   - Persisted state (localStorage via useColumnConfig)
//   - Sortable columns (click a header to sort; click again to flip)
//   - Group-by-status/category/queue/date (driven by store.groupBy)
//
// Row layout is a CSS grid; widths come from the column config. The
// row does NOT use a sticky layout — a long filename still truncates
// with ellipsis, the file icon stays anchored on the left.

import { useMemo, useState } from "react";
import type { DownloadTask, GroupBy, Sort, SortField } from "../types";
import { fileIconFor } from "./icons";
import {
  formatBytes,
  formatSpeed,
  formatEta,
  formatDate,
  statusLabel,
  domainOf,
} from "../lib/format";
import {
  COLUMNS,
  useColumnConfig,
  type ColumnId,
} from "../lib/columnConfig";
import { ColumnsMenu } from "./ColumnsMenu";

export interface DownloadTableProps {
  tasks: DownloadTask[];
  grouped: { key: string; items: DownloadTask[] }[];
  selected: Set<number>;
  sort: Sort;
  onSortChange: (s: Sort) => void;
  onSelect: (id: number, ev: React.MouseEvent) => void;
  onContextMenu: (id: number, x: number, y: number) => void;
  /** Double-click on a row → callback with id (used to open Properties
   *  for active downloads, Open File for completed). */
  onRowActivate?: (id: number) => void;
  groupBy: GroupBy;
}

export function DownloadTable({
  tasks,
  grouped,
  selected,
  sort,
  onSortChange,
  onSelect,
  onContextMenu,
  onRowActivate,
  groupBy,
}: DownloadTableProps) {
  const colCfg = useColumnConfig();
  const [columnsMenu, setColumnsMenu] = useState<{ x: number; y: number } | null>(null);

  if (tasks.length === 0) {
    return (
      <div className="flex h-full flex-col items-center justify-center text-center">
        <div className="text-sm text-txt-dim">No downloads</div>
        <div className="mt-1 text-xs text-txt-faint">
          Click <span className="text-txt-dim">Add URL</span> or grab links
          from a page.
        </div>
      </div>
    );
  }

  const visible = colCfg.cfg.order.filter((id) => colCfg.cfg.visibility[id]);

  return (
    <div className="flex h-full min-h-0 flex-col">
      <Header
        visible={visible}
        widths={colCfg.cfg.widths}
        sort={sort}
        onSortChange={onSortChange}
        onResize={colCfg.setWidth}
        onMove={colCfg.move}
        onOpenColumnsMenu={(x, y) => setColumnsMenu({ x, y })}
      />
      <div className="min-h-0 flex-1 overflow-y-auto">
        {grouped.map((g) => (
          <Group
            key={g.key || "all"}
            groupKey={g.key}
            items={g.items}
            visible={visible}
            widths={colCfg.cfg.widths}
            selected={selected}
            onSelect={onSelect}
            onContextMenu={onContextMenu}
            onRowActivate={onRowActivate}
          />
        ))}
      </div>
      {columnsMenu && (
        <ColumnsMenu
          cfg={colCfg.cfg}
          onToggle={colCfg.toggle}
          onReset={colCfg.reset}
          onClose={() => setColumnsMenu(null)}
          x={columnsMenu.x}
          y={columnsMenu.y}
        />
      )}
    </div>
  );
}

function gridTemplate(widths: Record<ColumnId, number>, visible: ColumnId[]) {
  return visible.map((id) => `${widths[id] ?? 100}px`).join(" ");
}

function Group({
  groupKey,
  items,
  visible,
  widths,
  selected,
  onSelect,
  onContextMenu,
  onRowActivate,
}: {
  groupKey: string;
  items: DownloadTask[];
  visible: ColumnId[];
  widths: Record<ColumnId, number>;
  selected: Set<number>;
  onSelect: (id: number, ev: React.MouseEvent) => void;
  onContextMenu: (id: number, x: number, y: number) => void;
  onRowActivate?: (id: number) => void;
}) {
  return (
    <>
      {groupKey && (
        <div className="px-3 pt-3 pb-1 text-[10.5px] font-semibold uppercase tracking-wider text-slate-500">
          {groupKey}{" "}
          <span className="ml-1 font-mono tabular-nums text-slate-600">
            ({items.length})
          </span>
        </div>
      )}
      {items.map((t) => (
        <Row
          key={t.id}
          t={t}
          visible={visible}
          widths={widths}
          selected={selected.has(t.id)}
          onSelect={onSelect}
          onContextMenu={onContextMenu}
          onRowActivate={onRowActivate}
        />
      ))}
    </>
  );
}

function Header({
  visible,
  widths,
  sort,
  onSortChange,
  onResize,
  onMove,
  onOpenColumnsMenu,
}: {
  visible: ColumnId[];
  widths: Record<ColumnId, number>;
  sort: Sort;
  onSortChange: (s: Sort) => void;
  onResize: (id: ColumnId, w: number) => void;
  onMove: (id: ColumnId, beforeId: ColumnId | null) => void;
  onOpenColumnsMenu: (x: number, y: number) => void;
}) {
  // Map column id → sort field. The store only supports a few, but we
  // accept the click regardless and degrade to "no-op" for the rest.
  const sortable: Partial<Record<ColumnId, SortField>> = {
    name: "name",
    size: "size",
    status: "status",
    time_left: "time_left",
    speed: "speed",
    downloaded: "downloaded",
    progress: "progress",
    avg_speed: "avg_speed",
    connections: "connections",
    category: "category",
    queue: "queue",
    added: "added_at",
  };

  const flip = (field: SortField) => {
    if (sort.field === field) {
      onSortChange({ field, dir: sort.dir === "asc" ? "desc" : "asc" });
    } else {
      onSortChange({ field, dir: "asc" });
    }
  };
  const arrow = (f: SortField) =>
    sort.field === f ? (sort.dir === "asc" ? " ↑" : " ↓") : "";

  const cell = (align: "left" | "right") =>
    `flex select-none items-center text-xs font-semibold uppercase tracking-wider text-slate-400 transition-colors hover:text-slate-200 ${
      align === "right" ? "justify-end" : "justify-start"
    }`;

  return (
    <div
      className="grid shrink-0 items-center gap-x-0 border-b border-white/5 bg-surface/50 px-3 py-1.5"
      style={{ gridTemplateColumns: gridTemplate(widths, visible) }}
      role="row"
    >
      {visible.map((id, i) => {
        const col = COLUMNS.find((c) => c.id === id)!;
        const sf = sortable[id];
        const isLast = i === visible.length - 1;
        return (
          <div key={id} className="relative flex items-center justify-end pr-3 last:pr-0">
            <div
              className={`${cell(col.right ? "right" : "left")} ${
                sf ? "cursor-pointer" : ""
              } w-full`}
              onClick={() => sf && flip(sf)}
            >
              {col.label}
              {sf && arrow(sf)}
            </div>
            {!isLast && (
              <ResizeHandle
                onResize={(dx) => onResize(id, widths[id] + dx)}
                minWidth={col.minWidth}
              />
            )}
            {/* HTML5 drag-to-reorder. The header cell is draggable; the
                next cell (or the wrapper around it) is the drop target.
                onDrop calls onMove(src, thisId) which inserts src just
                before thisId. */}
            <div
              draggable
              onDragStart={(e) => {
                if (e.dataTransfer) {
                  e.dataTransfer.setData("text/plain", id);
                  e.dataTransfer.effectAllowed = "move";
                }
              }}
              onDragOver={(e) => {
                e.preventDefault();
                if (e.dataTransfer) e.dataTransfer.dropEffect = "move";
              }}
              onDrop={(e) => {
                e.preventDefault();
                const src = e.dataTransfer?.getData("text/plain") ?? "";
                if (src && src !== id) onMove(src as ColumnId, id);
              }}
              className="absolute inset-y-0 right-3 w-2 cursor-grab active:cursor-grabbing"
              title="Drag to reorder"
              aria-label="Reorder column"
            />
            {/* Right-click on the LAST cell opens the columns menu (this
                is the only place the menu trigger lives — simpler than
                adding a separate cog icon). */}
            {isLast && (
              <button
                type="button"
                aria-label="Columns"
                title="Choose columns…"
                className="absolute inset-y-0 right-0 hidden w-6 items-center justify-center text-slate-500 hover:text-slate-200 group-hover:flex"
                onClick={(e) => {
                  e.stopPropagation();
                  const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
                  onOpenColumnsMenu(r.right, r.bottom);
                }}
                onContextMenu={(e) => {
                  e.preventDefault();
                  e.stopPropagation();
                  onOpenColumnsMenu(e.clientX, e.clientY);
                }}
              >
                <svg viewBox="0 0 16 16" className="h-3.5 w-3.5" fill="currentColor">
                  <circle cx="3" cy="8" r="1.4" />
                  <circle cx="8" cy="8" r="1.4" />
                  <circle cx="13" cy="8" r="1.4" />
                </svg>
              </button>
            )}
          </div>
        );
      })}
    </div>
  );
}

function ResizeHandle({
  onResize,
  minWidth,
}: {
  onResize: (dx: number) => void;
  minWidth: number;
}) {
  const onMouseDown = (e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    const startX = e.clientX;
    let lastW = 0;
    const onMove = (ev: MouseEvent) => {
      const dx = ev.clientX - startX;
      // Use the last applied width as the new baseline so the
      // delta accumulates correctly on each pixel of movement.
      onResize(dx - lastW);
      lastW = dx;
    };
    const onUp = () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
    // Silence unused.
    void minWidth;
  };
  return (
    <div
      onMouseDown={onMouseDown}
      className="absolute right-0 top-0 z-10 h-full w-1.5 cursor-col-resize hover:bg-indigo-500/30"
      title="Drag to resize"
    />
  );
}

function Row({
  t,
  visible,
  widths,
  selected,
  onSelect,
  onContextMenu,
  onRowActivate,
}: {
  t: DownloadTask;
  visible: ColumnId[];
  widths: Record<ColumnId, number>;
  selected: boolean;
  onSelect: (id: number, ev: React.MouseEvent) => void;
  onContextMenu: (id: number, x: number, y: number) => void;
  onRowActivate?: (id: number) => void;
}) {
  const iconResult = useMemo(
    () => fileIconFor(t.filename, t.category),
    [t.filename, t.category]
  );
  const Icon = iconResult.Icon;
  const iconColor = iconResult.color;
  const total = t.total_size || 0;
  const done = t.downloaded || 0;
  const pct = total > 0 ? Math.min(100, Math.round((done / total) * 100)) : 0;

  const numCell = "text-right font-mono text-xs tabular-nums text-slate-300";
  const dim = "text-xs text-slate-400";

  return (
    <div
      className={
        "group grid cursor-default items-center gap-x-0 border-b border-white/5 px-3 py-1.5 text-sm transition-colors hover:bg-white/[0.04] " +
        (selected ? "bg-indigo-500/10" : "")
      }
      style={{ gridTemplateColumns: gridTemplate(widths, visible) }}
      onClick={(e) => onSelect(t.id, e)}
      onContextMenu={(e) => {
        e.preventDefault();
        onContextMenu(t.id, e.clientX, e.clientY);
      }}
      onDoubleClick={() => onRowActivate?.(t.id)}
    >
      {visible.map((id) => {
        const col = COLUMNS.find((c) => c.id === id)!;
        const align = col.right ? "pr-3" : "pr-3";
        return (
          <div key={id} className={`${align} overflow-hidden`}>
            {renderCell(id, {
              t,
              Icon,
              iconColor,
              total,
              done,
              pct,
              numCell,
              dim,
            })}
          </div>
        );
      })}
    </div>
  );
}

function renderCell(
  id: ColumnId,
  ctx: {
    t: DownloadTask;
    Icon: any;
    iconColor: string;
    total: number;
    done: number;
    pct: number;
    numCell: string;
    dim: string;
  }
) {
  const { t, Icon, iconColor, total, done, pct, numCell, dim } = ctx;
  switch (id) {
    case "name":
      return (
        <div className="flex min-w-0 items-center gap-2" title={t.filename || t.url}>
          <span
            className={`inline-flex h-4 w-4 shrink-0 items-center justify-center ${iconColor}`}
          >
            <Icon size={14} />
          </span>
          <span className="truncate text-slate-100">
            {t.filename || t.url || "—"}
          </span>
        </div>
      );
    case "size":
      return <div className={numCell}>{formatBytes(total)}</div>;
    case "status":
      return <div className={"text-right text-xs " + statusColor(t.status)}>{statusLabel(t.status)}</div>;
    case "time_left":
      return (
        <div className={numCell}>
          {t.status === "downloading" && t.speed_bps > 0
            ? formatEta(total - done, t.speed_bps)
            : "—"}
        </div>
      );
    case "speed":
      return (
        <div className={numCell}>
          {t.status === "downloading" ? formatSpeed(t.speed_bps) : "—"}
        </div>
      );
    case "downloaded":
      return <div className={numCell}>{pct}%</div>;
    case "progress":
      return <ProgressBar pct={pct} status={t.status} />;
    case "avg_speed":
      return (
        <div className={numCell}>
          {t.avg_speed_bps > 0 ? formatSpeed(t.avg_speed_bps) : "—"}
        </div>
      );
    case "connections":
      return <div className={numCell}>{t.threads || 1}</div>;
    case "category":
      return <div className={dim + " truncate"}>{t.category || "other"}</div>;
    case "queue":
      return <div className={dim + " truncate"}>{t.queue || "general"}</div>;
    case "added":
      return <div className={numCell}>{formatDate(t.created_at)}</div>;
    case "save_to":
      return (
        <div className={dim + " truncate"} title={t.save_dir}>
          {t.save_dir}
        </div>
      );
    case "description":
      return (
        <div className={dim + " truncate"} title={t.description ?? ""}>
          {t.description || ""}
        </div>
      );
  }
}

function ProgressBar({ pct, status }: { pct: number; status: string }) {
  // Completed: render a static check instead of a 100% bar so the row
  // visually "feels finished" — the spec calls this out explicitly.
  if (status === "completed") {
    return (
      <div className="flex items-center gap-1.5 text-xs text-emerald-500">
        <svg viewBox="0 0 16 16" className="h-3.5 w-3.5" fill="none" stroke="currentColor" strokeWidth="1.8">
          <path d="M3 8.5l3 3 7-7" />
        </svg>
        <span>Complete</span>
      </div>
    );
  }
  if (status === "failed") {
    return (
      <div className="flex items-center gap-1.5 text-xs text-red-500">
        <span>Failed</span>
      </div>
    );
  }
  const fill =
    status === "paused"
      ? "from-amber-500/60 to-amber-500"
      : "from-indigo-500/70 to-indigo-400";
  return (
    <div className="h-1.5 w-full overflow-hidden rounded-full bg-slate-800">
      <div
        className={`h-full rounded-full bg-gradient-to-r ${fill} transition-[width] duration-300`}
        style={{ width: `${Math.max(0, Math.min(100, pct))}%` }}
      />
    </div>
  );
}

function statusColor(s: string): string {
  switch (s) {
    case "downloading":
      return "text-indigo-400";
    case "completed":
      return "text-emerald-500";
    case "paused":
      return "text-amber-500";
    case "failed":
      return "text-red-500";
    case "cancelled":
      return "text-slate-500";
    case "queued":
    case "scheduled":
      return "text-slate-400";
    default:
      return "text-slate-400";
  }
}
