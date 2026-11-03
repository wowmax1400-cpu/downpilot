import { useState, useEffect } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";

export function TitleBar() {
  const [appWindow, setAppWindow] = useState<any>(null);

  useEffect(() => {
    try {
      if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
        setAppWindow(getCurrentWindow());
      }
    } catch {
      // Browser preview mode
    }
  }, []);

  const handleMinimize = () => {
    appWindow?.minimize?.();
  };

  const handleMaximize = () => {
    appWindow?.toggleMaximize?.();
  };

  const handleClose = () => {
    appWindow?.close?.();
  };

  return (
    <div
      data-tauri-drag-region
      className="window-titlebar window-header flex h-[30px] shrink-0 items-center justify-between border-b border-[var(--line)] bg-[var(--bg-toolbar)] px-2 select-none text-[var(--text)]"
      style={{
        cursor: "default",
      }}
    >
      {/* Title + Icon */}
      <div
        data-tauri-drag-region
        className="flex items-center gap-2 pl-1 text-[12px] font-semibold text-[var(--text)] pointer-events-none"
      >
        <svg
          viewBox="0 0 16 16"
          className="h-4 w-4 text-cyan-400 drop-shadow-[0_0_6px_rgba(6,182,212,0.6)] pointer-events-none"
          fill="currentColor"
        >
          <path d="M.5 9.9a.5.5 0 0 1 .5.5v2.5a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-2.5a.5.5 0 0 1 1 0v2.5a2 2 0 0 1-2 2H2a2 2 0 0 1-2-2v-2.5a.5.5 0 0 1 .5-.5z" />
          <path d="M7.646 11.854a.5.5 0 0 0 .708 0l3-3a.5.5 0 0 0-.708-.708L8.5 10.293V1.5a.5.5 0 0 0-1 0v8.793L5.354 8.146a.5.5 0 1 0-.708.708l3 3z" />
        </svg>
        <span className="pointer-events-none tracking-wide">Internet Download Manager (DownPilot)</span>
      </div>

      {/* Modern Window Controls */}
      <div
        data-tauri-drag-region="false"
        className="window-controls flex items-center h-full"
        style={{ WebkitAppRegion: "no-drag" } as unknown as React.CSSProperties}
      >
        {/* Minimize _ */}
        <button
          type="button"
          onClick={handleMinimize}
          className="flex h-[28px] w-[36px] items-center justify-center text-[12px] text-[var(--text-dim)] hover:bg-[var(--surface-3)] hover:text-white transition-colors"
          style={{ WebkitAppRegion: "no-drag" } as unknown as React.CSSProperties}
          title="Minimize"
        >
          <span className="mb-[2px] leading-none">−</span>
        </button>

        {/* Maximize □ */}
        <button
          type="button"
          onClick={handleMaximize}
          className="flex h-[28px] w-[36px] items-center justify-center text-[11px] text-[var(--text-dim)] hover:bg-[var(--surface-3)] hover:text-white transition-colors"
          style={{ WebkitAppRegion: "no-drag" } as unknown as React.CSSProperties}
          title="Maximize"
        >
          <span className="leading-none">□</span>
        </button>

        {/* Close ✕ */}
        <button
          type="button"
          onClick={handleClose}
          className="flex h-[28px] w-[40px] items-center justify-center text-[12px] text-[var(--text-dim)] hover:bg-[#E81123] hover:text-white transition-colors"
          style={{ WebkitAppRegion: "no-drag" } as unknown as React.CSSProperties}
          title="Close"
        >
          <span className="leading-none">✕</span>
        </button>
      </div>
    </div>
  );
}
