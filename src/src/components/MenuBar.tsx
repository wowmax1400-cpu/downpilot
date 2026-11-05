// IDM-style menu bar with classic desktop dropdowns and active-menu hover switching.
import React, { useState, useEffect, useRef } from "react";
import type { DownloadsStore } from "../store/useDownloads";
import { getCurrentWindow } from "@tauri-apps/api/window";

type MenuKey = "tasks" | "file" | "downloads" | "view" | "help" | null;

type MenuItem =
  | {
      label: string;
      shortcut?: string;
      onClick?: () => void;
      disabled?: boolean;
    }
  | { sep: true };

type Props = {
  store: DownloadsStore;
  onAdd: () => void;
  onBatch: () => void;
  onGrabber: () => void;
  onQueues: () => void;
  onScheduler: () => void;
  onSites: () => void;
  onCategories: () => void;
  onSettings: () => void;
  onHistory: () => void;
  onAbout: () => void;
  onShortcuts?: () => void;
  onOpenProps: () => void;
  onToggleDetails: () => void;
  detailsOpen: boolean;
  onFocusSearch?: () => void;
};

export function MenuBar(props: Props) {
  const { store } = props;
  const [activeMenu, setActiveMenu] = useState<MenuKey>(null);
  const barRef = useRef<HTMLDivElement>(null);

  // Close when clicking outside or pressing Escape
  useEffect(() => {
    if (activeMenu === null) return;

    const onMouseDown = (e: MouseEvent) => {
      if (barRef.current && !barRef.current.contains(e.target as Node)) {
        setActiveMenu(null);
      }
    };

    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        setActiveMenu(null);
      }
    };

    document.addEventListener("mousedown", onMouseDown);
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("mousedown", onMouseDown);
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [activeMenu]);

  const sel = Array.from(store.selected);
  const hasSel = sel.length > 0;
  const single = sel.length === 1 ? sel[0] : null;
  const singleTask =
    single !== null ? store.tasks.find((t) => t.id === single) ?? null : null;

  const handleExit = async () => {
    try {
      const win = getCurrentWindow();
      await win.close();
    } catch {
      window.close?.();
    }
  };

  const completedCount = store.tasks.filter((t) => t.status === "completed").length;

  const handleDeleteCompleted = () => {
    const completedIds = store.tasks
      .filter((t) => t.status === "completed")
      .map((t) => t.id);
    for (const id of completedIds) {
      void store.remove(id);
    }
  };

  const menuDefinitions: Record<
    Exclude<MenuKey, null>,
    { label: string; items: MenuItem[] }
  > = {
    tasks: {
      label: "Tasks",
      items: [
        {
          label: "Add new download...",
          shortcut: "Ctrl+N",
          onClick: props.onAdd,
        },
        {
          label: "Batch Download...",
          onClick: props.onBatch,
        },
        {
          label: "Site Grabber...",
          onClick: props.onGrabber,
        },
        { sep: true },
        {
          label: "Exit",
          onClick: handleExit,
        },
      ],
    },
    file: {
      label: "File",
      items: [
        {
          label: "Stop all downloads",
          onClick: () => store.stopAll(),
        },
        {
          label: "Delete completed",
          onClick: handleDeleteCompleted,
          disabled: completedCount === 0,
        },
        { sep: true },
        {
          label: "Stop",
          onClick: () => sel.forEach((id) => store.cancel(id)),
          disabled: !hasSel,
        },
        {
          label: "Delete",
          shortcut: "Del",
          onClick: () => sel.forEach((id) => store.remove(id)),
          disabled: !hasSel,
        },
        {
          label: "Delete File from Disk",
          onClick: async () => {
            if (!singleTask) return;
            if (
              window.confirm(
                `Delete "${singleTask.filename || "file"}" and remove from disk?`
              )
            ) {
              void store.removeWithFile(singleTask.id);
            }
          },
          disabled: !singleTask,
        },
        { sep: true },
        {
          label: "Properties...",
          shortcut: "Ctrl+P",
          onClick: props.onOpenProps,
          disabled: !hasSel,
        },
      ],
    },
    downloads: {
      label: "Downloads",
      items: [
        {
          label: "Resume",
          shortcut: "Space",
          onClick: () => sel.forEach((id) => store.resume(id)),
          disabled: !hasSel,
        },
        {
          label: "Pause",
          onClick: () => sel.forEach((id) => store.pause(id)),
          disabled: !hasSel,
        },
        {
          label: "Delete",
          shortcut: "Del",
          onClick: () => sel.forEach((id) => store.remove(id)),
          disabled: !hasSel,
        },
        {
          label: "Redownload",
          onClick: () => sel.forEach((id) => store.redownload(id)),
          disabled: !hasSel,
        },
        { sep: true },
        {
          label: "Options",
          onClick: props.onSettings,
        },
      ],
    },
    view: {
      label: "View",
      items: [
        {
          label: props.detailsOpen
            ? "Hide Details Panel"
            : "Show Details Panel",
          onClick: props.onToggleDetails,
        },
        { sep: true },
        {
          label: "Sort by Name",
          onClick: () => store.setSort({ field: "name", dir: "asc" }),
        },
        {
          label: "Sort by Size",
          onClick: () => store.setSort({ field: "size", dir: "desc" }),
        },
        {
          label: "Sort by Date",
          onClick: () => store.setSort({ field: "added_at", dir: "desc" }),
        },
      ],
    },
    help: {
      label: "Help",
      items: [
        {
          label: "Keyboard Shortcuts",
          onClick: () => {
            if (props.onShortcuts) {
              props.onShortcuts();
            }
          },
        },
        { sep: true },
        {
          label: "About DownPilot",
          onClick: props.onAbout,
        },
      ],
    },
  };

  const menuKeys: Array<Exclude<MenuKey, null>> = [
    "tasks",
    "file",
    "downloads",
    "view",
    "help",
  ];

  return (
    <div
      ref={barRef}
      data-tauri-drag-region="false"
      className="menubar menu-bar flex h-[26px] shrink-0 items-center border-b border-[var(--line)] bg-[var(--bg-toolbar)] px-2 select-none text-[var(--text)] relative z-50 backdrop-blur-md"
      style={{ WebkitAppRegion: "no-drag" } as unknown as React.CSSProperties}
    >
      <ul className="flex items-center list-none m-0 p-0 gap-1 text-[12px]">
        {menuKeys.map((key) => {
          const menu = menuDefinitions[key];
          const isOpen = activeMenu === key;
          return (
            <li key={key} className="relative menu-item">
              <button
                type="button"
                className="menubar__btn"
                onClick={() => setActiveMenu(isOpen ? null : key)}
                onMouseEnter={() => {
                  if (activeMenu !== null) {
                    setActiveMenu(key);
                  }
                }}
                style={
                  {
                    padding: "2px 8px",
                    fontSize: "12px",
                    color: "var(--text)",
                    background: isOpen ? "var(--btn-hover)" : "transparent",
                    borderColor: isOpen ? "var(--line)" : "transparent",
                    WebkitAppRegion: "no-drag",
                  } as unknown as React.CSSProperties
                }
              >
                {menu.label}
              </button>

              {isOpen && (
                <div
                  className="menu-dropdown"
                  style={{
                    background: "var(--surface)",
                    border: "1px solid var(--line)",
                    boxShadow: "0 8px 24px rgba(0,0,0,0.5)",
                    padding: "3px 0",
                    minWidth: "160px",
                    zIndex: 1000,
                    fontSize: "12px",
                  }}
                >
                  {menu.items.map((item, idx) =>
                    "sep" in item ? (
                      <div key={idx} className="menu-sep" style={{ borderTop: "1px solid var(--line)", margin: "3px 0" }} />
                    ) : (
                      <button
                        key={idx}
                        type="button"
                        className="menu-row text-[var(--text)] hover:bg-[var(--row-hover)]"
                        disabled={item.disabled}
                        onClick={() => {
                          setActiveMenu(null);
                          item.onClick?.();
                        }}
                      >
                        <span>{item.label}</span>
                        {item.shortcut && (
                          <span className="menu-row__short text-[var(--text-faint)]">
                            {item.shortcut}
                          </span>
                        )}
                      </button>
                    )
                  )}
                </div>
              )}
            </li>
          );
        })}
      </ul>
    </div>
  );
}
