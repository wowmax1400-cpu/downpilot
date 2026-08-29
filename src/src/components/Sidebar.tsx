// Modern dark sidebar: clean muted navigation with indigo accent on active.
// Categories (built-in + user), Grabber (sites), Queues. No boxes, no tree
// chevrons — just a quiet vertical nav.

import { useMemo } from "react";
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

interface NavItem {
  key: SidebarKey;
  label: string;
  countKey: string;
  icon: React.ReactNode;
}

const iconWrap = (children: React.ReactNode) => (
  <span className="mr-2 inline-flex h-4 w-4 shrink-0 items-center justify-center">
    {children}
  </span>
);

function StatusIcon({ status }: { status: string }) {
  const c = "h-3.5 w-3.5";
  switch (status) {
    case "all":
      return (
        <svg viewBox="0 0 16 16" className={c} fill="none" stroke="currentColor" strokeWidth="1.5">
          <rect x="2" y="3" width="12" height="2.5" rx="0.5" />
          <rect x="2" y="6.75" width="12" height="2.5" rx="0.5" />
          <rect x="2" y="10.5" width="12" height="2.5" rx="0.5" />
        </svg>
      );
    case "unfinished":
      return (
        <svg viewBox="0 0 16 16" className={c} fill="none" stroke="currentColor" strokeWidth="1.5">
          <circle cx="8" cy="8" r="5.5" />
          <path d="M8 5v3.2l2 1.2" />
        </svg>
      );
    case "finished":
      return (
        <svg viewBox="0 0 16 16" className={c} fill="none" stroke="currentColor" strokeWidth="1.8">
          <path d="M3.5 8.5l3 3 6-6.5" />
        </svg>
      );
    case "paused":
      return (
        <svg viewBox="0 0 16 16" className={c} fill="currentColor">
          <rect x="4.5" y="3" width="2.5" height="10" rx="0.4" />
          <rect x="9" y="3" width="2.5" height="10" rx="0.4" />
        </svg>
      );
    case "failed":
      return (
        <svg viewBox="0 0 16 16" className={c} fill="none" stroke="currentColor" strokeWidth="1.5">
          <path d="M3.5 3.5l9 9M12.5 3.5l-9 9" />
        </svg>
      );
    case "grabber":
      return (
        <svg viewBox="0 0 16 16" className={c} fill="none" stroke="currentColor" strokeWidth="1.5">
          <circle cx="7" cy="7" r="4" />
          <path d="M10 10l3 3" />
        </svg>
      );
    default:
      return null;
  }
}

function CategoryIcon({ name, category }: { name: string; category: string }) {
  const iconResult = fileIconFor(name, category);
  const Comp = iconResult.Icon;
  return <Comp size={14} className={iconResult.color} />;
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
  const queueStats = useMemo(() => {
    const map: Record<string, { active: number; queued: number; total: number }> = {};
    for (const q of queues) {
      map[q.name] = { active: 0, queued: 0, total: 0 };
    }
    for (const t of tasks) {
      const slot = map[t.queue];
      if (!slot) continue;
      slot.total += 1;
      if (t.status === "downloading") slot.active += 1;
      else if (t.status === "queued" || t.status === "scheduled") slot.queued += 1;
    }
    return map;
  }, [queues, tasks]);
  const overriddenQueues = useMemo(() => {
    const s = new Set<string>();
    for (const p of activeProfiles) for (const q of p.affected_queues) s.add(q);
    return s;
  }, [activeProfiles]);
  const builtIn: NavItem[] = [
    { key: { kind: "all" }, label: "All Downloads", countKey: "all", icon: <StatusIcon status="all" /> },
    { key: { kind: "status", value: "unfinished" }, label: "Unfinished", countKey: "unfinished", icon: <StatusIcon status="unfinished" /> },
    { key: { kind: "status", value: "finished" }, label: "Finished", countKey: "finished", icon: <StatusIcon status="finished" /> },
    { key: { kind: "status", value: "paused" }, label: "Paused", countKey: "paused", icon: <StatusIcon status="paused" /> },
    { key: { kind: "status", value: "failed" }, label: "Errors", countKey: "failed", icon: <StatusIcon status="failed" /> },
  ];

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

  const NavButton = ({
    k,
    label,
    count,
    icon,
    title,
  }: {
    k: SidebarKey;
    label: string;
    count?: number;
    icon: React.ReactNode;
    title?: string;
  }) => {
    const activeHere = isActive(k);
    return (
      <button
        type="button"
        onClick={() => onSelect(k)}
        title={title}
        className={
          "group flex w-full items-center rounded-md px-2.5 py-1.5 text-left text-sm transition-colors " +
          (activeHere
            ? "bg-indigo-500/10 text-indigo-400"
            : "text-txt-dim hover:bg-white/5 hover:text-txt")
        }
      >
        {iconWrap(icon)}
        <span className="flex-1 truncate">{label}</span>
        {typeof count === "number" && (
          <span
            className={
              "ml-2 inline-flex min-w-[1.5rem] justify-end font-mono text-[11px] tabular-nums " +
              (activeHere ? "text-indigo-400/80" : "text-txt-faint")
            }
          >
            {count}
          </span>
        )}
      </button>
    );
  };

  return (
    <aside className="flex w-56 shrink-0 flex-col border-r border-white/5 bg-surface">
      {/* Categories */}
      <div className="flex flex-col gap-0.5 p-2">
        <div className="px-2 pb-1 pt-0.5 text-[10.5px] font-semibold uppercase tracking-wider text-txt-faint">
          Categories
        </div>
        {builtIn.map((it) => (
          <NavButton
            key={"bi-" + it.countKey}
            k={it.key}
            label={it.label}
            count={counts[it.countKey] ?? 0}
            icon={it.icon}
          />
        ))}
        {categories.map((c) => (
          <NavButton
            key={"cat-" + c.name}
            k={{ kind: "category", value: c.name }}
            label={c.label || c.name}
            count={counts[`cat:${c.name}`] ?? 0}
            icon={<CategoryIcon name={c.name} category={c.name} />}
            title={
              c.save_dir
                ? `${c.label || c.name} → ${c.save_dir}`
                : c.label || c.name
            }
          />
        ))}
      </div>

      {/* Grabber */}
      <div className="flex flex-col gap-0.5 border-t border-white/5 p-2">
        <div className="px-2 pb-1 pt-0.5 text-[10.5px] font-semibold uppercase tracking-wider text-txt-faint">
          Grabber
        </div>
        <NavButton
          k={{ kind: "site", value: "_grabber" }}
          label="Site Grabber"
          count={counts["grabber"] ?? 0}
          icon={<StatusIcon status="grabber" />}
        />
      </div>

      {/* Queues */}
      <div className="flex min-h-0 flex-1 flex-col gap-0.5 overflow-y-auto border-t border-white/5 p-2">
        <div className="px-2 pb-1 pt-0.5 text-[10.5px] font-semibold uppercase tracking-wider text-txt-faint">
          Queues
        </div>
        {queues.length === 0 ? (
          <div className="px-2 py-1 text-xs text-txt-faint">No queues yet</div>
        ) : (
          queues.map((q) => {
            const s = queueStats[q.name] ?? { active: 0, queued: 0, total: 0 };
            const eff = effectiveSpeed(q.name);
            const overridden = overriddenQueues.has(q.name);
            const cap = q.speed_limit_kbps;
            const capLabel = cap === 0 ? "∞" : cap >= 1024 ? Math.round(cap / 1024) + " MB/s" : cap + " kB/s";
            const effLabel = eff === 0 ? "∞" : eff >= 1024 ? Math.round(eff / 1024) + " MB/s" : eff + " kB/s";
            const isActive = active.kind === "queue" && active.value === q.name;
            return (
              <button
                key={"q-" + q.name}
                onClick={() => onSelect({ kind: "queue", value: q.name })}
                className={
                  "group flex w-full items-start gap-1.5 rounded-md px-2 py-1 text-left text-[12.5px] transition-colors " +
                  (isActive
                    ? "bg-indigo-500/15 text-txt"
                    : "text-txt-dim hover:bg-white/[0.04] hover:text-txt")
                }
              >
                <span className="mt-0.5 inline-flex h-4 w-4 shrink-0 items-center justify-center">
                  <svg viewBox="0 0 16 16" className="h-3.5 w-3.5" fill="none" stroke="currentColor" strokeWidth="1.5">
                    <path d="M2 4h12M2 8h12M2 12h8" />
                  </svg>
                </span>
                <span className="min-w-0 flex-1">
                  <span className="flex items-center justify-between gap-2">
                    <span className="truncate">{q.label || q.name}</span>
                    <span className="shrink-0 text-[10.5px] tabular-nums text-txt-faint">
                      {s.active}/{q.max_simultaneous}
                    </span>
                  </span>
                  <span className="flex items-center gap-1.5 text-[10.5px] text-txt-faint">
                    {s.active > 0 && (
                      <span title="active" className="inline-block h-1.5 w-1.5 rounded-full bg-emerald-400/90" />
                    )}
                    <span className="tabular-nums">{s.queued} queued</span>
                    <span>·</span>
                    <span className="tabular-nums" title={overridden ? "Schedule override active" : "Configured cap"}>
                      {overridden ? "⚡ " : ""}{capLabel}
                    </span>
                    {overridden && eff !== cap && (
                      <span className="tabular-nums text-amber-300/80">→ {effLabel}</span>
                    )}
                  </span>
                </span>
              </button>
            );
          })
        )}
      </div>
    </aside>
  );
}
