import type { FC } from "react";
import { useState } from "react";
import type { GrabbedLink } from "../types";
import { SearchIcon, XIcon } from "./icons";
import { IconButton } from "./IconButton";

export const SiteGrabberDialog: FC<{
  open: boolean;
  onClose: () => void;
  onGrab: (url: string, limit?: number) => Promise<GrabbedLink[]>;
  onAdd: (links: GrabbedLink[]) => void;
}> = ({ open, onClose, onGrab, onAdd }) => {
  const [url, setUrl] = useState("");
  const [links, setLinks] = useState<GrabbedLink[]>([]);
  const [grabbing, setGrabbing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [checked, setChecked] = useState<Set<number>>(new Set());

  if (!open) return null;
  const handleGrab = async () => {
    if (!url.trim()) return;
    setGrabbing(true);
    setError(null);
    setLinks([]);
    setChecked(new Set());
    try {
      const result = await onGrab(url.trim());
      setLinks(result);
      setChecked(new Set(result.map((_, i) => i)));
    } catch (e) {
      setError(String(e));
    } finally {
      setGrabbing(false);
    }
  };
  const toggle = (i: number) => {
    setChecked((p) => {
      const n = new Set(p);
      if (n.has(i)) n.delete(i);
      else n.add(i);
      return n;
    });
  };
  const handleAdd = () => {
    const selected = links.filter((_, i) => checked.has(i));
    onAdd(selected);
    setUrl("");
    setLinks([]);
    setChecked(new Set());
    onClose();
  };
  return (
    <>
      <div className="dp-backdrop fixed inset-0 z-40" onClick={onClose} />
      <div className="dp-pop fixed left-1/2 top-1/2 z-50 flex max-h-[80vh] w-[480px] -translate-x-1/2 -translate-y-1/2 flex-col rounded-xl border border-line bg-surface p-5 shadow-[var(--shadow)]">
        <div className="flex items-center justify-between gap-2 pb-3">
          <h2 className="text-sm font-semibold">Site Grabber</h2>
          <IconButton title="Close" onClick={onClose}>
            <XIcon className="h-3.5 w-3.5" />
          </IconButton>
        </div>
        <div className="flex gap-2 pb-3">
          <input
            className="h-9 flex-1 rounded-lg border border-line bg-surface-2 px-3 text-xs text-txt outline-none placeholder:text-txt-faint focus:border-accent"
            placeholder="Enter page URL..."
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && handleGrab()}
          />
          <button
            className="flex h-9 items-center gap-1.5 rounded-lg bg-accent px-3 text-xs font-semibold text-accent-text transition-all hover:brightness-110 active:scale-95 disabled:opacity-50"
            disabled={grabbing || !url.trim()}
            onClick={handleGrab}
          >
            <SearchIcon className="h-3.5 w-3.5" />
            Grab
          </button>
        </div>
        {error && <p className="pb-2 text-xs text-danger">{error}</p>}
        {grabbing && (
          <div className="flex items-center justify-center py-6 text-xs text-txt-dim">
            <div className="mr-2 h-4 w-4 animate-spin rounded-full border-2 border-line border-t-accent" />
            Fetching page...
          </div>
        )}
        {!grabbing && links.length > 0 && (
          <>
            <div className="min-h-0 flex-1 overflow-auto">
              <table className="w-full text-xs">
                <thead>
                  <tr className="border-b border-line text-[10px] font-medium uppercase tracking-wider text-txt-faint">
                    <th className="w-6 px-2 py-2" />
                    <th className="px-2 py-2 text-left">File</th>
                    <th className="w-16 px-2 py-2 text-left">Type</th>
                  </tr>
                </thead>
                <tbody>
                  {links.map((l, i) => (
                    <tr
                      key={i}
                      className="cursor-pointer border-b border-line transition-colors hover:bg-surface-2"
                      onClick={() => toggle(i)}
                    >
                      <td className="px-2 py-2 text-center">
                        <input
                          type="checkbox"
                          checked={checked.has(i)}
                          onChange={() => toggle(i)}
                          className="accent-accent"
                        />
                      </td>
                      <td className="max-w-0 truncate px-2 py-2 text-txt">
                        {l.filename}
                      </td>
                      <td className="px-2 py-2 text-txt-dim">.{l.ext}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            <div className="flex items-center justify-between pt-3">
              <span className="text-[10px] text-txt-dim">
                {checked.size} of {links.length} selected
              </span>
              <button
                className="rounded-lg bg-accent px-3 py-1.5 text-xs font-semibold text-accent-text transition-all hover:brightness-110 active:scale-95 disabled:opacity-50"
                disabled={checked.size === 0}
                onClick={handleAdd}
              >
                Add {checked.size} download{checked.size > 1 ? "s" : ""}
              </button>
            </div>
          </>
        )}
        {!grabbing && links.length === 0 && !error && (
          <div className="py-6 text-center text-xs text-txt-faint">
            Enter a URL above and click Grab to find downloadable files.
          </div>
        )}
      </div>
    </>
  );
};