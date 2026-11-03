// DownPilot app shell. Layout:
//   MenuBar -> Toolbar -> [Sidebar | DownloadTable] -> StatusBar.
// Drag-drop, keyboard, and clipboard hooks live at the top level so all
// dialogs and panels inherit them.

import { useCallback, useEffect, useRef, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { useDownloads } from "./store/useDownloads";
import { TitleBar } from "./components/TitleBar";
import { MenuBar } from "./components/MenuBar";
import { Toolbar, type ToolbarAction } from "./components/Toolbar";
import { Sidebar } from "./components/Sidebar";
import { DownloadTable } from "./components/DownloadTable";
import { DownloadCards } from "./components/DownloadCards";
import { DetailsPanel } from "./components/DetailsPanel";
import { StatusBar } from "./components/StatusBar";
import { ContextMenu } from "./components/ContextMenu";
import { AddDownloadDialog } from "./components/AddDownloadDialog";
import { PropertiesDialog } from "./components/PropertiesDialog";
import { IDMSegmentsDialog } from "./components/SegmentsModal";
import { SiteGrabberDialog } from "./components/SiteGrabberDialog";
import { SettingsDialog } from "./components/SettingsDialog";
import { SchedulerPanel } from "./components/SchedulerPanel";
import { QueuesPanel } from "./components/QueuesPanel";
import { AboutDialog } from "./components/AboutDialog";
import { ShortcutsDialog } from "./components/ShortcutsDialog";
import { BatchDialog } from "./components/BatchDialog";
import { HistoryPanel } from "./components/HistoryPanel";
import { subscribeToClipboard } from "./lib/api";
import { applyTheme } from "./themes";
import { useI18n } from "./i18n";
import * as api from "./lib/api";

/** Extract plausible URLs from arbitrary text (one per line). */
function extractUrlsFromText(text: string): string[] {
  const out: string[] = [];
  for (const raw of text.split(/\r?\n/)) {
    const t = raw.trim();
    if (!t) continue;
    if (/^https?:\/\//i.test(t) || (t.startsWith("ftp://") && t.includes("."))) {
      out.push(t);
    }
  }
  return out;
}

/** Same URL detector but on a single string (used for Ctrl+V). */
function looksLikeUrl(s: string): boolean {
  const t = s.trim();
  return /^https?:\/\//i.test(t);
}

export default function App() {
  const store = useDownloads();
  const { t } = useI18n();

  const [addOpen, setAddOpen] = useState(false);
  const [addUrl, setAddUrl] = useState<string | undefined>(undefined);
  const [batchOpen, setBatchOpen] = useState(false);
  const [batchUrls, setBatchUrls] = useState<string[] | undefined>(undefined);
  const [grabberOpen, setGrabberOpen] = useState(false);
  const [optionsOpen, setOptionsOpen] = useState(false);
  const [schedulerOpen, setSchedulerOpen] = useState(false);
  const [queuesOpen, setQueuesOpen] = useState(false);
  const [aboutOpen, setAboutOpen] = useState(false);
  const [shortcutsOpen, setShortcutsOpen] = useState(false);
  const [historyOpen, setHistoryOpen] = useState(false);
  const [propsId, setPropsId] = useState<number | null>(null);
  /** IDM-style per-segment progress window (double-click a running row). */
  const [segmentsId, setSegmentsId] = useState<number | null>(null);
  const [detailsOpen, setDetailsOpen] = useState(() => {
    try {
      return localStorage.getItem("downpilot.details.open") === "true";
    } catch {
      return false;
    }
  });
  const [ctx, setCtx] = useState<{ x: number; y: number; ids: number[] } | null>(
    null
  );
  const [dragHover, setDragHover] = useState(false);
  /** List layout: "table" is the dense IDM column grid default. Persisted so the choice survives restarts. */
  const [viewMode, setViewMode] = useState<"cards" | "table">(() => {
    try {
      const v = localStorage.getItem("downpilot.view.mode");
      return v === "cards" ? "cards" : "table";
    } catch {
      return "table";
    }
  });

  useEffect(() => {
    const th = store.settings?.theme || "deep-midnight";
    applyTheme(th, store.settings?.accent ?? undefined);
    try {
      localStorage.setItem("downpilot.theme", th);
    } catch {}
  }, [store.settings?.theme, store.settings?.accent]);

  useEffect(() => {
    const unsub = subscribeToClipboard((url) => {
      if (url && looksLikeUrl(url)) {
        setAddUrl(url);
        setAddOpen(true);
      }
    });
    return () => unsub();
  }, []);

  useEffect(() => {
    try {
      localStorage.setItem("downpilot.view.mode", viewMode);
    } catch {
      // Ignore unavailable storage.
    }
  }, [viewMode]);

  useEffect(() => {
    try {
      localStorage.setItem("downpilot.details.open", String(detailsOpen));
    } catch {
      // Ignore unavailable storage in private/browser mode.
    }
  }, [detailsOpen]);

  const onSelect = useCallback(
    (id: number, ev: React.MouseEvent) => {
      if (ev.shiftKey || ev.ctrlKey || ev.metaKey) {
        store.toggleSelect(id);
      } else {
        store.selectOnly(id);
      }
    },
    [store]
  );

  const onRowContext = useCallback(
    (id: number, x: number, y: number) => {
      if (!store.selected.has(id)) {
        store.selectOnly(id);
      }
      const ids = store.selected.has(id) ? Array.from(store.selected) : [id];
      setCtx({ x, y, ids });
    },
    [store]
  );

  /** Double-click on a row: Open File for completed downloads,
   *  or open IDM download progress dialog for in-flight/other transfers. */
  const onRowActivate = useCallback(
    (id: number) => {
      const t = store.tasks.find((x) => x.id === id);
      if (!t) return;
      if (t.status === "completed") {
        void store.openFile(t);
      } else {
        setSegmentsId(id);
      }
    },
    [store]
  );

  // Auto-open IDM's floating download progress modal when an active download starts
  const prevDownloadingRef = useRef<Set<number>>(new Set());
  useEffect(() => {
    const currentlyDownloading = new Set(
      store.tasks.filter((t) => t.status === "downloading").map((t) => t.id)
    );
    for (const id of currentlyDownloading) {
      if (!prevDownloadingRef.current.has(id)) {
        setSegmentsId(id);
        break;
      }
    }
    prevDownloadingRef.current = currentlyDownloading;
  }, [store.tasks]);

  /** Rename a single task. Used by F2 / context menu. */
  const renameSelected = useCallback(() => {
    const ids = Array.from(store.selected);
    if (ids.length === 0) return;
    const task = store.tasks.find((x) => x.id === ids[0]);
    if (!task) return;
    const next = window.prompt(t("prompt.renameTo"), task.filename);
    if (next && next.trim() && next !== task.filename) {
      void store.rename(task.id, next.trim());
    }
  }, [store, t]);

  // ---- Drag and drop (Tauri webview events) ----
  // On drop we look at `payload.urls` (text/uri-list) and `payload.paths`
  // (OS file paths). URL-list files are read by the backend.
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    let cancelled = false;
    (async () => {
      try {
        const wv = getCurrentWebview();
        const off = await wv.onDragDropEvent(async (event) => {
          if (cancelled) return;
          const p = event.payload as { type: string; urls?: string[]; paths?: string[] };
          if (p.type === "enter" || p.type === "over") {
            setDragHover(true);
            return;
          }
          if (p.type === "leave") {
            setDragHover(false);
            return;
          }
          if (p.type === "drop") {
            setDragHover(false);
            const urls = (p.urls ?? []).filter((u) => /^https?:\/\//i.test(u));
            const paths = p.paths ?? [];
            // If the user dropped a URL list file (no direct URLs), read it.
            let fromFiles: string[] = [];
            if (urls.length === 0 && paths.length > 0) {
              for (const fp of paths) {
                try {
                  const lines = await api.readUrlFile(fp);
                  fromFiles = fromFiles.concat(lines);
                } catch {
                  /* ignore non-text / non-URL files */
                }
              }
            }
            const all = urls.concat(fromFiles);
            if (all.length === 0) return;
            if (all.length === 1) {
              setAddUrl(all[0]);
              setAddOpen(true);
            } else {
              setBatchUrls(all);
              setBatchOpen(true);
            }
          }
        });
        if (cancelled) {
          off();
        } else {
          unlisten = off;
        }
      } catch {
        /* running outside Tauri (vite dev) — no drag/drop */
      }
    })();
    return () => {
      cancelled = true;
      if (unlisten) unlisten();
    };
  }, []);

  // ---- Global keyboard shortcuts (IDM-class) ----
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement | null)?.tagName;
      const editable =
        tag === "INPUT" ||
        tag === "TEXTAREA" ||
        (e.target as HTMLElement)?.isContentEditable;
      // Ctrl+V is allowed even in inputs — but if we're in one we don't
      // hijack it. The browser's default paste-into-input wins there.
      const ctrl = e.ctrlKey || e.metaKey;
      if (ctrl && !editable && e.key.toLowerCase() === "v") {
        e.preventDefault();
        void (async () => {
          try {
            const text = await navigator.clipboard.readText();
            if (text) {
              if (looksLikeUrl(text)) {
                setAddUrl(text.trim());
                setAddOpen(true);
              } else {
                const urls = extractUrlsFromText(text);
                if (urls.length > 0) {
                  setBatchUrls(urls);
                  setBatchOpen(true);
                }
              }
            }
          } catch {
            /* clipboard access denied — ignore */
          }
        })();
        return;
      }
      if (editable) return;
      if (ctrl && e.key.toLowerCase() === "a") {
        e.preventDefault();
        const all = store.tasks.map((t) => t.id);
        if (all.length === 0) return;
        store.selectOnly(all[0]);
        for (let i = 1; i < all.length; i++) store.toggleSelect(all[i]);
        return;
      }
      if (ctrl && e.key.toLowerCase() === "f") {
        e.preventDefault();
        document.dispatchEvent(new CustomEvent("downpilot:focus-search"));
        return;
      }
      if (ctrl && e.key.toLowerCase() === "n") {
        e.preventDefault();
        setAddUrl(undefined);
        setAddOpen(true);
        return;
      }
      if (ctrl && e.key.toLowerCase() === "p") {
        e.preventDefault();
        const first = Array.from(store.selected)[0];
        if (first !== undefined) setPropsId(first);
        return;
      }
      if (e.key === "Enter") {
        if (store.selected.size === 1)
          onRowActivate(Array.from(store.selected)[0]);
        return;
      }
      if (e.key === "Delete" || e.key === "Backspace") {
        e.preventDefault();
        store.bulk((id) => store.remove(id));
        return;
      }
      if (e.key === "F2") {
        e.preventDefault();
        renameSelected();
        return;
      }
      if (e.key === " ") {
        const ids = Array.from(store.selected);
        for (const id of ids) {
          const t = store.tasks.find((x) => x.id === id);
          if (!t) continue;
          if (t.status === "downloading") {
            void store.pause(t.id);
            break;
          } else if (
            t.status === "paused" ||
            t.status === "stopped" ||
            t.status === "failed"
          ) {
            void store.resume(t.id);
            break;
          }
        }
        return;
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [store, onRowActivate, renameSelected, t]);

  const handleToolbar = useCallback(
    (a: ToolbarAction) => {
      switch (a) {
        case "add":
          setAddUrl(undefined);
          setAddOpen(true);
          break;
        case "grabber":
          setGrabberOpen(true);
          break;
        case "start":
          store.bulk((id) => store.resume(id));
          break;
        case "pause":
          store.bulk((id) => store.pause(id));
          break;
        case "delete":
          store.bulk((id) => store.remove(id));
          break;
        case "deleteCompleted":
          store.clearFinished();
          break;
        case "stopAll":
          void store.stopAll();
          break;
        case "options":
          setOptionsOpen(true);
          break;
        case "scheduler":
          setSchedulerOpen(true);
          break;
        case "startQueue":
          void store.resumeAll();
          break;
        case "stopQueue":
          void store.stopAll();
          break;
        case "tellAFriend":
          setAboutOpen(true);
          break;
      }
    },
    [store]
  );

  return (
    <div className="app-root flex h-screen flex-col overflow-hidden bg-[var(--bg)] text-[var(--text)] select-none">
      <TitleBar />
      <MenuBar
        store={store}
        onAdd={() => {
          setAddUrl(undefined);
          setAddOpen(true);
        }}
        onBatch={() => {
          setBatchUrls(undefined);
          setBatchOpen(true);
        }}
        onGrabber={() => setGrabberOpen(true)}
        onQueues={() => setQueuesOpen(true)}
        onScheduler={() => setSchedulerOpen(true)}
        onSites={() => setOptionsOpen(true)}
        onCategories={() => setOptionsOpen(true)}
        onSettings={() => setOptionsOpen(true)}
        onHistory={() => setHistoryOpen(true)}
        onAbout={() => setAboutOpen(true)}
        onShortcuts={() => setShortcutsOpen(true)}
        onOpenProps={() => {
          const id = Array.from(store.selected)[0];
          if (id !== undefined) setPropsId(id);
        }}
        detailsOpen={detailsOpen}
        onToggleDetails={() => setDetailsOpen((open) => !open)}
      />
      <Toolbar store={store} onAction={handleToolbar} />
      {/* ===================== Panels Container ===================== */}
      <div className="flex min-h-0 flex-1 p-0 bg-[var(--bg)]">
        {/* Sidebar with flat border */}
        <div
          className="win32-sunken-panel flex shrink-0 bg-[var(--bg-sidebar)] overflow-hidden"
          style={{ borderRight: "1px solid var(--line)" }}
        >
          <Sidebar
            active={store.sidebarKey}
            onSelect={store.setSidebarKey}
            counts={store.counts}
            categories={store.categories}
            queues={store.queues}
            tasks={store.tasks}
            effectiveSpeed={(q: string) => store.effectiveSpeed[q] ?? 0}
            activeProfiles={store.activeProfiles}
          />
        </div>

        {/* Solid 4px vertical splitter bar */}
        <div
          className="w-[4px] shrink-0 bg-[var(--bg)] cursor-col-resize hover:bg-[var(--surface-3)] transition-colors"
          style={{ cursor: "col-resize" }}
        />

        {/* Main ListView with Flat Border */}
        <div
          className="win32-sunken-panel flex min-w-0 flex-1 bg-[var(--table-bg)] overflow-hidden"
          style={{ border: "1px solid var(--line)" }}
        >
          <main className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden bg-[var(--table-bg)]">
            <div className="min-h-0 flex-1 overflow-auto bg-[var(--table-bg)]">
              <DownloadTable
                tasks={store.tasks}
                grouped={store.grouped}
                selected={store.selected}
                sort={store.sort}
                onSortChange={store.setSort}
                onSelect={onSelect}
                onRetry={(id) => store.redownload(id)}
                onToggleAll={() => {
                  const ids = store.filtered.map((task) => task.id);
                  if (ids.length > 0 && ids.every((id) => store.selected.has(id))) {
                    store.clearSelection();
                  } else if (ids.length > 0) {
                    store.selectOnly(ids[0]);
                    for (const id of ids.slice(1)) store.toggleSelect(id);
                  }
                }}
                onContextMenu={onRowContext}
                onRowActivate={onRowActivate}
                groupBy={store.groupBy}
                speedHistory={store.getSpeedHistory}
                sidebarKey={store.sidebarKey}
                search={store.search}
              />
            </div>
          </main>
        </div>
      </div>
      <StatusBar store={store} />

      {dragHover && (
        <div className="drop-overlay" aria-hidden>
          <div className="drop-overlay__box">
            <div className="drop-overlay__icon">{t("drop.title")}</div>
            <div className="drop-overlay__sub">
              {t("drop.sub1")}<br />
              {t("drop.sub2")}
           </div>
         </div>
       </div>
      )}

      {ctx && (
        <ContextMenu
          x={ctx.x}
          y={ctx.y}
          store={store}
          ids={ctx.ids}
          onClose={() => setCtx(null)}
          onOpenProps={(id) => setPropsId(id)}
        />
      )}
      {addOpen && (
        <AddDownloadDialog
          store={store}
          initialUrl={addUrl}
          onClose={() => {
            setAddOpen(false);
            setAddUrl(undefined);
          }}
        />
      )}
      {batchOpen && (
        <BatchDialog
          store={store}
          initialUrls={batchUrls}
          onClose={() => {
            setBatchOpen(false);
            setBatchUrls(undefined);
          }}
        />
      )}
      {grabberOpen && (
        <SiteGrabberDialog
          open
          onClose={() => setGrabberOpen(false)}
          onGrab={async (url, rules, limit) => await store.grabSiteWithRules(url, limit, rules)}
          onAdd={(links) => {
            // Phase 3C: hand the whole selected set to addBatch so
            // a single bad URL doesn't abort the rest and so the
            // user gets per-row results back. Each link carries
            // its own queue / category / save_dir as chosen in
            // the grabber table.
            const nds = links.map((l) => ({
              url: l.url,
              filename: l.filename,
              category: l.category ?? undefined,
              queue: l.queue ?? undefined,
              save_dir: l.save_dir || undefined,
              start_immediately: true,
            }));
            void store.addBatch(nds);
          }}
          queues={(store.queues ?? []).map((q) => ({ name: q.name, label: q.label }))}
          categories={(store.categories ?? []).map((c) => ({ name: c.name, label: c.label }))}
        />
      )}
      {optionsOpen && <SettingsDialog store={store} onClose={() => setOptionsOpen(false)} />}
      {schedulerOpen && <SchedulerPanel store={store} onClose={() => setSchedulerOpen(false)} />}
      {queuesOpen && <QueuesPanel store={store} onClose={() => setQueuesOpen(false)} />}
      {historyOpen && <HistoryPanel store={store} onClose={() => setHistoryOpen(false)} />}
      {aboutOpen && <AboutDialog onClose={() => setAboutOpen(false)} />}
      {shortcutsOpen && <ShortcutsDialog onClose={() => setShortcutsOpen(false)} />}
      {segmentsId !== null && (
        <IDMSegmentsDialog
          store={store}
          id={segmentsId}
          onClose={() => setSegmentsId(null)}
        />
      )}
      {propsId !== null && (
        <PropertiesDialog store={store} id={propsId} onClose={() => setPropsId(null)} />
      )}
   </div>
  );
}
