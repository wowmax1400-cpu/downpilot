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
  // Mirror/multi-source: one alternate URL per line. Each is tried in
  // order if the primary URL fails with a retryable error.
  const [mirrors, setMirrors] = React.useState("");
  const [err, setErr] = React.useState("");
  const [probing, setProbing] = React.useState(false);
  const [probe, setProbe] = React.useState<api.ProbeInfo | null>(null);
  const [probeError, setProbeError] = React.useState<string | null>(null);

  const queues = [
    "general",
    ...store.queues.filter((q) => q.name !== "general").map((q) => q.name),
  ];
  // Fallback category list if store.categories is empty
  const defaultCategories: Category[] = React.useMemo(() => [
    { name: "compressed", label: "Compressed", extensions: ".zip,.rar,.7z,.tar,.gz,.xz,.bz2,.zst,.iso", save_dir: "", queue: "", color: "#60a5fa", builtin: true },
    { name: "documents", label: "Documents", extensions: ".pdf,.doc,.docx,.xls,.xlsx,.ppt,.pptx,.txt,.md,.odt,.epub,.csv", save_dir: "", queue: "", color: "#34d399", builtin: true },
    { name: "music", label: "Music", extensions: ".mp3,.flac,.wav,.aac,.ogg,.opus,.m4a,.wma", save_dir: "", queue: "", color: "#f472b6", builtin: true },
    { name: "video", label: "Video", extensions: ".mp4,.mkv,.webm,.avi,.mov,.wmv,.flv,.m4v,.ts,.mpg", save_dir: "", queue: "", color: "#a78bfa", builtin: true },
    { name: "programs", label: "Programs", extensions: ".exe,.msi,.deb,.rpm,.appimage,.apk,.dmg,.run,.sh,.flatpak,.flatpakref", save_dir: "", queue: "", color: "#fb923c", builtin: true },
    { name: "other", label: "Other", extensions: "", save_dir: "", queue: "", color: "#94a3b8", builtin: true },
  ], []);

  const allCategories = React.useMemo<Category[]>(() => {
    const list = store.categories.length > 0 ? store.categories : defaultCategories;
    const seen = new Set<string>();
    const out: Category[] = [];
    for (const c of list) {
      if (!seen.has(c.name)) {
        seen.add(c.name);
        out.push(c);
      }
    }
    if (!seen.has("other")) {
      out.push({ name: "other", label: "Other", extensions: "", save_dir: "", queue: "", color: "#94a3b8", builtin: true });
    }
    return out;
  }, [store.categories, defaultCategories]);

  React.useEffect(() => {
    setProbe(null);
    setProbeError(null);
    const trimmed = url.trim();
    if (trimmed) {
      const guess = filenameFromUrl(trimmed);
      if (guess) {
        setName(guess);
        const cat = guessCategory(guess);
        setCategory(cat);
      }
    } else {
      setName("");
      setCategory("other");
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
        mirrors: mirrors
          .split("\n")
          .map((m) => m.trim())
          .filter((m) => m && m !== url.trim()),
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
      width={600}
      footer={
        <>
          <Btn variant="ghost" onClick={onClose}>Cancel</Btn>
          <Btn variant="primary" onClick={submit}>Download</Btn>
        </>
      }
    >
      <div className="flex flex-col gap-2">
        {/* File Information Section */}
        <fieldset className="groupbox">
          <legend>File Information</legend>
          <div className="form-grid-kv">
            <dt>Address (URL)</dt>
            <dd>
              <textarea
                className="input input--area w-full"
                rows={2}
                placeholder="https://example.com/file.zip"
                value={url}
                onChange={(e) => setUrl(e.target.value)}
                onBlur={handleProbe}
                autoFocus
              />
            </dd>

            <dt></dt>
            <dd>
              <div className="probe-row">
                <button
                  type="button"
                  className="btn btn--sm"
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
            </dd>

            <dt>Save as</dt>
            <dd>
              <input
                className="input w-full"
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="(auto from URL)"
              />
            </dd>

            <dt>Save to</dt>
            <dd>
              <input
                className="input w-full"
                value={dir}
                onChange={(e) => setDir(e.target.value)}
                placeholder="Downloads folder"
              />
            </dd>

            <dt>Category</dt>
            <dd>
              <select
                className="select w-full"
                value={category}
                onChange={(e) => setCategory(e.target.value)}
              >
                {allCategories.map((c) => (
                  <option key={c.name} value={c.name}>
                    {c.label || c.name}
                  </option>
                ))}
              </select>
            </dd>
          </div>
        </fieldset>

        {/* Download Options Section */}
        <fieldset className="groupbox">
          <legend>Download Options</legend>
          <div className="form-grid-kv">
            <dt>Queue</dt>
            <dd>
              <select
                className="select w-full"
                value={queue}
                onChange={(e) => setQueue(e.target.value)}
              >
                {queues.map((q) => (
                  <option key={q} value={q}>
                    {q}
                  </option>
                ))}
              </select>
            </dd>

            <dt>Connections</dt>
            <dd>
              <input
                type="number"
                className="input w-24"
                min={1}
                max={store.settings?.max_threads ?? 16}
                value={connections}
                onChange={(e) => setConnections(Math.max(1, Number(e.target.value)))}
              />
            </dd>

            <dt>Description</dt>
            <dd>
              <input
                className="input w-full"
                value={description}
                onChange={(e) => setDescription(e.target.value)}
                placeholder="optional"
              />
            </dd>

            <dt>Mirrors</dt>
            <dd>
              <textarea
                className="input input--area w-full"
                rows={2}
                placeholder="https://mirror.example.com/file.zip (one per line)"
                value={mirrors}
                onChange={(e) => setMirrors(e.target.value)}
              />
            </dd>

            <dt></dt>
            <dd>
              <label className="check">
                <input
                  type="checkbox"
                  checked={startNow}
                  onChange={(e) => setStartNow(e.target.checked)}
                />
                <span>Start download immediately</span>
              </label>
            </dd>
          </div>
        </fieldset>

        {err ? <div className="form-error">{err}</div> : null}
      </div>
    </Dialog>
  );
}
