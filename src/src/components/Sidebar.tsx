import type { FC } from "react";
import type { DownloadsStore, CategoryFilter } from "../store/useDownloads";
import {
  ArrowDownCircleIcon,
  ArchiveIcon,
  CheckCircleIcon,
  ChevronLeftIcon,
  CpuIcon,
  FileTextIcon,
  FilmIcon,
  FolderIcon,
  GearIcon,
  LayoutGridIcon,
  MusicIcon,
  PauseCircleIcon,
} from "./icons";
import { IconButton } from "./IconButton";

const STATUS_CATS: { key: CategoryFilter; label: string; icon: FC<{ className?: string }> }[] = [
  { key: "all", label: "All", icon: LayoutGridIcon },
  { key: "downloading", label: "Active", icon: ArrowDownCircleIcon },
  { key: "paused", label: "Paused", icon: PauseCircleIcon },
  { key: "completed", label: "Completed", icon: CheckCircleIcon },
];

const TYPE_CATS: { key: CategoryFilter; label: string; icon: FC<{ className?: string }> }[] = [
  { key: "video", label: "Video", icon: FilmIcon },
  { key: "music", label: "Music", icon: MusicIcon },
  { key: "compressed", label: "Compressed", icon: ArchiveIcon },
  { key: "documents", label: "Documents", icon: FileTextIcon },
  { key: "programs", label: "Programs", icon: CpuIcon },
  { key: "other", label: "Other", icon: FolderIcon },
];

export const Sidebar: FC<{
  store: DownloadsStore;
  collapsed: boolean;
  onToggleCollapse: () => void;
  onOpenSettings: () => void;
}> = ({ store, collapsed, onToggleCollapse, onOpenSettings }) => {
  const NavBtn: FC<{ cat: (typeof STATUS_CATS)[number]; index: number }> = ({ cat, index }) => {
    const active = store.filter === cat.key;
    const Icon = cat.icon;
    return (
      <button
        key={cat.key}
        onClick={() => store.setFilter(cat.key)}
        title={collapsed ? cat.label : undefined}
        className={`group relative flex w-full items-center gap-3 rounded-lg text-[13px] transition-colors ${
          collapsed ? "justify-center px-0 py-2.5" : "px-3 py-2"
        } ${
          active
            ? "bg-accent-soft font-medium text-accent"
            : "text-txt-dim hover:bg-surface-3 hover:text-txt"
        }`}
      >
        <span className="relative">
          <Icon
            className={`h-[18px] w-[18px] ${
              active ? "text-accent" : "text-txt-faint group-hover:text-txt-dim"
            }`}
          />
          {index === 0 && !collapsed && store.counts.downloading > 0 && (
            <span className="absolute -right-1.5 -top-1.5 flex h-3.5 min-w-3.5 items-center justify-center rounded-full bg-accent px-0.5 text-[8px] font-bold text-accent-text">
              {store.counts.downloading}
            </span>
          )}
        </span>
        {!collapsed && <span className="flex-1 text-left">{cat.label}</span>}
        {!collapsed && (
          <span className="text-[11px] tabular-nums text-txt-faint">{store.counts[cat.key] ?? ""}</span>
        )}
      </button>
    );
  };

  return (
    <aside
      className={`dp-light-edge flex shrink-0 flex-col border-r border-line bg-surface transition-[width] duration-200 ${
        collapsed ? "w-14" : "w-52"
      }`}
    >
      {/* Logo + collapse toggle */}
      <div className={`flex h-14 items-center border-b border-line ${collapsed ? "justify-center" : "justify-between px-3"}`}>
        {!collapsed ? (
          <div className="flex items-center gap-2.5">
            <div
              className="flex h-8 w-8 items-center justify-center rounded-lg text-[15px] font-bold"
              style={{ background: "var(--accent-soft)", color: "var(--accent)" }}
            >
              D
            </div>
            <span className="text-sm font-medium text-txt">DownPilot</span>
          </div>
        ) : (
          <div
            className="flex h-8 w-8 items-center justify-center rounded-lg text-[15px] font-bold"
            style={{ background: "var(--accent-soft)", color: "var(--accent)" }}
          >
            D
          </div>
        )}
        <IconButton title={collapsed ? "Expand sidebar" : "Collapse sidebar"} onClick={onToggleCollapse} size="sm">
          <ChevronLeftIcon className={`h-4 w-4 transition-transform duration-200 ${collapsed ? "rotate-180" : ""}`} />
        </IconButton>
      </div>

      {/* Categories */}
      <nav className="flex-1 space-y-0.5 overflow-y-auto px-2 py-3">
        {!collapsed && (
          <div className="px-3 pb-1.5 text-[10px] font-semibold uppercase tracking-widest text-txt-faint">
            Library
          </div>
        )}
        {STATUS_CATS.map((cat, i) => (
          <NavBtn key={cat.key} cat={cat} index={i} />
        ))}

        {!collapsed && (
          <div className="px-3 pb-1.5 pt-4 text-[10px] font-semibold uppercase tracking-widest text-txt-faint">
            Categories
          </div>
        )}
        {TYPE_CATS.map((cat) => (
          <NavBtn key={cat.key} cat={cat} index={-1} />
        ))}
      </nav>

      {/* Bottom: settings */}
      <div className="border-t border-line p-2">
        <button
          onClick={onOpenSettings}
          title={collapsed ? "Settings" : undefined}
          className={`flex w-full items-center gap-3 rounded-lg text-[13px] text-txt-dim transition-colors duration-200 hover:bg-surface-3 hover:text-txt ${
            collapsed ? "justify-center px-0 py-2.5" : "px-3 py-2"
          }`}
        >
          <GearIcon className="h-[18px] w-[18px] text-txt-faint" />
          {!collapsed && <span>Settings</span>}
        </button>
      </div>
    </aside>
  );
};
