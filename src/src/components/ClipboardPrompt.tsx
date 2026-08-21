import type { FC } from "react";

export const ClipboardPrompt: FC<{
  url: string | null;
  onDismiss: () => void;
  onOpen: (url: string) => void;
}> = ({ url, onDismiss, onOpen }) => {
  if (!url) return null;
  return (
    <div className="fixed bottom-12 left-1/2 z-30 -translate-x-1/2 animate-[dp-fade_0.15s_ease]">
      <div className="flex items-center gap-3 rounded-lg border border-line bg-surface px-4 py-2.5 shadow-[var(--shadow)]">
        <span className="max-w-64 truncate text-xs text-txt" title={url}>
          {url}
        </span>
        <button
          className="rounded-md bg-accent px-2.5 py-1 text-[10px] font-semibold text-accent-text transition-all hover:brightness-110 active:scale-95"
          onClick={() => {
            onOpen(url);
            onDismiss();
          }}
        >
          Download
        </button>
        <button
          className="rounded-md bg-surface-3 px-2.5 py-1 text-[10px] text-txt-dim transition-colors hover:text-txt"
          onClick={onDismiss}
        >
          Dismiss
        </button>
      </div>
    </div>
  );
};