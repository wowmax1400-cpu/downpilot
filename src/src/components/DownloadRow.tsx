import type { FC } from "react";
import type { DownloadTask } from "../types";
import { formatBytes, formatEta, formatPercent, guessCategory } from "../lib/format";
import {
  ArchiveIcon,
  CpuIcon,
  FileTextIcon,
  FilmIcon,
  FolderIcon,
  FolderOpenIcon,
  MusicIcon,
  PauseIcon,
  PlayIcon,
  TrashIcon,
  XIcon,
} from "./icons";
import { IconButton } from "./IconButton";

const CAT_ICONS: Record<string, FC<{ className?: string }>> = {
  video: FilmIcon,
  music: MusicIcon,
  compressed: ArchiveIcon,
  documents: FileTextIcon,
  programs: CpuIcon,
  other: FolderIcon,
};

const STATUS_BADGE: Record<
  string,
  { label: string; pill: string; dot: string }
> = {
  queued: {
    label: "Queued",
    pill: "bg-surface-3 text-txt-dim",
    dot: "dp-dot dp-dot-muted",
  },
  downloading: {
    label: "Active",
    pill: "bg-[var(--accent-soft)] text-accent",
    dot: "dp-dot dp-dot-success",
  },
  paused: {
    label: "Paused",
    pill: "bg-surface-3 text-warning",
    dot: "dp-dot dp-dot-warning",
  },
  completed: {
    label: "Done",
    pill: "bg-surface-3 text-success",
    dot: "dp-dot dp-dot-success",
  },
  failed: {
    label: "Failed",
    pill: "bg-surface-3 text-danger",
    dot: "dp-dot dp-dot-danger",
  },
  cancelled: {
    label: "Cancelled",
    pill: "bg-surface-3 text-txt-faint line-through",
    dot: "dp-dot dp-dot-muted",
  },
};

export const DownloadRow: FC<{
  task: DownloadTask;
  onPause: () => void;
  onResume: () => void;
  onCancel: () => void;
  onRemove: () => void;
  onOpenFolder: () => void;
}> = ({ task, onPause, onResume, onCancel, onRemove, onOpenFolder }) => {
  const pct = formatPercent(task.downloaded, task.total_size);
  const progress = task.total_size > 0 ? task.downloaded / task.total_size : 0;

  // Defensive: treat any unknown status as queued so a row always gets
  // its actions (a stale/foreign status must never render a dead row).
  const known =
    task.status === "downloading" ||
    task.status === "queued" ||
    task.status === "paused" ||
    task.status === "completed" ||
    task.status === "failed" ||
    task.status === "cancelled";
  const status = known ? task.status : "queued";

  const badge = STATUS_BADGE[status] ?? STATUS_BADGE.queued;
  const Icon = CAT_ICONS[task.category] ?? CAT_ICONS.other;
  const name = task.filename || task.url.split("/").pop() || "untitled";

  const barColor =
    status === "completed"
      ? "#10b981" // emerald-500 — clean solid
      : status === "failed"
        ? "var(--danger)"
        : status === "paused"
          ? "#fbbf24" // amber-400 — muted paused
          : status === "queued"
            ? "#71717a" // zinc-500 — muted queued
            : "#34d399"; // emerald-400 base (gradient covers it while active)

  const isDownloading = status === "downloading";

  const active = status === "downloading" || status === "queued";

  return (
    <tr className="dp-row">
      {/* File icon */}
      <td className="px-4 py-3.5">
        <div className="flex h-9 w-9 items-center justify-center rounded-lg bg-surface-3">
          <Icon className="h-4 w-4 text-txt-dim" />
        </div>
      </td>

      {/* Name + url */}
      <td className="min-w-[180px] px-4 py-3.5">
        <div className="truncate text-[13px] font-medium text-txt" title={name}>
          {name}
        </div>
        <div className="truncate text-xs text-zinc-500" title={task.url}>
          {task.url}
        </div>
      </td>

      {/* Status badge pill */}
      <td className="px-4 py-3.5">
        <span
          className={`inline-flex items-center gap-1.5 rounded-full px-2.5 py-1 text-[11px] font-medium ${badge.pill}`}
        >
          <span className={`h-1.5 w-1.5 rounded-full ${badge.dot}`} />
          {badge.label}
        </span>
      </td>

      {/* Progress bar */}
      <td className="px-4 py-3.5">
        <div className="flex items-center gap-2.5">
          <div className="relative h-1.5 flex-1 overflow-hidden rounded-full bg-progress-track">
            <div
              className={`h-full rounded-full transition-all duration-500 ease-out ${
                isDownloading ? "dp-progress-fill animate-stripes" : ""
              }`}
              style={{ width: `${Math.min(100, Math.round(progress * 100))}%`, background: barColor }}
            />
          </div>
          <span className="tnum w-9 text-right text-[11px] text-txt-dim">{pct}</span>
        </div>
      </td>

      {/* Size */}
      <td className="px-4 py-3.5 text-right text-txt-dim">
        <span className="tnum block text-[11px]">{formatBytes(task.total_size)}</span>
        <span className="tnum block text-[10px] text-txt-faint">
          {status === "completed" ? "complete" : `${formatBytes(task.downloaded)} done`}
        </span>
      </td>

      {/* Speed */}
      <td className="px-4 py-3.5 text-right text-txt-dim">
        {status === "downloading" ? (
          <span className="tnum text-[11px] font-medium text-success">{formatBytes(task.speed_bps)}/s</span>
        ) : (
          <span className="tnum text-[11px] text-txt-faint">—</span>
        )}
      </td>

      {/* ETA */}
      <td className="px-4 py-3.5 text-right text-txt-dim">
        {status === "downloading" && task.total_size > 0 ? (
          <span className="tnum text-[11px]">{formatEta(task.total_size - task.downloaded, task.speed_bps)}</span>
        ) : (
          <span className="tnum text-[11px] text-txt-faint">—</span>
        )}
      </td>

      {/* Actions — circular icon buttons */}
      <td className="dp-sticky-actions px-4 py-3.5">
        <div className="flex items-center justify-end gap-1">
          {(active || status === "paused") && (
            <IconButton title={active ? "Pause" : "Resume"} onClick={active ? onPause : onResume} tone={active ? "default" : "accent"}>
              {active ? <PauseIcon className="h-3.5 w-3.5" /> : <PlayIcon className="h-3.5 w-3.5" />}
            </IconButton>
          )}
          {(active || status === "paused") && (
            <IconButton title="Cancel" onClick={onCancel} tone="danger">
              <XIcon className="h-3.5 w-3.5" />
            </IconButton>
          )}
          {status === "completed" && (
            <IconButton title="Open folder" onClick={onOpenFolder} tone="default">
              <FolderOpenIcon className="h-3.5 w-3.5" />
            </IconButton>
          )}
          {(status === "completed" || status === "failed" || status === "cancelled") && (
            <IconButton title="Remove from list" onClick={onRemove} tone="danger">
              <TrashIcon className="h-3.5 w-3.5" />
            </IconButton>
          )}
        </div>
      </td>
    </tr>
  );
};
