import React from "react";
import type { DownloadsStore } from "../store/useDownloads";
import type { SiteRule } from "../types";
import { Dialog, Field, Btn } from "./Dialog";
import * as api from "../lib/api";

type Props = { store: DownloadsStore; onClose: () => void };

export function SitesPanel({ store, onClose }: Props) {
  const [list, setList] = React.useState<SiteRule[]>(store.sites);
  const [edit, setEdit] = React.useState<SiteRule | null>(null);
  const [creating, setCreating] = React.useState(false);
  React.useEffect(() => { api.listSites().then(setList); }, []);

  const refresh = () => api.listSites().then(setList);

  const blank = (): SiteRule => ({
    domain: "",
    queue: "general",
    save_dir: "",
    auth_ref: "",
    proxy_url: "",
    auto_capture: false,
    max_connections: 0,
    bandwidth_kbps: 0,
    notes: "",
  });

  const save = async (s: SiteRule) => {
    if (!s.domain.trim()) return;
    await api.saveSite(s);
    setEdit(null);
    setCreating(false);
    await refresh();
    await store.refresh();
  };
  const remove = async (domain: string) => {
    await api.deleteSite(domain);
    await refresh();
  };

  const editing = edit ?? (creating ? blank() : null);

  return (
    <Dialog title="Site Manager" onClose={onClose} width={620}
      footer={editing ? undefined : <Btn variant="ghost" onClick={onClose}>Close</Btn>}>
      {!editing && (
        <div className="panel">
          <div className="panel__list">
            {list.map((s) => (
              <div key={s.domain} className="panel__row">
                <div className="panel__row-main">
                  <span className="panel__row-title">{s.domain}</span>
                  <span className="panel__row-sub">
                    queue: {s.queue} · {s.save_dir || "default folder"} · {s.auto_capture ? "auto-capture" : "manual"}
                  </span>
                </div>
                <div className="panel__row-actions">
                  <button className="btn btn--default btn--sm" onClick={() => setEdit({ ...s })}>Edit</button>
                  <button className="btn btn--ghost btn--sm" onClick={() => remove(s.domain)}>Delete</button>
                </div>
              </div>
            ))}
            {list.length === 0 && <div className="panel__empty">No site rules yet.</div>}
          </div>
          <button className="btn btn--primary btn--sm" onClick={() => setCreating(true)}>+ Add Site Rule</button>
        </div>
      )}
      {editing && (
        <div className="form">
          <div className="form-grid">
            <Field label="Domain">
              <input className="input" value={editing.domain} disabled={!creating}
                onChange={(e) => setEdit({ ...editing, domain: e.target.value })} placeholder="github.com" />
            </Field>
            <Field label="Queue">
              <select className="select" value={editing.queue} onChange={(e) => setEdit({ ...editing, queue: e.target.value })}>
                <option value="general">general</option>
                {store.queues.filter((q) => q.name !== "general").map((q) => (
                  <option key={q.name} value={q.name}>{q.name}</option>
                ))}
              </select>
            </Field>
            <Field label="Save folder">
              <input className="input" value={editing.save_dir} onChange={(e) => setEdit({ ...editing, save_dir: e.target.value })} />
            </Field>
            <Field label="Proxy URL">
              <input className="input" value={editing.proxy_url} onChange={(e) => setEdit({ ...editing, proxy_url: e.target.value })} placeholder="http://127.0.0.1:8080" />
            </Field>
            <Field label="Max connections (0=default)">
              <input type="number" className="input" value={editing.max_connections}
                onChange={(e) => setEdit({ ...editing, max_connections: Number(e.target.value) })} />
            </Field>
            <Field label="Bandwidth kbps (0=unlimited)">
              <input type="number" className="input" value={editing.bandwidth_kbps}
                onChange={(e) => setEdit({ ...editing, bandwidth_kbps: Number(e.target.value) })} />
            </Field>
          </div>
          <label className="check">
            <input type="checkbox" checked={editing.auto_capture} onChange={(e) => setEdit({ ...editing, auto_capture: e.target.checked })} />
            Automatically capture downloads from this domain
          </label>
          <div className="dialog-actions">
            <Btn variant="ghost" onClick={() => { setEdit(null); setCreating(false); }}>Cancel</Btn>
            <Btn variant="primary" onClick={() => save(editing)} disabled={!editing.domain.trim()}>Save</Btn>
          </div>
        </div>
      )}
    </Dialog>
  );
}
