import type { FC } from "react";
import type { DownloadsStore } from "../store/useDownloads";
import { ChevronDownIcon, GaugeIcon, GlobeIcon, PaletteIcon, PlusIcon, SearchIcon } from "./icons";
import { IconButton } from "./IconButton";

const FILTER_TITLES: Record<string, string> = {
  all: "All Downloads",
  downloading: "Active Downloads",
  paused: "Paused Downloads",
  completed: "Completed Downloads",
  video: "Videos",
  music: "Music",
  compressed: "Archives",
  documents: "Documents",
  programs: "Programs",
  other: "Other Files",
};

export const Toolbar: FC<{
  store: DownloadsStore;
  onAdd: () => void;
  onOpenTheme: () => void;
  onOpenGrabber: () => void;
}> = ({ store, onAdd, onOpenTheme, onOpenGrabber }) => {
  const title = FILTER_TITLES[store.filter] ?? "Downloads";
  return (
    <header className="flex h-14 shrink-0 items-center gap-3 border-b border-line bg-surface px-4">
      {/* Section title — anchors the user in the app */}
      <div className="flex min-w-0 items-baseline gap-2">
        <h1 className="truncate text-sm font-medium text-txt">{title}</h1>
        <span className="tnum text-xs text-txt-faint">{store.tasks.length}</span>
      </div>

      {/* Breathing room before the search */}
      <div className="mx-2 h-5 w-px shrink-0 bg-line" />

      {/* Search — generous width, centered in remaining space */}
      <div className="relative min-w-0 flex-1">
        <SearchIcon className="pointer-events-none absolute left-3 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-txt-faint" />
        <input
          type="text"
          placeholder="Search downloads…"
          value={store.search}
          onChange={(e) => store.setSearch(e.target.value)}
          className="h-9 w-full max-w-md rounded-lg border border-line bg-surface-2 pl-9 pr-3 text-xs text-txt outline-none transition-all duration-200 placeholder:text-txt-faint focus:border-accent focus:bg-surface"
        />
      </div>

      {/* Global actions — right side, evenly spaced */}
      <div className="flex shrink-0 items-center gap-2.5">
        {/* Speed limit */}
        <label className="flex h-9 items-center gap-1.5 rounded-lg border border-line bg-surface-2 px-2.5 transition-colors duration-200 focus-within:border-accent">
          <GaugeIcon className="h-3.5 w-3.5 text-txt-faint" />
          <select
            className="tnum bg-transparent text-xs text-txt outline-none [&>option]:bg-surface-2"
            value={store.settings?.speed_limit_kbps ?? 0}
            onChange={(e) => store.updateSetting("speed_limit_kbps", e.target.value)}
            title="Speed limit"
          >
            <option value="0">Unlimited</option>
            <option value="1024">1 MB/s</option>
            <option value="5120">5 MB/s</option>
            <option value="10240">10 MB/s</option>
            <option value="51200">50 MB/s</option>
          </select>
          <ChevronDownIcon className="pointer-events-none h-3 w-3 text-txt-faint" />
        </label>

        <IconButton title="Site grabber — list downloadable files from a page" onClick={onOpenGrabber}>
          <GlobeIcon className="h-4 w-4" />
        </IconButton>

        <IconButton title="Change theme" onClick={onOpenTheme}>
          <PaletteIcon className="h-4 w-4" />
        </IconButton>

        {/* Primary action */}
        <button
          onClick={onAdd}
          className="ml-1 flex h-9 items-center gap-1.5 rounded-lg bg-accent px-3.5 text-xs font-semibold text-accent-text shadow-sm transition-all duration-200 hover:brightness-110 active:scale-95"
        >
          <PlusIcon className="h-3.5 w-3.5" />
          New Download
        </button>
      </div>
    </header>
  );
};
