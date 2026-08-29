// IDM-class right-click context menu for download rows.
//
// Every entry is wired to a real action. Visibility is state-aware
// (Pause is hidden on completed tasks, Retry only shows on failed,
// Open File only on completed, etc.). Multi-select passes the same
// action to every selected row.

import React from "react";
import type { DownloadsStore } from "../store/useDownloads";
import {
  IconStart,
  IconPause,
  IconStop,
  IconDelete,
  IconRedownload,
  IconFolder,
  IconProperties,
  IconQueue,
  IconCategory,
  IconForceStart,
  IconOpenFile,
  IconCopy,
  IconRetry,
} from "./icons";

type Props = {
  x: number;
  y: number;
  store: DownloadsStore;
  ids: number[];
  onClose: () => void;
  onOpenProps: (id: number) => void;
};

function Item({
  label,
  icon,
  onClick,
  divider,
  danger,
  disabled,
}: {
  label?: string;
  icon?: React.ReactNode;
  onClick?: () => void;
  divider?: boolean;
  danger?: boolean;
  disabled?: boolean;
}) {
  if (divider) return <div className="ctx-sep" />;
  return (
    <button
      className={`ctx-row ${danger ? "danger" : ""}`}
      disabled={disabled}
      onClick={() => {
        onClick?.();
      }}
    >
      {icon && <span className="ctx-row__icon">{icon}</span>}
      <span>{label}</span>
    </button>
  );
}

export function ContextMenu(props: Props) {
  const { store, ids } = props;
  const single = ids.length === 1 ? store.tasks.find((t) => t.id === ids[0]) : null;
  const act = (fn: (id: number) => void) => {
    ids.forEach((id) => fn(id));
    props.onClose();
  };
  const copyText = (text: string) => {
    if (navigator.clipboard?.writeText) {
      void navigator.clipboard.writeText(text);
    }
    props.onClose();
  };

  // clamp position to viewport
  const X = Math.min(props.x, window.innerWidth - 230);
  const Y = Math.min(props.y, window.innerHeight - 460);

  // ---- State derivation ----
  const selectedTasks = ids
    .map((id) => store.tasks.find((t) => t.id === id))
    .filter((t): t is NonNullable<typeof t> => Boolean(t));
  const anyDownloading = selectedTasks.some((t) => t.status === "downloading");
  const anyPaused = selectedTasks.some((t) => t.status === "paused" || t.status === "stopped");
  const anyCompleted = selectedTasks.some((t) => t.status === "completed");
  const anyFailed = selectedTasks.some((t) => t.status === "failed");
  const anyResumable = selectedTasks.some((t) =>
    t.status === "paused" || t.status === "stopped" || t.status === "queued" || t.status === "failed"
  );

  const canResume = anyResumable;
  const canPause = anyDownloading;
  const canStop = selectedTasks.length > 0 && !anyCompleted;
  const canForce = !!ids.length;
  const canRetry = anyFailed;
  const canRedownload = !!ids.length;
  const canOpenFile = !!single && single.status === "completed";
  const canOpenFolder = !!single;
  const canRename = !!single;
  const canCopyUrl = !!single;
  const canCopyPath = !!single;
  const canMoveToQueue = !!single;
  const canChangeCategory = !!single;
  const canProps = !!single;
  const canDelete = !!ids.length;
  const canDeleteFile = !!single;

  const renamePrompt = () => {
    if (!single) return;
    const next = window.prompt("Rename to:", single.filename);
    if (next && next.trim() && next.trim() !== single.filename) {
      void store.rename(single.id, next.trim());
    }
    props.onClose();
  };

  const moveToQueuePrompt = () => {
    if (!single) return;
    const names = store.queues.map((q) => q.label || q.name).join(", ");
    const next = window.prompt(
      "Move to queue (" + names + "):",
      single.queue ?? "general"
    );
    if (next && next.trim()) {
      void store.moveToQueue(single.id, next.trim());
    }
    props.onClose();
  };

  const changeCategoryPrompt = () => {
    if (!single) return;
    const names = store.categories.map((c) => c.label || c.name).join(", ");
    const next = window.prompt(
      "Change category (" + names + ", or other):",
      single.category ?? "other"
    );
    if (next && next.trim()) {
      void store.setCategory(single.id, next.trim());
    }
    props.onClose();
  };

  const moveToFolderPrompt = () => {
    if (!single) return;
    const next = window.prompt(
      "New save folder (absolute path):",
      single.save_dir ?? ""
    );
    if (next !== null) {
      void store.setSaveDir(single.id, next.trim());
    }
    props.onClose();
  };

  return (
    <>
      <div
        className="ctx-backdrop"
        onMouseDown={props.onClose}
        onContextMenu={(e) => {
          e.preventDefault();
          props.onClose();
        }}
      />
      <div
        className="ctx-menu"
        style={{ left: X, top: Y }}
        onContextMenu={(e) => e.preventDefault()}
      >
        <Item
          label="Resume"
          icon={<IconStart size={13} />}
          onClick={() => act(store.resume)}
          disabled={!canResume}
        />
        <Item
          label="Pause"
          icon={<IconPause size={13} />}
          onClick={() => act(store.pause)}
          disabled={!canPause}
        />
        <Item
          label="Stop"
          icon={<IconStop size={13} />}
          onClick={() => act(store.cancel)}
          disabled={!canStop}
        />
        <Item
          label="Force Start"
          icon={<IconForceStart size={13} />}
          onClick={() => act(store.forceStart)}
          disabled={!canForce}
        />
        <Item
          label="Retry"
          icon={<IconRetry size={13} />}
          onClick={() => act(store.resume)}
          disabled={!canRetry}
        />
        <Item
          label="Redownload"
          icon={<IconRedownload size={13} />}
          onClick={() => act(store.redownload)}
          disabled={!canRedownload}
        />
        <Item divider />
        <Item
          label="Open File"
          icon={<IconOpenFile size={13} />}
          onClick={() => single && store.openFile(single)}
          disabled={!canOpenFile}
        />
        <Item
          label="Open Folder"
          icon={<IconFolder size={13} />}
          onClick={() => single && store.openFolder(single)}
          disabled={!canOpenFolder}
        />
        <Item
          label="Rename…"
          icon={<IconProperties size={13} />}
          onClick={renamePrompt}
          disabled={!canRename}
        />
        <Item
          label="Copy URL"
          icon={<IconCopy size={13} />}
          onClick={() => single && copyText(single.url)}
          disabled={!canCopyUrl}
        />
        <Item
          label="Copy Path"
          icon={<IconCopy size={13} />}
          onClick={() =>
            single &&
            copyText(
              (single.save_dir || "") +
                (single.save_dir && single.filename ? "/" : "") +
                (single.filename || "")
            )
          }
          disabled={!canCopyPath}
        />
        <Item divider />
        <Item
          label="Move to Folder…"
          icon={<IconFolder size={13} />}
          onClick={moveToFolderPrompt}
          disabled={!canMoveToQueue}
        />
        <Item
          label="Move to Queue…"
          icon={<IconQueue size={13} />}
          onClick={moveToQueuePrompt}
          disabled={!canMoveToQueue}
        />
        <Item
          label="Change Category…"
          icon={<IconCategory size={13} />}
          onClick={changeCategoryPrompt}
          disabled={!canChangeCategory}
        />
        <Item divider />
        <Item
          label="Properties"
          icon={<IconProperties size={13} />}
          onClick={() => single && props.onOpenProps(single.id)}
          disabled={!canProps}
        />
        <Item divider />
        <Item
          label="Delete"
          icon={<IconDelete size={13} />}
          onClick={() => act(store.remove)}
          disabled={!canDelete}
        />
        <Item
          label="Delete + File"
          icon={<IconDelete size={13} />}
          danger
          onClick={() => {
            // Single only — multi-select does not delete files for safety.
            if (single && window.confirm(`Delete "${single.filename}" and the file on disk?`)) {
              void store.removeWithFile(single.id);
            }
            props.onClose();
          }}
          disabled={!canDeleteFile}
        />
      </div>
    </>
  );
}
