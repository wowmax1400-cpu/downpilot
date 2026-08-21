import { useState, type FC } from "react";
import type { DownloadsStore } from "../store/useDownloads";

export const SettingsPanel: FC<{
  store: DownloadsStore;
  onClose: () => void;
}> = ({ store, onClose }) => {
  const s = store.settings;
  const [saveDir, setSaveDir] = useState(s?.save_dir || "/home/amir/Downloads");
  const [threads, setThreads] = useState(s?.threads ?? 8);
  const [speedLimit, setSpeedLimit] = useState(s?.speed_limit_kbps ?? 0);
  const [proxyUrl, setProxyUrl] = useState(s?.proxy_url || "");
  const [schedEnabled, setSchedEnabled] = useState(s?.schedule_enabled ?? false);
  const [schedStart, setSchedStart] = useState(s?.schedule_start || "23:00");
  const [schedEnd, setSchedEnd] = useState(s?.schedule_end || "06:00");

  const handleSave = () => {
    store.updateSetting("save_dir", saveDir);
    store.updateSetting("threads", String(threads));
    store.updateSetting("speed_limit_kbps", String(speedLimit));
    store.updateSetting("proxy_url", proxyUrl);
    store.updateSetting("schedule_enabled", String(schedEnabled));
    store.updateSetting("schedule_start", schedStart);
    store.updateSetting("schedule_end", schedEnd);
    onClose();
  };

  return (
    <div className="dp-backdrop fixed inset-0 z-50 flex items-center justify-center">
      <div className="dp-pop max-h-[90vh] w-[420px] max-w-[94vw] overflow-auto rounded-xl border border-line bg-surface p-5 shadow-lg">
        <h2 className="mb-4 text-base font-semibold">Settings</h2>

        <div className="space-y-3 text-sm">
          {/* Save directory */}
          <label className="block">
            <span className="text-xs text-txt-dim">Download Directory</span>
            <input
              type="text"
              value={saveDir}
              onChange={(e) => setSaveDir(e.target.value)}
              className="mt-1 w-full rounded-md border border-line bg-surface-2 px-3 py-2 text-xs text-txt outline-none focus:border-accent"
            />
          </label>

          {/* Default threads */}
          <label className="block">
            <span className="text-xs text-txt-dim">Default Threads</span>
            <select
              value={threads}
              onChange={(e) => setThreads(Number(e.target.value))}
              className="mt-1 block w-full rounded-md border border-line bg-surface-2 px-3 py-2 text-xs text-txt outline-none"
            >
              {[1, 2, 4, 8, 16, 32].map((n) => (
                <option key={n} value={n}>
                  {n} {n === 1 ? "thread" : "threads"}
                </option>
              ))}
            </select>
          </label>

          {/* Speed limit */}
          <label className="block">
            <span className="text-xs text-txt-dim">Global Speed Limit</span>
            <select
              value={speedLimit}
              onChange={(e) => setSpeedLimit(Number(e.target.value))}
              className="mt-1 block w-full rounded-md border border-line bg-surface-2 px-3 py-2 text-xs text-txt outline-none"
            >
              <option value={0}>Unlimited</option>
              <option value={512}>512 KB/s</option>
              <option value={1024}>1 MB/s</option>
              <option value={5120}>5 MB/s</option>
              <option value={10240}>10 MB/s</option>
              <option value={51200}>50 MB/s</option>
            </select>
          </label>

          {/* Proxy */}
          <label className="block">
            <span className="text-xs text-txt-dim">
              Proxy URL (leave empty for system default)
            </span>
            <input
              type="text"
              value={proxyUrl}
              onChange={(e) => setProxyUrl(e.target.value)}
              placeholder="e.g. socks5://127.0.0.1:1080"
              className="mt-1 w-full rounded-md border border-line bg-surface-2 px-3 py-2 text-xs text-txt outline-none placeholder:text-txt-faint focus:border-accent"
            />
          </label>

          {/* Schedule window */}
          <div className="rounded-md border border-line bg-surface-2 p-3">
            <label className="flex items-center gap-2 pb-2">
              <input
                type="checkbox"
                checked={schedEnabled}
                onChange={(e) => setSchedEnabled(e.target.checked)}
                className="accent-accent"
              />
              <span className="text-xs font-medium text-txt">
                Schedule window (off-peak downloads)
              </span>
            </label>
            <div className="flex items-center gap-2 text-xs">
              <span className="text-txt-dim">Download only between</span>
              <input
                type="time"
                value={schedStart}
                onChange={(e) => setSchedStart(e.target.value)}
                className="rounded-md border border-line bg-surface px-2 py-1 text-xs text-txt outline-none focus:border-accent"
              />
              <span className="text-txt-dim">and</span>
              <input
                type="time"
                value={schedEnd}
                onChange={(e) => setSchedEnd(e.target.value)}
                className="rounded-md border border-line bg-surface px-2 py-1 text-xs text-txt outline-none focus:border-accent"
              />
            </div>
            <p className="pt-1 text-[10px] text-txt-faint">
              Active downloads are auto-paused outside the window and resume
              when it opens. Handles overnight (e.g. 23:00–06:00).
            </p>
          </div>

          {/* Extension integration hint */}
          <div className="rounded-md border border-line bg-surface-2 px-3 py-2 text-[11px] text-txt-dim">
            💡 Chrome extension + native messaging host in{" "}
            <code className="text-txt-faint">extension/</code> and{" "}
            <code className="text-txt-faint">native-host/</code> — Load
            unpacked in chrome://extensions (ID:{" "}
            <code className="text-txt-faint">onmpegdpcecafeebchmmmohbdbjfcepb</code>
            ).
          </div>
        </div>

        <div className="mt-4 flex justify-end gap-2">
          <button
            onClick={onClose}
            className="rounded-md border border-line px-4 py-1.5 text-xs text-txt-dim transition-colors hover:bg-surface-2 hover:text-txt"
          >
            Cancel
          </button>
          <button
            onClick={handleSave}
            className="rounded-md bg-accent px-4 py-1.5 text-xs font-semibold text-accent-text"
          >
            Save
          </button>
        </div>
      </div>
    </div>
  );
};