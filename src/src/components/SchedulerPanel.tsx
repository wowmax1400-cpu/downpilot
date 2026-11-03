import React from "react";
import type { DownloadsStore } from "../store/useDownloads";
import type { Schedule } from "../types";
import { Dialog, Field, Btn } from "./Dialog";

type Props = { store: DownloadsStore; onClose: () => void };

const DAYS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

function parseOverrides(json: string | undefined | null): Record<string, number> {
  if (!json) return {};
  try {
    const v = JSON.parse(json);
    if (v && typeof v === "object") {
      const out: Record<string, number> = {};
      for (const [k, val] of Object.entries(v as Record<string, unknown>)) {
        const n = Number(val);
        if (Number.isFinite(n)) out[k] = n;
      }
      return out;
    }
  } catch {
    /* ignore */
  }
  return {};
}

function emptyOverrides(): Record<string, number> {
  return {};
}

function overridesToJson(map: Record<string, number>): string {
  const entries = Object.entries(map).filter(([, v]) => Number.isFinite(v) && v >= 0);
  if (entries.length === 0) return "";
  return JSON.stringify(Object.fromEntries(entries));
}

function dayMaskLabel(days: number): string {
  if (days === 127) return "Every day";
  if (days === 31) return "Mon–Fri";
  if (days === 96) return "Sat–Sun";
  return DAYS.filter((_, i) => (days >> i) & 1).join(" ");
}

export function SchedulerPanel({ store, onClose }: Props) {
  const [list, setList] = React.useState<Schedule[]>([]);
  const [edit, setEdit] = React.useState<Schedule | null>(null);
  const [creating, setCreating] = React.useState(false);

  const refresh = React.useCallback(async () => {
    const ls = await Promise.resolve(store.schedules);
    setList(ls);
  }, [store]);

  React.useEffect(() => {
    void refresh();
  }, [refresh]);

  const blank = (): Schedule => ({
    id: 0,
    name: "",
    queue: "general",
    enabled: true,
    kind: "weekly",
    start_time: "23:00",
    end_time: "06:00",
    days: 127,
    action: "bandwidth",
    bandwidth_kbps: 0,
    on_complete: "",
    on_complete_command: "",
    priority: 0,
    overrides_json: "",
  });

  const save = async (s: Schedule) => {
    if (!s.name.trim()) return;
    await store.saveSchedule(s);
    setEdit(null);
    setCreating(false);
    await refresh();
  };

  const remove = async (id: number) => {
    await store.deleteSchedule(id);
    await refresh();
  };

  const toggleDay = (s: Schedule, bit: number) => {
    const mask = 1 << bit;
    const days = (s.days & mask) ? (s.days & ~mask) : (s.days | mask);
    setEdit({ ...s, days });
  };

  const editing = edit ?? (creating ? blank() : null);

  const activeSet = new Set(store.activeProfiles.map((p) => p.schedule_id));
  const activeQueues = new Set(store.activeProfiles.flatMap((p) => p.affected_queues));

  return (
    <Dialog
      title="Scheduler"
      onClose={onClose}
      width={720}
      footer={editing ? undefined : <Btn variant="ghost" onClick={onClose}>Close</Btn>}
    >
      {!editing && (
        <div className="panel">
          <div className="panel__list">
            {list.map((s) => {
              const isActive = activeSet.has(s.id);
              const targets = parseOverrides(s.overrides_json ?? "");
              const targetNames = Object.keys(targets);
              const targetText =
                targetNames.length === 0
                  ? s.queue
                  : targetNames.length <= 2
                    ? targetNames.join(", ")
                    : `${targetNames.length} queues`;
              return (
                <div key={s.id} className="panel__row">
                  <div className="panel__row-main">
                    <span className="panel__row-title">{s.name || "(unnamed)"}</span>
                    <span className="panel__row-sub">
                      {s.kind} · {dayMaskLabel(s.days)} · {s.start_time}–{s.end_time} ·{" "}
                      {s.action} {targetText} · priority {s.priority ?? 100}
                   </span>
                 </div>
                  <div className="panel__row-actions">
                    {isActive && (
                      <span className="badge badge--ok" title="Currently active">
                        Active
                     </span>
                    )}
                    <label className="check check--inline">
                      <input
                        type="checkbox"
                        checked={s.enabled}
                        onChange={(e) => {
                          void store.saveSchedule({ ...s, enabled: e.target.checked });
                        }}
                      />
                      Enabled
                   </label>
                    <button
                      className="btn btn--default btn--sm"
                      onClick={() => setEdit({ ...s })}
                    >
                      Edit
                   </button>
                    <button
                      className="btn btn--ghost btn--sm"
                      onClick={() => remove(s.id)}
                    >
                      Delete
                   </button>
                 </div>
               </div>
              );
            })}
            {list.length === 0 && (
              <div className="panel__empty">
                No schedules yet. Click "+ Add Schedule" to create one.
             </div>
            )}
         </div>
          <button
            className="btn btn--primary btn--sm"
            onClick={() => setCreating(true)}
          >
            + Add Schedule
         </button>
       </div>
      )}

      {editing && (
        <ScheduleEditor
          schedule={editing}
          queues={store.queues}
          onChange={setEdit}
          onToggleDay={(bit) => toggleDay(editing, bit)}
          onSave={() => save(editing)}
          onCancel={() => {
            setEdit(null);
            setCreating(false);
          }}
        />
      )}
   </Dialog>
  );
}

type EditorProps = {
  schedule: Schedule;
  queues: { name: string; label: string; speed_limit_kbps: number }[];
  onChange: (s: Schedule) => void;
  onToggleDay: (bit: number) => void;
  onSave: () => void;
  onCancel: () => void;
};

function ScheduleEditor({
  schedule: s,
  queues,
  onChange,
  onToggleDay,
  onSave,
  onCancel,
}: EditorProps) {
  const overrides = parseOverrides(s.overrides_json ?? "");
  const setOverride = (qname: string, kbps: number) => {
    const next = { ...overrides };
    if (!kbps || kbps <= 0) {
      delete next[qname];
    } else {
      next[qname] = kbps;
    }
    onChange({ ...s, overrides_json: overridesToJson(next) });
  };

  const suggestion = (qname: string) =>
    queues.find((q) => q.name === qname)?.speed_limit_kbps ?? 0;

  const conflicts: string[] = [];
  if (s.action === "bandwidth" && Object.keys(overrides).length === 0) {
    conflicts.push("Bandwidth profile with no queues has no effect.");
  }
  if (s.start_time === s.end_time) {
    conflicts.push("Start and end are identical; the window never opens.");
  }

  return (
    <div className="flex flex-col gap-2">
      <fieldset className="groupbox">
        <legend>Schedule Profile</legend>
        <div className="form-grid-kv">
          <dt>Name</dt>
          <dd>
            <input
              className="input w-full"
              value={s.name}
              onChange={(e) => onChange({ ...s, name: e.target.value })}
              placeholder="Night Profile"
            />
          </dd>

          <dt>Type</dt>
          <dd>
            <select
              className="select w-full"
              value={s.kind}
              onChange={(e) => onChange({ ...s, kind: e.target.value })}
            >
              <option value="weekly">Weekly (days)</option>
              <option value="daily">Daily</option>
              <option value="once">One-time</option>
            </select>
          </dd>

          <dt>Action</dt>
          <dd>
            <select
              className="select w-full"
              value={s.action}
              onChange={(e) => onChange({ ...s, action: e.target.value })}
            >
              <option value="bandwidth">Bandwidth limit</option>
              <option value="start">Start queue</option>
              <option value="stop">Stop queue</option>
            </select>
          </dd>

          <dt>Priority</dt>
          <dd>
            <input
              type="number"
              className="input w-28"
              value={s.priority ?? 100}
              onChange={(e) =>
                onChange({ ...s, priority: Number(e.target.value) })
              }
            />
          </dd>

          <dt>Start time</dt>
          <dd>
            <input
              className="input w-28"
              value={s.start_time}
              onChange={(e) => onChange({ ...s, start_time: e.target.value })}
              placeholder="23:00"
            />
          </dd>

          <dt>End time</dt>
          <dd>
            <input
              className="input w-28"
              value={s.end_time}
              onChange={(e) => onChange({ ...s, end_time: e.target.value })}
              placeholder="06:00"
            />
          </dd>
        </div>
      </fieldset>

      {s.kind === "weekly" && (
        <fieldset className="groupbox">
          <legend>Active Days</legend>
          <div className="flex gap-1.5 flex-wrap">
            {DAYS.map((d, i) => (
              <button
                key={d}
                type="button"
                className={`btn btn--sm ${(s.days >> i) & 1 ? "btn--primary" : "btn--ghost"}`}
                onClick={() => onToggleDay(i)}
              >
                {d}
              </button>
            ))}
          </div>
        </fieldset>
      )}

      {s.action === "bandwidth" && (
        <fieldset className="groupbox">
          <legend>Bandwidth per Queue (kbps, 0 = unlimited)</legend>
          <div className="flex flex-col gap-2">
            {queues.map((q) => {
              const v = overrides[q.name] ?? 0;
              const sug = suggestion(q.name);
              return (
                <div key={q.name} className="flex items-center gap-2">
                  <span className="w-28 truncate text-xs font-medium">{q.label || q.name}</span>
                  <input
                    type="number"
                    className="input w-24"
                    value={v}
                    onChange={(e) => setOverride(q.name, Number(e.target.value))}
                    placeholder={sug > 0 ? `default ${sug}` : "unlimited"}
                  />
                  <button
                    type="button"
                    className="btn btn--sm"
                    onClick={() => setOverride(q.name, sug)}
                    disabled={sug === v}
                    title="Use the queue's configured limit"
                  >
                    Queue default
                  </button>
                  <button
                    type="button"
                    className="btn btn--sm"
                    onClick={() => setOverride(q.name, 0)}
                    disabled={v === 0}
                    title="Unlimited while this profile is active"
                  >
                    Unlimited
                  </button>
                </div>
              );
            })}
            {queues.length === 0 && (
              <div className="text-xs text-gray-500">No queues configured yet</div>
            )}
          </div>
        </fieldset>
      )}

      <label className="check mt-1">
        <input
          type="checkbox"
          checked={s.enabled}
          onChange={(e) => onChange({ ...s, enabled: e.target.checked })}
        />
        <span>Enabled</span>
      </label>

      {conflicts.length > 0 && (
        <div className="text-xs text-amber-600 bg-amber-50 p-2 border border-amber-200 rounded">
          {conflicts.map((c, i) => (
            <div key={i}>{c}</div>
          ))}
        </div>
      )}

      <div className="flex justify-end gap-2 mt-2">
        <Btn variant="ghost" onClick={onCancel}>
          Cancel
        </Btn>
        <Btn variant="primary" onClick={onSave} disabled={!s.name.trim()}>
          Save
        </Btn>
      </div>
    </div>
  );
}
