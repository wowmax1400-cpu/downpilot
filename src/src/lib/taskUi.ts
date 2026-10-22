// Shared UI derivation for a download task.
//
// The engine parks a finished transfer in "paused" until the user
// acknowledges it, and keeps other transient states around too. Every
// surface (list view, card view, sidebar counters) must agree on what
// the user SEES, otherwise the counters and the rows contradict each
// other. This module is the single source of truth for that fusion.
//
// Nothing here mutates the engine state — only the display mapping.

import type { DownloadTask } from "../types";
import { statusLabel } from "./format";

/**
 * The status the user should see for a task.
 *
 * All bytes on disk means the transfer is done, whatever transient
 * state the engine parked it in.
 */
export function uiStatus(t: DownloadTask): string {
  const total = t.total_size || 0;
  const done = t.downloaded || 0;
  if (total > 0 && done >= total) return "completed";
  return t.status;
}

/** Completion percentage, 0 when the total size is unknown. */
export function uiPct(t: DownloadTask): number {
  const total = t.total_size || 0;
  if (total <= 0) return 0;
  return Math.min(100, Math.max(0, Math.round(((t.downloaded || 0) / total) * 100)));
}

/**
 * Human label for the fused status. Downloading/paused/stopped rows
 * carry their percentage; failed rows carry the HTTP code when the
 * engine recorded one.
 */
export function uiStatusLabel(t: DownloadTask, pct: number): string {
  const st = uiStatus(t);
  if (st === "failed") {
    return t.error_code ? `${statusLabel(st)} / HTTP ${t.error_code}` : statusLabel(st);
  }
  if (st === "completed" || st === "cancelled") return statusLabel(st);
  if ((st === "downloading" || st === "paused" || st === "stopped") && pct > 0) {
    return `${statusLabel(st)} ${pct}%`;
  }
  return statusLabel(st);
}

/**
 * Fixed-width status glyph. Shape differs per state, so status never
 * relies on color alone (color-blind safe).
 */
export function statusGlyph(s: string): string {
  switch (s) {
    case "downloading":
      return "▶";
    case "completed":
      return "✓";
    case "paused":
      return "⏸";
    case "failed":
      return "✗";
    case "queued":
    case "scheduled":
      return "◷";
    case "stopped":
      return "■";
    case "cancelled":
    default:
      return "·";
  }
}

/** Tailwind text color for a fused status. */
export function statusTextClass(s: string): string {
  switch (s) {
    case "downloading":
      return "text-[var(--accent)]";
    case "completed":
      return "text-[var(--success)]";
    case "paused":
      return "text-[var(--warning)]";
    case "failed":
      return "text-[var(--danger)]";
    case "cancelled":
    case "queued":
    case "scheduled":
    default:
      return "text-txt-faint";
  }
}

/**
 * Contextual empty-state copy. Depends on the active sidebar filter
 * and the search query so "nothing here" never looks broken.
 */
export function emptyState(
  sidebarKey: { kind: string; value?: string | number } | undefined,
  search: string | undefined,
): { title: string; hint: string } {
  if (search) {
    return { title: "No matches", hint: `No downloads match “${search}”.` };
  }
  switch (sidebarKey?.kind) {
    case "status":
      if (sidebarKey.value === "unfinished")
        return { title: "All caught up", hint: "No unfinished downloads." };
      if (sidebarKey.value === "finished")
        return { title: "No history yet", hint: "Completed downloads will appear here." };
      if (sidebarKey.value === "failed")
        return { title: "No failures", hint: "Failed downloads will appear here." };
      if (sidebarKey.value === "paused")
        return { title: "Nothing paused", hint: "Paused downloads will appear here." };
      if (sidebarKey.value === "scheduled")
        return { title: "No scheduled downloads", hint: "Schedule a download from the toolbar." };
      return { title: "No downloads", hint: "Add one to get started." };
    case "queue":
      return { title: "Empty queue", hint: `No downloads in “${sidebarKey.value}”.` };
    case "category":
      return { title: "Empty category", hint: `No downloads in “${sidebarKey.value}”.` };
    case "site":
      return { title: "No downloads from this site", hint: "Capture a link to add one." };
    case "schedule":
      return { title: "Schedule not active", hint: "This scheduled window has no downloads." };
    case "all":
    default:
      return { title: "No downloads yet", hint: "Click Add URL or grab links from a page." };
  }
}
