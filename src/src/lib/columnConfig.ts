// Column configuration for the downloads table.
//
// Each column is a string id; the UI reads the persisted list from
// localStorage and renders the headers + cells in that order. The
// `useColumnConfig` hook handles load/persist + resize.

import { useCallback, useEffect, useState } from "react";

export type ColumnId =
  | "name"
  | "size"
  | "status"
  | "time_left"
  | "speed"
  | "speed_graph"
  | "downloaded"
  | "progress"
  | "avg_speed"
  | "connections"
  | "category"
  | "queue"
  | "last_try"
  | "added"
  | "save_to"
  | "description";

export interface ColumnDef {
  id: ColumnId;
  /** Display header text (uppercased in the table). */
  label: string;
  /** Right-align (true for numeric columns). */
  right?: boolean;
  /** Default width in px (used on first render). */
  defaultWidth: number;
  /** Minimum width when resizing. */
  minWidth: number;
  /** True if the column is visible by default. Hidden columns can be
   *  re-enabled from the Columns menu. Defaults to true. */
  defaultVisible?: boolean;
}

export const COLUMNS: ColumnDef[] = [
  { id: "name",        label: "File Name",   defaultWidth: 280, minWidth: 140 },
  { id: "size",        label: "Size",        right: true,  defaultWidth: 88,  minWidth: 60 },
  { id: "status",      label: "Status",      right: true,  defaultWidth: 150, minWidth: 96 },
  { id: "time_left",   label: "Time Left",   right: true,  defaultWidth: 80,  minWidth: 64 },
  { id: "speed",       label: "Speed",       right: true,  defaultWidth: 92,  minWidth: 64 },
  { id: "progress",    label: "Progress",                   defaultWidth: 160, minWidth: 100, defaultVisible: false },
  { id: "last_try",    label: "Last Try",    right: true,  defaultWidth: 132, minWidth: 96 },
  { id: "queue",       label: "Queue Name",                 defaultWidth: 120, minWidth: 80 },
  { id: "speed_graph", label: "Graph",                      defaultWidth: 96,  minWidth: 64, defaultVisible: false },
  { id: "downloaded",  label: "Done",        right: true,  defaultWidth: 72,  minWidth: 60, defaultVisible: false },
  { id: "avg_speed",   label: "Avg Speed",   right: true,  defaultWidth: 92,  minWidth: 72, defaultVisible: false },
  { id: "connections", label: "Conn",        right: true,  defaultWidth: 60,  minWidth: 48, defaultVisible: false },
  { id: "category",    label: "Category",                   defaultWidth: 100, minWidth: 80, defaultVisible: false },
  { id: "added",       label: "Added",       right: true,  defaultWidth: 88,  minWidth: 72, defaultVisible: false },
  { id: "save_to",     label: "Save To",                    defaultWidth: 200, minWidth: 120, defaultVisible: false },
  { id: "description", label: "Description",                defaultWidth: 200, minWidth: 100, defaultVisible: false },
];

// v4: IDM strict grid (File Name / Size / Status / Time Left / Speed /
// Last Try / Queue Name / Progress). Last Try + Queue Name are
// default-visible per spec. Bumped so persisted v3 layouts don't mask
// the new defaults on existing installs.
const STORAGE_KEY = "downpilot.columns.v4";

export interface ColumnConfig {
  /** Ordered list of visible column ids. */
  order: ColumnId[];
  /** Map of column id → width in px. */
  widths: Record<ColumnId, number>;
  /** Map of column id → visibility (only persisted when user changes it). */
  visibility: Record<ColumnId, boolean>;
}

function defaults(): ColumnConfig {
  const order: ColumnId[] = [];
  const widths: Record<ColumnId, number> = {} as Record<ColumnId, number>;
  const visibility: Record<ColumnId, boolean> = {} as Record<ColumnId, boolean>;
  for (const c of COLUMNS) {
    if (c.defaultVisible !== false) order.push(c.id);
    widths[c.id] = c.defaultWidth;
    visibility[c.id] = c.defaultVisible !== false;
  }
  return { order, widths, visibility };
}

function loadFromStorage(): ColumnConfig {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return defaults();
    const parsed = JSON.parse(raw) as Partial<ColumnConfig>;
    const base = defaults();
    // Order: take persisted, append any new columns at the end (in
    // the position they appear in COLUMNS).
    const known = new Set<ColumnId>(COLUMNS.map((c) => c.id));
    const order = (parsed.order ?? []).filter((c): c is ColumnId =>
      known.has(c as ColumnId)
    );
    for (const c of COLUMNS) {
      if (!order.includes(c.id)) order.push(c.id);
    }
    const widths = { ...base.widths, ...(parsed.widths ?? {}) };
    const visibility = { ...base.visibility, ...(parsed.visibility ?? {}) };
    return { order, widths, visibility };
  } catch {
    return defaults();
  }
}

function saveToStorage(cfg: ColumnConfig) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(cfg));
  } catch {
    /* ignore quota / private mode */
  }
}

export function useColumnConfig() {
  const [cfg, setCfg] = useState<ColumnConfig>(() => loadFromStorage());

  // Persist on every change.
  useEffect(() => {
    saveToStorage(cfg);
  }, [cfg]);

  const setWidth = useCallback((id: ColumnId, w: number) => {
    setCfg((c) => ({
      ...c,
      widths: { ...c.widths, [id]: Math.max(40, Math.round(w)) },
    }));
  }, []);

  const toggle = useCallback((id: ColumnId) => {
    setCfg((c) => {
      // Never allow hiding all columns — keep at least the Name column.
      if (c.visibility[id] && id === "name") return c;
      const visible = !c.visibility[id];
      return {
        ...c,
        visibility: { ...c.visibility, [id]: visible },
        order: visible && !c.order.includes(id) ? [...c.order, id] : c.order,
      };
    });
  }, []);

  /** Move column `id` so it sits immediately before `beforeId` in the
   *  ordered list. Used by drag-to-reorder on the table header. If
   *  `beforeId` is null the column is moved to the end. */
  const move = useCallback((id: ColumnId, beforeId: ColumnId | null) => {
    setCfg((c) => {
      // Always work on visible columns; hidden ones keep their relative order
      // so re-enabling them later doesn't scramble the layout.
      const visible = c.order.filter((x) => c.visibility[x]);
      const others = c.order.filter((x) => !visible.includes(x));
      const without = visible.filter((x) => x !== id);
      let next: ColumnId[];
      if (beforeId == null) {
        next = [...without, id];
      } else {
        const idx = without.indexOf(beforeId);
        if (idx < 0) {
          next = [...without, id];
        } else {
          next = [...without.slice(0, idx), id, ...without.slice(idx)];
        }
      }
      return { ...c, order: [...next, ...others] };
    });
  }, []);

  const reset = useCallback(() => setCfg(defaults()), []);

  /** Apply a named view preset. Replaces the entire column config
   *  (order, visibility, widths) with the preset's values. The preset
   *  is then persisted automatically by the effect above. */
  const applyPreset = useCallback((name: string) => {
    const preset = VIEW_PRESETS.find((p) => p.name === name);
    if (!preset) return;
    const order: ColumnId[] = [];
    const visibility: Record<string, boolean> = {};
    const widths: Record<string, number> = {};
    for (const c of COLUMNS) {
      const visible = preset.columns.includes(c.id);
      visibility[c.id] = visible;
      if (visible) order.push(c.id);
      // Reset widths to defaults so the user can resize from a known
      // starting point (avoids carrying over their custom widths from
      // a previous preset).
      widths[c.id] = c.defaultWidth;
    }
    setCfg({ order, visibility, widths });
  }, []);

  /** Build a CSS grid template string for the visible columns, in order. */
  const gridTemplate = cfg.order
    .filter((id) => cfg.visibility[id])
    .map((id) => {
      const w = cfg.widths[id] ?? 100;
      return `${w}px`;
    })
    .join(" ");

  return { cfg, setWidth, toggle, move, reset, applyPreset, gridTemplate };
}

/// Named view presets. The spec calls out three: Compact (essential
/// columns only), Detailed (a richer view for everyday use), and Power
/// User (everything visible). Users can still resize, reorder, and
/// hide individual columns on top of a preset.
export interface ViewPreset {
  name: string;
  label: string;
  description: string;
  columns: ColumnId[];
}

export const VIEW_PRESETS: ViewPreset[] = [
  {
    name: "compact",
    label: "Compact",
    description: "Name / Status / Size / Speed / ETA",
    columns: ["name", "size", "status", "speed", "time_left"],
  },
  {
    name: "detailed",
    label: "Detailed",
    description: "Adds progress, connections, category, and queue",
    columns: [
      "name",
      "size",
      "status",
      "time_left",
      "speed",
      "downloaded",
      "connections",
      "category",
      "queue",
    ],
  },
  {
    name: "power",
    label: "Power User",
    description: "Every available useful column",
    columns: [
      "name",
      "size",
      "status",
      "time_left",
      "speed",
      "downloaded",
      "progress",
      "avg_speed",
      "speed_graph",
      "connections",
      "category",
      "queue",
      "added",
      "save_to",
      "description",
    ],
  },
];
