import React from "react";
import type { GrabbedLink, GrabRules } from "../types";
import { Dialog, Btn, Field } from "./Dialog";
import { IconGrabber, IconSearch } from "./icons";
import { formatBytes } from "../lib/format";

type Props = {
  open: boolean;
  onClose: () => void;
  /** Returns the discovered links. The dialog also passes the
   *  user's `GrabRules` so the server applies the same filter. */
  onGrab: (url: string, rules: GrabRules, limit: number) => Promise<GrabbedLink[]>;
  /** Called with the final, category-validated list when the user
   *  clicks "Add". Each link has a `url`, `filename`, `ext`,
   *  `category`, and the user's chosen `queue` / `save_dir`. */
  onAdd: (links: { url: string; filename: string; category: string; queue: string; save_dir: string }[]) => void;
  /** Queues available — same list the BatchDialog uses. */
  queues?: { name: string; label?: string }[];
  /** Categories available — for the rule editor. */
  categories?: { name: string; label?: string }[];
};

const DEFAULT_RULES: GrabRules = {
  include_extensions: [
    "zip", "7z", "rar", "tar", "gz", "bz2", "xz", "iso",
    "mp4", "mkv", "webm", "avi", "mov",
    "mp3", "flac", "wav", "m4a", "ogg",
    "pdf", "epub", "doc", "docx", "txt",
    "exe", "msi", "apk", "deb", "rpm", "appimage", "dmg", "sh",
  ],
  exclude_extensions: [
    "css", "js", "mjs",
    "png", "jpg", "jpeg", "gif", "webp", "svg", "ico",
    "woff", "woff2", "ttf", "eot",
    "html", "htm", "json", "xml",
  ],
  include_mime: [],
  exclude_mime: ["text/css", "application/javascript"],
  exclude_domains: [],
  min_size: 0,
  max_size: 0,
};

type Row = GrabbedLink & {
  /** Per-row override queue (defaults to "general"). */
  queue: string;
  /** Per-row override save dir (defaults to "" → category default). */
  save_dir: string;
};

type SortKey = "name" | "ext" | "size" | "category";

function hostnameFromUrl(url: string): string {
  try {
    return new URL(url).hostname;
  } catch {
    return "—";
  }
}

export function SiteGrabberDialog({ open, onClose, onGrab, onAdd, queues, categories }: Props) {
  const [url, setUrl] = React.useState("");
  const [rules, setRules] = React.useState<GrabRules>(DEFAULT_RULES);
  const [showRules, setShowRules] = React.useState(false);
  const [rows, setRows] = React.useState<Row[]>([]);
  const [checked, setChecked] = React.useState<Set<number>>(new Set());
  const [grabbing, setGrabbing] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  const [search, setSearch] = React.useState("");
  const [sortKey, setSortKey] = React.useState<SortKey>("name");
  const [sortDir, setSortDir] = React.useState<"asc" | "desc">("asc");
  const [commonQueue, setCommonQueue] = React.useState<string>("");
  const [commonCat, setCommonCat] = React.useState<string>("");

  const handleGrab = async () => {
    if (!url.trim()) return;
    setGrabbing(true);
    setError(null);
    try {
      const result = await onGrab(url.trim(), rules, 200);
      const initialQueue = queues?.[0]?.name ?? "general";
      setRows(
        result.map<Row>((l) => ({
          ...l,
          queue: initialQueue,
          save_dir: "",
        })),
      );
      setChecked(new Set(result.map((_, i) => i)));
    } catch (e) {
      setError(String(e));
    } finally {
      setGrabbing(false);
    }
  };

  const toggle = (i: number) =>
    setChecked((p) => {
      const n = new Set(p);
      if (n.has(i)) n.delete(i);
      else n.add(i);
      return n;
    });

  const selectAll = () => setChecked(new Set(rows.map((_, i) => i)));
  const deselectAll = () => setChecked(new Set());

  const setRow = (idx: number, patch: Partial<Row>) =>
    setRows((rs) => rs.map((r, i) => (i === idx ? { ...r, ...patch } : r)));

  const removeRow = (idx: number) => {
    setRows((rs) => rs.filter((_, i) => i !== idx));
    setChecked((p) => {
      const n = new Set<number>();
      for (const x of p) if (x < idx) n.add(x);
      else if (x > idx) n.add(x - 1);
      return n;
    });
  };

  const applyCommon = () =>
    setRows((rs) =>
      rs.map((r) => {
        if (!checked.has(rs.indexOf(r))) return r;
        return {
          ...r,
          category: commonCat || r.category,
          queue: commonQueue || r.queue,
        };
      }),
    );

  const handleAdd = () => {
    onAdd(
      rows
        .filter((_, i) => checked.has(i))
        .map((r) => ({
          url: r.url,
          filename: r.filename,
          category: r.category ?? "other",
          queue: r.queue || "general",
          save_dir: r.save_dir,
        })),
    );
    setUrl("");
    setRows([]);
    setChecked(new Set());
    onClose();
  };

  // Filter + sort
  const visible = React.useMemo(() => {
    const q = search.trim().toLowerCase();
    let list = rows;
    if (q) {
      list = list.filter(
        (r) =>
          r.filename.toLowerCase().includes(q) ||
          r.url.toLowerCase().includes(q) ||
          (r.category ?? "").toLowerCase().includes(q),
      );
    }
    const dir = sortDir === "asc" ? 1 : -1;
    return [...list].sort((a, b) => {
      const av =
        sortKey === "name"
          ? a.filename.toLowerCase()
          : sortKey === "ext"
            ? a.ext
            : sortKey === "size"
              ? a.size
              : (a.category ?? "");
      const bv =
        sortKey === "name"
          ? b.filename.toLowerCase()
          : sortKey === "ext"
            ? b.ext
            : sortKey === "size"
              ? b.size
              : (b.category ?? "");
      if (av < bv) return -1 * dir;
      if (av > bv) return 1 * dir;
      return 0;
    });
  }, [rows, search, sortKey, sortDir]);

  const setSort = (k: SortKey) => {
    if (sortKey === k) setSortDir(sortDir === "asc" ? "desc" : "asc");
    else {
      setSortKey(k);
      setSortDir("asc");
    }
  };

  const sortArrow = (k: SortKey) =>
    sortKey === k ? (sortDir === "asc" ? " ▲" : " ▼") : "";

  if (!open) return null;

  return (
    <Dialog
      title="Download Grabber"
      onClose={onClose}
      width={840}
      footer={
        <>
          <Btn variant="ghost" onClick={onClose}>Close</Btn>
          <Btn variant="primary" onClick={handleAdd} disabled={checked.size === 0}>
            Add {checked.size} download{checked.size === 1 ? "" : "s"}
          </Btn>
        </>
      }
    >
      <div className="form">
        <div className="grab-row">
          <input
            className="input"
            placeholder="Enter page URL to scan for media/files…"
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && handleGrab()}
          />
          <button
            className="btn btn--primary btn--sm"
            onClick={handleGrab}
            disabled={grabbing || !url.trim()}
          >
            <IconSearch size={13} /> Grab
          </button>
          <button
            className="btn btn--default btn--sm"
            onClick={() => setShowRules((s) => !s)}
            title="Configure which links to keep"
          >
            {showRules ? "Hide rules" : "Rules…"}
          </button>
        </div>

        {showRules && (
          <div className="grab-rules">
            <div className="form-grid form-grid--2">
              <Field label="Include extensions (comma-separated)">
                <input
                  className="input"
                  value={rules.include_extensions.join(",")}
                  onChange={(e) =>
                    setRules({
                      ...rules,
                      include_extensions: e.target.value
                        .split(",")
                        .map((s) => s.trim().replace(/^\./, "").toLowerCase())
                        .filter(Boolean),
                    })
                  }
                  placeholder="mp4, mkv, zip"
                />
              </Field>
              <Field label="Exclude extensions">
                <input
                  className="input"
                  value={rules.exclude_extensions.join(",")}
                  onChange={(e) =>
                    setRules({
                      ...rules,
                      exclude_extensions: e.target.value
                        .split(",")
                        .map((s) => s.trim().replace(/^\./, "").toLowerCase())
                        .filter(Boolean),
                    })
                  }
                  placeholder="css, js, png"
                />
              </Field>
              <Field label="Exclude domains (hostname-aware)">
                <input
                  className="input"
                  value={rules.exclude_domains.join(",")}
                  onChange={(e) =>
                    setRules({
                      ...rules,
                      exclude_domains: e.target.value
                        .split(",")
                        .map((s) => s.trim().toLowerCase())
                        .filter(Boolean),
                    })
                  }
                  placeholder="tracker.example, ads.example"
                />
              </Field>
              <Field label="Exclude MIME prefixes">
                <input
                  className="input"
                  value={rules.exclude_mime.join(",")}
                  onChange={(e) =>
                    setRules({
                      ...rules,
                      exclude_mime: e.target.value
                        .split(",")
                        .map((s) => s.trim())
                        .filter(Boolean),
                    })
                  }
                  placeholder="text/css, application/javascript"
                />
              </Field>
            </div>
            <div className="form-grid form-grid--2">
              <Field label="Min size (bytes; 0 = no min)">
                <input
                  type="number"
                  className="input"
                  value={rules.min_size}
                  onChange={(e) =>
                    setRules({ ...rules, min_size: Number(e.target.value) || 0 })
                  }
                />
              </Field>
              <Field label="Max size (bytes; 0 = no max)">
                <input
                  type="number"
                  className="input"
                  value={rules.max_size}
                  onChange={(e) =>
                    setRules({ ...rules, max_size: Number(e.target.value) || 0 })
                  }
                />
              </Field>
            </div>
          </div>
        )}

        {error && <div className="form-error">{error}</div>}
        {grabbing && <div className="grab-status">Scanning page…</div>}

        {!grabbing && rows.length > 0 && (
          <>
            <div className="grab-toolbar">
              <button className="btn btn--default btn--sm" onClick={selectAll}>
                Select all
              </button>
              <button className="btn btn--default btn--sm" onClick={deselectAll}>
                Deselect all
              </button>
              <button className="btn btn--default btn--sm" onClick={applyCommon}>
                Apply to selected
              </button>
              <select
                className="select select--sm"
                value={commonCat}
                onChange={(e) => setCommonCat(e.target.value)}
              >
                <option value="">(category)</option>
                {(categories ?? []).map((c) => (
                  <option key={c.name} value={c.name}>
                    {c.label || c.name}
                  </option>
                ))}
              </select>
              <select
                className="select select--sm"
                value={commonQueue}
                onChange={(e) => setCommonQueue(e.target.value)}
              >
                <option value="">(queue)</option>
                {(queues ?? []).map((q) => (
                  <option key={q.name} value={q.name}>
                    {q.label || q.name}
                  </option>
                ))}
              </select>
              <input
                className="input input--sm"
                style={{ marginLeft: "auto", width: 180 }}
                placeholder="Search…"
                value={search}
                onChange={(e) => setSearch(e.target.value)}
              />
            </div>

            <div className="batch-table">
              <div className="batch-table__head">
                <div className="batch-cell batch-cell--sel"></div>
                <div
                  className="batch-cell batch-cell--name batch-cell--sortable"
                  onClick={() => setSort("name")}
                  role="button"
                >
                  Filename{sortArrow("name")}
                </div>
                <div
                  className="batch-cell batch-cell--ext batch-cell--sortable"
                  onClick={() => setSort("ext")}
                  role="button"
                >
                  Ext{sortArrow("ext")}
                </div>
                <div
                  className="batch-cell batch-cell--size batch-cell--sortable"
                  onClick={() => setSort("size")}
                  role="button"
                >
                  Size{sortArrow("size")}
                </div>
                <div className="batch-cell batch-cell--type">Type</div>
                <div className="batch-cell batch-cell--domain">Domain</div>
                <div className="batch-cell batch-cell--dest">Destination</div>
                <div
                  className="batch-cell batch-cell--cat batch-cell--sortable"
                  onClick={() => setSort("category")}
                  role="button"
                >
                  Category{sortArrow("category")}
                </div>
                <div className="batch-cell batch-cell--q">Queue</div>
                <div className="batch-cell batch-cell--rm"></div>
              </div>
              <div className="batch-table__body">
                {visible.map((r) => {
                  const i = rows.indexOf(r);
                  return (
                    <div
                      key={r.url}
                      className={
                        "batch-row" + (checked.has(i) ? "" : " batch-row--off")
                      }
                    >
                      <div className="batch-cell batch-cell--sel">
                        <input
                          type="checkbox"
                          checked={checked.has(i)}
                          onChange={() => toggle(i)}
                        />
                      </div>
                      <div className="batch-cell batch-cell--name" title={r.url}>
                        <span>{r.filename}</span>
                        {r.source && (
                          <span
                            className="grab-pill grab-pill--src"
                            title={`Extracted from the ${r.source} attribute of the page`}
                          >
                            {r.source}
                          </span>
                        )}
                        {r.matched && (
                          <span className="grab-pill" title={r.matched}>
                            {r.matched}
                          </span>
                        )}
                      </div>
                      <div className="batch-cell batch-cell--ext">.{r.ext}</div>
                      <div className="batch-cell batch-cell--size">
                        {r.size > 0 ? formatBytes(r.size) : "—"}
                      </div>
                      <div className="batch-cell batch-cell--type" title={r.content_type ?? undefined}>
                        {r.content_type ?? "—"}
                      </div>
                      <div className="batch-cell batch-cell--domain" title={r.url}>
                        {hostnameFromUrl(r.url)}
                      </div>
                      <div className="batch-cell batch-cell--dest" title={r.save_dir || "Use category/site rule destination"}>
                        <input
                          className="input input--bare"
                          value={r.save_dir}
                          placeholder="(rule/category)"
                          onChange={(e) => setRow(i, { save_dir: e.target.value })}
                        />
                      </div>
                      <div className="batch-cell batch-cell--cat">
                        <select
                          className="select select--bare"
                          value={r.category ?? "other"}
                          onChange={(e) =>
                            setRow(i, { category: e.target.value })
                          }
                        >
                          {(categories ?? []).map((c) => (
                            <option key={c.name} value={c.name}>
                              {c.label || c.name}
                            </option>
                          ))}
                          <option value="other">other</option>
                        </select>
                      </div>
                      <div className="batch-cell batch-cell--q">
                        <select
                          className="select select--bare"
                          value={r.queue}
                          onChange={(e) => setRow(i, { queue: e.target.value })}
                        >
                          {(queues ?? []).map((q) => (
                            <option key={q.name} value={q.name}>
                              {q.label || q.name}
                            </option>
                          ))}
                          <option value="general">general</option>
                        </select>
                      </div>
                      <div className="batch-cell batch-cell--rm">
                        <button
                          className="btn btn--ghost btn--xs"
                          onClick={() => removeRow(i)}
                          title="Remove"
                        >
                          ×
                        </button>
                      </div>
                    </div>
                  );
                })}
                {visible.length === 0 && (
                  <div className="batch-empty">No links match the search.</div>
                )}
              </div>
            </div>
          </>
        )}

        {!grabbing && rows.length === 0 && !error && (
          <div className="grab-empty">
            <IconGrabber size={48} />
            <p>Enter a URL above and click Grab to find downloadable files.</p>
          </div>
        )}
      </div>
    </Dialog>
  );
}
