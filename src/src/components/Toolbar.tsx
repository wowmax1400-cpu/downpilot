// IDM-style toolbar: large icons stacked ABOVE their labels, grouped
// and separated by vertical dividers.

import {
  IdmIconAdd,
  IdmIconResume,
  IdmIconStop,
  IdmIconStopAll,
  IdmIconDelete,
  IdmIconDeleteCompleted,
  IdmIconOptions,
  IdmIconScheduler,
  IdmIconStartQueue,
  IdmIconStopQueue,
  IdmIconGrabber,
  IdmIconTellAFriend,
} from "./icons";
import type { DownloadsStore } from "../store/useDownloads";
import { useState } from "react";

export type ToolbarAction =
  | "add"
  | "grabber"
  | "start"
  | "pause"
  | "stopAll"
  | "delete"
  | "deleteCompleted"
  | "options"
  | "scheduler"
  | "startQueue"
  | "stopQueue"
  | "tellAFriend";

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
 *  rounded corners, no pill shapes. */
function Btn({ icon, label, onClick, disabled, variant = "default", title }: BtnProps) {
  const [hovered, setHovered] = useState(false);
  const [pressed, setPressed] = useState(false);
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      title={title ?? label}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => {
        setHovered(false);
        setPressed(false);
      }}
      onMouseDown={() => !disabled && setPressed(true)}
      onMouseUp={() => setPressed(false)}
      className="toolbar-btn inline-flex flex-col items-center justify-center min-w-[58px] px-2 h-[48px] select-none transition-all shrink-0 rounded-[6px] disabled:cursor-not-allowed disabled:opacity-25"
      style={{
        background: hovered && !disabled
          ? (pressed ? "rgba(255, 255, 255, 0.12)" : "rgba(255, 255, 255, 0.06)")
          : "transparent",
        border: hovered && !disabled
          ? "1px solid rgba(255, 255, 255, 0.15)"
          : "1px solid transparent",
        boxShadow: hovered && !disabled ? "0 2px 8px rgba(0,0,0,0.3)" : "none",
      }}
    >
      <span className="inline-flex h-[28px] w-[28px] items-center justify-center pointer-events-none drop-shadow-[0_2px_4px_rgba(0,0,0,0.4)]">{icon}</span>
      <span className="whitespace-nowrap text-[11px] leading-tight font-medium text-[var(--text)] mt-[3px] px-0.5 pointer-events-none tracking-tight">{label}</span>
    </button>
  );
}

function Sep() {
  return (
    <div
      className="toolbar-group h-[32px] w-[1px] mx-1.5 self-center bg-[var(--line)] shrink-0 opacity-70"
      aria-hidden
    />
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

  const completedTasks = store.tasks.filter((t) => t.status === "completed");

  return (
    <div className="toolbar flex h-[58px] shrink-0 items-center border-b border-[var(--line)] bg-[var(--bg-toolbar)] px-2 select-none overflow-x-auto flex-nowrap" style={{ overflowX: "auto", flexWrap: "nowrap" }}>
      <div className="flex min-w-max h-full items-center gap-1.5 overflow-x-auto flex-nowrap" style={{ overflowX: "auto", flexWrap: "nowrap" }}>
        {/* Group 1: Add URL, Resume, Stop, Stop All */}
        <Btn
          icon={<IdmIconAdd size={32} />}
          label="Add URL"
          onClick={() => onAction("add")}
          title="Add a new download (Ctrl+N)"
        />
        <Btn
          icon={<IdmIconResume size={32} />}
          label="Resume"
          onClick={() => onAction("start")}
          disabled={!canResume}
        />
        <Btn
          icon={<IdmIconStop size={32} />}
          label="Stop"
          onClick={() => onAction("pause")}
          disabled={!anyDownloading}
        />
        <Btn
          icon={<IdmIconStopAll size={32} />}
          label="Stop All"
          onClick={() => onAction("stopAll")}
          title="Stop every active download"
        />

        <Sep />

        {/* Group 2: Delete, Delete Completed */}
        <Btn
          icon={<IdmIconDelete size={32} />}
          label="Delete"
          onClick={() => onAction("delete")}
          disabled={selTasks.length === 0}
        />
        <Btn
          icon={<IdmIconDeleteCompleted size={32} />}
          label="Delete Comp."
          onClick={() => onAction("deleteCompleted")}
          disabled={completedTasks.length === 0}
          title="Delete Completed Downloads"
        />

        <Sep />

        {/* Group 3: Options, Scheduler */}
        <Btn
          icon={<IdmIconOptions size={32} />}
          label="Options"
          onClick={() => onAction("options")}
        />
        <Btn
          icon={<IdmIconScheduler size={32} />}
          label="Scheduler"
          onClick={() => onAction("scheduler")}
        />

        <Sep />

        {/* Group 4: Start Queue, Stop Queue */}
        <Btn
          icon={<IdmIconStartQueue size={32} />}
          label="Start Queue"
          onClick={() => onAction("startQueue")}
          title="Start Main Queue"
        />
        <Btn
          icon={<IdmIconStopQueue size={32} />}
          label="Stop Queue"
          onClick={() => onAction("stopQueue")}
          title="Stop Main Queue"
        />

        <Sep />

        {/* Group 5: Grabber, Tell a Friend */}
        <Btn
          icon={<IdmIconGrabber size={32} />}
          label="Grabber"
          onClick={() => onAction("grabber")}
        />
        <Btn
          icon={<IdmIconTellAFriend size={32} />}
          label="Tell a Friend"
          onClick={() => onAction("tellAFriend")}
          title="Tell a Friend / About"
        />
      </div>
    </div>
  );
}
