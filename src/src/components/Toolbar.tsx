// Modern dark toolbar: a single unified container with rounded hover
// states. Inline icon+label buttons (no stacked IDM-style). Subtle dark
// border bottom separates it from the body. The right side has a
// bandwidth selector (IDM-style: Unlimited / 1 / 5 / 10 MB/s / Custom).

import {
  IconAdd,
  IconStart,
  IconPause,
  IconStop,
  IconDelete,
  IconOptions,
  IconScheduler,
  IconQueue,
  IconGrabber,
  IconStopAll,
  IconDeleteAll,
  IconForceStart,
} from "./icons";
import type { DownloadsStore } from "../store/useDownloads";
import type { AppSettings } from "../types";

export type ToolbarAction =
  | "add"
  | "grabber"
  | "start"
  | "pause"
  | "stop"
  | "stopAll"
  | "forceStart"
  | "delete"
  | "deleteAll"
  | "scheduler"
  | "queues"
  | "options";

interface ToolbarProps {
  store: DownloadsStore;
  onAction: (a: ToolbarAction) => void;
}

interface BtnProps {
  icon: React.ReactNode;
  label: string;
  onClick: () => void;
  disabled?: boolean;
  variant?: "default" | "accent" | "danger";
  title?: string;
}

function Btn({ icon, label, onClick, disabled, variant = "default", title }: BtnProps) {
  const base =
    "inline-flex h-8 items-center gap-1.5 rounded-md px-2.5 text-sm transition-colors disabled:cursor-not-allowed disabled:opacity-40";
  const color =
    variant === "accent"
      ? "text-indigo-400 hover:bg-indigo-500/10"
      : variant === "danger"
      ? "text-red-400 hover:bg-red-500/10"
      : "text-txt-dim hover:bg-white/5 hover:text-txt";
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      title={title ?? label}
      className={`${base} ${color}`}
    >
      <span className="inline-flex h-4 w-4 items-center justify-center">{icon}</span>
      <span>{label}</span>
    </button>
  );
}

function Sep() {
  return <div className="mx-1 h-5 w-px bg-white/5" aria-hidden />;
}

/** Bandwidth cap options shown in the toolbar selector. "0" means
 *  Unlimited (the default). The engine stores kbps in
 *  `speed_limit_kbps` and converts via *1024. */
const BANDWIDTH_OPTIONS: Array<{ label: string; kbps: number }> = [
  { label: "Unlimited", kbps: 0 },
  { label: "1 MB/s", kbps: 1024 },
  { label: "5 MB/s", kbps: 5 * 1024 },
  { label: "10 MB/s", kbps: 10 * 1024 },
];

function BandwidthSelect({
  settings,
  onChange,
}: {
  settings: AppSettings | null;
  onChange: (kbps: number) => void;
}) {
  const currentKbps = parseInt(String(settings?.speed_limit_kbps ?? "0"), 10) || 0;
  const isPreset = BANDWIDTH_OPTIONS.some((o) => o.kbps === currentKbps);
  const value = isPreset ? String(currentKbps) : "custom-" + currentKbps;
  return (
    <div className="bw" title="Global bandwidth cap (applies to all downloads)">
      <svg viewBox="0 0 16 16" className="h-3.5 w-3.5" fill="none" stroke="currentColor" strokeWidth="1.5">
        <path d="M5 2v3M11 2v3M5 11v3M11 11v3M2 5h3M2 11h3M11 5h3M11 11h3M8 5l-2 3h4l-2 3" />
      </svg>
      <select
        className="bw__select"
        value={value}
        onChange={(e) => {
          const v = e.target.value;
          if (v.startsWith("custom")) return;
          onChange(parseInt(v, 10));
        }}
      >
        {BANDWIDTH_OPTIONS.map((o) => (
          <option key={o.kbps} value={String(o.kbps)}>{o.label}</option>
        ))}
        {!isPreset && <option value={"custom-" + currentKbps}>{currentKbps} kbps</option>}
      </select>
    </div>
  );
}

export function Toolbar({ store, onAction }: ToolbarProps) {
  const sel = store.selected;
  const selArr = Array.from(sel);
  const selTasks = selArr
    .map((id) => store.tasks.find((t) => t.id === id))
    .filter((t): t is NonNullable<typeof t> => Boolean(t));

  const anyDownloading = selTasks.some((t) => t.status === "downloading");
  const anyPaused = selTasks.some((t) => t.status === "paused");
  const anyFinished = selTasks.some((t) => t.status === "completed");
  const canResume = selTasks.length > 0 && !anyDownloading;

  return (
    <div className="border-b border-white/5 bg-surface/80">
      <div className="flex h-11 items-center gap-0.5 overflow-x-auto px-2">
        {/* Group 1: add */}
        <Btn
          icon={<IconAdd size={15} />}
          label="Add URL"
          onClick={() => onAction("add")}
          variant="accent"
          title="Add a new download (Ctrl+N)"
        />
        <Btn
          icon={<IconGrabber size={15} />}
          label="Grabber"
          onClick={() => onAction("grabber")}
        />
        <Sep />

        {/* Group 2: control selected */}
        <Btn
          icon={<IconStart size={15} />}
          label={anyPaused ? "Resume" : "Start"}
          onClick={() => onAction("start")}
          disabled={!canResume}
          variant="accent"
        />
        <Btn
          icon={<IconPause size={15} />}
          label="Pause"
          onClick={() => onAction("pause")}
          disabled={!anyDownloading}
        />
        <Btn
          icon={<IconStop size={15} />}
          label="Stop"
          onClick={() => onAction("stop")}
          disabled={!anyDownloading && !anyPaused}
        />
        <Btn
          icon={<IconForceStart size={15} />}
          label="Force Start"
          onClick={() => onAction("forceStart")}
          disabled={selTasks.length === 0}
        />
        <Btn
          icon={<IconDelete size={15} />}
          label="Delete"
          onClick={() => onAction("delete")}
          disabled={selTasks.length === 0}
          variant="danger"
        />
        <Sep />

        {/* Group 3: stop all / clear */}
        <Btn
          icon={<IconStopAll size={15} />}
          label="Stop All"
          onClick={() => onAction("stopAll")}
        />
        <Btn
          icon={<IconDeleteAll size={15} />}
          label="Delete All"
          onClick={() => onAction("deleteAll")}
          disabled={!anyFinished && store.tasks.length === 0}
          variant="danger"
        />
        <Sep />

        {/* Group 4: config */}
        <Btn
          icon={<IconOptions size={15} />}
          label="Options"
          onClick={() => onAction("options")}
        />
        <Btn
          icon={<IconScheduler size={15} />}
          label="Scheduler"
          onClick={() => onAction("scheduler")}
        />
        <Btn
          icon={<IconQueue size={15} />}
          label="Queues"
          onClick={() => onAction("queues")}
        />

        <div className="flex-1" />
        <BandwidthSelect
          settings={store.settings}
          onChange={(kbps) => store.setBandwidth(kbps)}
        />
      </div>
    </div>
  );
}
