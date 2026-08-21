import { useState, type FC } from "react";
import type { DownloadsStore } from "../store/useDownloads";
import type { MediaInfo, MediaFormat } from "../types";
import { formatBytes, formatDuration } from "../lib/format";

export const AddDownloadDialog: FC<{
  store: DownloadsStore;
  onClose: () => void;
  initialUrl?: string;
}> = ({ store, onClose, initialUrl }) => {
  const [url, setUrl] = useState(initialUrl || "");
  const [analyzing, setAnalyzing] = useState(false);
  const [media, setMedia] = useState<MediaInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selectedFormat, setSelectedFormat] = useState<string | null>(null);
  const [threads, setThreads] = useState(8);

  const handleAnalyze = async () => {
    if (!url.trim()) return;
    setAnalyzing(true);
    setError(null);
    try {
      const info = await store.analyze(url.trim());
      setMedia(info);
      if (info.best_formats.length > 0) {
        setSelectedFormat(info.best_formats[0].format_id);
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setAnalyzing(false);
    }
  };

  const handleDownload = async () => {
    if (!url.trim()) return;
    try {
      const id = await store.add({
        url: url.trim(),
        filename: media?.default_filename || undefined,
        webpage_url: media?.webpage_url || undefined,
        format_id: selectedFormat || undefined,
        threads,
        start_immediately: true,
      });
      if (id) onClose();
    } catch (e) {
      setError(String(e));
    }
  };

  // Quick-add: just enqueue without analyzing
  const handleQuickAdd = async () => {
    if (!url.trim()) return;
    try {
      await store.add({
        url: url.trim(),
        threads,
        start_immediately: true,
      });
      onClose();
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <div className="dp-backdrop fixed inset-0 z-50 flex items-center justify-center">
      <div className="dp-pop w-[520px] max-w-[94vw] rounded-xl border border-line bg-surface p-5 shadow-lg">
        <h2 className="mb-4 text-base font-semibold">New Download</h2>

        {/* URL input */}
        <div className="flex gap-2">
          <input
            type="text"
            placeholder="Paste URL (video, file, stream…) and press Analyze"
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && handleAnalyze()}
            className="min-w-0 flex-1 rounded-md border border-line bg-surface-2 px-3 py-2 text-sm text-txt outline-none placeholder:text-txt-faint focus:border-accent"
            autoFocus
          />
          <button
            onClick={handleAnalyze}
            disabled={analyzing || !url.trim()}
            className="shrink-0 rounded-md bg-accent px-4 py-2 text-sm font-semibold text-accent-text disabled:opacity-50"
          >
            {analyzing ? "Analyzing…" : "Analyze"}
          </button>
        </div>

        {error && (
          <div className="mt-2 rounded-md bg-danger/10 px-3 py-2 text-xs text-danger">
            {error}
          </div>
        )}

        {/* Media info */}
        {media && (
          <div className="mt-3 rounded-lg border border-line bg-surface-2 p-3">
            <div className="flex items-start gap-3">
              {media.thumbnail && (
                <img
                  src={media.thumbnail}
                  alt=""
                  className="h-14 w-20 rounded object-cover"
                />
              )}
              <div className="min-w-0 flex-1">
                <div className="truncate text-sm font-medium text-txt" title={media.title}>
                  {media.title}
                </div>
                <div className="mt-0.5 text-[11px] text-txt-dim">
                  {media.uploader && <span>{media.uploader} · </span>}
                  {media.duration && <span>{formatDuration(media.duration)}</span>}
                </div>
              </div>
            </div>

            {/* Format picker */}
            <div className="mt-3 space-y-1">
              <div className="text-[11px] font-medium text-txt-dim">Quality</div>
              {media.best_formats.map((f) => (
                <label
                  key={f.format_id}
                  className={`flex cursor-pointer items-center gap-3 rounded-md border px-3 py-2 text-xs transition-colors ${
                    selectedFormat === f.format_id
                      ? "border-accent bg-accent-soft"
                      : "border-line hover:bg-surface-3"
                  }`}
                >
                  <input
                    type="radio"
                    name="format"
                    value={f.format_id}
                    checked={selectedFormat === f.format_id}
                    onChange={() => setSelectedFormat(f.format_id)}
                    className="accent-accent"
                  />
                  <span className="min-w-14 font-medium text-txt">
                    {f.resolution || "Audio"}
                  </span>
                  <span className="text-txt-dim">
                    {f.note || f.ext}
                  </span>
                  <span className="ml-auto tabular-nums text-txt-faint">
                    {f.filesize && f.filesize > 0 ? formatBytes(f.filesize) : f.ext}
                  </span>
                </label>
              ))}
            </div>
          </div>
        )}

        {/* Threads + download actions */}
        <div className="mt-4 flex items-center justify-between">
          <div className="flex items-center gap-2 text-xs text-txt-dim">
            <span>Threads:</span>
            <select
              value={threads}
              onChange={(e) => setThreads(Number(e.target.value))}
              className="rounded border border-line bg-surface-2 px-2 py-1 text-xs text-txt outline-none"
            >
              {[1, 2, 4, 8, 16, 32].map((n) => (
                <option key={n} value={n}>
                  {n}
                </option>
              ))}
            </select>
          </div>
          <div className="flex gap-2">
            <button
              onClick={onClose}
              className="rounded-md border border-line px-4 py-1.5 text-xs text-txt-dim transition-colors hover:bg-surface-2 hover:text-txt"
            >
              Cancel
            </button>
            {media ? (
              <button
                onClick={handleDownload}
                className="rounded-md bg-accent px-4 py-1.5 text-xs font-semibold text-accent-text"
              >
                Download
              </button>
            ) : (
              <button
                onClick={handleQuickAdd}
                disabled={!url.trim()}
                className="rounded-md bg-accent px-4 py-1.5 text-xs font-semibold text-accent-text disabled:opacity-50"
              >
                Add to Queue
              </button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
};