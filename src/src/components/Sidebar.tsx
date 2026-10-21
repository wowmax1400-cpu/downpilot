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
    <svg
      viewBox="0 0 16 16"
      className={`h-3 w-3 shrink-0 transition-transform ${open ? "rotate-90 text-txt-dim" : "text-txt-faint"}`}
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
    >
      <path d="M6 4l4.5 4L6 12" />
    </svg>
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
      className="flex w-full items-center gap-1 rounded-none px-1 py-[2px] text-[10px] font-semibold uppercase tracking-wider text-gray-500 hover:bg-[var(--row-hover)] hover:text-gray-300"
    >
      <Chevron open={!!open[id]} />
      {title}
    </button>
  );

  /** One selectable tree row: depth indent, icon, label, right counter. */
  const TreeRow = ({
    k,
    label,
    count,
    icon,
    title,
    depth = 0,
  }: {
    k: SidebarKey;
    label: string;
    count?: number;
    icon: React.ReactNode;
    title?: string;
    depth?: number;
  }) => {
    const activeHere = isActive(k);
    return (
      <button
        type="button"
        onClick={() => onSelect(k)}
        title={title}
        className={
          "flex w-full items-center rounded-none py-[3px] pr-1.5 text-left text-xs transition-colors " +
          (activeHere
            ? "bg-accent-soft text-accent"
            : "text-txt-dim hover:bg-[var(--row-hover)] hover:text-txt")
        }
        style={{ paddingLeft: `${6 + depth * 14}px` }}
      >
        {iconWrap(icon)}
        <span className="flex-1 truncate">{label}</span>
        {typeof count === "number" && (
          <span
            className={
              "ml-2 inline-flex min-w-[1.25rem] justify-end font-mono text-[10.5px] tabular-nums " +
              (activeHere ? "text-accent" : "text-txt-faint")
            }
          >
            {count}
          </span>
        )}
      </button>
    );
  };

  return (
    <aside className="flex w-[216px] shrink-0 flex-col overflow-y-auto border-r border-[var(--line)] bg-[var(--surface)]">
      {/* ▼ 📁 All Downloads + nested file-type categories */}
        <div className="flex flex-col gap-0 px-1.5 pb-0.5">
        <SectionHeader id="categories" title="Categories" />
        {open.categories && (
          <>
            <TreeRow
              k={{ kind: "all" }}
              label="All Downloads"
              count={counts["all"] ?? 0}
              icon={
                <svg viewBox="0 0 16 16" className="h-3.5 w-3.5 text-sky-300" fill="none" stroke="currentColor" strokeWidth="1.4">
                  <path d="M1.5 4.5A1.5 1.5 0 013 3h3l1.5 1.5H13A1.5 1.5 0 0114.5 6v6A1.5 1.5 0 0113 13.5H3A1.5 1.5 0 011.5 12V4.5z" />
                </svg>
              }
            />
            {orderedCategories.map((c) => (
              <TreeRow
                key={"cat-" + c.name}
                k={{ kind: "category", value: c.name }}
                label={c.label || c.name}
                count={counts[`cat:${c.name}`] ?? 0}
                depth={1}
                icon={<CategoryIcon name={c.name} category={c.name} />}
                title={
                  (c.save_dir ? `${c.label || c.name} → ${c.save_dir}` : c.label || c.name) +
                  (c.extensions ? `  (${c.extensions})` : "")
                }
              />
            ))}
          </>
        )}
      </div>

      {/* ▼ ⏱ Queues */}
      <div className="flex flex-col gap-0 px-1.5 pb-0.5">
        <SectionHeader id="queues" title="Queues" />
        {open.queues &&
          (queues.length === 0 ? (
            <div className="py-1 pl-7 text-xs text-txt-faint">No queues yet</div>
          ) : (
            queues.map((q) => {
              const s = queueStats[q.name] ?? { active: 0, queued: 0, total: 0 };
              const label = queueLabel(q.name, q.label);
              // 🟢 while the queue has live transfers, 🕒 when idle.
              const dot =
                s.active > 0 ? (
                  <span className="inline-block h-1.5 w-1.5 rounded-[1px] bg-emerald-500" />
                ) : (
                  <svg viewBox="0 0 16 16" className="h-3.5 w-3.5 text-txt-faint" fill="none" stroke="currentColor" strokeWidth="1.4">
                    <circle cx="8" cy="8" r="6" />
                    <path d="M8 4.5V8l2.5 1.5" />
                  </svg>
                );
              return (
                <TreeRow
                  key={"q-" + q.name}
                  k={{ kind: "queue", value: q.name }}
                  label={label}
                  count={s.total}
                  depth={1}
                  icon={dot}
                  title={`${label} · ${s.active} active / ${q.max_simultaneous} max · ${s.queued} queued`}
                />
              );
            })
          ))}
      </div>

      {/* ▼ ❓ Unfinished */}
      <div className="flex flex-col gap-0 px-1.5 pb-1">
        <SectionHeader id="unfinished" title="Unfinished" />
        {open.unfinished && (
          <TreeRow
            k={{ kind: "status", value: "unfinished" }}
            label="Unfinished"
            count={counts["unfinished"] ?? 0}
            icon={
              <svg viewBox="0 0 16 16" className="h-3.5 w-3.5 text-amber-300/90" fill="none" stroke="currentColor" strokeWidth="1.4">
                <circle cx="8" cy="8" r="6" />
                <path d="M6.2 6.2a1.9 1.9 0 113.1 1.4c-.6.45-1.3.75-1.3 1.6M8 11.5v.02" />
              </svg>
            }
            title="Downloads that have not finished yet (any non-complete state)"
          />
        )}
      </div>
    </aside>
  );
}
