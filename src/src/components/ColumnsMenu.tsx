// Right-click on the table header opens this small menu. The user
// can hide/show any column, and there's a "Reset to defaults" link
// to restore the IDM-style default 7 columns.
//
// Visibility is wired through the useColumnConfig hook; this
// component is just the visual surface.

import { useEffect, useRef } from "react";
import { COLUMNS, type ColumnConfig, type ColumnId } from "../lib/columnConfig";

type Props = {
  cfg: ColumnConfig;
  onToggle: (id: ColumnId) => void;
  onReset: () => void;
  onClose: () => void;
  /** Anchor position from the right-click. */
  x: number;
  y: number;
};

export function ColumnsMenu({ cfg, onToggle, onReset, onClose, x, y }: Props) {
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

  // Clamp to viewport.
  const left = Math.min(x, window.innerWidth - 220);
  const top = Math.min(y, window.innerHeight - 380);

  return (
    <div
      ref={ref}
      className="ctx-menu"
      style={{ left, top, minWidth: 200 }}
      onContextMenu={(e) => e.preventDefault()}
    >
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
                className="h-3 w-3 accent-indigo-500"
                tabIndex={-1}
              />
            </span>
            <span className={isName ? "text-txt-dim" : ""}>{c.label}</span>
          </button>
        );
      })}
      <div className="ctx-sep" />
      <button className="ctx-row" onClick={onReset}>
        <span className="ctx-row__icon" />
        <span>Reset to defaults</span>
      </button>
    </div>
  );
}
