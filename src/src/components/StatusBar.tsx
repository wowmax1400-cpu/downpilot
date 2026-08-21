import type { FC } from "react";
import type { DownloadsStore } from "../store/useDownloads";
import { formatBytes } from "../lib/format";

export const StatusBar: FC<{ store: DownloadsStore }> = ({ store }) => {
  const { activeSpeed, activeCount } = store.stats;
  return (
    <footer className="dp-light-edge flex h-8 shrink-0 items-center gap-5 border-t border-line bg-surface px-4 text-[11px] text-txt-faint">
      <span className="flex items-center gap-1.5">
        <span className="tnum font-semibold text-txt-dim">{store.allTasks.length}</span> total
      </span>
      <span className="flex items-center gap-1.5">
        {activeCount > 0 && (
          <span className="dp-dot dp-dot-success h-1.5 w-1.5" />
        )}
        <span className={`tnum font-semibold ${activeCount > 0 ? "text-success" : "text-txt-dim"}`}>
          {activeCount}
        </span>{" "}
        active
      </span>
      {activeSpeed > 0 && (
        <span className="tnum flex items-center gap-1.5">
          <svg
            className="h-3 w-3 text-success"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth={2}
            strokeLinecap="round"
            strokeLinejoin="round"
          >
            <path d="M12 3v12" />
            <path d="m7 10 5 5 5-5" />
            <path d="M5 21h14" />
          </svg>
          {formatBytes(activeSpeed)}/s
        </span>
      )}
      <span className="flex-1" />
      <span className="tnum">DownPilot v0.1.0</span>
    </footer>
  );
};
