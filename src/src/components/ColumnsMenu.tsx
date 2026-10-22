// Right-click on the table header opens this small menu. The user
// can hide/show any column, apply a named view preset (Compact /
// Detailed / Power User), and restore the IDM-style default layout.

import { useEffect, useRef } from "react";
import {
  COLUMNS,
  VIEW_PRESETS,
  type ColumnConfig,
  type ColumnId,
} from "../lib/columnConfig";

type Props = {
  cfg: ColumnConfig;
  onToggle: (id: ColumnId) => void;
  onReset: () => void;
  onApplyPreset: (name: string) => void;
  onClose: () => void;
  /** Anchor position from the right-click. */
  x: number;
  y: number;
};

export function ColumnsMenu({
  cfg,
  onToggle,
  onReset,
  onApplyPreset,
  onClose,
  x,
  y,
}: Props) {
  const ref = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    const onDoc = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) onClose();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("mousedown", onDoc);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDoc);
      document.removeEventListener("keydown", onKey);
    };
  }, [onClose]);

  // Clamp to viewport. The presets section adds ~70px of height, so
  // the clamp ceiling grows from 380 to 460.
  const left = Math.min(x, window.innerWidth - 240);
  const top = Math.min(y, window.innerHeight - 460);

  return (
    <div
      ref={ref}
      className="ctx-menu"
      style={{ left, top, minWidth: 220 }}
      onContextMenu={(e) => e.preventDefault()}
    >
      <div className="px-3 py-1.5 text-[10.5px] font-semibold uppercase tracking-wider text-txt-faint">
        View preset
      </div>
      {VIEW_PRESETS.map((p) => (
        <button
          key={p.name}
          className="ctx-row"
          onClick={() => {
            onApplyPreset(p.name);
            onClose();
          }}
          title={p.description}
        >
          <span className="ctx-row__icon" aria-hidden>
            <span className="inline-block h-2 w-2 rounded-[1px] bg-cyan-400/70" />
          </span>
          <span className="flex-1 truncate">{p.label}</span>
        </button>
      ))}
      <div className="ctx-sep" />
      <div className="px-3 py-1.5 text-[10.5px] font-semibold uppercase tracking-wider text-txt-faint">
        Columns
      </div>
      {COLUMNS.map((c) => {
        const visible = cfg.visibility[c.id];
        const isName = c.id === "name";
        return (
          <button
            key={c.id}
            className="ctx-row"
            disabled={isName}
            onClick={() => !isName && onToggle(c.id)}
            title={isName ? "Name is always shown" : c.label}
          >
            <span className="ctx-row__icon" aria-hidden>
              <input
                type="checkbox"
                checked={visible}
                readOnly
                className="h-3 w-3 accent-[var(--accent)]"
                tabIndex={-1}
              />
            </span>
            <span className={isName ? "text-txt-dim" : ""}>{c.label}</span>
          </button>
        );
      })}
      <div className="ctx-sep" />
      <button
        className="ctx-row"
        onClick={() => {
          onReset();
          onClose();
        }}
      >
        <span className="ctx-row__icon" />
        <span>Reset to defaults</span>
      </button>
    </div>
  );
}
