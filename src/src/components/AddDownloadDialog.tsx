import React from "react";
import type { DownloadsStore } from "../store/useDownloads";
import type { Category } from "../types";
import { Dialog, Field, Btn } from "./Dialog";
import * as api from "../lib/api";
import { formatBytes, guessCategory, filenameFromUrl } from "../lib/format";

type Props = {
  store: DownloadsStore;
  initialUrl?: string;
  initialUrls?: string[];
  onClose: () => void;
};

export function AddDownloadDialog({ store, initialUrl, initialUrls, onClose }: Props) {
  const [url, setUrl] = React.useState(initialUrl ?? initialUrls?.[0] ?? "");
  const [name, setName] = React.useState("");
  const [dir, setDir] = React.useState(store.settings?.download_dir ?? "");
  const [queue, setQueue] = React.useState("general");
  const [category, setCategory] = React.useState("other");
  const [connections, setConnections] = React.useState<number>(
    store.settings?.default_threads ?? 8
  );
  const [description, setDescription] = React.useState("");
  const [startNow, setStartNow] = React.useState(true);
  const [err, setErr] = React.useState("");
  const [probing, setProbing] = React.useState(false);
  const [probe, setProbe] = React.useState<api.ProbeInfo | null>(null);
  const [probeError, setProbeError] = React.useState<string | null>(null);

  const queues = [
    "general",
    ...store.queues.filter((q) => q.name !== "general").map((q) => q.name),
  ];
  const allCategories = React.useMemo<Category[]>(() => {
    const seen = new Set<string>();
    const out: Category[] = [];
    for (const c of store.categories) {
      if (!seen.has(c.name)) {
        seen.add(c.name);
        out.push(c);
      }
    }
    return out;
  }, [store.categories]);

  React.useEffect(() => {
    setProbe(null);
    setProbeError(null);
    if (!name) {
      const guess = filenameFromUrl(url);
      if (guess) setName(guess);
    }
    if (url) {
      const cat = guessCategory(filenameFromUrl(url));
      setCategory(cat);
    }
  }, [url]);

  React.useEffect(() => {
    const cat = allCategories.find((c) => c.name === category);
    if (cat) {
      if (cat.save_dir && (!dir || dir === (store.settings?.download_dir ?? ""))) {
        setDir(cat.save_dir);
      }
      if (cat.queue) setQueue(cat.queue);
    }
  }, [category, allCategories]);

  const handleProbe = async () => {
    if (!url.trim()) return;
    setProbing(true);
    setProbeError(null);
    try {
      const info = await store.probeUrl(url.trim());
      setProbe(info);
      if (info.filename) setName(info.filename);
      const ct = info.content_type ?? "";
      if (ct.startsWith("video/")) setCategory("video");
      else if (ct.startsWith("audio/")) setCategory("music");
      else if (ct.startsWith("image/")) setCategory("images");
      else if (ct === "application/zip" || ct.includes("compressed") || ct.includes("archive"))
        setCategory("compressed");
      else if (ct === "application/pdf" || ct.includes("officedocument") || ct.startsWith("text/"))
        setCategory("documents");
    } catch (e) {
      setProbeError(String(e));
    } finally {
      setProbing(false);
    }
  };

  const submit = async () => {
    if (!url.trim()) {
      setErr("Enter a URL to download.");
      return;
    }
    try {
      await store.add({
        url: url.trim(),
        filename: name.trim() || null,
        save_dir: dir.trim() || null,
        queue,
        category,
        max_connections: connections,
        threads: connections,
        description: description.trim() || null,
        start_immediately: startNow,
      });
      onClose();
    } catch (e) {
      setErr(String(e));
    }
  };

  return (
    <Dialog
      title="Add Download"
      onClose={onClose}
      width={620}
      footer={
        <>
          <Btn variant="ghost" onClick={onClose}>Cancel</Btn>
          <Btn variant="primary" onClick={submit}>Download</Btn>
        </>
      }
    >
      <div className="form">
        <Field label="URL">
          <textarea
            className="input input--area"
            rows={2}
            placeholder="https://example.com/file.zip"
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            onBlur={handleProbe}
            autoFocus
          />
       </Field>

        <div className="probe-row">
          <button
            type="button"
            className="btn btn--default btn--sm"
            onClick={handleProbe}
            disabled={probing || !url.trim()}
          >
            {probing ? "Probing..." : "Probe"}
         </button>
          {probe && (
            <div className="probe-row__results">
              <span className="probe-pill">
                {probe.size > 0 ? formatBytes(probe.size) : "size unknown"}
             </span>
              {probe.content_type ? (
                <span className="probe-pill">{probe.content_type}</span>
              ) : null}
              {probe.supports_resume ? (
                <span className="probe-pill probe-pill--ok">resumable</span>
              ) : null}
           </div>
          )}
          {probeError ? (
            <span className="probe-row__err">{probeError}</span>
          ) : null}
       </div>

        <div className="form-grid form-grid--3">
          <Field label="Save as">
            <input
              className="input"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="(auto from URL)"
            />
         </Field>
          <Field label="Save to">
            <input
              className="input"
              value={dir}
              onChange={(e) => setDir(e.target.value)}
              placeholder="Downloads folder"
            />
         </Field>
          <Field label="Connections">
            <input
              type="number"
              className="input"
              min={1}
              max={store.settings?.max_threads ?? 16}
              value={connections}
              onChange={(e) => setConnections(Math.max(1, Number(e.target.value)))}
            />
         </Field>
          <Field label="Category">
            <select
              className="select"
              value={category}
              onChange={(e) => setCategory(e.target.value)}
            >
              {allCategories.map((c) => (
                <option key={c.name} value={c.name}>
                  {c.label || c.name}
               </option>
              ))}
           </select>
         </Field>
          <Field label="Queue">
            <select
              className="select"
              value={queue}
              onChange={(e) => setQueue(e.target.value)}
            >
              {queues.map((q) => (
                <option key={q} value={q}>
                  {q}
               </option>
              ))}
           </select>
         </Field>
          <Field label="Description">
            <input
              className="input"
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              placeholder="optional"
            />
         </Field>
       </div>

        <label className="check">
          <input
            type="checkbox"
            checked={startNow}
            onChange={(e) => setStartNow(e.target.checked)}
          />
          Start download immediately
       </label>
        {err ? <div className="form-error">{err}</div> : null}
     </div>
   </Dialog>
  );
}
