// IDM-style Properties window for a single download. All values come
// straight from the live task in the store; the few editable fields
// (Description, Save path) save on Apply and refresh the store.

import React, { useState } from "react";
import type { DownloadsStore } from "../store/useDownloads";
import { Dialog, Field, Btn } from "./Dialog";
import {
  formatBytes,
  formatSpeed,
  formatEta,
  formatPercent,
  formatDate,
  formatDateTime,
  formatDuration,
  domainOf,
} from "../lib/format";

type Props = { store: DownloadsStore; id: number; onClose: () => void };

const TABS = ["General", "Transfer", "Source", "Network", "Timestamps", "Advanced"] as const;
type Tab = (typeof TABS)[number];

export function PropertiesDialog({ store, id, onClose }: Props) {
  const task = store.tasks.find((t) => t.id === id) ?? null;
  const [tab, setTab] = useState<Tab>("General");
  const [desc, setDesc] = useState(task?.description ?? "");
  const [saveDir, setSaveDir] = useState(task?.save_dir ?? "");

  if (!task) return null;

  const remaining = task.total_size - task.downloaded;
  const elapsed =
    task.started_at && !task.completed_at
      ? Math.max(0, Math.floor(Date.now() / 1000) - task.started_at)
      : task.started_at && task.completed_at
      ? Math.max(0, task.completed_at - task.started_at)
      : null;

  const dirty = desc !== (task.description ?? "") || saveDir !== task.save_dir;

  const apply = async () => {
    try {
      if (desc !== (task.description ?? "")) {
        await store.setDescription(task.id, desc);
      }
      if (saveDir !== task.save_dir) {
        await store.setSaveDir(task.id, saveDir);
      }
    } catch {
      /* ignore */
    }
  };

  return (
    <Dialog
      title={`Properties — ${task.filename || task.url}`}
      onClose={onClose}
      width={680}
      footer={
        <>
          {dirty && <span className="text-xs text-txt-dim">Unsaved changes</span>}
          <Btn variant="ghost" onClick={onClose}>Close</Btn>
          <Btn variant="primary" onClick={apply} disabled={!dirty}>Apply</Btn>
        </>
      }
    >
      <div className="tabs">
        {TABS.map((t) => (
          <button key={t} className={`tab ${tab === t ? "active" : ""}`} onClick={() => setTab(t)}>
            {t}
          </button>
        ))}
      </div>
      <div className="tab-body">
        {tab === "General" && (
          <dl className="kv">
            <Row k="File name" v={task.filename || "(unknown)"} />
            <Row k="Save path" v={task.save_dir} />
            <Row k="Description" v={desc} editable onChange={setDesc} />
            <Row k="Category" v={task.category} />
            <Row k="Queue" v={task.queue} />
            <Row k="Site" v={task.site_id || "—"} />
            <Row k="File size" v={formatBytes(task.total_size)} />
            <Row k="Downloaded" v={formatBytes(task.downloaded)} />
            <Row k="Remaining" v={formatBytes(remaining)} />
            <Row k="Progress" v={formatPercent(task.downloaded, task.total_size)} />
            <Row k="Status" v={task.status} />
            {task.error && <Row k="Error" v={task.error} />}
          </dl>
        )}

        {tab === "Transfer" && (
          <dl className="kv">
            <Row k="Current speed" v={task.status === "downloading" ? formatSpeed(task.speed_bps) : "—"} />
            <Row k="Average speed" v={task.avg_speed_bps > 0 ? formatSpeed(task.avg_speed_bps) : "—"} />
            <Row k="Peak speed" v={task.peak_speed_bps > 0 ? formatSpeed(task.peak_speed_bps) : "—"} />
            <Row
              k="Time remaining"
              v={task.status === "downloading" && remaining > 0 ? formatEta(remaining, task.speed_bps) : "—"}
            />
            <Row k="Time elapsed" v={elapsed != null ? formatDuration(elapsed) : "—"} />
            <Row k="Connections" v={String(task.threads || 1)} />
            <Row k="Segments" v={String(task.segments || 0)} />
            <Row k="Retry count" v={String(task.retry_count)} />
            <Row k="Resume support" v={task.resume_support ? "Yes" : "No"} />
          </dl>
        )}

        {tab === "Source" && (
          <dl className="kv">
            <Row k="Original URL" v={task.url} />
            <Row k="Referrer" v={task.referer ?? "—"} />
            <Row k="Domain" v={domainOf(task.url)} />
            <Row k="HTTP status" v={task.http_status ? String(task.http_status) : "—"} />
            <Row k="Content type" v={task.content_type ?? "—"} />
            <Row k="Webpage URL" v={task.webpage_url ?? "—"} />
            <Row k="Format" v={task.format_id ?? "—"} />
          </dl>
        )}

        {tab === "Network" && (
          <dl className="kv">
            <Row k="Proxy" v={task.cookies || task.user_agent ? "Configured" : "—"} />
            <Row k="Connection mode" v={task.threads > 1 ? "Segmented" : "Single"} />
            <Row k="User agent" v={task.user_agent ?? "(default)"} />
            <Row k="Authentication" v={task.cookies || task.site_id ? "Provided" : "None"} />
          </dl>
        )}

        {tab === "Timestamps" && (
          <dl className="kv">
            <Row k="Added" v={formatDateTime(task.created_at)} />
            <Row k="Started" v={task.started_at ? formatDateTime(task.started_at) : "—"} />
            <Row k="Last resumed" v={task.last_resumed_at ? formatDateTime(task.last_resumed_at) : "—"} />
            <Row k="Completed" v={task.completed_at ? formatDateTime(task.completed_at) : "—"} />
            <Row k="Last update" v={formatDate(task.updated_at)} />
          </dl>
        )}

        {tab === "Advanced" && (
          <dl className="kv">
            <Row k="Max connections" v={String(task.threads)} />
            <Row k="Retry behavior" v={`${task.max_retries} retries, ${task.retry_delay_ms}ms delay`} />
            <Row k="Conn timeout" v={`${task.conn_timeout_ms} ms`} />
            <Row k="Read timeout" v={`${task.read_timeout_ms} ms`} />
            <Row k="Checksum algorithm" v={task.checksum_algorithm || "—"} />
            <Row k="Checksum" v={task.checksum_value || "—"} />
            <Row k="Checksum verified" v={task.checksum_verified ? "✓ Yes" : "—"} />
            <Row k="Error category" v={task.error_category || "—"} />
            <Row k="Error code" v={task.error_code ? String(task.error_code) : "—"} />
            <Row k="On complete" v={task.on_complete || "(none)"} />
            <Row k="Save path (edit)" v={saveDir} editable onChange={setSaveDir} />
          </dl>
        )}
      </div>
    </Dialog>
  );
}

function Row({
  k,
  v,
  editable,
  onChange,
}: {
  k: string;
  v: string;
  editable?: boolean;
  onChange?: (s: string) => void;
}) {
  if (editable) {
    return (
      <div className="kv__row">
        <dt>{k}</dt>
        <dd><input className="input input--inline" value={v} onChange={(e) => onChange?.(e.target.value)} /></dd>
      </div>
    );
  }
  return (
    <div className="kv__row">
      <dt>{k}</dt>
      <dd title={v}>{v}</dd>
    </div>
  );
}
