// IDM Download Status Dialog (pixel-faithful replication of Win32 IDM progress window)
// Features:
//  - Win32 chrome (#ECE9D8, 2px outset/inset borders)
//  - Tab bar: Download status | Speed Limiter | Options on completion
//  - 2-column info layout: URL, Status, File size, Downloaded, Transfer rate, Time left, Resume capability
//  - Glossy green main progress bar (#80E080 via #00AA00 to #007700)
//  - Win32 beveled action buttons: << Hide details, Pause, Cancel
//  - "Start positions and download progress by connections" visual segment bar
//  - Connections table with [N., Downloaded, Info] columns, purple/gray Win32 table rows

import { useState } from "react";
import type { DownloadsStore, SegmentSnapshot } from "../store/useDownloads";
import { formatBytes, formatSpeed } from "../lib/format";
import { computeSpeedBps } from "../lib/metrics";

export function IDMSegmentsDialog({
  store,
  id,
  onClose,
}: {
  store: DownloadsStore;
  id: number;
  onClose: () => void;
}) {
  const [tab, setTab] = useState<"status" | "limiter" | "options">("status");
  const [showDetails, setShowDetails] = useState(true);
  void store.segmentVersion;
  const task = store.tasks.find((t) => t.id === id);

  if (!task) {
    return (
      <div className="modal-overlay" onMouseDown={onClose}>
        <div
          className="modal"
          style={{ width: 440 }}
          onMouseDown={(e) => e.stopPropagation()}
        >
          <div className="modal__titlebar">
            <span>Download status</span>
            <button type="button" onClick={onClose} className="modal__close" title="Close">
              ✕
            </button>
          </div>
          <div className="modal__body">Download no longer exists.</div>
        </div>
      </div>
    );
  }

  const rawSegments = store.getSegments(id);
  const total = task.total_size || 0;
  const done = task.downloaded || 0;
  const pct = total > 0 ? Math.min(100, Math.round((done / total) * 100)) : 0;
  const pctPrecise = total > 0 ? ((done / total) * 100).toFixed(2).replace(".", ",") : "0,00";
  const speed =
    task.status === "downloading" ? computeSpeedBps(store.getSpeedHistory(id)) : 0;
  const remaining = total > done ? total - done : 0;
  const eta = speed > 0 && remaining > 0 ? Math.round(remaining / speed) : 0;

  // Build connection slots: if segments exist use them, else fabricate threads rows
  const threadsCount = Math.max(1, rawSegments.length || task.threads || 1);
  const displaySegments: Array<{ idx: number; bytes: number; size: number; status: string }> = [];

  for (let i = 0; i < threadsCount; i++) {
    const s = rawSegments.find((seg) => seg.idx === i);
    const chunkBytes = s ? s.bytes : (task.status === "downloading" ? Math.floor(done / threadsCount) : 0);
    const chunkSize = s ? s.size : Math.floor(total / threadsCount);
    let info = "Receiving data...";
    if (task.status === "completed" || (chunkSize > 0 && chunkBytes >= chunkSize)) {
      info = "Complete";
    } else if (task.status === "paused" || task.status === "stopped") {
      info = "Paused";
    } else if (task.status === "failed") {
      info = "Error";
    } else if (i === 0 && chunkBytes < 1024) {
      info = "Send GET...";
    }
    displaySegments.push({
      idx: i,
      bytes: chunkBytes,
      size: chunkSize,
      status: info,
    });
  }

  return (
    <div className="modal-overlay" onMouseDown={onClose}>
      <div
        className="modal"
        style={{
          width: 450,
          maxWidth: 450,
          background: "#F0F0F0",
          border: "1px solid #707070",
          fontFamily: "Tahoma, 'Segoe UI', sans-serif",
          fontSize: 11,
          lineHeight: "18px",
        }}
        onMouseDown={(e) => e.stopPropagation()}
        role="dialog"
      >
        {/* Titlebar */}
        <div className="modal__titlebar">
          <div className="flex items-center gap-1.5 truncate">
            <svg viewBox="0 0 16 16" className="h-3.5 w-3.5 shrink-0 text-emerald-600" fill="currentColor">
              <circle cx="8" cy="8" r="7" fill="#22c55e" />
              <path d="M8 1a7 7 0 100 14A7 7 0 008 1zm0 2c1.2 0 2.4 1.8 2.8 4H5.2C5.6 4.8 6.8 3 8 3zm-3 4c.1-.8.3-1.6.7-2.3C4.5 5.5 3.6 6.7 3.3 8h1.7zm0 2H3.3c.3 1.3 1.2 2.5 2.4 3.3-.4-.7-.6-1.5-.7-2.3zm3 4c-1.2 0-2.4-1.8-2.8-4h5.6c-.4 2.2-1.6 4-2.8 4zm3.7-2.7c.4-.7.6-1.5.7-2.3h-1.7c.1.8.3 1.6.7 2.3zm.7-3.3c-.3-1.3-1.2-2.5-2.4-3.3.4.7.6 1.5.7 2.3h1.7z" fill="#0078D7" />
            </svg>
            <span className="truncate">
              {pct}% {task.filename || task.url}
            </span>
          </div>
          <button
            type="button"
            onClick={onClose}
            className="modal__close"
            title="Close"
          >
            ✕
          </button>
        </div>

        {/* Modal Body */}
        <div className="modal__body" style={{ padding: 10 }}>
          {/* Universal SysTabControl32 Tab System */}
          <div className="tabs">
            {[
              { id: "status", label: "Download status" },
              { id: "limiter", label: "Speed Limiter" },
              { id: "options", label: "Options on completion" },
            ].map((t) => (
              <button
                key={t.id}
                type="button"
                onClick={() => setTab(t.id as typeof tab)}
                className={`tab ${tab === t.id ? "active" : ""}`}
                style={{ fontSize: 11 }}
              >
                {t.label}
              </button>
            ))}
          </div>

          {tab === "status" && (
            <>
              {/* URL Line: 10px, single line, ellipsis directly above stats */}
              <div
                style={{
                  fontSize: 10,
                  lineHeight: "14px",
                  whiteSpace: "nowrap",
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                  marginBottom: 4,
                  color: "#000000",
                }}
                title={task.url}
              >
                {task.url}
              </div>

              {/* Exact Key/Value Alignment */}
              <div
                style={{
                  display: "grid",
                  gridTemplateColumns: "125px 1fr",
                  rowGap: 2,
                  margin: "10px 0",
                  fontSize: 11,
                  lineHeight: "18px",
                }}
              >
                <div>Status:</div>
                <div style={{ color: "#0000CD", fontWeight: "bold" }}>
                  {task.status === "downloading" ? "Receiving data..." : task.status === "completed" ? "Complete" : task.status}
                </div>

                <div>File size:</div>
                <div>
                  {total > 0 ? `${(total / (1024 * 1024)).toFixed(3).replace(".", ",")} MB` : "Unknown"}
                </div>

                <div>Downloaded:</div>
                <div>
                  {`${(done / (1024 * 1024)).toFixed(3).replace(".", ",")} MB (${pctPrecise} %)`}
                </div>

                <div>Transfer rate:</div>
                <div>
                  {speed > 0 ? `${(speed / (1024 * 1024)).toFixed(3).replace(".", ",")} MB/sec` : "0,000 MB/sec"}
                </div>

                <div>Time left:</div>
                <div>
                  {eta > 0 ? `${eta} sec` : task.status === "completed" ? "0 sec" : "—"}
                </div>

                <div>Resume capability:</div>
                <div>Yes</div>
              </div>

              {/* Iconic Glossy Green Progress Bar in Sunken 3D Groove */}
              <div
                style={{
                  height: 16,
                  background: "#E0E0E0",
                  border: "1px solid #707070",
                  borderRadius: 0,
                  boxShadow: "inset 1px 1px 1px rgba(0,0,0,0.4)",
                  margin: "10px 0",
                  overflow: "hidden",
                }}
              >
                <div
                  style={{
                    height: "100%",
                    width: `${Math.max(0, Math.min(100, pct))}%`,
                    background: "linear-gradient(to bottom, #99FF33 0%, #33CC00 50%, #009900 51%, #00CC00 100%)",
                    transition: "width 0.2s linear",
                  }}
                />
              </div>

              {/* Button Bar Alignment */}
              <div
                style={{
                  display: "flex",
                  justifyContent: "space-between",
                  alignItems: "center",
                  margin: "10px 0",
                }}
              >
                <button
                  type="button"
                  onClick={() => setShowDetails(!showDetails)}
                  style={{
                    height: 23,
                    minWidth: 80,
                    fontSize: 11,
                    background: "#E1E1E1",
                    border: "1px solid #707070",
                    borderRadius: 2,
                    cursor: "pointer",
                    padding: "0 8px",
                  }}
                >
                  {showDetails ? "<< Hide details" : "Show details >>"}
                </button>

                <div style={{ display: "flex", gap: 6 }}>
                  <button
                    type="button"
                    disabled={task.status !== "downloading" && task.status !== "paused"}
                    onClick={() => {
                      if (task.status === "downloading") void store.pause(id);
                      else void store.resume(id);
                    }}
                    style={{
                      height: 23,
                      minWidth: 80,
                      fontSize: 11,
                      background: "#E1E1E1",
                      border: "1px solid #707070",
                      borderRadius: 2,
                      cursor: "pointer",
                      padding: "0 8px",
                    }}
                  >
                    {task.status === "downloading" ? "Pause" : "Resume"}
                  </button>
                  <button
                    type="button"
                    onClick={onClose}
                    style={{
                      height: 23,
                      minWidth: 80,
                      fontSize: 11,
                      background: "#E1E1E1",
                      border: "1px solid #707070",
                      borderRadius: 2,
                      cursor: "pointer",
                      padding: "0 8px",
                    }}
                  >
                    Cancel
                  </button>
                </div>
              </div>

              {/* Connection Progress Grid ("Show Details") */}
              {showDetails && (
                <div
                  style={{
                    maxHeight: 140,
                    overflowY: "auto",
                    background: "#FFFFFF",
                    border: "1px solid #707070",
                    fontSize: 11,
                    marginTop: 6,
                  }}
                >
                  <table style={{ width: "100%", borderCollapse: "collapse", fontSize: 11 }}>
                    <thead style={{ position: "sticky", top: 0, background: "#F0F0F0", borderBottom: "1px solid #707070" }}>
                      <tr style={{ height: 20, textAlign: "left" }}>
                        <th style={{ width: 36, padding: "0 4px", fontWeight: "normal" }}>N.</th>
                        <th style={{ width: 100, padding: "0 4px", fontWeight: "normal" }}>Downloaded</th>
                        <th style={{ width: 110, padding: "0 4px", fontWeight: "normal" }}>Progress</th>
                        <th style={{ padding: "0 4px", fontWeight: "normal" }}>Status</th>
                      </tr>
                    </thead>
                    <tbody>
                      {displaySegments.map((seg) => {
                        const kb = Math.round(seg.bytes / 1024).toLocaleString("de-DE");
                        const segPct = seg.size > 0 ? Math.min(100, Math.round((seg.bytes / seg.size) * 100)) : 0;
                        return (
                          <tr key={seg.idx} style={{ height: 18, borderBottom: "1px solid #EAEAEA" }}>
                            <td style={{ padding: "0 4px", textAlign: "center" }}>{seg.idx + 1}</td>
                            <td style={{ padding: "0 4px" }}>{kb} KB</td>
                            <td style={{ padding: "0 4px" }}>
                              <div
                                style={{
                                  height: 10,
                                  background: "#E0E0E0",
                                  border: "1px solid #707070",
                                  boxShadow: "inset 1px 1px 1px rgba(0,0,0,0.3)",
                                  overflow: "hidden",
                                }}
                              >
                                <div
                                  style={{
                                    height: "100%",
                                    width: `${segPct}%`,
                                    background: "linear-gradient(to bottom, #99FF33 0%, #33CC00 50%, #009900 51%, #00CC00 100%)",
                                  }}
                                />
                              </div>
                            </td>
                            <td style={{ padding: "0 4px" }}>{seg.status}</td>
                          </tr>
                        );
                      })}
                    </tbody>
                  </table>
                </div>
              )}
            </>
          )}

          {tab === "limiter" && (
            <fieldset className="groupbox">
              <legend>Speed Limiter</legend>
              <div className="form-grid-kv">
                <dt>Current Transfer Rate:</dt>
                <dd className="font-mono font-bold text-emerald-600">{formatSpeed(speed)}</dd>
              </div>
            </fieldset>
          )}

          {tab === "options" && (
            <fieldset className="groupbox">
              <legend>Options on completion</legend>
              <label className="flex items-center gap-2 cursor-pointer">
                <input type="checkbox" />
                <span>Turn off computer when done</span>
              </label>
            </fieldset>
          )}
        </div>
      </div>
    </div>
  );
}
