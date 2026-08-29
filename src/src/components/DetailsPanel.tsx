import type { DownloadsStore } from "../store/useDownloads";
import type { DownloadTask } from "../types";
import { formatBytes, formatSpeed, formatEta, formatPercent, formatDate, domainOf } from "../lib/format";
import { IconStart, IconPause, IconStop, IconDelete, IconProperties, IconFolder, IconRedownload } from "./icons";

type Props = {
  store: DownloadsStore;
  onOpenProps: (id: number) => void;
};

export function DetailsPanel({ store, onOpenProps }: Props) {
  const sel = [...store.selected];
  const task: DownloadTask | null =
    sel.length === 1 ? store.tasks.find((t) => t.id === sel[0]) ?? null : null;

  return (
    <aside className="details">
      <div className="details__head">
        <span className="details__title">Details</span>
        {task && (
          <button className="icon-btn" title="Properties" onClick={() => onOpenProps(task.id)}>
            <IconProperties size={14} />
          </button>
        )}
      </div>

      {!task && (
        <div className="details__empty">
          {sel.length > 1
            ? `${sel.length} downloads selected`
            : "Select a download to see details."}
        </div>
      )}

      {task && (
        <div className="details__body">
          <div className="details__name" title={task.filename || task.url}>
            {task.filename || task.url}
          </div>

          <div className="details__actions">
            {task.status === "downloading" ? (
              <button className="btn btn--default btn--sm" onClick={() => store.pause(task.id)}>
                <IconPause size={13} /> Pause
              </button>
            ) : (
              <button className="btn btn--primary btn--sm" onClick={() => store.resume(task.id)}>
                <IconStart size={13} /> {task.status === "paused" ? "Resume" : "Start"}
              </button>
            )}
            <button className="btn btn--default btn--sm" onClick={() => store.cancel(task.id)}>
              <IconStop size={13} /> Stop
            </button>
            <button className="btn btn--default btn--sm" onClick={() => store.redownload(task.id)}>
              <IconRedownload size={13} /> Redownload
            </button>
            <button className="btn btn--ghost btn--sm" onClick={() => store.openFolder(task)}>
              <IconFolder size={13} /> Open Folder
            </button>
          </div>

          <dl className="kv">
            <Detail k="Status" v={task.status} />
            <Detail k="Size" v={formatBytes(task.total_size)} />
            <Detail k="Downloaded" v={formatBytes(task.downloaded)} />
            <Detail
              k="Progress"
              v={formatPercent(task.downloaded, task.total_size)}
            />
            <Detail k="Speed" v={task.status === "downloading" ? formatSpeed(task.speed_bps) : "—"} />
            <Detail
              k="Time Left"
              v={
                task.status === "downloading" && task.total_size - task.downloaded > 0
                  ? formatEta(task.total_size - task.downloaded, task.speed_bps)
                  : "—"
              }
            />
            <Detail k="Category" v={task.category} />
            <Detail k="Queue" v={task.queue} />
            <Detail k="Connections" v={String(task.threads || 1)} />
            <Detail k="Added" v={formatDate(task.created_at)} />
            <Detail k="Domain" v={domainOf(task.url)} />
            <Detail k="Save To" v={task.save_dir} />
          </dl>

          {task.error && task.status === "failed" && (
            <div className="details__error">
              <strong>Error:</strong> {task.error}
            </div>
          )}
        </div>
      )}
    </aside>
  );
}

function Detail({ k, v }: { k: string; v: string }) {
  return (
    <div className="kv__row">
      <dt>{k}</dt>
      <dd title={v}>{v}</dd>
    </div>
  );
}
