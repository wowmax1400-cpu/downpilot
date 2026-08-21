import type { FC } from "react";
import type { DownloadsStore } from "../store/useDownloads";
import { DownloadRow } from "./DownloadRow";
import { EmptyState } from "./EmptyState";
import { formatBytes } from "../lib/format";
import { PlusIcon, SearchIcon } from "./icons";

export const DownloadTable: FC<{
  store: DownloadsStore;
  onAdd: () => void;
}> = ({ store, onAdd }) => {
  const { activeSpeed, activeCount } = store.stats;

  return (
    <div className="dp-light-edge flex min-h-0 flex-1 flex-col overflow-hidden rounded-xl border border-line bg-surface-2 shadow-[var(--shadow)]">
      {/* Card header */}
      <div className="flex items-center justify-between gap-3 border-b border-line px-4 py-3">
        <div className="flex items-center gap-2.5">
          <h2 className="text-[11px] font-semibold uppercase tracking-wider text-txt-dim">Downloads</h2>
          <span className="tnum rounded-full bg-surface-3 px-2 py-0.5 text-[10px] font-medium text-txt-faint">
            {store.allTasks.length}
          </span>
        </div>
        {activeCount > 0 && (
          <div className="flex items-center gap-3 text-[11px] text-txt-dim">
            <span className="flex items-center gap-1.5">
              <span className="dp-dot dp-dot-success h-1.5 w-1.5" />
              {activeCount} active
            </span>
            <span className="tnum">↓ {formatBytes(activeSpeed)}/s</span>
          </div>
        )}
      </div>

      {/* Loading */}
      {!store.loaded && (
        <div className="flex min-h-0 flex-1 items-center justify-center">
          <div className="flex items-center gap-3 text-sm text-txt-faint">
            <div className="h-4 w-4 animate-spin rounded-full border-2 border-line border-t-accent" />
            Loading…
          </div>
        </div>
      )}

      {/* Empty — gorgeous ghost state, context-aware subtitle */}
      {store.loaded && store.tasks.length === 0 && (
        <EmptyState
          icon={<SearchIcon className="h-6 w-6" />}
          title={
            store.search
              ? "No matches"
              : store.filter !== "all"
                ? "Nothing here yet"
                : "No downloads yet"
          }
          subtitle={
            store.search
              ? `No downloads match “${store.search}”`
              : store.filter !== "all"
                ? "This view is empty — try another category"
                : "Add a URL to get started"
          }
          action={
            !store.search && store.filter === "all" ? (
              <button
                onClick={onAdd}
                className="flex items-center gap-1.5 rounded-lg bg-accent px-4 py-2 text-xs font-semibold text-accent-text shadow-sm transition-all duration-200 hover:brightness-110 active:scale-95"
              >
                <PlusIcon className="h-3.5 w-3.5" />
                Add first download
              </button>
            ) : undefined
          }
        />
      )}

      {/* Table */}
      {store.loaded && store.tasks.length > 0 && (
        <div className="min-h-0 flex-1 overflow-auto">
          <table className="w-full min-w-[880px] border-separate border-spacing-0 text-xs">
            <thead className="sticky top-0 z-10">
              <tr className="dp-thead">
                <th className="w-14 px-4 py-2.5 text-left text-[10px] font-semibold uppercase tracking-wider text-txt-faint">
                  Type
                </th>
                <th className="px-4 py-2.5 text-left text-[10px] font-semibold uppercase tracking-wider text-txt-faint">
                  Name
                </th>
                <th className="w-24 px-4 py-2.5 text-left text-[10px] font-semibold uppercase tracking-wider text-txt-faint">
                  Status
                </th>
                <th className="w-40 px-4 py-2.5 text-left text-[10px] font-semibold uppercase tracking-wider text-txt-faint">
                  Progress
                </th>
                <th className="w-24 px-4 py-2.5 text-right text-[10px] font-semibold uppercase tracking-wider text-txt-faint">
                  Size
                </th>
                <th className="w-20 px-4 py-2.5 text-right text-[10px] font-semibold uppercase tracking-wider text-txt-faint">
                  Speed
                </th>
                <th className="w-16 px-4 py-2.5 text-right text-[10px] font-semibold uppercase tracking-wider text-txt-faint">
                  ETA
                </th>
                <th className="dp-sticky-actions w-28 px-4 py-2.5 text-right text-[10px] font-semibold uppercase tracking-wider text-txt-faint">
                  Actions
                </th>
              </tr>
            </thead>
            <tbody>
              {store.tasks.map((task) => (
                <DownloadRow
                  key={task.id}
                  task={task}
                  onPause={() => store.pause(task.id)}
                  onResume={() => store.resume(task.id)}
                  onCancel={() => store.cancel(task.id)}
                  onRemove={() => store.remove(task.id)}
                  onOpenFolder={() => store.openFolder(task)}
                />
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
};