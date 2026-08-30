// IDM-style Properties window for a single download. All values come
// straight from the live task in the store; the few editable fields
// (Description, Save path) save on Apply and refresh the store.

import React, { useState } from "react";
import type { DownloadsStore } from "../store/useDownloads";
import { Dialog, Field, Btn } from "./Dialog";
import {
  formatBytes,
  formatSpeed,
  formatEta,
  formatPercent,
  formatDate,
  formatDateTime,
  formatDuration,
  domainOf,
} from "../lib/format";
import * as api from "../lib/api";

interface ChecksumOutcome {
  algorithm: string;
  computed: string;
  expected: string;
  matches: boolean | null;
  file_size: number;
}

type Props = { store: DownloadsStore; id: number; onClose: () => void };

const TABS = ["General", "Transfer", "Source", "Network", "Timestamps", "Advanced"] as const;
type Tab = (typeof TABS)[number];

export function PropertiesDialog({ store, id, onClose }: Props) {
  const task = store.tasks.find((t) => t.id === id) ?? null;
  const [tab, setTab] = useState<Tab>("General");
  const [desc, setDesc] = useState(task?.description ?? "");
  const [saveDir, setSaveDir] = useState(task?.save_dir ?? "");
  const [expectedSha, setExpectedSha] = useState(task?.expected_sha256 ?? "");
  const [verifyResult, setVerifyResult] = useState<ChecksumOutcome | null>(null);
  const [proxyMode, setProxyMode] = useState<"inherit" | "direct" | "custom">(
    task?.proxy_url == null ? "inherit" : task.proxy_url === "" ? "direct" : "custom",
  );
  const [proxyValue, setProxyValue] = useState(task?.proxy_url ?? "");
  const [verifyBusy, setVerifyBusy] = useState(false);
  const [verifyError, setVerifyError] = useState<string | null>(null);

  if (!task) return null;

  const remaining = task.total_size - task.downloaded;
  const elapsed =
    task.started_at && !task.completed_at
      ? Math.max(0, Math.floor(Date.now() / 1000) - task.started_at)
      : task.started_at && task.completed_at
      ? Math.max(0, task.completed_at - task.started_at)
      : null;

  const effectiveProxy = (() => {
    if (task.proxy_url != null) {
      return { label: task.proxy_url ? "Custom" : "Direct", value: task.proxy_url };
    }
    const site = store.sites.find((s) => s.domain && domainOf(task.url).toLowerCase() === s.domain.toLowerCase() || s.domain && domainOf(task.url).toLowerCase().endsWith(`.${s.domain.toLowerCase()}`));
    if (site?.proxy_url) return { label: `Site Rule — ${site.domain}`, value: site.proxy_url };
    const queue = store.queues.find((q) => q.name === task.queue);
    if (queue?.proxy_url) return { label: `Queue — ${queue.label || queue.name}`, value: queue.proxy_url };
    if (store.settings?.proxy_url) return { label: "Global", value: store.settings.proxy_url };
    return { label: "Direct", value: "" };
  })();
  const dirty =
    desc !== (task.description ?? "") ||
    saveDir !== task.save_dir ||
    expectedSha !== (task.expected_sha256 ?? "") ||
    proxyMode !== (task.proxy_url == null ? "inherit" : task.proxy_url === "" ? "direct" : "custom") ||
    (proxyMode === "custom" && proxyValue !== (task.proxy_url ?? ""));

  const apply = async () => {
    try {
      if (desc !== (task.description ?? "")) {
        await store.setDescription(task.id, desc);
      }
      if (saveDir !== task.save_dir) {
        await store.setSaveDir(task.id, saveDir);
      }
      if (expectedSha !== (task.expected_sha256 ?? "")) {
        await api.setExpectedSha256(task.id, expectedSha);
      }
      const nextProxy = proxyMode === "inherit" ? null : proxyMode === "direct" ? "" : proxyValue.trim();
      if (nextProxy !== (task.proxy_url ?? null)) {
        await store.setProxyUrl(task.id, nextProxy);
      }
    } catch {
      /* ignore */
    }
  };

  const runVerify = async () => {
    if (!task) return;
    setVerifyBusy(true);
    setVerifyError(null);
    setVerifyResult(null);
    try {
      const out = await api.verifyChecksum(task.id);
      setVerifyResult(out);
    } catch (e) {
      setVerifyError(String(e));
    } finally {
      setVerifyBusy(false);
    }
  };

  return (
    <Dialog
      title={`Properties — ${task.filename || task.url}`}
      onClose={onClose}
      width={680}
      footer={
        <>
          {dirty && <span className="text-xs text-txt-dim">Unsaved changes</span>}
          <Btn variant="ghost" onClick={onClose}>Close</Btn>
          <Btn variant="primary" onClick={apply} disabled={!dirty}>Apply</Btn>
        </>
      }
    >
      <div className="tabs">
        {TABS.map((t) => (
          <button key={t} className={`tab ${tab === t ? "active" : ""}`} onClick={() => setTab(t)}>
            {t}
          </button>
        ))}
      </div>
      <div className="tab-body">
        {tab === "General" && (
          <dl className="kv">
            <Row k="File name" v={task.filename || "(unknown)"} />
            <Row k="Save path" v={task.save_dir} />
            <Row k="Description" v={desc} editable onChange={setDesc} />
            <Row k="Category" v={task.category} />
            <Row k="Queue" v={task.queue} />
            <Row k="Site" v={task.site_id || "—"} />
            <Row k="File size" v={formatBytes(task.total_size)} />
            <Row k="Downloaded" v={formatBytes(task.downloaded)} />
            <Row k="Remaining" v={formatBytes(remaining)} />
            <Row k="Progress" v={formatPercent(task.downloaded, task.total_size)} />
            <Row k="Status" v={task.status} />
            <Row
              k="Priority"
              v={
                task.priority >= api.PRIO_HIGH
                  ? "High"
                  : task.priority <= api.PRIO_LOW
                  ? "Low"
                  : "Normal"
              }
            />
            {task.force_started && (
              <Row k="Force Started" v="Yes" />
            )}
            <div className="kv__row kv__row--full">
              <dt>Set priority</dt>
              <dd className="flex items-center gap-2">
                <select
                  className="input input--inline"
                  value={
                    task.priority >= api.PRIO_HIGH
                      ? "high"
                      : task.priority <= api.PRIO_LOW
                      ? "low"
                      : "normal"
                  }
                  onChange={(e) => {
                    const v = api.priorityFromString(
                      e.target.value as api.TaskPriority,
                    );
                    void store.setDownloadPriority(task.id, v);
                  }}
                >
                  <option value="high">High</option>
                  <option value="normal">Normal</option>
                  <option value="low">Low</option>
                </select>
                <span title="Promote this task immediately, bypassing the queue ordering">
                <Btn
                  variant="default"
                  onClick={() => void store.forceStart(task.id)}
                  disabled={task.status === "downloading" || task.status === "completed"}
                >
                  Force Start
                </Btn>
              </span>
              </dd>
            </div>
            {task.expected_sha256 && (
              <Row k="Expected SHA-256" v={task.expected_sha256} />
            )}
            {task.checksum_algorithm && task.checksum_value && (
              <Row
                k="Checksum"
                v={`${task.checksum_algorithm}: ${task.checksum_value.slice(0, 16)}…  ${task.checksum_verified ? "✓ verified" : "(unverified)"}`}
              />
            )}
            {task.error && <Row k="Error" v={task.error} />}
          </dl>
        )}

        {tab === "Transfer" && (
          <dl className="kv">
            <Row k="Current speed" v={task.status === "downloading" ? formatSpeed(task.speed_bps) : "—"} />
            <Row k="Average speed" v={task.avg_speed_bps > 0 ? formatSpeed(task.avg_speed_bps) : "—"} />
            <Row k="Peak speed" v={task.peak_speed_bps > 0 ? formatSpeed(task.peak_speed_bps) : "—"} />
            <Row
              k="Time remaining"
              v={task.status === "downloading" && remaining > 0 ? formatEta(remaining, task.speed_bps) : "—"}
            />
            <Row k="Time elapsed" v={elapsed != null ? formatDuration(elapsed) : "—"} />
            <Row k="Connections" v={String(task.threads || 1)} />
            <Row k="Segments" v={String(task.segments || 0)} />
            <Row k="Retry count" v={String(task.retry_count)} />
            <Row k="Resume support" v={task.resume_support ? "Yes" : "No"} />
          </dl>
        )}

        {tab === "Source" && (
          <dl className="kv">
            <Row k="Original URL" v={task.url} />
            <Row k="Referrer" v={task.referer ?? "—"} />
            <Row k="Domain" v={domainOf(task.url)} />
            <Row k="HTTP status" v={task.http_status ? String(task.http_status) : "—"} />
            <Row k="Content type" v={task.content_type ?? "—"} />
            <Row k="Webpage URL" v={task.webpage_url ?? "—"} />
            <Row k="Format" v={task.format_id ?? "—"} />
          </dl>
        )}

        {tab === "Network" && (
          <dl className="kv">
            <Row k="Effective proxy" v={effectiveProxy.label} />
            <div className="kv__row">
              <dt>Proxy mode</dt>
              <dd>
                <select
                  className="input input--sm"
                  value={proxyMode}
                  onChange={(e) => setProxyMode(e.target.value as "inherit" | "direct" | "custom")}
                >
                  <option value="inherit">Inherit</option>
                  <option value="direct">Direct</option>
                  <option value="custom">Custom</option>
                </select>
              </dd>
            </div>
            {proxyMode === "custom" && (
              <Row k="Proxy URL" v={proxyValue} editable onChange={setProxyValue} />
            )}
            <Row k="Connection mode" v={task.threads > 1 ? "Segmented" : "Single"} />
            <Row k="User agent" v={task.user_agent ?? "(default)"} />
            <Row k="Authentication" v={task.cookies || task.site_id ? "Provided" : "None"} />
          </dl>
        )}

        {tab === "Timestamps" && (
          <dl className="kv">
            <Row k="Added" v={formatDateTime(task.created_at)} />
            <Row k="Started" v={task.started_at ? formatDateTime(task.started_at) : "—"} />
            <Row k="Last resumed" v={task.last_resumed_at ? formatDateTime(task.last_resumed_at) : "—"} />
            <Row k="Completed" v={task.completed_at ? formatDateTime(task.completed_at) : "—"} />
            <Row k="Last update" v={formatDate(task.updated_at)} />
          </dl>
        )}

        {tab === "Advanced" && (
          <dl className="kv">
            <Row k="Max connections" v={String(task.threads)} />
            <Row k="Retry behavior" v={`${task.max_retries} retries, ${task.retry_delay_ms}ms delay`} />
            <Row k="Conn timeout" v={`${task.conn_timeout_ms} ms`} />
            <Row k="Read timeout" v={`${task.read_timeout_ms} ms`} />
            <Row k="Checksum algorithm" v={task.checksum_algorithm || "—"} />
            <Row k="Checksum" v={task.checksum_value || "—"} />
            <Row k="Checksum verified" v={task.checksum_verified ? "✓ Yes" : "—"} />
            <Row k="Error category" v={task.error_category || "—"} />
            <Row k="Error code" v={task.error_code ? String(task.error_code) : "—"} />
            <Row k="On complete" v={task.on_complete || "(none)"} />
            <Row k="Save path (edit)" v={saveDir} editable onChange={setSaveDir} />
            <Row k="Expected SHA-256" v={expectedSha} editable onChange={setExpectedSha} />
            <div className="kv__row kv__row--full">
              <dt>Verification</dt>
              <dd className="flex flex-col gap-2">
                <div className="flex items-center gap-2">
                  <Btn
                    variant="primary"
                    onClick={runVerify}
                    disabled={verifyBusy}
                  >
                    {verifyBusy ? "Verifying…" : "Verify SHA-256 now"}
                  </Btn>
                  <span className="text-xs text-txt-dim">
                    Reads the file and hashes it. The expected value above
                    is compared case-insensitively.
                  </span>
                </div>
                {verifyError && (
                  <div className="text-xs text-red-400">
                    {verifyError}
                  </div>
                )}
                {verifyResult && (
                  <div className="rounded border border-white/10 bg-white/[0.02] p-2 text-xs">
                    <div>
                      <span className="text-txt-dim">Algorithm:</span>{" "}
                      {verifyResult.algorithm}
                    </div>
                    <div>
                      <span className="text-txt-dim">File size:</span>{" "}
                      {formatBytes(verifyResult.file_size)}
                    </div>
                    <div className="break-all">
                      <span className="text-txt-dim">Computed:</span>{" "}
                      <code>{verifyResult.computed}</code>
                    </div>
                    {verifyResult.expected ? (
                      <div>
                        <span className="text-txt-dim">Expected:</span>{" "}
                        <code>{verifyResult.expected}</code>
                      </div>
                    ) : (
                      <div className="text-txt-dim">
                        No expected digest entered — actual recorded only.
                      </div>
                    )}
                    <div className="mt-1 text-sm font-medium">
                      {verifyResult.matches === true && (
                        <span className="text-emerald-400">
                          ✓ Verified — checksum matches.
                        </span>
                      )}
                      {verifyResult.matches === false && (
                        <span className="text-red-400">
                          ✕ Mismatch — file does not match expected digest.
                        </span>
                      )}
                      {verifyResult.matches === null && (
                        <span className="text-txt-dim">
                          No expected digest; recorded the actual hash.
                        </span>
                      )}
                    </div>
                  </div>
                )}
              </dd>
            </div>
          </dl>
        )}
      </div>
    </Dialog>
  );
}

function Row({
  k,
  v,
  editable,
  onChange,
}: {
  k: string;
  v: string;
  editable?: boolean;
  onChange?: (s: string) => void;
}) {
  if (editable) {
    return (
      <div className="kv__row">
        <dt>{k}</dt>
        <dd><input className="input input--inline" value={v} onChange={(e) => onChange?.(e.target.value)} /></dd>
      </div>
    );
  }
  return (
    <div className="kv__row">
      <dt>{k}</dt>
      <dd title={v}>{v}</dd>
    </div>
  );
}
