import { useState, type FC } from "react";
import type { DownloadsStore } from "../store/useDownloads";
import { THEMES, DEFAULT_ACCENTS, applyTheme, hexToRgba } from "../themes";

export const ThemePicker: FC<{
  store: DownloadsStore;
  onClose: () => void;
}> = ({ store, onClose }) => {
  const currentTheme = store.settings?.theme || "deep-midnight";
  const currentAccent = store.settings?.accent || "";
  const [selectedTheme, setSelectedTheme] = useState(currentTheme);
  const [selectedAccent, setSelectedAccent] = useState(currentAccent);

  const handleApply = () => {
    applyTheme(selectedTheme, selectedAccent || undefined);
    store.updateSetting("theme", selectedTheme);
    if (selectedAccent) store.updateSetting("accent", selectedAccent);
    onClose();
  };

  return (
    <div className="dp-backdrop fixed inset-0 z-50 flex items-center justify-center">
      <div className="dp-pop w-[440px] max-w-[94vw] rounded-xl border border-line bg-surface p-5 shadow-lg">
        <h2 className="mb-4 text-base font-semibold">Theme & Accent</h2>

        {/* Theme cards */}
        <div className="grid grid-cols-2 gap-2.5">
          {THEMES.map((t) => {
            const active = selectedTheme === t.id;
            return (
              <button
                key={t.id}
                onClick={() => setSelectedTheme(t.id)}
                className={`relative flex flex-col items-start gap-1.5 rounded-lg border p-3 text-left transition-all ${
                  active
                    ? "border-accent bg-accent-soft"
                    : "border-line bg-surface-2 hover:bg-surface-3"
                }`}
                style={{
                  "--card-bg": t.swatch,
                  "--card-border": active ? "var(--accent)" : undefined,
                } as React.CSSProperties}
              >
                {/* Swatch */}
                <div
                  className="h-8 w-full rounded-md border"
                  style={{
                    background: t.swatch,
                    borderColor: t.swatch === "#ffffff" ? "var(--line)" : "transparent",
                  }}
                />
                <div className="text-xs font-medium text-txt">{t.name}</div>
                <div className="text-[10px] text-txt-faint">{t.blurb}</div>
                {active && (
                  <span className="absolute right-2 top-2 text-xs text-accent">✓</span>
                )}
              </button>
            );
          })}
        </div>

        {/* Accent color picker */}
        <div className="mt-4">
          <div className="mb-2 text-xs font-medium text-txt-dim">Accent Color</div>
          <div className="flex flex-wrap gap-2">
            {DEFAULT_ACCENTS.map((c) => (
              <button
                key={c}
                onClick={() => setSelectedAccent(c)}
                className={`h-7 w-7 rounded-full border-2 transition-all ${
                  selectedAccent === c
                    ? "border-accent"
                    : "border-transparent hover:scale-110"
                }`}
                style={{ background: c }}
                title={c}
              />
            ))}
            <input
              type="color"
              value={selectedAccent || "#7c5cff"}
              onChange={(e) => setSelectedAccent(e.target.value)}
              className="h-7 w-7 cursor-pointer rounded-full border-0 bg-transparent p-0 outline-none"
              title="Custom color"
            />
          </div>
        </div>

        {/* Actions */}
        <div className="mt-4 flex justify-end gap-2">
          <button
            onClick={onClose}
            className="rounded-md border border-line px-4 py-1.5 text-xs text-txt-dim transition-colors hover:bg-surface-2 hover:text-txt"
          >
            Cancel
          </button>
          <button
            onClick={handleApply}
            className="rounded-md bg-accent px-4 py-1.5 text-xs font-semibold text-accent-text"
          >
            Apply
          </button>
        </div>
      </div>
    </div>
  );
};