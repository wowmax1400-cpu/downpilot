// Categories section inside the Options / Settings dialog.
// Reuses `store.saveCategory` / `deleteCategory` / `listCategories` so all
// category routing rules remain in the engine. This UI is purely a
// configurator — no second store of extension rules.

import React from "react";
import type { DownloadsStore } from "../store/useDownloads";
import type { Category, DownloadQueue, SiteRule } from "../types";
import { Dialog, Field, Btn } from "./Dialog";

type Props = { store: DownloadsStore; onClose: () => void };

function blankCategory(queues: DownloadQueue[]): Category {
  return {
    name: "",
    label: "",
    extensions: "",
    save_dir: "",
    queue: queues[0]?.name ?? "general",
    color: "#64748b",
    builtin: false,
  };
}

export function CategoriesSection({ store }: { store: DownloadsStore }) {
  const [list, setList] = React.useState<Category[]>(store.categories);
  const [edit, setEdit] = React.useState<Category | null>(null);
  const [creating, setCreating] = React.useState(false);
  const [err, setErr] = React.useState<string | null>(null);

  const refresh = React.useCallback(() => {
    import("../lib/api").then((a) => a.listCategories().then(setList));
  }, []);

  React.useEffect(() => {
    refresh();
  }, [refresh, store.categories]);

  const save = async (c: Category) => {
    setErr(null);
    if (!c.name.trim()) {
      setErr("Name is required");
      return;
    }
    try {
      await store.saveCategory({ ...c, label: c.label || c.name });
      setEdit(null);
      setCreating(false);
      refresh();
    } catch (e) {
      setErr(String(e));
    }
  };

  const remove = async (name: string) => {
    if (!window.confirm(`Delete category "${name}"? Tasks keep their category label.`)) {
      return;
    }
    await store.deleteCategory(name);
    refresh();
  };

  const editing = edit ?? (creating ? blankCategory(store.queues) : null);
  const queues = store.queues;

  return (
    <div className="form">
      {!editing && (
        <>
          <div className="cat-grid">
            <div className="cat-grid__head">
              <div>Name</div>
              <div>Extensions</div>
              <div>Save to</div>
              <div>Queue</div>
              <div>Actions</div>
           </div>
            <div className="cat-grid__body">
              {list.length === 0 && (
                <div className="panel__empty">No categories configured</div>
              )}
              {list.map((c) => {
                const extList = c.extensions || "(none)";
                const qName = c.queue || "general";
                const dispLabel = c.label || c.name;
                return (
                  <div key={c.name} className="cat-grid__row">
                    <div className="cat-grid__name">
                      <span
                        className="side-row__dot"
                        style={{ background: c.color ?? "#64748b" }}
                      />
                      <span>{dispLabel}</span>
                      {c.builtin && <span className="cat-badge">built-in</span>}
                   </div>
                    <div className="cat-grid__ext" title={extList}>
                      {extList}
                   </div>
                    <div className="cat-grid__dir" title={c.save_dir}>
                      {c.save_dir || "(default)"}
                   </div>
                    <div className="cat-grid__q">{qName}</div>
                    <div className="cat-grid__actions">
                      <button
                        className="btn btn--default btn--sm"
                        onClick={() => setEdit({ ...c })}
                      >
                        Edit
                     </button>
                      <button
                        className="btn btn--ghost btn--sm"
                        onClick={() => remove(c.name)}
                        disabled={c.builtin}
                      >
                        Del
                     </button>
                   </div>
                 </div>
                );
              })}
           </div>
         </div>
          <div style={{ marginTop: 12 }}>
            <Btn variant="primary" onClick={() => setCreating(true)}>
              + Add category
           </Btn>
         </div>
        </>
      )}

      {editing && (
        <>
          <div className="form-grid">
            <Field label="Name (id)">
              <input
                className="input"
                value={editing.name}
                disabled={!creating}
                onChange={(e) => setEdit({ ...editing, name: e.target.value })}
                placeholder="ebooks"
              />
           </Field>
            <Field label="Label">
              <input
                className="input"
                value={editing.label}
                onChange={(e) => setEdit({ ...editing, label: e.target.value })}
                placeholder="E-books"
              />
           </Field>
            <Field label="Extensions (comma-separated)" hint=".epub,.mobi — drives auto-routing">
              <input
                className="input"
                value={editing.extensions}
                onChange={(e) =>
                  setEdit({ ...editing, extensions: e.target.value })
                }
                placeholder=".epub,.mobi"
              />
           </Field>
            <Field label="Default save folder" hint="e.g. ~/Downloads/Books">
              <input
                className="input"
                value={editing.save_dir}
                onChange={(e) =>
                  setEdit({ ...editing, save_dir: e.target.value })
                }
              />
           </Field>
            <Field label="Default queue">
              <select
                className="select"
                value={editing.queue}
                onChange={(e) =>
                  setEdit({ ...editing, queue: e.target.value })
                }
              >
                {queues.map((q) => (
                  <option key={q.name} value={q.name}>
                    {q.label || q.name}
                 </option>
                ))}
             </select>
           </Field>
            <Field label="Color">
              <input
                type="color"
                className="input input--color"
                value={editing.color ?? "#64748b"}
                onChange={(e) => setEdit({ ...editing, color: e.target.value })}
              />
           </Field>
         </div>
          {err && <div className="form-error">{err}</div>}
          <div className="dialog-actions">
            <Btn
              variant="ghost"
              onClick={() => {
                setEdit(null);
                setCreating(false);
                setErr(null);
              }}
            >
              Cancel
           </Btn>
            <Btn
              variant="primary"
              onClick={() => save(editing)}
              disabled={!editing.name.trim()}
            >
              Save
           </Btn>
         </div>
        </>
      )}
   </div>
  );
}

export function SchedulesSection({ store }: { store: DownloadsStore }) {
  // Self-contained: clicking "Open" mounts a child Dialog hosting the
  // scheduler panel, so we own the onClose here.
  const [open, setOpen] = React.useState(false);
  const [Panel, setPanel] = React.useState<null | typeof import("./SchedulerPanel").SchedulerPanel>(null);
  React.useEffect(() => {
    let mounted = true;
    import("./SchedulerPanel").then((m) => {
      if (mounted) setPanel(() => m.SchedulerPanel);
    });
    return () => {
      mounted = false;
    };
  }, []);
  return (
    <div>
      <div className="mb-2 text-xs text-txt-dim">
        Time-based bandwidth profiles. A profile can target one or more
        queues; overlapping windows are resolved by priority (highest wins).
        When the active period ends, each affected queue returns to its
        configured cap.
      </div>
      <Btn variant="default" onClick={() => setOpen(true)}>
        Open Schedules & Bandwidth Profiles...
      </Btn>
      {open && Panel && (
        <Dialog
          title="Schedules & Bandwidth Profiles"
          onClose={() => setOpen(false)}
          width={760}
          footer={
            <Btn variant="primary" onClick={() => setOpen(false)}>
              Close
            </Btn>
          }
        >
          <Panel store={store} onClose={() => setOpen(false)} />
        </Dialog>
      )}
    </div>
  );
}

export function SiteRulesSection({ store }: { store: DownloadsStore }) {
  const [sites, setSites] = React.useState<SiteRule[]>(store.sites ?? []);
  const [editing, setEditing] = React.useState<SiteRule | null>(null);
  const [previewUrl, setPreviewUrl] = React.useState<string>("");
  const [previewRule, setPreviewRule] = React.useState<SiteRule | null>(null);

  React.useEffect(() => {
    setSites(store.sites ?? []);
  }, [store.sites]);

  const cats = store.categories ?? [];
  const queues = store.queues ?? [];

  const empty = (): SiteRule => ({
    domain: "",
    queue: "general",
    save_dir: "",
    auth_ref: "",
    proxy_url: "",
    auto_capture: true,
    max_connections: 8,
    bandwidth_kbps: 0,
    notes: "",
    category: "",
    user_agent: "",
  });

  const refresh = async () => {
    await store.refreshAux();
  };

  const save = async (s: SiteRule) => {
    await store.saveSite(s);
    await refresh();
    setEditing(null);
  };

  const remove = async (domain: string) => {
    if (!window.confirm(`Delete site rule for "${domain}"?`)) return;
    await store.deleteSite(domain);
    await refresh();
  };

  // When the user is editing a domain, fetch a live preview of which
  // rule would actually fire for a URL they type.
  const testMatch = async (url: string) => {
    if (!url.trim()) {
      setPreviewRule(null);
      return;
    }
    const r = await store.testSiteMatch(url);
    setPreviewRule(r);
  };

  return (
    <div className="form">
      <div className="form-row form-row--between">
        <h4 className="settings__h">Site Rules</h4>
        <button
          className="btn btn--primary btn--sm"
          onClick={() => setEditing(empty())}
        >
          Add rule…
        </button>
      </div>
      <p className="settings__hint">
        Per-host configuration. Rules are matched in order:
        exact host → longest parent domain → global rule (empty domain).
        Hostnames are matched safely — <code>example.com</code> does NOT
        match <code>notexample.com</code> or <code>example.com.evil.tld</code>.
      </p>

      <div className="sites-table">
        <div className="sites-table__head">
          <div className="sites-cell sites-cell--domain">Domain</div>
          <div className="sites-cell sites-cell--cat">Category</div>
          <div className="sites-cell sites-cell--queue">Queue</div>
          <div className="sites-cell sites-cell--dir">Save to</div>
          <div className="sites-cell sites-cell--bw">Bandwidth</div>
          <div className="sites-cell sites-cell--auth">Auth</div>
          <div className="sites-cell sites-cell--rm"></div>
        </div>
        <div className="sites-table__body">
          {sites.length === 0 && (
            <div className="sites-empty">No site rules yet. Click "Add rule…" to create one.</div>
          )}
          {sites.map((s) => (
            <div key={s.domain || "(global)"} className="sites-row">
              <div className="sites-cell sites-cell--domain" title={s.domain}>
                <code>{s.domain || <em>(global)</em>}</code>
              </div>
              <div className="sites-cell sites-cell--cat">{s.category || "—"}</div>
              <div className="sites-cell sites-cell--queue">{s.queue}</div>
              <div className="sites-cell sites-cell--dir" title={s.save_dir}>
                {s.save_dir || "—"}
              </div>
              <div className="sites-cell sites-cell--bw">
                {s.bandwidth_kbps > 0 ? `${s.bandwidth_kbps} kbps` : "∞"}
              </div>
              <div className="sites-cell sites-cell--auth">
                {s.auth_ref ? "🔒" : "—"}
              </div>
              <div className="sites-cell sites-cell--rm">
                <button
                  className="btn btn--ghost btn--xs"
                  onClick={() => setEditing(s)}
                >
                  Edit
                </button>
                <button
                  className="btn btn--ghost btn--xs"
                  onClick={() => remove(s.domain)}
                >
                  ×
                </button>
              </div>
            </div>
          ))}
        </div>
      </div>

      {editing && (
        <Dialog
          title={editing.domain ? `Edit ${editing.domain}` : "New site rule"}
          onClose={() => setEditing(null)}
          width={520}
          footer={
            <>
              <button className="btn btn--ghost" onClick={() => setEditing(null)}>
                Cancel
              </button>
              <button
                className="btn btn--primary"
                onClick={() => save(editing)}
                disabled={!editing.domain.trim() && editing.notes !== "(global)"}
              >
                Save
              </button>
            </>
          }
        >
          <div className="form">
            <Field
              label="Domain (leave empty for global defaults)"
              hint="Hostname-aware. Use only the host (no scheme/path)."
            >
              <input
                className="input"
                value={editing.domain}
                onChange={(e) =>
                  setEditing({ ...editing, domain: e.target.value.trim().toLowerCase() })
                }
                placeholder="example.com"
              />
            </Field>
            <div className="form-grid form-grid--2">
              <Field label="Category">
                <select
                  className="select"
                  value={editing.category ?? ""}
                  onChange={(e) =>
                    setEditing({ ...editing, category: e.target.value })
                  }
                >
                  <option value="">(use default)</option>
                  {cats.map((c) => (
                    <option key={c.name} value={c.name}>
                      {c.label || c.name}
                    </option>
                  ))}
                </select>
              </Field>
              <Field label="Queue">
                <select
                  className="select"
                  value={editing.queue}
                  onChange={(e) => setEditing({ ...editing, queue: e.target.value })}
                >
                  {queues.map((q) => (
                    <option key={q.name} value={q.name}>
                      {q.label || q.name}
                    </option>
                  ))}
                </select>
              </Field>
            </div>
            <Field label="Save directory (absolute path; empty = use category default)">
              <input
                className="input"
                value={editing.save_dir}
                onChange={(e) => setEditing({ ...editing, save_dir: e.target.value })}
                placeholder="~/Downloads/Programs"
              />
            </Field>
            <div className="form-grid form-grid--2">
              <Field
                label="Bandwidth limit (kbps; 0 = unlimited)"
                hint="Per-queue cap. The site rule only takes effect for downloads on the chosen queue."
              >
                <input
                  type="number"
                  className="input"
                  value={editing.bandwidth_kbps}
                  onChange={(e) =>
                    setEditing({ ...editing, bandwidth_kbps: Number(e.target.value) || 0 })
                  }
                />
              </Field>
              <Field label="Max connections">
                <input
                  type="number"
                  className="input"
                  value={editing.max_connections}
                  onChange={(e) =>
                    setEditing({ ...editing, max_connections: Number(e.target.value) || 1 })
                  }
                />
              </Field>
            </div>
            <Field label="User-Agent override (empty = engine default)">
              <input
                className="input"
                value={editing.user_agent ?? ""}
                onChange={(e) =>
                  setEditing({ ...editing, user_agent: e.target.value })
                }
                placeholder="Mozilla/5.0…"
              />
            </Field>
            <Field
              label="Auth reference (queue credential name)"
              hint="Reuses the existing CredentialStore. Passwords are never stored on the task."
            >
              <input
                className="input"
                value={editing.auth_ref}
                onChange={(e) => setEditing({ ...editing, auth_ref: e.target.value })}
                placeholder="my-github-creds"
              />
            </Field>
            <Field label="Proxy URL (empty = use global setting)">
              <input
                className="input"
                value={editing.proxy_url}
                onChange={(e) => setEditing({ ...editing, proxy_url: e.target.value })}
                placeholder="http://127.0.0.1:8080"
              />
            </Field>
            <Field label="Notes">
              <input
                className="input"
                value={editing.notes}
                onChange={(e) => setEditing({ ...editing, notes: e.target.value })}
                placeholder="Why this rule exists"
              />
            </Field>
            <label className="check">
              <input
                type="checkbox"
                checked={editing.auto_capture}
                onChange={(e) =>
                  setEditing({ ...editing, auto_capture: e.target.checked })
                }
              />
              Auto-capture browser downloads from this host
            </label>

            <hr className="rule" />
            <h5 className="settings__h">Test match</h5>
            <Field label="URL to test" hint="See which rule would apply at insert time.">
              <input
                className="input"
                value={previewUrl}
                onChange={(e) => {
                  setPreviewUrl(e.target.value);
                  void testMatch(e.target.value);
                }}
                placeholder="https://www.example.com/path/file.zip"
              />
            </Field>
            {previewRule ? (
              <div className="form-ok">
                Matched: <code>{previewRule.domain || "(global)"}</code> → queue{" "}
                <code>{previewRule.queue}</code>
                {previewRule.category ? <> · category <code>{previewRule.category}</code></> : null}
                {previewRule.bandwidth_kbps > 0 ? (
                  <> · cap {previewRule.bandwidth_kbps} kbps</>
                ) : null}
              </div>
            ) : previewUrl.trim() ? (
              <div className="form-hint">No rule matched — engine defaults will apply.</div>
            ) : null}
          </div>
        </Dialog>
      )}
    </div>
  );
}

export function CaptureSection({ store }: { store: DownloadsStore }) {
  const s = store.settings;
  const [mode, setMode] = React.useState<string>(s?.capture_mode ?? "ask");
  const [allowed, setAllowed] = React.useState<string>(s?.allowed_extensions ?? "");
  const [excluded, setExcluded] = React.useState<string>(s?.exclude_extensions ?? "");
  const [ignoredDomains, setIgnoredDomains] = React.useState<string>(s?.exclude_domains ?? "");
  const [always, setAlways] = React.useState<string>(s?.always_allow_extensions ?? "");

  // Re-sync when the store changes (e.g. after a refresh).
  React.useEffect(() => {
    if (!s) return;
    setMode(s.capture_mode ?? "ask");
    setAllowed(s.allowed_extensions ?? "");
    setExcluded(s.exclude_extensions ?? "");
    setIgnoredDomains(s.exclude_domains ?? "");
    setAlways(s.always_allow_extensions ?? "");
  }, [s?.capture_mode, s?.allowed_extensions, s?.exclude_extensions, s?.exclude_domains, s?.always_allow_extensions]);

  const save = (key: string, value: string) => store.updateSetting(key, value);
  const listHint = (text: string) => text
    ? "Comma-separated. Lowercased. Subdomains match: `example.com` covers `www.example.com`."
    : "Empty = no rule.";

  return (
    <div className="form">
      <Field label="Capture mode" hint="How Chrome downloads are intercepted by DownPilot.">
        <select
          className="input"
          value={mode}
          onChange={(e) => { setMode(e.target.value); save("capture_mode", e.target.value); }}
        >
          <option value="off">Off — let Chrome handle downloads</option>
          <option value="ask">Ask — notify before each capture</option>
          <option value="auto">Automatic — capture without asking</option>
        </select>
      </Field>
      <Field label="Allowed extensions" hint={listHint(allowed)}>
        <input
          className="input"
          placeholder="mp4, mkv, zip, iso"
          value={allowed}
          onChange={(e) => setAllowed(e.target.value)}
          onBlur={() => save("allowed_extensions", allowed.trim())}
        />
      </Field>
      <Field label="Ignored extensions" hint={listHint(excluded)}>
        <input
          className="input"
          placeholder="png, jpg, html"
          value={excluded}
          onChange={(e) => setExcluded(e.target.value)}
          onBlur={() => save("exclude_extensions", excluded.trim())}
        />
      </Field>
      <Field label="Ignored domains" hint="One host per entry. Subdomains match automatically (example.com covers www.example.com).">
        <input
          className="input"
          placeholder="example.com, another.test"
          value={ignoredDomains}
          onChange={(e) => setIgnoredDomains(e.target.value)}
          onBlur={() => save("exclude_domains", ignoredDomains.trim())}
        />
      </Field>
      <Field label="Always capture" hint="Extensions that bypass Ask mode. Persists across restarts. Click ✕ to remove.">
        <div className="flex flex-wrap gap-1">
          {always
            .split(",")
            .map((x) => x.trim().toLowerCase())
            .filter(Boolean)
            .map((e) => (
              <span key={e} className="chip">
                {e}
                <button
                  type="button"
                  className="chip__rm"
                  onClick={() => {
                    const next = always
                      .split(",")
                      .map((x) => x.trim().toLowerCase())
                      .filter((x) => x && x !== e)
                      .join(",");
                    setAlways(next);
                    save("always_allow_extensions", next);
                  }}
                  title={`Stop always-capturing .${e}`}
                >×</button>
              </span>
            ))}
          <input
            className="input"
            placeholder="+ extension"
            onKeyDown={(e) => {
              if (e.key !== "Enter") return;
              const v = (e.currentTarget.value || "").trim().toLowerCase().replace(/^\./, "");
              if (!v) return;
              const set = new Set(always.split(",").map((x) => x.trim().toLowerCase()).filter(Boolean));
              set.add(v);
              const next = Array.from(set).sort().join(",");
              setAlways(next);
              save("always_allow_extensions", next);
              e.currentTarget.value = "";
            }}
          />
        </div>
      </Field>
    </div>
  );
}

export function SettingsDialog({ store, onClose }: Props) {
  const [search, setSearch] = React.useState("");
  const [active, setActive] = React.useState("general");
  const s = store.settings;

  const setStr = (key: string, value: string) => store.updateSetting(key, value);

  const baseSections: Array<{ id: string; title: string; fields: Array<{ key: string; label: string; type: string; options?: string[]; hint?: string }> }> = [
    {
      id: "general",
      title: "General",
      fields: [
        { key: "download_dir", label: "Default download folder", type: "text" },
        {
          key: "theme",
          label: "Theme",
          type: "select",
          options: ["deep-midnight", "modern-dark", "clean-light", "cyberpunk"],
        },
        { key: "accent", label: "Accent color", type: "text" },
      ],
    },
    {
      id: "downloads",
      title: "Downloads",
      fields: [
        { key: "default_threads", label: "Default connections per download", type: "number" },
        { key: "max_threads", label: "Max connections per download", type: "number" },
        { key: "max_simultaneous", label: "Max simultaneous downloads", type: "number" },
        { key: "speed_limit_kbps", label: "Global speed limit (kbps, 0=unlimited)", type: "number" },
      ],
    },
    {
      id: "connections",
      title: "Connections",
      fields: [
        { key: "conn_timeout_ms", label: "Connection timeout (ms)", type: "number" },
        { key: "read_timeout_ms", label: "Read timeout (ms)", type: "number" },
        { key: "max_retries", label: "Max retries", type: "number" },
        { key: "retry_delay_ms", label: "Retry delay (ms)", type: "number" },
      ],
    },
    {
      id: "proxy",
      title: "Proxy",
      fields: [
        {
          key: "proxy_url",
          label: "Proxy URL (http/https/socks5)",
          type: "text",
          hint: "Blank = no proxy",
        },
      ],
    },
    {
      id: "browser",
      title: "Browser Integration",
      fields: [
        {
          key: "auto_intercept",
          label: "Automatically capture browser downloads",
          type: "toggle",
        },
        {
          key: "use_ytdlp_for_media",
          label: "Use yt-dlp for media links",
          type: "toggle",
        },
      ],
    },
    {
      id: "clipboard",
      title: "Clipboard",
      fields: [
        {
          key: "clipboard_monitor",
          label: "Monitor clipboard for download URLs",
          type: "toggle",
        },
      ],
    },
    {
      id: "completion",
      title: "Completion Actions",
      fields: [
        {
          key: "on_complete",
          label: "Default action",
          type: "select",
          options: [
            "none",
            "open_file",
            "open_folder",
            "notification",
            "run_command",
            "start_queue",
            "shutdown",
            "suspend",
            "hibernate",
          ],
        },
      ],
    },
    {
      id: "notifications",
      title: "Notifications",
      fields: [
        { key: "schedule_enabled", label: "Enable scheduled windows", type: "toggle" },
        { key: "schedule_start", label: "Schedule start (HH:MM)", type: "text" },
        { key: "schedule_end", label: "Schedule end (HH:MM)", type: "text" },
      ],
    },
  ];

  const matches = (sec: { id: string; title: string }) =>
    !search ||
    sec.title.toLowerCase().includes(search.toLowerCase()) ||
    sec.id.includes(search.toLowerCase());

  const visibleSections = baseSections.filter(matches);
  const categoriesMatch = matches({ id: "categories", title: "Categories" });
  const showCategories = categoriesMatch;
  const schedulesMatch = matches({ id: "schedules", title: "Schedules & Bandwidth Profiles" });
  const showSchedules = schedulesMatch;
  const captureMatch = matches({ id: "capture", title: "Browser Capture" });
  const showCapture = captureMatch;
  const sitesMatch = matches({ id: "sites", title: "Site Rules" });
  const showSites = sitesMatch;
  const showAnyBase = visibleSections.length > 0;

  return (
    <Dialog
      title="Options"
      onClose={onClose}
      width={760}
      footer={<Btn variant="primary" onClick={onClose}>Close</Btn>}
    >
      <div className="settings">
        <div className="settings__side">
          <input
            className="input input--search"
            placeholder="Search settings..."
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
          <div className="settings__nav">
            {visibleSections.map((sec) => (
              <button
                key={sec.id}
                className={
                  "settings__nav-item " + (active === sec.id ? "active" : "")
                }
                onClick={() => setActive(sec.id)}
              >
                {sec.title}
             </button>
            ))}
            {showCategories && (
              <button
                className={
                  "settings__nav-item " + (active === "categories" ? "active" : "")
                }
                onClick={() => setActive("categories")}
              >
                Categories
             </button>
            )}
            {showSchedules && (
              <button
                className={
                  "settings__nav-item " + (active === "schedules" ? "active" : "")
                }
                onClick={() => setActive("schedules")}
              >
                Schedules
              </button>
            )}
            {showCapture && (
              <button
                className={
                  "settings__nav-item " + (active === "capture" ? "active" : "")
                }
                onClick={() => setActive("capture")}
              >
                Browser Capture
              </button>
            )}
            {showSites && (
              <button
                className={
                  "settings__nav-item " + (active === "sites" ? "active" : "")
                }
                onClick={() => setActive("sites")}
              >
                Site Rules
              </button>
            )}
         </div>
       </div>
        <div className="settings__main">
          {visibleSections
            .filter((sec) => !search || active === sec.id)
            .map((sec) => (
              <div key={sec.id} className="settings__section">
                <h3 className="settings__h">{sec.title}</h3>
                <div className="form">
                  {sec.fields.map((f) => {
                    const raw = (s as unknown as Record<string, unknown>)?.[f.key];
                    const val = raw == null ? "" : String(raw);
                    return (
                      <Field key={f.key} label={f.label} hint={f.hint}>
                        {f.type === "toggle" ? (
                          <input
                            type="checkbox"
                            checked={val === "true" || val === "1"}
                            onChange={(e) =>
                              setStr(f.key, e.target.checked ? "true" : "false")
                            }
                          />
                        ) : f.type === "select" ? (
                          <select
                            className="select"
                            value={val}
                            onChange={(e) => setStr(f.key, e.target.value)}
                          >
                            {f.options!.map((o) => (
                              <option key={o} value={o}>
                                {o}
                             </option>
                            ))}
                         </select>
                        ) : f.type === "number" ? (
                          <input
                            type="number"
                            className="input"
                            value={val}
                            onChange={(e) => setStr(f.key, e.target.value)}
                          />
                        ) : (
                          <input
                            className="input"
                            value={val}
                            onChange={(e) => setStr(f.key, e.target.value)}
                          />
                        )}
                     </Field>
                    );
                  })}
               </div>
             </div>
            ))}
          {showCategories && (active === "categories" || search) && (
            <div className="settings__section">
              <h3 className="settings__h">Categories</h3>
              <CategoriesSection store={store} />
           </div>
          )}
          {showCapture && (active === "capture" || search) && (
            <div className="settings__section">
              <h3 className="settings__h">Browser Capture</h3>
              <CaptureSection store={store} />
            </div>
          )}
          {showSites && (active === "sites" || search) && (
            <div className="settings__section">
              <h3 className="settings__h">Site Rules</h3>
              <SiteRulesSection store={store} />
            </div>
          )}
          {!showAnyBase && !showCategories && !showCapture && !showSites && (
            <div className="panel__empty">No settings match</div>
          )}
       </div>
     </div>
   </Dialog>
  );
}
