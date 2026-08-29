import React from "react";
import type { DownloadsStore } from "../store/useDownloads";
import type { Category } from "../types";
import { Dialog, Field, Btn } from "./Dialog";
import { formatBytes, filenameFromUrl } from "../lib/format";

type Props = {
  store: DownloadsStore;
  /** Pre-populated URL list (used by drag/drop, clipboard, etc.). */
  initialUrls?: string[];
  onClose: () => void;
};

type Row = {
  url: string;
  filename: string;
  size: number;
  content_type: string | null;
  category: string;
  queue: string;
  save_dir: string;
  selected: boolean;
  probing?: boolean;
  probeError?: string;
};

const DEFAULT_CAT = "other";
const DEFAULT_QUEUE = "general";

function inferCategory(url: string): string {
  const name = filenameFromUrl(url).toLowerCase();
  if (/\.(zip|7z|rar|tar|gz|bz2|xz|zst|iso)$/.test(name)) return "compressed";
  if (/\.(mp4|mkv|webm|avi|mov|wmv|flv|m4v|ts|mpg)$/.test(name)) return "video";
  if (/\.(mp3|flac|wav|ogg|m4a|aac|opus)$/.test(name)) return "music";
  if (/\.(pdf|epub|doc|docx|txt|md)$/.test(name)) return "documents";
  if (/\.(exe|msi|deb|rpm|appimage|apk|dmg|run|sh)$/.test(name)) return "programs";
  return DEFAULT_CAT;
}

export function BatchDialog({ store, initialUrls, onClose }: Props) {
  const [text, setText] = React.useState(
    () => (initialUrls ?? []).join("\n")
  );
  const [search, setSearch] = React.useState("");
  const [sortKey, setSortKey] = React.useState<"name" | "size" | "cat">("name");
  const [sortDir, setSortDir] = React.useState<"asc" | "desc">("asc");
  const [rows, setRows] = React.useState<Row[]>(() =>
    (initialUrls ?? []).map<Row>((u) => ({
      url: u,
      filename: filenameFromUrl(u),
      size: 0,
      content_type: null,
      category: inferCategory(u),
      queue: DEFAULT_QUEUE,
      save_dir: "",
      selected: true,
    }))
  );
  const [commonCat, setCommonCat] = React.useState<string>("");
  const [commonQueue, setCommonQueue] = React.useState<string>("");
  const [commonDir, setCommonDir] = React.useState<string>("");
  const [probeOn, setProbeOn] = React.useState(true);
  const [started, setStarted] = React.useState(0);

  const cats = store.categories ?? [];
  const queues = store.queues ?? [];

  /** Rebuild the row list from the textarea. */
  const rebuildFromText = (txt: string) => {
    const urls = txt.split(/\r?\n/).map((s) => s.trim()).filter(Boolean);
    setRows((prev) => {
      const next: Row[] = urls.map((u) => {
        const existing = prev.find((r) => r.url === u);
        if (existing) return { ...existing, selected: true };
        return {
          url: u,
          filename: filenameFromUrl(u),
          size: 0,
          content_type: null,
          category: inferCategory(u),
          queue: DEFAULT_QUEUE,
          save_dir: "",
          selected: true,
        };
      });
      return next;
    });
  };

  React.useEffect(() => {
    rebuildFromText(text);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [text]);

  // File import: pick a .txt / .list / no-extension file and load the
  // URLs from it. The file is read in the renderer (Tauri browsers
  // don't expose a /api/file API in the same way a real browser
  // does; this is the simplest path).
  const onPickFile = () => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = ".txt,.list,.urls,text/plain";
    input.onchange = async () => {
      const f = input.files?.[0];
      if (!f) return;
      const buf = await f.text();
      // Append to the textarea so the user can still edit before
      // clicking "Add".
      setText((prev) => (prev ? prev + "\n" + buf : buf));
    };
    input.click();
  };

  // Clipboard paste: read the system clipboard, then merge its URLs
  // into the textarea. Falls back to prompting the user when
  // navigator.clipboard isn't available (e.g. older webviews).
  const onPasteClipboard = async () => {
    try {
      if (navigator.clipboard?.readText) {
        const txt = await navigator.clipboard.readText();
        if (txt) setText((prev) => (prev ? prev + "\n" + txt : txt));
        return;
      }
    } catch {
      // fall through
    }
    // Fallback: prompt the user to paste manually.
    const v = window.prompt("Paste URLs (one per line):");
    if (v) setText((prev) => (prev ? prev + "\n" + v : v));
  };

  // Filtered + sorted view of the rows.
  const visible = React.useMemo(() => {
    const q = search.trim().toLowerCase();
    let list = rows;
    if (q) {
      list = list.filter(
        (r) =>
          r.filename.toLowerCase().includes(q) ||
          r.url.toLowerCase().includes(q) ||
          r.category.toLowerCase().includes(q),
      );
    }
    const dir = sortDir === "asc" ? 1 : -1;
    return [...list].sort((a, b) => {
      const av =
        sortKey === "name" ? a.filename.toLowerCase() : sortKey === "size" ? a.size : a.category;
      const bv =
        sortKey === "name" ? b.filename.toLowerCase() : sortKey === "size" ? b.size : b.category;
      if (av < bv) return -1 * dir;
      if (av > bv) return 1 * dir;
      return 0;
    });
  }, [rows, search, sortKey, sortDir]);

  const setSort = (k: "name" | "size" | "cat") => {
    if (sortKey === k) setSortDir(sortDir === "asc" ? "desc" : "asc");
    else {
      setSortKey(k);
      setSortDir("asc");
    }
  };
  const sortArrow = (k: "name" | "size" | "cat") =>
    sortKey === k ? (sortDir === "asc" ? " ▲" : " ▼") : "";

  /** Probe each row's URL and fill in filename/size/type. Bounded
   *  concurrency (4) so we don't hammer the server with 1000 HEADs. */
  React.useEffect(() => {
    if (!probeOn) return;
    const need = rows.filter((r) => r.selected && !r.probing && r.size === 0 && !r.probeError);
    if (need.length === 0) return;
    let cancelled = false;
    const run = async () => {
      const pool = need.slice(0, 16);
      setRows((rs) =>
        rs.map((r) => (pool.some((p) => p.url === r.url) ? { ...r, probing: true } : r))
      );
      const tasks = pool.map(async (r) => {
        try {
          const info = await store.probeUrl(r.url);
          if (cancelled) return;
          setRows((rs) =>
            rs.map((x) =>
              x.url === r.url
                ? {
                    ...x,
                    probing: false,
                    filename: info.filename || x.filename,
                    size: info.size ?? x.size,
                    content_type: info.content_type ?? x.content_type,
                  }
                : x
            )
          );
        } catch (e) {
          if (cancelled) return;
          setRows((rs) =>
            rs.map((x) =>
              x.url === r.url ? { ...x, probing: false, probeError: String(e) } : x
            )
          );
        }
      });
      await Promise.allSettled(tasks);
    };
    void run();
    return () => {
      cancelled = true;
    };
  }, [rows, probeOn, store]);

  const setRow = (idx: number, patch: Partial<Row>) =>
    setRows((rs) => rs.map((r, i) => (i === idx ? { ...r, ...patch } : r)));

  const removeRow = (idx: number) =>
    setRows((rs) => rs.filter((_, i) => i !== idx));

  const selectAll = () => setRows((rs) => rs.map((r) => ({ ...r, selected: true })));
  const deselectAll = () => setRows((rs) => rs.map((r) => ({ ...r, selected: false })));

  /** Apply common category / queue / dir to every selected row. */
  const applyCommon = () => {
    setRows((rs) =>
      rs.map((r) => {
        if (!r.selected) return r;
        let dir = r.save_dir;
        if (commonDir) dir = commonDir;
        else if (!dir && commonCat) {
          const cat = cats.find((c) => c.name === commonCat);
          if (cat && cat.save_dir) dir = cat.save_dir;
        }
        return {
          ...r,
          category: commonCat || r.category,
          queue: commonQueue || r.queue,
          save_dir: dir,
        };
      })
    );
  };

  const startSelected = async () => {
    let count = 0;
    for (const r of rows) {
      if (!r.selected) continue;
      await store.add({
        url: r.url,
        filename: r.filename || null,
        save_dir: r.save_dir || null,
        category: r.category,
        queue: r.queue,
        start_immediately: true,
      });
      count++;
    }
    setStarted(count);
  };

  const selectedCount = rows.filter((r) => r.selected).length;

  return (
    <Dialog
      title="Batch Download"
      onClose={onClose}
      width={780}
      footer={
        <>
          <Btn variant="ghost" onClick={onClose}>Close</Btn>
          <Btn variant="primary" onClick={startSelected} disabled={selectedCount === 0}>
            Add {selectedCount} download{selectedCount === 1 ? "" : "s"}
         </Btn>
        </>
      }
    >
      <div className="form">
        <Field label="URL list (one per line)">
          <textarea
            className="input input--area"
            rows={4}
            placeholder={"https://example.com/file-001.zip\nhttps://example.com/file-002.zip\n..."}
            value={text}
            onChange={(e) => setText(e.target.value)}
          />
       </Field>
        <div className="form-grid form-grid--3">
          <Field label="Set category for selected">
            <select className="select" value={commonCat} onChange={(e) => setCommonCat(e.target.value)}>
              <option value="">(keep</option>
              <option value={DEFAULT_CAT}>other</option>
              {cats.map((c) => <option key={c.name} value={c.name}>{c.label || c.name}</option>)}
           </select>
         </Field>
          <Field label="Set queue for selected">
            <select className="select" value={commonQueue} onChange={(e) => setCommonQueue(e.target.value)}>
              <option value="">(keep</option>
              <option value={DEFAULT_QUEUE}>general</option>
              {queues.filter((q) => q.name !== "general").map((q) => (
                <option key={q.name} value={q.name}>{q.label || q.name}</option>
              ))}
           </select>
         </Field>
          <Field label="Set save folder for selected">
            <input
              className="input"
              value={commonDir}
              onChange={(e) => setCommonDir(e.target.value)}
              placeholder="(keep / use category)"
            />
         </Field>
       </div>
        <div className="batch-toolbar">
          <button className="btn btn--default btn--sm" onClick={selectAll}>Select all</button>
          <button className="btn btn--default btn--sm" onClick={deselectAll}>Deselect all</button>
          <button className="btn btn--default btn--sm" onClick={applyCommon}>Apply</button>
          <button className="btn btn--default btn--sm" onClick={onPickFile} title="Import URLs from a .txt / .list file">
            Import file…
          </button>
          <button className="btn btn--default btn--sm" onClick={onPasteClipboard} title="Paste URLs from the clipboard">
            Paste clipboard
          </button>
          <label className="check" style={{ marginLeft: "auto" }}>
            <input type="checkbox" checked={probeOn} onChange={(e) => setProbeOn(e.target.checked)} />
            Probe server for size / filename
         </label>
        </div>
        <div className="batch-toolbar" style={{ marginTop: 4 }}>
          <input
            className="input input--sm"
            style={{ width: 220 }}
            placeholder="Filter filename / URL / category"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
          <button
            className={"btn btn--default btn--xs" + (sortKey === "name" ? " active" : "")}
            onClick={() => setSort("name")}
            title="Sort by filename"
          >
            Name{sortArrow("name")}
          </button>
          <button
            className={"btn btn--default btn--xs" + (sortKey === "size" ? " active" : "")}
            onClick={() => setSort("size")}
            title="Sort by size"
          >
            Size{sortArrow("size")}
          </button>
          <button
            className={"btn btn--default btn--xs" + (sortKey === "cat" ? " active" : "")}
            onClick={() => setSort("cat")}
            title="Sort by category"
          >
            Category{sortArrow("cat")}
          </button>
          <span className="batch-count" style={{ marginLeft: "auto" }}>
            {visible.length} of {rows.length} shown
          </span>
        </div>

        <div className="batch-table">
          <div className="batch-table__head">
            <div className="batch-cell batch-cell--sel"></div>
            <div className="batch-cell batch-cell--name">Filename</div>
            <div className="batch-cell batch-cell--size">Size</div>
            <div className="batch-cell batch-cell--type">Type</div>
            <div className="batch-cell batch-cell--cat">Category</div>
            <div className="batch-cell batch-cell--q">Queue</div>
            <div className="batch-cell batch-cell--dir">Save to</div>
            <div className="batch-cell batch-cell--rm"></div>
         </div>
          <div className="batch-table__body">
            {visible.length === 0 && (
              <div className="batch-empty">Add URLs above to preview them here</div>
            )}
            {visible.map((r) => {
              const i = rows.indexOf(r);
              return (
                <div key={`${r.url}-${i}`} className={"batch-row" + (r.selected ? "" : " batch-row--off")}>
                  <div className="batch-cell batch-cell--sel">
                    <input
                      type="checkbox"
                      checked={r.selected}
                      onChange={(e) => setRow(i, { selected: e.target.checked })}
                    />
                 </div>
                  <div className="batch-cell batch-cell--name" title={r.url}>
                    <input
                      className="input input--bare"
                      value={r.filename}
                      onChange={(e) => setRow(i, { filename: e.target.value })}
                    />
                    {r.probing && <span className="batch-pill">probing</span>}
                    {r.probeError && <span className="batch-pill batch-pill--err" title={r.probeError}></span>}
                 </div>
                  <div className="batch-cell batch-cell--size">{r.size ? formatBytes(r.size) : "—"}</div>
                  <div className="batch-cell batch-cell--type">{r.content_type ?? "—"}</div>
                  <div className="batch-cell batch-cell--cat">
                    <select className="select select--bare" value={r.category} onChange={(e) => setRow(i, { category: e.target.value })}>
                      <option value={DEFAULT_CAT}>other</option>
                      {cats.map((c) => <option key={c.name} value={c.name}>{c.label || c.name}</option>)}
                 </select>
               </div>
                  <div className="batch-cell batch-cell--q">
                    <select className="select select--bare" value={r.queue} onChange={(e) => setRow(i, { queue: e.target.value })}>
                      <option value={DEFAULT_QUEUE}>general</option>
                      {queues.filter((q) => q.name !== "general").map((q) => (
                        <option key={q.name} value={q.name}>{q.label || q.name}</option>
                      ))}
                 </select>
               </div>
                  <div className="batch-cell batch-cell--dir">
                    <input
                      className="input input--bare"
                      value={r.save_dir}
                      onChange={(e) => setRow(i, { save_dir: e.target.value })}
                      placeholder="(default)"
                    />
                 </div>
                  <div className="batch-cell batch-cell--rm">
                    <button className="btn btn--ghost btn--sm" title="Remove row" onClick={() => removeRow(i)}>x</button>
                 </div>
               </div>
              );
            })}
         </div>
       </div>
        {started > 0 && <div className="form-ok">Added {started} downloads</div>}
     </div>
   </Dialog>
  );
}
