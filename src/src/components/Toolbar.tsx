// IDM-style toolbar: large icons stacked ABOVE their labels, grouped
// and separated by vertical dividers (spec):
//   [Add URL] | [Resume, Pause, Stop All] | [Delete] |
//   [Options, Scheduler, Site Grabber]  (spec 1)
// The right side keeps the bandwidth selector (Unlimited / 1 / 5 /
// 10 MB/s / Custom) — IDM's own cap dropdown lives in the same place.

import {
  IconAdd,
  IconStart,
  IconPause,
  IconDelete,
  IconOptions,
  IconScheduler,
  IconGrabber,
  IconStop,
} from "./icons";
import type { DownloadsStore } from "../store/useDownloads";
import type { AppSettings } from "../types";

export type ToolbarAction =
  | "add"
  | "grabber"
  | "start"
  | "pause"
  | "stopAll"
  | "delete"
  | "options"
  | "scheduler";

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

/** Dense utility-strip button: icon + label on ONE line (classic
 *  desktop toolbar), rectangular with a sharp hairline border. No
 *  rounded corners, no pill shapes (refactor rule 2). */
function Btn({ icon, label, onClick, disabled, variant = "default", title }: BtnProps) {
  const base =
    "inline-flex h-[24px] items-center gap-1.5 rounded-[2px] border border-transparent px-1.5 transition-colors disabled:cursor-not-allowed disabled:opacity-40";
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
      <span className="inline-flex h-[14px] w-[14px] items-center justify-center">{icon}</span>
      <span className="max-w-[86px] truncate text-[11px] leading-none">{label}</span>
    </button>
  );
}

function Sep() {
  return <div className="toolbar__sep" aria-hidden />;
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
  const canResume =
    selTasks.length > 0 &&
    selTasks.every((t) => t.status !== "downloading");

  return (
    <div className="toolbar border-b border-[var(--chrome-line)]">
      <div className="flex min-w-max h-[30px] items-center gap-[3px] overflow-x-auto px-1.5">
        {/* Primary action */}
        <Btn
          icon={<IconAdd size={13} />}
          label="Add URL"
          onClick={() => onAction("add")}
          variant="accent"
          title="Add a new download (Ctrl+N)"
        />
        <Sep />

        {/* Download control */}
        <Btn
          icon={<IconStart size={13} />}
          label="Resume"
          onClick={() => onAction("start")}
          disabled={!canResume}
          variant="accent"
        />
        <Btn
          icon={<IconPause size={13} />}
          label="Pause"
          onClick={() => onAction("pause")}
          disabled={!anyDownloading}
        />
        <Btn
          icon={<IconStop size={13} />}
          label="Stop All"
          onClick={() => onAction("stopAll")}
          title="Stop every active download"
        />
        <Sep />

        <Btn
          icon={<IconDelete size={13} />}
          label="Delete"
          onClick={() => onAction("delete")}
          disabled={selTasks.length === 0}
          variant="danger"
        />
        <Sep />

        {/* Tools */}
        <Btn
          icon={<IconOptions size={13} />}
          label="Options"
          onClick={() => onAction("options")}
        />
        <Btn
          icon={<IconScheduler size={13} />}
          label="Scheduler"
          onClick={() => onAction("scheduler")}
        />
        <Btn
          icon={<IconGrabber size={13} />}
          label="Site Grabber"
          onClick={() => onAction("grabber")}
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
