// Queue manager panel. Power-user surface for each DownloadQueue:
//   - name + label
//   - active / queued / total counts (live from the store)
//   - per-queue bandwidth cap (inline editable; 0 = unlimited)
//   - maximum simultaneous downloads
//   - enabled / schedule toggle
//   - actions: Start, Pause, Stop, Edit, Delete
//   - Authentication section (collapsed by default): proxy / cookies /
//     user-agent / credential ref / password (stored in OS credential
//     store, never in SQLite)
//
// Per-queue start/pause/stop go through the existing engine methods
// (`resume_queue` / `pause_queue` / `stop_queue`) — no parallel state
// machine. The "Start" button just flips the queue back to enabled and
// re-runs `govern_concurrency`, which promotes queued tasks.

import React from "react";
import type { DownloadsStore } from "../store/useDownloads";
import type { DownloadQueue } from "../types";
import { Dialog, Field, Btn } from "./Dialog";
import { formatBytes } from "../lib/format";

type Props = { store: DownloadsStore; onClose: () => void };

function countsForQueue(store: DownloadsStore, name: string) {
  let active = 0,
    queued = 0,
    total = 0;
  for (const t of store.tasks) {
    if (t.queue !== name) continue;
    total++;
    if (t.status === "downloading") active++;
    else if (t.status === "queued" || t.status === "scheduled") queued++;
  }
  return { active, queued, total };
}

function blankQueue(): DownloadQueue {
  return {
    name: "",
    label: "",
    max_simultaneous: 3,
    speed_limit_kbps: 0,
    enabled: true,
    priority: 0,
    schedule_enabled: false,
    schedule_start: "23:00",
    schedule_end: "06:00",
    on_complete: "",
    on_complete_command: "",
    proxy_url: "",
    cookies: "",
    user_agent: "",
    auth_ref: "",
    auth_user: "",
    has_password: false,
  };
}

export function QueuesPanel({ store, onClose }: Props) {
  const [list, setList] = React.useState<DownloadQueue[]>(store.queues);
  const [edit, setEdit] = React.useState<DownloadQueue | null>(null);
  const [creating, setCreating] = React.useState(false);
  const [passwordDraft, setPasswordDraft] = React.useState("");
  const [authError, setAuthError] = React.useState("");

  const refresh = React.useCallback(() => {
    import("../lib/api").then((a) =>
      a.listQueues().then((q) => setList(q))
    );
  }, []);

  React.useEffect(() => {
    refresh();
  }, [refresh, store.queues]);

  // Clear the password draft whenever we open a different queue's editor.
  React.useEffect(() => {
    setPasswordDraft("");
    setAuthError("");
  }, [edit?.name]);

  const save = async (q: DownloadQueue) => {
    if (!q.name.trim()) return;
    const body = { ...q, label: q.label || q.name };
    await store.updateQueue(body);
    setEdit(null);
    setCreating(false);
    refresh();
  };

  const remove = async (name: string) => {
    if (!window.confirm(`Delete queue "${name}"?`)) return;
    await store.deleteQueue(name);
    refresh();
  };

  const editing = edit ?? (creating ? blankQueue() : null);

  return (
    <Dialog
      title="Queues"
      onClose={onClose}
      width={720}
      footer={
        editing ? undefined : (
          <>
            <Btn variant="primary" onClick={() => setCreating(true)}>
              + New queue
           </Btn>
            <Btn variant="ghost" onClick={onClose}>Close</Btn>
          </>
        )
      }
    >
      {!editing && (
        <div className="form">
          <div className="queue-grid">
            <div className="queue-grid__head">
              <div>Queue</div>
              <div className="queue-cell--num">Active</div>
              <div className="queue-cell--num">Queued</div>
              <div className="queue-cell--num">Total</div>
              <div>Bandwidth</div>
              <div className="queue-cell--num">Max</div>
              <div>Actions</div>
           </div>
            <div className="queue-grid__body">
              {list.length === 0 && (
                <div className="panel__empty">No queues configured</div>
              )}
              {list.map((q) => {
                const c = countsForQueue(store, q.name);
                return (
                  <QueueRow
                    key={q.name}
                    q={q}
                    counts={c}
                    onEdit={() => setEdit({ ...q })}
                    onDelete={() => remove(q.name)}
                    onToggleEnabled={() => store.setQueueEnabled(q.name, !q.enabled)}
                    onSetBandwidth={(kbps) => store.setQueueBandwidth(q.name, kbps)}
                    onStart={() => store.resumeQueueAction(q.name)}
                    onPause={() => store.pauseQueueAction(q.name)}
                    onStop={() => store.stopQueueAction(q.name)}
                  />
                );
              })}
           </div>
         </div>
       </div>
      )}

      {editing && (
        <div className="form">
          <div className="form-grid">
            <Field label="Name (id)">
              <input
                className="input"
                value={editing.name}
                disabled={!creating}
                onChange={(e) => setEdit({ ...editing, name: e.target.value })}
                placeholder="my-queue"
              />
           </Field>
            <Field label="Label">
              <input
                className="input"
                value={editing.label}
                onChange={(e) => setEdit({ ...editing, label: e.target.value })}
              />
           </Field>
            <Field label="Max simultaneous">
              <input
                type="number"
                className="input"
                value={editing.max_simultaneous}
                onChange={(e) =>
                  setEdit({ ...editing, max_simultaneous: Number(e.target.value) })
                }
              />
           </Field>
            <Field label="Speed limit (kbps, 0=unlimited)">
              <input
                type="number"
                className="input"
                value={editing.speed_limit_kbps}
                onChange={(e) =>
                  setEdit({ ...editing, speed_limit_kbps: Number(e.target.value) })
                }
              />
           </Field>
            <Field label="Priority">
              <input
                type="number"
                className="input"
                value={editing.priority}
                onChange={(e) =>
                  setEdit({ ...editing, priority: Number(e.target.value) })
                }
              />
           </Field>
         </div>
          <label className="check">
            <input
              type="checkbox"
              checked={editing.enabled}
              onChange={(e) =>
                setEdit({ ...editing, enabled: e.target.checked })
              }
            />
            Queue enabled
         </label>
          <label className="check">
            <input
              type="checkbox"
              checked={editing.schedule_enabled}
              onChange={(e) =>
                setEdit({ ...editing, schedule_enabled: e.target.checked })
              }
            />
            Schedule active window
         </label>
          {editing.schedule_enabled && (
            <div className="form-grid">
              <Field label="Start (HH:MM)">
                <input
                  className="input"
                  value={editing.schedule_start}
                  onChange={(e) =>
                    setEdit({ ...editing, schedule_start: e.target.value })
                  }
                />
             </Field>
              <Field label="End (HH:MM)">
                <input
                  className="input"
                  value={editing.schedule_end}
                  onChange={(e) =>
                    setEdit({ ...editing, schedule_end: e.target.value })
                  }
                />
             </Field>
           </div>
          )}

          <details
            className="queue-auth"
            open={Boolean(
              editing.proxy_url ||
              editing.user_agent ||
              editing.cookies ||
              editing.auth_ref ||
              editing.auth_user ||
              editing.has_password
            )}
          >
            <summary>
              <span>Authentication</span>
              <span className="queue-auth__state">
                {editing.has_password ? "[ Configured ]" : "[ Not configured ]"}
             </span>
           </summary>
            <div className="form-grid">
              <Field label="Proxy URL">
                <input
                  className="input"
                  placeholder="http://user:pass@host:port"
                  value={editing.proxy_url}
                  onChange={(e) =>
                    setEdit({ ...editing, proxy_url: e.target.value })
                  }
                />
             </Field>
              <Field label="User-Agent">
                <input
                  className="input"
                  placeholder="(inherit global)"
                  value={editing.user_agent}
                  onChange={(e) =>
                    setEdit({ ...editing, user_agent: e.target.value })
                  }
                />
             </Field>
              <Field label="Cookies (Netscape format)">
                <input
                  className="input"
                  placeholder="(none)"
                  value={editing.cookies}
                  onChange={(e) =>
                    setEdit({ ...editing, cookies: e.target.value })
                  }
                />
             </Field>
              <Field label="Credential reference">
                <input
                  className="input"
                  placeholder="keyring://service/account"
                  value={editing.auth_ref}
                  onChange={(e) =>
                    setEdit({ ...editing, auth_ref: e.target.value })
                  }
                />
             </Field>
              <Field label="Username (no password)">
                <input
                  className="input"
                  placeholder="(optional)"
                  value={editing.auth_user}
                  onChange={(e) =>
                    setEdit({ ...editing, auth_user: e.target.value })
                  }
                />
             </Field>
           </div>
            <div className="queue-auth__password">
              <Field label="Password (stored in OS credential store, never in SQLite)">
                <input
                  className="input"
                  type="password"
                  autoComplete="off"
                  placeholder={
                    editing.has_password ? "(set - type to replace)" : "(not set)"
                  }
                  value={passwordDraft}
                  onChange={(e) => setPasswordDraft(e.target.value)}
                />
             </Field>
              <div className="queue-auth__buttons">
                <button
                  type="button"
                  className="btn btn--default btn--sm"
                  disabled={!passwordDraft}
                  onClick={async () => {
                    try {
                      setAuthError("");
                      await store.saveQueuePassword(editing.name, passwordDraft);
                      setPasswordDraft("");
                      await refresh();
                    } catch (e) {
                      setAuthError(String(e));
                    }
                  }}
                >
                  Configure / Replace
               </button>
                <button
                  type="button"
                  className="btn btn--ghost btn--sm"
                  disabled={!editing.has_password}
                  onClick={async () => {
                    try {
                      setAuthError("");
                      await store.clearQueuePassword(editing.name);
                      await refresh();
                    } catch (e) {
                      setAuthError(String(e));
                    }
                  }}
                >
                  Remove
               </button>
                <button
                  type="button"
                  className="btn btn--default btn--sm"
                  onClick={async () => {
                    try {
                      setAuthError("");
                      const ok = await store.testQueueAuth(editing.name);
                      setAuthError(
                        ok
                          ? "Credentials resolved."
                          : "No credentials configured for this queue."
                      );
                    } catch (e) {
                      setAuthError(String(e));
                    }
                  }}
                >
                  Test
               </button>
             </div>
              {authError && <div className="form-hint">{authError}</div>}
              <p className="queue-auth__hint">
                DownPilot never stores passwords in SQLite. They live in the OS
                credential store under{" "}
                <code>downpilot/queue/{editing.name || "(name)"}</code>.
             </p>
           </div>
         </details>

          <div className="dialog-actions">
            <Btn variant="ghost" onClick={() => { setEdit(null); setCreating(false); }}>
              Cancel
           </Btn>
            <Btn variant="primary" onClick={() => save(editing)} disabled={!editing.name.trim()}>
              Save
           </Btn>
         </div>
       </div>
      )}
   </Dialog>
  );
}

type RowProps = {
  q: DownloadQueue;
  counts: { active: number; queued: number; total: number };
  onEdit: () => void;
  onDelete: () => void;
  onToggleEnabled: () => void;
  onSetBandwidth: (kbps: number) => void;
  onStart: () => void;
  onPause: () => void;
  onStop: () => void;
};

function QueueRow({
  q,
  counts,
  onEdit,
  onDelete,
  onToggleEnabled,
  onSetBandwidth,
  onStart,
  onPause,
  onStop,
}: RowProps) {
  const [bwEditing, setBwEditing] = React.useState(false);
  const [bwDraft, setBwDraft] = React.useState(String(q.speed_limit_kbps));

  React.useEffect(() => {
    setBwDraft(String(q.speed_limit_kbps));
  }, [q.speed_limit_kbps]);

  const commitBw = () => {
    setBwEditing(false);
    const n = Number(bwDraft);
    if (!isNaN(n) && n >= 0 && n !== q.speed_limit_kbps) {
      onSetBandwidth(n);
    }
  };

  const disabled = !q.enabled || counts.active === 0;

  // Show the configured limit and any currently-effective scheduled limit.
  const effective = q.speed_limit_kbps;
  const configured = q.speed_limit_kbps;

  return (
    <div className={"queue-grid__row" + (q.enabled ? "" : " queue-grid__row--disabled")}>
      <div className="queue-grid__name">
        <span className="queue-grid__title">{q.label || q.name}</span>
        <span className="queue-grid__sub">{q.name}</span>
        {q.has_password && (
          <span className="queue-grid__auth-dot" title="Has stored credentials" />
        )}
     </div>
      <div className="queue-cell--num">
        <span className={"queue-count" + (counts.active ? " queue-count--on" : "")}>
          {counts.active}
       </span>
     </div>
      <div className="queue-cell--num">
        <span className={"queue-count" + (counts.queued ? " queue-count--wait" : "")}>
          {counts.queued}
       </span>
     </div>
      <div className="queue-cell--num">
        <span className="queue-count">{counts.total}</span>
     </div>
      <div className="queue-grid__bw">
        {bwEditing ? (
          <input
            type="number"
            className="input"
            value={bwDraft}
            autoFocus
            onChange={(e) => setBwDraft(e.target.value)}
            onBlur={commitBw}
            onKeyDown={(e) => {
              if (e.key === "Enter") commitBw();
              else if (e.key === "Escape") {
                setBwDraft(String(configured));
                setBwEditing(false);
              }
            }}
            style={{ width: 90 }}
          />
        ) : (
          <button
            className="btn btn--ghost btn--sm"
            title="Click to edit bandwidth cap"
            onClick={() => setBwEditing(true)}
          >
            {effective > 0
              ? `${formatBytes(effective * 1024)}/s`
              : "unlimited"}
         </button>
        )}
     </div>
      <div className="queue-cell--num">{q.max_simultaneous}</div>
      <div className="queue-grid__actions">
        <button
          className="btn btn--ghost btn--sm"
          title="Start all queued/paused tasks in this queue"
          onClick={onStart}
          disabled={!q.enabled}
        >
          Start
       </button>
        <button
          className="btn btn--ghost btn--sm"
          title="Pause every downloading task in this queue"
          onClick={onPause}
          disabled={disabled}
        >
          Pause
       </button>
        <button
          className="btn btn--ghost btn--sm"
          title="Stop every non-terminal task in this queue"
          onClick={onStop}
          disabled={counts.active === 0 && counts.queued === 0}
        >
          Stop
       </button>
        <button
          className="btn btn--default btn--sm"
          title={q.enabled ? "Disable queue" : "Enable queue"}
          onClick={onToggleEnabled}
        >
          {q.enabled ? "On" : "Off"}
       </button>
        <button className="btn btn--default btn--sm" onClick={onEdit}>
          Edit
       </button>
        <button className="btn btn--ghost btn--sm" onClick={onDelete}>
          Del
       </button>
     </div>
   </div>
  );
}
