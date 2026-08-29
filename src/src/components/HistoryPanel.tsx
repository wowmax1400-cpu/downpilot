import React from "react";
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

export function HistoryPanel({ store, onClose }: Props) {
  const [q, setQ] = React.useState("");
  const [status, setStatus] = React.useState<StatusFilter>("all");

  const tasks = React.useMemo(
    () => store.tasks.filter((t) => TERMINAL.has(t.status)),
    [store.tasks]
  );

  const filtered = React.useMemo(() => {
    const needle = q.trim().toLowerCase();
    return tasks.filter((t) => {
      if (status !== "all" && t.status !== status) return false;
      if (!needle) return true;
      const hay = [
        t.filename ?? "",
        t.url,
        domainOf(t.url),
        t.save_dir ?? "",
      ].join(" ").toLowerCase();
      return hay.includes(needle);
    });
  }, [tasks, q, status]);

  const counts = React.useMemo(() => {
    const c = {
      all: tasks.length,
      completed: 0,
      failed: 0,
      stopped: 0,
      cancelled: 0,
    } as Record<StatusFilter, number>;
    for (const t of tasks) {
      if (
        t.status === "completed" ||
        t.status === "failed" ||
        t.status === "stopped" ||
        t.status === "cancelled"
      ) {
        c[t.status]++;
      }
    }
    return c;
  }, [tasks]);

  const openFile = (id: number) => {
    const t = tasks.find((x) => x.id === id);
    if (t) void store.openFile(t);
  };
  const openFolder = (id: number) => {
    const t = tasks.find((x) => x.id === id);
    if (t) void api.openInFileManager(t.save_dir);
  };
  const copyUrl = (id: number) => {
    const t = tasks.find((x) => x.id === id);
    if (t) void api.copyTextToClipboard(t.url);
  };
  const redownload = (id: number) => void store.redownload(id);
  const remove = (id: number) => void store.removeKeepFile(id);
  const clearAll = () => {
    if (window.confirm("Clear all history? Files on disk are not affected.")) {
      void store.clearFinished();
    }
  };

  return (
    <Dialog
      title="Download History"
      onClose={onClose}
      width={820}
      footer={
        <>
          <Btn variant="ghost" onClick={onClose}>Close</Btn>
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
            placeholder="Search filename, URL, domain, folder..."
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
            <div>Name</div>
            <div>Size</div>
            <div>Completed</div>
            <div>Status</div>
            <div>Category</div>
            <div>Source</div>
            <div>Folder</div>
            <div>Actions</div>
        </div>
          <div className="history-grid__body">
            {filtered.length === 0 && (
              <div className="panel__empty">No history matches</div>
            )}
            {filtered.map((t) => {
              const dispName = t.filename || t.url.split("/").pop() || "—";
              const dispDate = t.completed_at
                ? formatDateTime(t.completed_at)
                : formatDateTime(t.updated_at);
              return (
                <div key={t.id} className="history-grid__row">
                  <div className="history-grid__name" title={t.url}>
                    {dispName}
                </div>
                  <div className="history-grid__size">
                    {formatBytes(t.total_size)}
                </div>
                  <div className="history-grid__date">{dispDate}</div>
                  <div className="history-grid__status">
                    <span className={"status-pill status-pill--" + t.status}>
                      {statusLabel(t.status)}
                  </span>
                </div>
                  <div className="history-grid__cat">{t.category}</div>
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
                      onClick={() => openFile(t.id)}
                      disabled={t.status !== "completed"}
                    >
                      Open
                  </button>
                    <button
                      className="btn btn--ghost btn--sm"
                      title="Open containing folder"
                      onClick={() => openFolder(t.id)}
                    >
                      Folder
                  </button>
                    <button
                      className="btn btn--ghost btn--sm"
                      title="Redownload (re-queue with same options)"
                      onClick={() => redownload(t.id)}
                    >
                      Re-DL
                  </button>
                    <button
                      className="btn btn--ghost btn--sm"
                      title="Copy URL"
                      onClick={() => copyUrl(t.id)}
                    >
                      URL
                  </button>
                    <button
                      className="btn btn--ghost btn--sm"
                      title="Remove from history (keeps file on disk)"
                      onClick={() => remove(t.id)}
                    >
                      Hide
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
