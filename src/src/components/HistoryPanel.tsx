// IDM-style Download History. Shows terminal-state tasks (completed,
// failed, stopped, cancelled) with full per-row actions.
//
// Column layout (8 columns): checkbox, name, size, status, category,
// queue, added, completed, source, save dir, actions.
//
// Actions per row:
//   - Open File      (only when status == "completed")
//   - Open Folder    (opens the save directory in the system file manager)
//   - Redownload     (re-queue with the same options)
//   - Copy URL
//   - Copy Path      (absolute path on disk, or the save_dir if file unknown)
//   - Delete File    (removes the row AND the file on disk — destructive)
//   - Hide           (removes from history only, file untouched)
//
// Multi-select: a header checkbox selects/deselects all visible rows.
// Selection is local to this dialog (does not affect the main table).
//
// Sort: clicking a sortable column header sorts by that column. State
// is component-local; we deliberately don't persist sort across opens
// because users re-open History with different intents.

import React from "react";
import type { DownloadTask } from "../types";
import type { DownloadsStore } from "../store/useDownloads";
import { Dialog, Btn } from "./Dialog";
import {
  formatBytes,
  formatDateTime,
  domainOf,
  statusLabel,
} from "../lib/format";
import * as api from "../lib/api";

type Props = { store: DownloadsStore; onClose: () => void };

type StatusFilter = "all" | "completed" | "failed" | "stopped" | "cancelled";

const TERMINAL = new Set(["completed", "failed", "stopped", "cancelled"]);

type SortKey = "name" | "size" | "completed" | "added" | "status" | "source";
type SortDir = "asc" | "desc";

export function HistoryPanel({ store, onClose }: Props) {
  const [q, setQ] = React.useState("");
  const [status, setStatus] = React.useState<StatusFilter>("all");
  const [sortKey, setSortKey] = React.useState<SortKey>("completed");
  const [sortDir, setSortDir] = React.useState<SortDir>("desc");
  const [selected, setSelected] = React.useState<Set<number>>(new Set());

  const tasks = React.useMemo(
    () => store.tasks.filter((t) => TERMINAL.has(t.status)),
    [store.tasks]
  );

  const counts = React.useMemo(() => {
    const c = {
      all: tasks.length,
      completed: 0,
      failed: 0,
      stopped: 0,
      cancelled: 0,
    } as Record<StatusFilter, number>;
    for (const t of tasks) {
      if (t.status === "completed" || t.status === "failed" || t.status === "stopped" || t.status === "cancelled") {
        c[t.status as StatusFilter]++;
      }
    }
    return c;
  }, [tasks]);

  const filtered = React.useMemo(() => {
    const needle = q.trim().toLowerCase();
    const list = tasks.filter((t) => {
      if (status !== "all" && t.status !== status) return false;
      if (!needle) return true;
      const hay = [
        t.filename ?? "",
        t.url,
        domainOf(t.url),
        t.save_dir ?? "",
        t.category ?? "",
        t.queue ?? "",
        t.description ?? "",
      ]
        .join(" ")
        .toLowerCase();
      return hay.includes(needle);
    });
    const dir = sortDir === "asc" ? 1 : -1;
    const keyVal = (t: DownloadTask): string | number => {
      switch (sortKey) {
        case "name":
          return (t.filename || t.url).toLowerCase();
        case "size":
          return t.total_size;
        case "completed":
          return t.completed_at ?? t.updated_at ?? 0;
        case "added":
          return t.created_at ?? 0;
        case "status":
          return t.status;
        case "source":
          return domainOf(t.url);
      }
    };
    return [...list].sort((a, b) => {
      const va = keyVal(a);
      const vb = keyVal(b);
      if (va < vb) return -1 * dir;
      if (va > vb) return 1 * dir;
      return a.id - b.id;
    });
  }, [tasks, q, status, sortKey, sortDir]);

  const allSelected =
    filtered.length > 0 && filtered.every((t) => selected.has(t.id));
  const someSelected = filtered.some((t) => selected.has(t.id));

  const toggleOne = (id: number) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };
  const toggleAll = () => {
    if (allSelected) {
      setSelected(new Set());
    } else {
      setSelected(new Set(filtered.map((t) => t.id)));
    }
  };

  const flipSort = (k: SortKey) => {
    if (sortKey === k) {
      setSortDir((d) => (d === "asc" ? "desc" : "asc"));
    } else {
      setSortKey(k);
      setSortDir(k === "size" || k === "completed" || k === "added" ? "desc" : "asc");
    }
  };
  const sortArrow = (k: SortKey) =>
    sortKey === k ? (sortDir === "asc" ? " ↑" : " ↓") : "";

  const openFile = (t: DownloadTask) => void store.openFile(t);
  const openFolder = (t: DownloadTask) => void api.openInFileManager(t.save_dir);
  const copyUrl = (t: DownloadTask) => void api.copyTextToClipboard(t.url);
  const copyPath = (t: DownloadTask) =>
    void api.copyTextToClipboard(t.save_dir + (t.filename ? "/" + t.filename : ""));
  const redownloadOne = (t: DownloadTask) => void store.redownload(t.id);
  const hideOne = (t: DownloadTask) => void store.removeKeepFile(t.id);
  const deleteOne = (t: DownloadTask) => {
    const name = t.filename || t.url;
    if (
      window.confirm(
        `Delete the file from disk and remove from history?\n\n${name}\n\nThis cannot be undone.`
      )
    ) {
      void store.removeWithFile(t.id);
    }
  };

  const bulkHide = () => {
    if (selected.size === 0) return;
    if (
      !window.confirm(
        `Remove ${selected.size} item${selected.size === 1 ? "" : "s"} from history? Files on disk are not affected.`
      )
    )
      return;
    for (const id of selected) void store.removeKeepFile(id);
    setSelected(new Set());
  };
  const bulkDelete = () => {
    if (selected.size === 0) return;
    if (
      !window.confirm(
        `Delete ${selected.size} file${selected.size === 1 ? "" : "s"} from disk and remove from history?\n\nThis cannot be undone.`
      )
    )
      return;
    for (const id of selected) void store.removeWithFile(id);
    setSelected(new Set());
  };
  const clearAll = () => {
    if (window.confirm("Clear all history? Files on disk are not affected.")) {
      void api.clearFinished();
    }
  };

  return (
    <Dialog
      title="Download History"
      onClose={onClose}
      width={960}
      footer={
        <>
          {selected.size > 0 && (
            <>
              <span className="text-xs text-txt-dim mr-auto">
                {selected.size} selected
              </span>
              <Btn variant="ghost" onClick={bulkHide}>
                Hide
              </Btn>
              <Btn variant="danger" onClick={bulkDelete}>
                Delete files
              </Btn>
              <span className="mx-2 h-5 w-px bg-white/10" />
            </>
          )}
          <Btn variant="ghost" onClick={onClose}>
            Close
          </Btn>
          <Btn variant="danger" onClick={clearAll} disabled={tasks.length === 0}>
            Clear history
          </Btn>
        </>
      }
    >
      <div className="form">
        <div className="table-toolbar">
          <input
            className="input input--search"
            placeholder="Search filename, URL, domain, folder, category..."
            value={q}
            onChange={(e) => setQ(e.target.value)}
          />
          <select
            className="select"
            value={status}
            onChange={(e) => setStatus(e.target.value as StatusFilter)}
            style={{ width: 160 }}
          >
            <option value="all">All ({counts.all})</option>
            <option value="completed">Completed ({counts.completed})</option>
            <option value="failed">Failed ({counts.failed})</option>
            <option value="stopped">Stopped ({counts.stopped})</option>
            <option value="cancelled">Cancelled ({counts.cancelled})</option>
          </select>
        </div>

        <div className="history-grid">
          <div className="history-grid__head">
            <div className="history-grid__check">
              <input
                type="checkbox"
                checked={allSelected}
                ref={(el) => {
                  if (el) el.indeterminate = !allSelected && someSelected;
                }}
                onChange={toggleAll}
                aria-label="Select all"
              />
            </div>
            <div className="cursor-pointer select-none" onClick={() => flipSort("name")}>
              Name{sortArrow("name")}
            </div>
            <div className="cursor-pointer select-none" onClick={() => flipSort("size")}>
              Size{sortArrow("size")}
            </div>
            <div className="cursor-pointer select-none" onClick={() => flipSort("status")}>
              Status{sortArrow("status")}
            </div>
            <div>Category</div>
            <div>Queue</div>
            <div className="cursor-pointer select-none" onClick={() => flipSort("added")}>
              Added{sortArrow("added")}
            </div>
            <div className="cursor-pointer select-none" onClick={() => flipSort("completed")}>
              Completed{sortArrow("completed")}
            </div>
            <div className="cursor-pointer select-none" onClick={() => flipSort("source")}>
              Source{sortArrow("source")}
            </div>
            <div>Save location</div>
            <div>Actions</div>
          </div>
          <div className="history-grid__body">
            {filtered.length === 0 && (
              <div className="panel__empty">
                {q || status !== "all" ? "No history matches" : "No completed downloads."}
              </div>
            )}
            {filtered.map((t) => {
              const dispName = t.filename || t.url.split("/").pop() || "—";
              const addedDate = t.created_at ? formatDateTime(t.created_at) : "—";
              const completedDate = t.completed_at
                ? formatDateTime(t.completed_at)
                : formatDateTime(t.updated_at);
              return (
                <div
                  key={t.id}
                  className={`history-grid__row ${selected.has(t.id) ? "is-selected" : ""}`}
                >
                  <div className="history-grid__check">
                    <input
                      type="checkbox"
                      checked={selected.has(t.id)}
                      onChange={() => toggleOne(t.id)}
                      aria-label={`Select ${dispName}`}
                    />
                  </div>
                  <div className="history-grid__name" title={t.url}>
                    {dispName}
                  </div>
                  <div className="history-grid__size">{formatBytes(t.total_size)}</div>
                  <div className="history-grid__status">
                    <span className={"status-pill status-pill--" + t.status}>
                      {statusLabel(t.status)}
                    </span>
                  </div>
                  <div className="history-grid__cat">{t.category || "—"}</div>
                  <div className="history-grid__queue">{t.queue || "—"}</div>
                  <div className="history-grid__date">{addedDate}</div>
                  <div className="history-grid__date">{completedDate}</div>
                  <div className="history-grid__src" title={t.url}>
                    {domainOf(t.url)}
                  </div>
                  <div className="history-grid__folder" title={t.save_dir}>
                    {t.save_dir || "—"}
                  </div>
                  <div className="history-grid__actions">
                    <button
                      className="btn btn--ghost btn--sm"
                      title="Open file"
                      onClick={() => openFile(t)}
                      disabled={t.status !== "completed"}
                    >
                      Open
                    </button>
                    <button
                      className="btn btn--ghost btn--sm"
                      title="Open containing folder"
                      onClick={() => openFolder(t)}
                    >
                      Folder
                    </button>
                    <button
                      className="btn btn--ghost btn--sm"
                      title="Redownload (re-queue with same options)"
                      onClick={() => redownloadOne(t)}
                    >
                      Re-DL
                    </button>
                    <button
                      className="btn btn--ghost btn--sm"
                      title="Copy URL"
                      onClick={() => copyUrl(t)}
                    >
                      URL
                    </button>
                    <button
                      className="btn btn--ghost btn--sm"
                      title="Copy absolute path to file"
                      onClick={() => copyPath(t)}
                    >
                      Path
                    </button>
                    <button
                      className="btn btn--ghost btn--sm"
                      title="Remove from history (keeps file on disk)"
                      onClick={() => hideOne(t)}
                    >
                      Hide
                    </button>
                    <button
                      className="btn btn--ghost btn--sm btn--danger"
                      title="Delete file from disk and remove from history"
                      onClick={() => deleteOne(t)}
                    >
                      Delete
                    </button>
                  </div>
                </div>
              );
            })}
          </div>
        </div>
      </div>
    </Dialog>
  );
}
