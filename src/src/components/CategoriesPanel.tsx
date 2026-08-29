import React from "react";
import type { DownloadsStore } from "../store/useDownloads";
import type { Category } from "../types";
import { Dialog, Field, Btn } from "./Dialog";
import * as api from "../lib/api";

type Props = { store: DownloadsStore; onClose: () => void };

export function CategoriesPanel({ store, onClose }: Props) {
  const [list, setList] = React.useState<Category[]>(store.categories);
  const [edit, setEdit] = React.useState<Category | null>(null);
  const [creating, setCreating] = React.useState(false);
  React.useEffect(() => { api.listCategories().then(setList); }, []);

  const refresh = () => api.listCategories().then(setList);

  const blank = (): Category => ({
    name: "",
    label: "",
    extensions: "",
    save_dir: "",
    queue: "general",
    color: "#64748b",
    builtin: false,
  });

  const save = async (c: Category) => {
    if (!c.name.trim()) return;
    await api.saveCategory({ ...c, label: c.label || c.name });
    setEdit(null);
    setCreating(false);
    await refresh();
    await store.refresh();
  };
  const remove = async (name: string) => {
    await api.deleteCategory(name);
    await refresh();
  };

  const editing = edit ?? (creating ? blank() : null);

  return (
    <Dialog title="Categories" onClose={onClose} width={600}
      footer={editing ? undefined : <Btn variant="ghost" onClick={onClose}>Close</Btn>}>
      {!editing && (
        <div className="panel">
          <div className="panel__list">
            {list.map((c) => (
              <div key={c.name} className="panel__row">
                <div className="panel__row-main">
                  <span className="side-row__dot" style={{ background: c.color ?? "#64748b" }} />
                  <span className="panel__row-title">{c.label || c.name}</span>
                  <span className="panel__row-sub">{c.extensions || "(no extensions)"} → {c.save_dir || "default"}</span>
                </div>
                <div className="panel__row-actions">
                  {!c.builtin && <button className="btn btn--default btn--sm" onClick={() => setEdit({ ...c })}>Edit</button>}
                  {!c.builtin && <button className="btn btn--ghost btn--sm" onClick={() => remove(c.name)}>Delete</button>}
                </div>
              </div>
            ))}
          </div>
          <button className="btn btn--primary btn--sm" onClick={() => setCreating(true)}>+ Add Category</button>
        </div>
      )}
      {editing && (
        <div className="form">
          <div className="form-grid">
            <Field label="Name (id)">
              <input className="input" value={editing.name} disabled={!creating}
                onChange={(e) => setEdit({ ...editing, name: e.target.value })} placeholder="ebooks" />
            </Field>
            <Field label="Label">
              <input className="input" value={editing.label} onChange={(e) => setEdit({ ...editing, label: e.target.value })} />
            </Field>
            <Field label="Extensions (comma-separated)">
              <input className="input" value={editing.extensions} onChange={(e) => setEdit({ ...editing, extensions: e.target.value })} placeholder=".epub,.mobi" />
            </Field>
            <Field label="Save folder">
              <input className="input" value={editing.save_dir} onChange={(e) => setEdit({ ...editing, save_dir: e.target.value })} />
            </Field>
            <Field label="Queue">
              <select className="select" value={editing.queue} onChange={(e) => setEdit({ ...editing, queue: e.target.value })}>
                <option value="general">general</option>
                {store.queues.filter((q) => q.name !== "general").map((q) => (
                  <option key={q.name} value={q.name}>{q.name}</option>
                ))}
              </select>
            </Field>
            <Field label="Color">
              <input type="color" className="input input--color" value={editing.color ?? "#64748b"} onChange={(e) => setEdit({ ...editing, color: e.target.value })} />
            </Field>
          </div>
          <div className="dialog-actions">
            <Btn variant="ghost" onClick={() => { setEdit(null); setCreating(false); }}>Cancel</Btn>
            <Btn variant="primary" onClick={() => save(editing)} disabled={!editing.name.trim()}>Save</Btn>
          </div>
        </div>
      )}
    </Dialog>
  );
}
