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

  // Default-suggested bandwidth = the queue's own configured cap. This
  // makes "set Night = same cap as queue" a single click.
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
    <div className="form">
      <div className="form-grid">
        <Field label="Name">
          <input
            className="input"
            value={s.name}
            onChange={(e) => onChange({ ...s, name: e.target.value })}
            placeholder="Night Profile"
          />
       </Field>
        <Field label="Type">
          <select
            className="select"
            value={s.kind}
            onChange={(e) => onChange({ ...s, kind: e.target.value })}
          >
            <option value="weekly">Weekly (days</option>
            <option value="daily">Daily</option>
            <option value="once">One-time</option>
         </select>
       </Field>
        <Field label="Action">
          <select
            className="select"
            value={s.action}
            onChange={(e) => onChange({ ...s, action: e.target.value })}
          >
            <option value="bandwidth">Bandwidth limit</option>
            <option value="start">Start queue</option>
            <option value="stop">Stop queue</option>
         </select>
       </Field>
        <Field label="Priority (higher wins on overlap)">
          <input
            type="number"
            className="input"
            value={s.priority ?? 100}
            onChange={(e) =>
              onChange({ ...s, priority: Number(e.target.value) })
            }
          />
       </Field>
        <Field label="Start (HH:MM)">
          <input
            className="input"
            value={s.start_time}
            onChange={(e) => onChange({ ...s, start_time: e.target.value })}
            placeholder="23:00"
          />
       </Field>
        <Field label="End (HH:MM)">
          <input
            className="input"
            value={s.end_time}
            onChange={(e) => onChange({ ...s, end_time: e.target.value })}
            placeholder="06:00"
          />
       </Field>
     </div>

      {s.kind === "weekly" && (
        <div className="field">
          <span className="field__label">Days</span>
          <div className="day-picker">
            {DAYS.map((d, i) => (
              <button
                key={d}
                type="button"
                className={`day-pick ${(s.days >> i) & 1 ? "on" : ""}`}
                onClick={() => onToggleDay(i)}
              >
                {d}
             </button>
            ))}
         </div>
       </div>
      )}

      {s.action === "bandwidth" && (
        <div className="field">
          <span className="field__label">Bandwidth per queue (kbps, 0=unlimited</span>
          <div className="bw-overrides">
            {queues.map((q) => {
              const v = overrides[q.name] ?? 0;
              const sug = suggestion(q.name);
              return (
                <div key={q.name} className="bw-overrides__row">
                  <span className="bw-overrides__name">{q.label || q.name}</span>
                  <input
                    type="number"
                    className="input"
                    value={v}
                    onChange={(e) => setOverride(q.name, Number(e.target.value))}
                    placeholder={sug > 0 ? `default ${sug}` : "unlimited"}
                  />
                  <button
                    type="button"
                    className="btn btn--ghost btn--sm"
                    onClick={() => setOverride(q.name, sug)}
                    disabled={sug === v}
                    title="Use the queue's configured limit"
                  >
                    Use queue default
                 </button>
                  <button
                    type="button"
                    className="btn btn--ghost btn--sm"
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
              <div className="form-hint">No queues configured yet</div>
            )}
         </div>
       </div>
      )}

      <label className="check">
        <input
          type="checkbox"
          checked={s.enabled}
          onChange={(e) => onChange({ ...s, enabled: e.target.checked })}
        />
        Enabled
     </label>

      {conflicts.length > 0 && (
        <div className="form-warn">
          {conflicts.map((c, i) => (
            <div key={i}>{c}</div>
          ))}
       </div>
      )}

      <div className="dialog-actions">
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
