// IDM-style menu bar with a small integrated search input on the
// right. Every menu entry is wired to a real action — nothing here
// is a placeholder. Disabled entries reflect actual capability.

import React, { useEffect, useRef, useState } from "react";
import type { DownloadsStore } from "../store/useDownloads";

type Props = {
  store: DownloadsStore;
  onAdd: () => void;
  onBatch: () => void;
  onGrabber: () => void;
  onQueues: () => void;
  onScheduler: () => void;
  onSites: () => void;
  onCategories: () => void;
  onSettings: () => void;
  onHistory: () => void;
  onAbout: () => void;
  onOpenProps: () => void;
  onToggleDetails: () => void;
  detailsOpen: boolean;
  onFocusSearch?: () => void;
};

type Item =
  | { label: string; onClick?: () => void; shortcut?: string; disabled?: boolean }
  | { sep: true };

function Menu({ label, items }: { label: string; items: Item[] }) {
  const [open, setOpen] = React.useState(false);
  const ref = React.useRef<HTMLLIElement>(null);
  React.useEffect(() => {
    if (!open) return;
    const onDoc = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", onDoc);
    return () => document.removeEventListener("mousedown", onDoc);
  }, [open]);
  return (
    <li className="menubar__item" ref={ref}>
      <button className="menubar__btn" onClick={() => setOpen((o) => !o)}>
        {label}
      </button>
      {open && (
        <div className="menu-dropdown">
          {items.map((it, i) =>
            "sep" in it ? (
              <div className="menu-sep" key={i} />
            ) : (
              <button
                key={i}
                className="menu-row"
                disabled={it.disabled}
                onClick={() => {
                  setOpen(false);
                  it.onClick?.();
                }}
              >
                <span className="menu-row__label">{it.label}</span>
                {it.shortcut && <span className="menu-row__short">{it.shortcut}</span>}
              </button>
            )
          )}
        </div>
      )}
    </li>
  );
}

export function MenuBar(props: Props) {
  const { store } = props;
  const sel = [...store.selected];
  const single = sel.length === 1 ? sel[0] : null;
  const hasSel = sel.length > 0;
  const singleTask = single !== null
    ? store.tasks.find((t) => t.id === single) ?? null
    : null;

  const pauseSel = () => sel.forEach((id) => store.pause(id));
  const resumeSel = () => sel.forEach((id) => store.resume(id));
  const cancelSel = () => sel.forEach((id) => store.cancel(id));
  const removeSel = () => sel.forEach((id) => store.remove(id));
  const redownSel = () => sel.forEach((id) => store.redownload(id));
  const retrySel = () => sel.forEach((id) => {
    const t = store.tasks.find((x) => x.id === id);
    if (t && t.status === "failed") store.resume(id);
  });
  const forceStartSel = () => sel.forEach((id) => store.forceStart(id));
  const openFileSel = () => {
    if (singleTask) void store.openFile(singleTask);
  };
  const openFolderSel = () => {
    if (singleTask) void store.openFolder(singleTask);
  };
  const copyUrl = async () => {
    if (singleTask) {
      try {
        await navigator.clipboard.writeText(singleTask.url);
      } catch {/* ignore */}
    }
  };
  const copyPath = async () => {
    if (singleTask) {
      const p = (singleTask.save_dir || "") + (singleTask.save_dir && singleTask.filename ? "/" : "") + (singleTask.filename || "");
      try {
        await navigator.clipboard.writeText(p);
      } catch {/* ignore */}
    }
  };
  const moveToQueuePrompt = () => {
    if (!singleTask) return;
    const next = window.prompt("Move to queue (name):", singleTask.queue);
    if (next && next.trim()) void store.moveToQueue(singleTask.id, next.trim());
  };
  const changeCategoryPrompt = () => {
    if (!singleTask) return;
    const next = window.prompt("Change category:", singleTask.category);
    if (next && next.trim()) void store.setCategory(singleTask.id, next.trim());
  };
  const renameSel = () => {
    if (!singleTask) return;
    const next = window.prompt("Rename to:", singleTask.filename);
    if (next && next.trim() && next !== singleTask.filename) {
      void store.rename(singleTask.id, next.trim());
    }
  };
  /** Remove the task AND the file on disk. The store only knows
   *  remove(id); the file deletion needs a Tauri command. The
   *  existing `deleteFile` API does exactly this. */
  const removeWithFile = async () => {
    if (!singleTask) return;
    if (!window.confirm(`Delete ${singleTask.filename || "this file"} and the on-disk file?`)) return;
    try {
      const api = await import("../lib/api");
      await api.deleteFile(singleTask.id);
    } catch {
      await store.remove(singleTask.id);
    }
  };

  return (
    <div className="menubar">
      <div className="menubar__brand">DownPilot</div>
      <ul className="menubar__list">
        <Menu
          label="File"
          items={[
            { label: "New Download…", shortcut: "Ctrl+N", onClick: props.onAdd },
            { label: "Add Batch Download…", onClick: props.onBatch },
            { label: "Import Downloads…", onClick: () => importExport("import") },
            { label: "Export Downloads…", onClick: () => importExport("export") },
            { sep: true },
            { label: "View History", onClick: props.onHistory },
            { sep: true },
            { label: "Close", onClick: () => window.close?.() },
          ]}
        />
        <Menu
          label="Downloads"
          items={[
            { label: "Resume", shortcut: "Space", onClick: resumeSel, disabled: !hasSel },
            { label: "Pause", onClick: pauseSel, disabled: !hasSel },
            { label: "Stop", onClick: cancelSel, disabled: !hasSel },
            { label: "Force Start", onClick: forceStartSel, disabled: !hasSel },
            { label: "Retry", onClick: retrySel, disabled: !sel.some((id) => store.tasks.find((t) => t.id === id)?.status === "failed") },
            { label: "Redownload", onClick: redownSel, disabled: !hasSel },
            { sep: true },
            { label: "Open File", onClick: openFileSel, disabled: !(singleTask && singleTask.status === "completed") },
            { label: "Open Folder", onClick: openFolderSel, disabled: !singleTask },
            { sep: true },
            { label: "Copy URL", onClick: copyUrl, disabled: !singleTask },
            { label: "Copy Path", onClick: copyPath, disabled: !singleTask },
            { sep: true },
            { label: "Rename", shortcut: "F2", onClick: renameSel, disabled: !single },
            { label: "Move to Queue…", onClick: moveToQueuePrompt, disabled: !singleTask },
            { label: "Change Category…", onClick: changeCategoryPrompt, disabled: !singleTask },
            { label: "Properties", shortcut: "Ctrl+P", onClick: props.onOpenProps, disabled: !single },
            { sep: true },
            { label: "Delete", shortcut: "Del", onClick: removeSel, disabled: !hasSel },
            { label: "Delete + File", onClick: removeWithFile, disabled: !singleTask },
          ]}
        />
        <Menu
          label="View"
          items={[
            { label: "All Downloads", onClick: () => store.setSidebarKey({ kind: "all" }) },
            { label: "Unfinished", onClick: () => store.setSidebarKey({ kind: "status", value: "unfinished" }) },
            { label: "Finished", onClick: () => store.setSidebarKey({ kind: "status", value: "finished" }) },
            { label: "Paused", onClick: () => store.setSidebarKey({ kind: "status", value: "paused" }) },
            { label: "Errors", onClick: () => store.setSidebarKey({ kind: "status", value: "failed" }) },
            { sep: true },
            { label: props.detailsOpen ? "Hide Details" : "Show Details", onClick: props.onToggleDetails },
          ]}
        />
        <Menu
          label="Tasks"
          items={[
            { label: "Add URL…", shortcut: "Ctrl+N", onClick: props.onAdd },
            { label: "Batch Download…", onClick: props.onBatch },
            { label: "Grabber…", onClick: props.onGrabber },
            { label: "Site Manager…", onClick: props.onSites },
            { label: "Categories…", onClick: props.onCategories },
            { label: "Scheduler…", onClick: props.onScheduler },
            { label: "Queue Manager…", onClick: props.onQueues },
            { sep: true },
            { label: "Start All", onClick: () => store.resumeAll() },
            { label: "Pause All", onClick: () => store.pauseAll() },
            { label: "Stop All", onClick: () => store.stopAll() },
            { label: "Retry All Failed", onClick: () => store.retryAll() },
            { sep: true },
            { label: "Clear Finished", onClick: () => store.clearFinished() },
          ]}
        />
        <Menu
          label="Tools"
          items={[
            { label: "Options…", shortcut: "Ctrl+,", onClick: props.onSettings },
            { label: "Browser Integration…", onClick: props.onSettings },
            { sep: true },
            { label: "About DownPilot", onClick: props.onAbout },
          ]}
        />
        <Menu
          label="Help"
          items={[
            { label: "Keyboard Shortcuts", onClick: () => {
              alert(
                "DownPilot keyboard shortcuts\n\n" +
                "  Ctrl+N          New download\n" +
                "  Ctrl+F          Focus search\n" +
                "  Ctrl+A          Select all\n" +
                "  Ctrl+P          Properties\n" +
                "  Ctrl+,          Options\n" +
                "  Enter           Open / properties\n" +
                "  Del             Remove selected\n" +
                "  F2              Rename\n" +
                "  Space           Pause/Resume\n" +
                "  Double-click    Open file if done, else Properties"
              );
            } },
            { label: "About DownPilot", onClick: props.onAbout },
          ]}
        />
      </ul>
      <SearchBox store={store} />
    </div>
  );
}

function SearchBox({ store }: { store: DownloadsStore }) {
  const [value, setValue] = useState(store.search);
  const ref = useRef<HTMLInputElement | null>(null);
  useEffect(() => setValue(store.search), [store.search]);
  useEffect(() => {
    const onFocus = () => ref.current?.focus();
    document.addEventListener("downpilot:focus-search", onFocus);
    return () => document.removeEventListener("downpilot:focus-search", onFocus);
  }, []);
  return (
    <div className="menubar__search">
      <svg viewBox="0 0 16 16" className="h-3.5 w-3.5 text-slate-500" fill="none" stroke="currentColor" strokeWidth="1.5">
        <circle cx="7" cy="7" r="4.5" />
        <path d="M10.5 10.5l3 3" />
      </svg>
      <input
        ref={ref}
        type="text"
        value={value}
        onChange={(e) => {
          setValue(e.target.value);
          store.setSearch(e.target.value);
        }}
        placeholder="Search downloads…"
        title="Search supports: bare text, since:YYYY-MM-DD, until:YYYY-MM-DD, today, week"
        className="menubar__search-input"
      />
      {value && (
        <button
          type="button"
          aria-label="Clear search"
          className="menubar__search-clear"
          onClick={() => {
            setValue("");
            store.setSearch("");
            ref.current?.focus();
          }}
        >
          ×
        </button>
      )}
    </div>
  );
}

async function importExport(kind: "import" | "export") {
  if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
    try {
      const dialog = await import("@tauri-apps/plugin-dialog");
      if (kind === "export") {
        const p = await dialog.save({
          defaultPath: "downpilot-exports.json",
          filters: [{ name: "JSON", extensions: ["json"] }],
        });
        if (p) await import("../lib/api").then((a) => a.exportDownloads(p));
      } else {
        const f = await dialog.open({
          filters: [{ name: "JSON", extensions: ["json"] }],
        });
        if (typeof f === "string")
          await import("../lib/api").then((a) => a.importDownloads(f, true));
      }
    } catch {
      /* ignore */
    }
  }
}
