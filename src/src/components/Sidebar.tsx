// IDM-style expandable category TREE (spec 2):
//
//   ▼ 📁 All Downloads (n)
//        ├─ 📦 Compressed (n)
//        ├─ 📄 Documents (n)
//        ├─ 🎵 Music (n)
//        ├─ ⚙️ Programs (n)
//        └─ 🎬 Videos (n)
//   ▼ ⏱ Queues
//        ├─ 🟢 Main Download Queue (n)
//        └─ 🕒 Synchronization Queue (n)
//   ▼ ❓ Unfinished (n)
//
// Sections collapse via their chevron; every row carries a
// right-aligned counter badge. Category rows reuse the table's
// `fileIconFor` glyphs so a .zip looks like a .zip everywhere. Queue
// rows show a glowing green dot while they have active transfers and a
// clock when idle — the 🟢/🕒 of the spec.

import { useMemo, useState } from "react";
import type { Category, DownloadQueue, SidebarKey } from "../types";
import { fileIconFor } from "./icons";

type Counts = Record<string, number>;

interface SidebarProps {
  active: SidebarKey;
  onSelect: (k: SidebarKey) => void;
  counts: Counts;
  categories: Category[];
  queues: DownloadQueue[];
  tasks: { queue: string; status: string }[];
  effectiveSpeed: (queue: string) => number;
  activeProfiles: { affected_queues: string[] }[];
}

const iconWrap = (children: React.ReactNode) => (
  <span className="mr-1.5 inline-flex h-3.5 w-3.5 shrink-0 items-center justify-center">
    {children}
  </span>
);

/** Representative filename per category so the shared `fileIconFor`
 *  resolver yields the right glyph (it keys mostly off the extension). */
const CATEGORY_PROBE: Record<string, string> = {
  all: "folder",
  compressed: "archive.zip",
  documents: "file.pdf",
  music: "track.flac",
  video: "clip.mp4",
  programs: "pkg.AppImage",
  images: "image.png",
  other: "file",
  unfinished: "partial.download",
};

function CategoryIcon({ name, category }: { name: string; category: string }) {
  const probe = CATEGORY_PROBE[category] ?? name;
  const iconResult = fileIconFor(probe, category);
  const Comp = iconResult.Icon;
  return <Comp size={14} className={iconResult.color} />;
}

/** IDM's own ordering, so the tree doesn't reorder itself when the
 *  engine adds a category. Unknown categories fall to the end. */
const IDM_CATEGORY_ORDER = [
  "compressed",
  "documents",
  "music",
  "programs",
  "video",
  "images",
  "other",
];

/** IDM label mapping for the queue tree rows. */
function queueLabel(name: string, label?: string): string {
  if (name === "general") return "Main Download Queue";
  if (name === "synchronization") return "Synchronization Queue";
  return label || name;
}

function Chevron({ open }: { open: boolean }) {
  return (
    <span className="inline-flex h-3 w-3 items-center justify-center text-[10px] leading-none text-gray-500 hover:text-[#0078D7] mr-1 cursor-pointer select-none">
      {open ? "▼" : "▶"}
    </span>
  );
}

export function Sidebar({
  active,
  onSelect,
  counts,
  categories,
  queues,
  tasks,
  effectiveSpeed,
  activeProfiles,
}: SidebarProps) {
  // Collapsible tree sections — all expanded by default (IDM ships the
  // tree expanded; the chevron collapses a whole branch).
  const [open, setOpen] = useState<Record<string, boolean>>({
    categories: true,
    queues: true,
    unfinished: true,
  });
  const toggle = (k: string) => setOpen((o) => ({ ...o, [k]: !o[k] }));

  const queueStats = useMemo(() => {
    const map: Record<string, { active: number; queued: number; total: number }> = {};
    for (const q of queues) {
      map[q.name] = map[q.name] ?? { active: 0, queued: 0, total: 0 };
    }
    for (const t of tasks) {
      const slot = map[t.queue] ?? (map[t.queue] = { active: 0, queued: 0, total: 0 });
      slot.total += 1;
      if (t.status === "downloading") slot.active += 1;
      else if (t.status === "queued" || t.status === "scheduled") slot.queued += 1;
    }
    return map;
  }, [queues, tasks]);

  void effectiveSpeed;
  void activeProfiles;

  const orderedCategories = useMemo(() => {
    const rank = (name: string) => {
      const i = IDM_CATEGORY_ORDER.indexOf(name);
      return i === -1 ? IDM_CATEGORY_ORDER.length : i;
    };
    return [...categories].sort((a, b) => {
      const ra = rank(a.name);
      const rb = rank(b.name);
      if (ra !== rb) return ra - rb;
      return a.name.localeCompare(b.name);
    });
  }, [categories]);

  const isActive = (k: SidebarKey) => {
    if (k.kind !== active.kind) return false;
    if (k.kind === "all") return true;
    if (k.kind === "status") return active.kind === "status" && active.value === k.value;
    if (k.kind === "category") return active.kind === "category" && active.value === k.value;
    if (k.kind === "queue") return active.kind === "queue" && active.value === k.value;
    if (k.kind === "site") return active.kind === "site" && active.value === k.value;
    if (k.kind === "schedule") return active.kind === "schedule" && active.value === k.value;
    return false;
  };

/** Section header: small disclosure triangle + tiny caps title,
 *  collapses the branch (Windows Explorer / IDM tree pane, rule 3). */
  const SectionHeader = ({ id, title }: { id: string; title: string }) => (
    <button
      type="button"
      onClick={() => toggle(id)}
      className="flex w-full items-center gap-1 px-1 py-[2px] text-[10px] font-bold uppercase tracking-wider text-black hover:bg-[#e8f0fe]"
    >
      <Chevron open={!!open[id]} />
      {title}
    </button>
  );

  /** One selectable tree row: depth indent, icon, label, right counter. */
  const TreeRow = ({
    k,
    label,
    icon,
    title,
    depth = 0,
    hasToggle = false,
    isOpen = false,
    onToggle,
  }: {
    k: SidebarKey;
    label: string;
    icon: React.ReactNode;
    title?: string;
    depth?: number;
    hasToggle?: boolean;
    isOpen?: boolean;
    onToggle?: () => void;
  }) => {
    const activeHere = isActive(k);
    return (
      <div className={depth > 0 ? "tree-node" : "relative pl-0 leading-5"}>
        <div className="flex items-center">
          {hasToggle ? (
            <button
              type="button"
              onClick={(e) => {
                e.stopPropagation();
                onToggle?.();
              }}
              className="inline-flex h-4 w-4 items-center justify-center text-[10px] leading-none text-slate-400 hover:text-cyan-400 mr-1 cursor-pointer shrink-0 z-10"
            >
              {isOpen ? "▼" : "▶"}
            </button>
          ) : depth === 0 ? (
            <span className="w-4 shrink-0 mr-1" />
          ) : null}
          <button
            type="button"
            onClick={() => onSelect(k)}
            title={title}
            className={
              "flex flex-1 items-center gap-2 px-2.5 py-1.5 text-left select-none sidebar-row-btn min-w-0 transition-all " +
              (activeHere
                ? "sidebar-selected-win32 font-semibold"
                : "text-slate-300 hover:bg-[#131d33] hover:text-white")
            }
          >
            {iconWrap(icon)}
            <span className="truncate tracking-tight">{label}</span>
          </button>
        </div>
      </div>
    );
  };

  return (
    <aside className="flex w-[215px] shrink-0 flex-col overflow-y-auto bg-[var(--bg-sidebar)] select-none border-r border-[#162032] shadow-[2px_0_10px_rgba(0,0,0,0.2)]">
      <div className="flex h-[28px] items-center border-b border-[#162032] bg-[#080c16] px-3 text-[11px] font-bold uppercase tracking-wider text-slate-400">
        <span>Categories</span>
      </div>

      <div className="flex flex-col gap-0.5 px-1.5 py-1.5 bg-[var(--bg-sidebar)] text-[12px]">
        {/* [-] All Downloads */}
        <TreeRow
          k={{ kind: "all" }}
          label="All Downloads"
          hasToggle
          isOpen={!!open.categories}
          onToggle={() => toggle("categories")}
          icon={
            <svg viewBox="0 0 16 16" className="h-3.5 w-3.5 text-amber-500" fill="currentColor">
              <path d="M1.5 3A1.5 1.5 0 0 0 0 4.5v7A1.5 1.5 0 0 0 1.5 13h13a1.5 1.5 0 0 0 1.5-1.5v-5A1.5 1.5 0 0 0 14.5 5H7.707L6.354 3.646A.5.5 0 0 0 6 3.5H1.5z" />
            </svg>
          }
        />
        {open.categories && (
          <div className="relative ml-1">
            {orderedCategories.map((c) => (
              <TreeRow
                key={"cat-" + c.name}
                k={{ kind: "category", value: c.name }}
                label={c.label || c.name}
                depth={1}
                icon={<CategoryIcon name={c.name} category={c.name} />}
                title={
                  (c.save_dir ? `${c.label || c.name} → ${c.save_dir}` : c.label || c.name) +
                  (c.extensions ? `  (${c.extensions})` : "")
                }
              />
            ))}
          </div>
        )}

        {/* [+] Unfinished */}
        <TreeRow
          k={{ kind: "status", value: "unfinished" }}
          label="Unfinished"
          hasToggle
          isOpen={!!open.unfinished}
          onToggle={() => toggle("unfinished")}
          icon={
            <svg viewBox="0 0 16 16" className="h-3.5 w-3.5 text-amber-600" fill="none" stroke="currentColor" strokeWidth="1.5">
              <circle cx="8" cy="8" r="6" />
              <path d="M6.2 6.2a1.9 1.9 0 113.1 1.4c-.6.45-1.3.75-1.3 1.6M8 11.5v.02" />
            </svg>
          }
        />

        {/* [+] Finished */}
        <TreeRow
          k={{ kind: "status", value: "finished" }}
          label="Finished"
          hasToggle
          isOpen={false}
          onToggle={() => onSelect({ kind: "status", value: "finished" })}
          icon={
            <svg viewBox="0 0 16 16" className="h-3.5 w-3.5 text-emerald-600" fill="none" stroke="currentColor" strokeWidth="1.8">
              <circle cx="8" cy="8" r="6" />
              <path d="M5 8.5l2 2 4-4" />
            </svg>
          }
        />

        {/* [+] Grabber projects */}
        <TreeRow
          k={{ kind: "site", value: "" }}
          label="Grabber projects"
          hasToggle
          isOpen={false}
          onToggle={() => {}}
          icon={
            <svg viewBox="0 0 16 16" className="h-3.5 w-3.5 text-sky-600" fill="none" stroke="currentColor" strokeWidth="1.4">
              <circle cx="8" cy="8" r="6" />
              <ellipse cx="8" cy="8" rx="3" ry="6" />
              <line x1="2" y1="8" x2="14" y2="8" />
            </svg>
          }
        />

        {/* [+] Queues */}
        <TreeRow
          k={{ kind: "queue", value: "general" }}
          label="Queues"
          hasToggle
          isOpen={!!open.queues}
          onToggle={() => toggle("queues")}
          icon={
            <svg viewBox="0 0 16 16" className="h-3.5 w-3.5 text-gray-600" fill="none" stroke="currentColor" strokeWidth="1.4">
              <circle cx="8" cy="8" r="6" />
              <path d="M8 4.5V8l2.5 1.5" />
            </svg>
          }
        />
        {open.queues && (
          <div className="relative ml-1">
            {queues.map((q) => {
              const label = queueLabel(q.name, q.label);
              return (
                <TreeRow
                  key={"q-" + q.name}
                  k={{ kind: "queue", value: q.name }}
                  label={label}
                  depth={1}
                  icon={
                    <svg viewBox="0 0 16 16" className="h-3 w-3 text-amber-500" fill="none" stroke="currentColor" strokeWidth="1.4">
                      <circle cx="8" cy="8" r="6" />
                      <path d="M8 4.5V8l2 1" />
                    </svg>
                  }
                  title={label}
                />
              );
            })}
          </div>
        )}
      </div>
    </aside>
  );
}
