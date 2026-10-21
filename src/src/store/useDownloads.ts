// Central state hook: polls the backend, listens for push events,
// and exposes actions + derived views (filtered, sorted, grouped) for the UI.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import * as api from "../lib/api";
import { domainOf } from "../lib/format";
import { computeSpeedBps } from "../lib/metrics";
import type {
  ActiveProfile,
  AppSettings,
  DownloadTask,
  EngineEvent,
  GrabbedLink,
  GrabRules,
  MediaInfo,
  NewDownload,
  Schedule,
  SiteRule,
  Sort,
  SortField,
  GroupBy,
  SidebarKey,
  DownloadQueue,
  Category,
  SchedulerStatus,
} from "../types";

const STATUS_RANK: Record<string, number> = {
  downloading: 0,
  queued: 1,
  scheduled: 2,
  paused: 3,
  failed: 4,
  completed: 5,
  cancelled: 6,
};

const EMPTY_SAMPLES: number[] = Object.freeze([]) as unknown as number[];

export interface SegmentSnapshot {
  idx: number;
  bytes: number;
  size: number;
}

const EMPTY_SEGMENTS: SegmentSnapshot[] = Object.freeze([]) as unknown as SegmentSnapshot[];

function sideMatch(t: DownloadTask, key: SidebarKey): boolean {
  switch (key.kind) {
    case "all":
      return true;
    case "status":
      if (key.value === "unfinished")
        return t.status !== "completed" && t.status !== "cancelled";
      if (key.value === "finished") return t.status === "completed";
      if (key.value === "failed") return t.status === "failed";
      if (key.value === "paused") return t.status === "paused" || t.status === "stopped";
      if (key.value === "scheduled") return t.status === "scheduled";
      return true;
    case "category":
      return t.category === key.value;
    case "queue":
      return t.queue === key.value;
    case "site":
      return t.site_id === key.value || (t.site_id === "" && false);
    case "schedule":
      return false;
  }
}

function sortValue(t: DownloadTask, f: SortField): number | string {
  switch (f) {
    case "name":
      return (t.filename || t.url).toLowerCase();
    case "status":
      return STATUS_RANK[t.status] ?? 9;
    case "size":
      return t.total_size;
    case "downloaded":
      return t.downloaded;
    case "progress":
      return t.total_size > 0 ? (t.downloaded / t.total_size) * 100 : 0;
    case "speed":
      return t.status === "downloading" ? t.speed_bps : 0;
    case "avg_speed":
      return t.avg_speed_bps;
    case "time_left":
      return t.total_size - t.downloaded;
    case "connections":
      return t.threads;
    case "category":
      return t.category;
    case "queue":
      return t.queue;
    case "added_at":
      return t.created_at;
    case "completed_at":
      return t.completed_at ?? 0;
    case "save_dir":
      return t.save_dir;
    case "last_try":
      return t.last_resumed_at ?? t.started_at ?? t.updated_at;
  }
}

export function useDownloads() {
  const [tasks, setTasks] = useState<DownloadTask[]>([]);
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [sidebarKey, setSidebarKey] = useState<SidebarKey>({ kind: "all" });
  const [search, setSearch] = useState("");
  const [sort, setSort] = useState<Sort>({ field: "added_at", dir: "desc" });
  const [groupBy, setGroupBy] = useState<GroupBy>("none");
  const [loaded, setLoaded] = useState(false);
  const [queues, setQueues] = useState<DownloadQueue[]>([]);
  const [categories, setCategories] = useState<Category[]>([]);
  const [sites, setSites] = useState<SiteRule[]>([]);
  const [schedules, setSchedules] = useState<Schedule[]>([]);
  const [activeProfiles, setActiveProfiles] = useState<ActiveProfile[]>([]);
  /** Phase 3B: live scheduler snapshot — used by the status bar and
   *  the queue details panel. Refreshed alongside the rest of the
   *  aux data so the UI doesn't have to poll. */
  const [schedulerStatus, setSchedulerStatus] = useState<SchedulerStatus | null>(
    null,
  );
  /** Effective (current) speed limit for each queue, in kbps. Cached
   *  after each refresh so the UI can show "Scheduled: X" without a
   *  roundtrip per row. */
  const [effectiveSpeed, setEffectiveSpeed] = useState<Record<string, number>>({});
  const tasksRef = useRef<DownloadTask[]>([]);
  const selectedRef = useRef(selected);
  selectedRef.current = selected;

  // Per-task recent speed samples, oldest first, newest last. The cap is
  // 120 samples: at 500ms per sample that's a 60-second rolling window,
  // long enough to spot a throttled download but short enough to keep
  // memory bounded across hundreds of concurrent tasks.
  const SPEED_HISTORY_CAP = 120;
  const speedHistoryRef = useRef<Map<number, number[]>>(new Map());
  // Byte baseline per task for the CURRENT sampling window (bytes already
  // downloaded when the window opened). Reset together with the ring.
  const speedBaselineRef = useRef<Map<number, number>>(new Map());
  // Last time we recorded a sample for a given task — used to throttle
  // progress events (which can fire multiple times per second per chunk)
  // down to ~2 Hz so the graph renders smoothly.
  const speedHistoryLastRef = useRef<Map<number, number>>(new Map());
  const SPEED_HISTORY_INTERVAL_MS = 500;
  const [speedHistoryVersion, setSpeedHistoryVersion] = useState(0);
  // Bump this once per ring-buffer write so the table can re-render only
  // the rows that show the graph (we re-render the whole table — fine,
  // a 200-row table re-renders in <2ms).

  // Per-task segment progress for the IDM-style multi-segment dialog.
  // taskId -> (chunkIndex -> {bytes, size}). Ephemeral: never persisted;
  // dropped when a task is removed. A `segmentVersion` bump is the
  // re-render trigger for the dialog (same pattern as speedHistory).
  const segmentsRef = useRef<Map<number, Map<number, { bytes: number; size: number }>>>(
    new Map()
  );
  const [segmentVersion, setSegmentVersion] = useState(0);

  const refresh = useCallback(async () => {
    const list = await api.listDownloads();
    // Backend may omit extension fields; coerce to a full DownloadTask.
    const norm = list.map(normalizeTask);
    tasksRef.current = norm;
    setTasks(norm);
    setLoaded(true);
  }, []);

  // Surface engine-pushed notifications to the user. The Notify event
  // variant is produced by the engine for completion, checksum results,
  // destructive-action blocks, and file-op errors. We don't render the
  // toast here — that's the UI layer's job — but we keep the latest
  // message in a state slot the StatusBar can subscribe to.
  const [lastNotification, setLastNotification] = useState<{
    level: string;
    title: string;
    body: string;
    at: number;
  } | null>(null);

  // Phase 3A: clear the visible toast when the user dismisses it.
  // The actual `EngineEvent::Notify` from Rust is consumed by the
  // desktop-notification plugin in parallel; this is just the in-app
  // toast surface.
  const dismissNotification = useCallback(() => {
    setLastNotification(null);
  }, []);

  const applyEvent = useCallback((raw: EngineEvent) => {
    // Rust serializes events as {"type":"added", ...} (tag = "type",
    // snake_case). The store's EngineEvent type uses `kind`, so read both.
    const ev = raw as unknown as Record<string, any>;
    const kind: string = ev.type ?? ev.kind;
    if (kind === "notify") {
      setLastNotification({
        level: String(ev.level ?? "info"),
        title: String(ev.title ?? ""),
        body: String(ev.body ?? ""),
        at: Date.now(),
      });
      // Phase 3A: also forward to the OS desktop-notification
      // surface (libnotify / KDE / Gnome via the
      // tauri-plugin-notification plugin). The plugin handles
      // permission checks and silent-fail in browser dev. We do
      // NOT await — notifications are best-effort and must never
      // block the event loop.
      const level = String(ev.level ?? "info");
      void import("../lib/api").then((m) =>
        m.notify(
          String(ev.title ?? ""),
          String(ev.body ?? ""),
          (level === "error" || level === "warning" || level === "info"
            ? level
            : "info") as "error" | "warning" | "info",
        ),
      );
    }
    // A status change / finished event means the next progress burst
    // is a fresh download. Drop the throttle clock so we don't wait
    // 500ms before recording the first sample of the new run.
    if (kind === "status_changed" || kind === "finished") {
      const resetId: number | undefined = ev.id ?? ev.task?.id;
      if (resetId !== undefined) speedHistoryLastRef.current.delete(resetId);
      // Fresh run: drop per-task segment snapshots so the dialog resets.
      const segMap = segmentsRef.current.get(resetId ?? -1);
      if (segMap) segMap.clear();
      segmentsRef.current.delete(resetId ?? -1);
      setSegmentVersion((v) => v + 1);
    }
    if (kind === "removed") {
      tasksRef.current = tasksRef.current.filter((t) => t.id !== ev.id);
      // Free the ring buffer for removed tasks so the Map doesn't grow
      // without bound across a long session.
      speedHistoryRef.current.delete(ev.id);
      speedBaselineRef.current.delete(ev.id);
      speedHistoryLastRef.current.delete(ev.id);
      segmentsRef.current.delete(ev.id);
      setSegmentVersion((v) => v + 1);
    } else if (kind === "added") {
      tasksRef.current = [normalizeTask(ev.task), ...tasksRef.current];
    } else if (kind === "status_changed") {
      tasksRef.current = tasksRef.current.map((t) =>
        t.id === ev.id
          ? { ...t, status: ev.status, error: ev.error ?? t.error }
          : t
      );
    } else if (kind === "finished") {
      tasksRef.current = tasksRef.current.map((t) =>
        t.id === ev.task.id ? normalizeTask(ev.task) : t
      );
    } else if (kind === "progress") {
      // Per-task byte accounting for speed sampling. The first sample
      // closes after a short 200 ms window (fast live feedback); after
      // that the standard 500 ms cadence applies. Each sample is
      // bytes-delta ÷ time-delta over its own window — real measured
      // throughput, never a fabricated number (no samples ⇒ UI shows
      // the "…" placeholder via computeSpeedBps returning 0).
      const now = performance.now();
      const last = speedHistoryLastRef.current.get(ev.id) ?? 0;
      const elapsedMs = now - last;
      const FAST_FIRST_SAMPLE_MS = 200;
      const hasSamples = speedHistoryRef.current.has(ev.id);
      const threshold = hasSamples ? SPEED_HISTORY_INTERVAL_MS : FAST_FIRST_SAMPLE_MS;
      if (last === 0) {
        // First progress event ever: record the byte baseline and arm the
        // timestamp. Speed stays "…" until this window closes.
        speedHistoryLastRef.current.set(ev.id, now);
        speedBaselineRef.current.set(ev.id, ev.downloaded);
      } else if (elapsedMs >= threshold) {
        const base = speedBaselineRef.current.get(ev.id) ?? ev.downloaded;
        const dBytes = Math.max(0, ev.downloaded - base);
        speedHistoryLastRef.current.set(ev.id, now);
        speedBaselineRef.current.set(ev.id, ev.downloaded);
        const sample = dBytes / (elapsedMs / 1000);
        const buf = speedHistoryRef.current.get(ev.id);
        if (buf) {
          buf.push(sample);
          if (buf.length > SPEED_HISTORY_CAP) buf.shift();
        } else {
          speedHistoryRef.current.set(ev.id, [sample]);
        }
        setSpeedHistoryVersion((v) => v + 1);
      }
      const hist = speedHistoryRef.current.get(ev.id) ?? [];
      const winSpeed = computeSpeedBps(hist);
      tasksRef.current = tasksRef.current.map((t) =>
        t.id === ev.id
          ? {
              ...t,
              downloaded: ev.downloaded,
              total_size: ev.total ?? t.total_size,
              speed_bps: winSpeed,
            }
          : t
      );
    } else if (kind === "segment_progress") {
      // IDM-style segment snapshot: (taskId, chunkIdx) -> bytes/size.
      // Only bumps the version when something actually changed — the
      // engine re-sends unchanged chunks on every 150ms tick, and a
      // dialog re-render per tick would churn 16 bars needlessly.
      let m = segmentsRef.current.get(ev.id);
      if (!m) {
        m = new Map();
        segmentsRef.current.set(ev.id, m);
      }
      const cur = m.get(ev.idx);
      if (!cur || cur.bytes !== ev.bytes || cur.size !== ev.size) {
        m.set(ev.idx, { bytes: ev.bytes, size: ev.size });
        setSegmentVersion((v) => v + 1);
      }
    }
    setTasks([...tasksRef.current]);
  }, []);

  const refreshAux = useCallback(async () => {
    // allSettled, not all: a single failing aux call (e.g. scheduler
    // status) used to reject the whole batch, leaving queues AND
    // categories empty even though the DB has both ("No queues yet"
    // bug). Each call now applies independently; failures warn so the
    // log names the culprit instead of silently blanking the sidebar.
    const [qR, cR, stR, apR, sitesR, schedR] = await Promise.allSettled([
      api.listQueues(),
      api.listCategories(),
      api.listSchedules(),
      api.activeProfiles(),
      api.listSites(),
      api.schedulerStatus(),
    ]);
    const warn = (name: string, r: PromiseSettledResult<unknown>) => {
      if (r.status === "rejected")
        console.warn(`[refreshAux] ${name} failed:`, r.reason);
    };
    warn("listQueues", qR);
    warn("listCategories", cR);
    warn("listSchedules", stR);
    warn("activeProfiles", apR);
    warn("listSites", sitesR);
    warn("schedulerStatus", schedR);
    if (qR.status === "fulfilled") setQueues(qR.value);
    if (cR.status === "fulfilled") setCategories(cR.value);
    if (sitesR.status === "fulfilled") setSites(sitesR.value);
    if (stR.status === "fulfilled") setSchedules(stR.value);
    if (apR.status === "fulfilled") setActiveProfiles(apR.value);
    // Phase 3B: live scheduler snapshot drives the queue details
    // panel and the status bar.
    if (schedR.status === "fulfilled") setSchedulerStatus(schedR.value);
    try {
      // Refresh effective speeds for every queue we know about.
      const map: Record<string, number> = {};
      await Promise.all(
        (qR.status === "fulfilled" ? qR.value : []).map(async (qq) => {
          map[qq.name] = await api.effectiveQueueSpeedKbps(qq.name);
        })
      );
      setEffectiveSpeed(map);
    } catch {
      /* ignore */
    }
  }, []);

  useEffect(() => {
    void refresh();
    void refreshAux();
    const unsub = api.subscribeToEvents(applyEvent);
    const iv = window.setInterval(() => {
      void refresh();
    }, 1500);
    const onVis = () => {
      document.documentElement.classList.toggle("tab-hidden", document.hidden);
      if (!document.hidden) void refresh();
    };
    document.addEventListener("visibilitychange", onVis);
    onVis();
    void api.getSettings().then(setSettings);
    return () => {
      unsub();
      window.clearInterval(iv);
      document.removeEventListener("visibilitychange", onVis);
    };
  }, [refresh, applyEvent, refreshAux]);

  // ---- selection ----
  const selectOnly = useCallback((id: number) => {
    setSelected(new Set([id]));
  }, []);
  const toggleSelect = useCallback((id: number) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }, []);
  const selectRange = useCallback(
    (ids: number[], anchor: number, id: number) => {
      const a = ids.indexOf(anchor);
      const b = ids.indexOf(id);
      if (a < 0 || b < 0) return;
      const [lo, hi] = a < b ? [a, b] : [b, a];
      const next = new Set(selectedRef.current);
      for (let i = lo; i <= hi; i++) next.add(ids[i]);
      setSelected(next);
    },
    []
  );
  const clearSelection = useCallback(() => setSelected(new Set()), []);

  // ---- actions ----
  const add = useCallback(
    async (nd: NewDownload) => {
      // Apply the category's destination folder if the user picked a
      // category that has a save_dir set. Existing explicit save_dir
      // wins — this only fills it in when the caller didn't.
      if (!nd.save_dir && nd.category) {
        const cat = categories.find(
          (c) => c.name.toLowerCase() === (nd.category ?? "").toLowerCase()
        );
        if (cat && cat.save_dir) {
          nd = { ...nd, save_dir: cat.save_dir };
        }
      }
      const id = await api.addDownload(nd);
      await refresh();
      return id;
    },
    [refresh, categories]
  );

  /** Phase 3C: batch-insert many downloads in a single round-trip.
   *  Each row is reported independently so a single malformed URL
   *  does not abort the rest. The returned ids / errors mirror the
   *  input order, which is what the Batch dialog renders inline.
   *  Like `add`, this also honors each task's category's
   *  destination folder when save_dir is missing. */
  const addBatch = useCallback(
    async (
      nds: NewDownload[],
    ): Promise<Array<{ ok: true; id: number } | { ok: false; error: string }>> => {
      if (nds.length === 0) return [];
      // Apply category save_dir for rows that didn't pick one.
      const expanded = nds.map((nd) => {
        if (nd.save_dir || !nd.category) return nd;
        const cat = categories.find(
          (c) => c.name.toLowerCase() === (nd.category ?? "").toLowerCase(),
        );
        if (cat && cat.save_dir) return { ...nd, save_dir: cat.save_dir };
        return nd;
      });
      const out = await api.addDownloadsBatch(expanded);
      await refresh();
      return out;
    },
    [refresh, categories],
  );

  /** Phase 3C: count how many rows already exist for a URL. The
   *  Batch dialog uses this to highlight duplicates in its
   *  preview. */
  const countExistingByUrl = useCallback(
    (url: string) => api.countExistingByUrl(url),
    [],
  );

  const bulk = useCallback(
    async (fn: (id: number) => Promise<void>) => {
      for (const id of selectedRef.current) await fn(id);
      await refresh();
    },
    [refresh]
  );

  const pause = useCallback((id: number) => api.pauseDownload(id), []);
  const resume = useCallback((id: number) => api.resumeDownload(id), []);
  const cancel = useCallback((id: number) => api.cancelDownload(id), []);
  const remove = useCallback(async (id: number) => {
    try {
      await api.removeDownload(id);
    } catch (e) {
      await api.notify("Cannot remove download", String(e), "error");
      throw e;
    }
  }, []);
  /** Remove the history row but keep the file on disk. */
  const removeKeepFile = useCallback(async (id: number) => {
    try {
      await api.removeKeepFile(id);
    } catch (e) {
      await api.notify("Cannot remove from history", String(e), "error");
      throw e;
    }
  }, []);
  /** Remove the row AND delete the file on disk. Two-step so the
   *  file deletion failure doesn't leave a dangling DB row. */
  const removeWithFile = useCallback(
    async (id: number) => {
      try {
        await api.deleteFile(id);
      } catch (e) {
        // The file may already be gone. That's OK; the next call
        // removes the DB row either way. We do surface the error
        // because the user might have a permissions problem that's
        // worth knowing about.
        await api.notify("File delete failed", String(e), "warning");
      }
      await api.removeDownload(id);
    },
    []
  );
  const redownload = useCallback((id: number) => api.redownload(id), []);
  const moveToQueue = useCallback(
    (id: number, q: string) => api.moveToQueue(id, q),
    []
  );
  const setCategory = useCallback(
    (id: number, c: string) => api.setCategory(id, c),
    []
  );
  const rename = useCallback(
    async (id: number, name: string) => {
      try {
        await api.renameDownload(id, name);
      } catch (e) {
        await api.notify("Cannot rename", String(e), "error");
        throw e;
      }
    },
    []
  );
  const setSaveDir = useCallback(
    (id: number, dir: string) => api.setSaveDir(id, dir),
    []
  );
  const setProxyUrl = useCallback(
    (id: number, proxyUrl: string | null) => api.setProxyUrl(id, proxyUrl),
    []
  );
  const setDescription = useCallback(
    (id: number, description: string) => api.setDescription(id, description),
    []
  );
  const openFolder = useCallback(async (task: DownloadTask) => {
    const path = task.save_dir + (task.filename ? "/" + task.filename : "");
    try {
      await api.openInFileManager(path);
    } catch (e) {
      await api.notify(
        "Cannot open folder",
        `${task.save_dir || path}\n${String(e)}`,
        "error",
      );
      throw e;
    }
  }, []);
  const openFile = useCallback(async (task: DownloadTask) => {
    const path = task.save_dir + (task.filename ? "/" + task.filename : "");
    try {
      await api.openFile(path);
    } catch (e) {
      // The most common cause on Linux is no application associated
      // with the file's MIME type. Surface a clear message instead of
      // failing silently in the dark.
      await api.notify(
        "Cannot open file",
        `${task.filename || path}\n${String(e)}`,
        "error",
      );
      throw e;
    }
  }, []);
  /** Phase 3B: Force Start a download.
 *
 *  Bypasses the normal queue ordering — the task is promoted out of
 *  `Queued` immediately. Hard safety caps (global concurrency, queue
 *  concurrency, queue enabled flag) are still respected: see
 *  `engine::force_start`. The persistent `priority` field is NOT
 *  mutated — only the one-shot `force_started` flag is set, and the
 *  engine clears it when the task leaves Downloading. */
  const forceStart = useCallback(
    async (id: number) => {
      await api.forceStartDownload(id);
      await refresh();
    },
    [refresh]
  );
  /** Phase 3B: set a download's priority (High=+10, Normal=0, Low=-10).
   *  Persisted by the engine, so the change survives a restart. */
  const setDownloadPriority = useCallback(
    async (id: number, priority: number) => {
      await api.setDownloadPriority(id, priority);
      await refresh();
    },
    [refresh]
  );
  const recheck = useCallback((id: number) => api.recheckDownload(id), []);
  const analyze = useCallback(
    (url: string, cookies?: string | null) => api.analyzeUrl(url, cookies),
    []
  );
  const grabSite = useCallback((url: string, limit?: number) => api.grabSite(url, limit), []);
  const grabSiteWithRules = useCallback(
    (url: string, limit: number, rules: GrabRules) => api.grabSiteWithRules(url, limit, rules),
    [],
  );
  /** Apply a global bandwidth cap (kbps; 0 = unlimited). The engine
   *  stores it as `speed_limit_kbps` and applies it through the
   *  downloader's limiter. Persists across restarts via settings. */
  const setBandwidth = useCallback(
    async (kbps: number) => {
      await api.setSetting("speed_limit_kbps", String(kbps));
      setSettings((s) => (s ? { ...s, speed_limit_kbps: kbps } : s));
    },
    []
  );
  /** Set per-queue bandwidth cap (kbps; 0 = unlimited). */
  const setQueueBandwidth = useCallback(
    async (queue: string, kbps: number) => {
      await api.setQueueSpeed(queue, kbps);
      setQueues((qs) => qs.map((q) => (q.name === queue ? { ...q, speed_limit_kbps: kbps } : q)));
    },
    []
  );
  /** Toggle a queue's enabled flag. */
  const setQueueEnabled = useCallback(
    async (name: string, enabled: boolean) => {
      await api.setQueueEnabled(name, enabled);
      setQueues((qs) => qs.map((q) => (q.name === name ? { ...q, enabled } : q)));
    },
    []
  );
  const resumeQueueAction = useCallback(
    async (queue: string) => {
      await api.resumeQueue(queue);
      await refresh();
    },
    [refresh]
  );
  const pauseQueueAction = useCallback(
    async (queue: string) => {
      await api.pauseQueue(queue);
      await refresh();
    },
    [refresh]
  );
  const stopQueueAction = useCallback(
    async (queue: string) => {
      await api.stopQueue(queue);
      await refresh();
    },
    [refresh]
  );
  /** Save an updated queue definition (used by the Queue panel
   *  inline-editable bandwidth cap). */
  const updateQueue = useCallback(
    async (q: DownloadQueue) => {
      await api.updateQueue(q);
      setQueues((qs) => qs.map((x) => (x.name === q.name ? q : x)));
    },
    []
  );
  const createQueue = useCallback(
    async (q: DownloadQueue) => {
      await api.createQueue(q);
      const list = await api.listQueues();
      setQueues(list);
    },
    []
  );
  const deleteQueue = useCallback(
    async (name: string) => {
      await api.deleteQueue(name);
      const list = await api.listQueues();
      setQueues(list);
    },
    []
  );
  /** Phase 3B: move a queue up (-1) or down (+1) in the ordering. */
  const reorderQueueAction = useCallback(
    async (name: string, direction: number) => {
      await api.reorderQueue(name, direction);
      const list = await api.listQueues();
      setQueues(list);
    },
    []
  );
  /** Phase 3B: move a queue to the absolute top or bottom. */
  const moveQueueToAction = useCallback(
    async (name: string, position: "top" | "bottom") => {
      await api.moveQueueTo(name, position);
      const list = await api.listQueues();
      setQueues(list);
    },
    []
  );
  /** Save a category. */
  const saveSchedule = useCallback(
    async (sch: Schedule) => {
      const id = await api.saveSchedule(sch);
      const list = await api.listSchedules();
      setSchedules(list);
      // Re-read active state since schedule composition may have changed.
      const ap = await api.activeProfiles();
      setActiveProfiles(ap);
      return id;
    },
    []
  );
  const deleteSchedule = useCallback(
    async (id: number) => {
      await api.deleteSchedule(id);
      const list = await api.listSchedules();
      setSchedules(list);
      const ap = await api.activeProfiles();
      setActiveProfiles(ap);
    },
    []
  );
  const saveSite = useCallback(async (site: SiteRule) => {
    await api.saveSite(site);
    const list = await api.listSites();
    setSites(list);
  }, []);
  const deleteSite = useCallback(async (domain: string) => {
    await api.deleteSite(domain);
    const list = await api.listSites();
    setSites(list);
  }, []);
  const testSiteMatch = useCallback((url: string) => api.testSiteMatch(url), []);
  const saveQueuePassword = useCallback(
    async (queue: string, password: string) => {
      await api.saveQueuePassword(queue, password);
      const list = await api.listQueues();
      setQueues(list);
    },
    []
  );
  const clearQueuePassword = useCallback(
    async (queue: string) => {
      await api.clearQueuePassword(queue);
      const list = await api.listQueues();
      setQueues(list);
    },
    []
  );
  const testQueueAuth = useCallback(
    async (queue: string) => api.testQueueAuth(queue),
    []
  );

  const saveCategory = useCallback(
    async (c: Category) => {
      await api.saveCategory(c);
      const list = await api.listCategories();
      setCategories(list);
    },
    []
  );
  const deleteCategory = useCallback(
    async (name: string) => {
      await api.deleteCategory(name);
      const list = await api.listCategories();
      setCategories(list);
    },
    []
  );
  const probeUrl = useCallback((url: string) => api.probeUrl(url), []);
  const updateSetting = useCallback(
    async (key: string, value: string) => {
      await api.setSetting(key, value);
      setSettings((s) => (s ? { ...s, [key]: key === "threads" ? Number(value) : value } : s));
    },
    []
  );

  // ---- derived views ----
  const filtered = useMemo(() => {
    let list = tasks;
    if (sidebarKey.kind !== "all") list = list.filter((t) => sideMatch(t, sidebarKey));
    if (search) {
      // The search box supports a compact filter syntax in addition to
      // a plain substring match:
      //   `since:2026-01-01`   → only tasks added on or after that date
      //   `until:2026-08-29`   → only tasks added on or before that date
      //   `today`              → tasks added today
      //   `week`               → tasks added in the last 7 days
      // Bare terms (anything else) are matched against filename, URL,
      // domain, category, queue, and description. Multiple terms are
      // AND-ed.
      const tokens = search.split(/\s+/).filter(Boolean);
      const dayMs = 24 * 60 * 60;
      const todayStart = Math.floor(Date.now() / 1000 / dayMs) * dayMs;
      const weekStart = todayStart - 6 * dayMs;
      const dateFilters: ((t: DownloadTask) => boolean)[] = [];
      const textTerms: string[] = [];
      for (const tok of tokens) {
        const lower = tok.toLowerCase();
        if (lower === "today") {
          dateFilters.push((t) => (t.created_at ?? 0) >= todayStart);
          continue;
        }
        if (lower === "week") {
          dateFilters.push((t) => (t.created_at ?? 0) >= weekStart);
          continue;
        }
        const sinceMatch = /^since:(\d{4}-\d{2}-\d{2})$/i.exec(tok);
        if (sinceMatch) {
          const ts = Math.floor(new Date(sinceMatch[1] + "T00:00:00Z").getTime() / 1000);
          if (!isNaN(ts)) {
            dateFilters.push((t) => (t.created_at ?? 0) >= ts);
            continue;
          }
        }
        const untilMatch = /^until:(\d{4}-\d{2}-\d{2})$/i.exec(tok);
        if (untilMatch) {
          const ts = Math.floor(new Date(untilMatch[1] + "T23:59:59Z").getTime() / 1000);
          if (!isNaN(ts)) {
            dateFilters.push((t) => (t.created_at ?? 0) <= ts);
            continue;
          }
        }
        textTerms.push(lower);
      }
      for (const df of dateFilters) list = list.filter(df);
      if (textTerms.length > 0) {
        list = list.filter((t) =>
          textTerms.every(
            (q) =>
              (t.filename ?? "").toLowerCase().includes(q) ||
              t.url.toLowerCase().includes(q) ||
              domainOf(t.url).includes(q) ||
              (t.category ?? "").toLowerCase().includes(q) ||
              (t.queue ?? "").toLowerCase().includes(q) ||
              (t.description ?? "").toLowerCase().includes(q)
          )
        );
      }
    }
    const dir = sort.dir === "asc" ? 1 : -1;
    const sorted = [...list].sort((a, b) => {
      const va = sortValue(a, sort.field);
      const vb = sortValue(b, sort.field);
      if (va < vb) return -1 * dir;
      if (va > vb) return 1 * dir;
      return a.id - b.id;
    });
    return sorted;
  }, [tasks, sidebarKey, search, sort]);

  const grouped = useMemo(() => {
    if (groupBy === "none") return [{ key: "", items: filtered }];
    const map = new Map<string, DownloadTask[]>();
    for (const t of filtered) {
      let g = "";
      if (groupBy === "status") g = t.status;
      else if (groupBy === "category") g = t.category || "other";
      else if (groupBy === "queue") g = t.queue || "general";
      else if (groupBy === "date") {
        // day bucket
        const d = t.created_at ? new Date(t.created_at * 1000) : new Date();
        g = d.toLocaleDateString(undefined, { month: "short", day: "numeric" });
      }
      if (!map.has(g)) map.set(g, []);
      map.get(g)!.push(t);
    }
    return [...map.entries()].map(([k, v]) => ({ key: k, items: v }));
  }, [filtered, groupBy]);

  const counts = useMemo(() => {
    const c: Record<string, number> = { all: tasks.length };
    for (const t of tasks) {
      // UI-fused status (matches DownloadTable.renderStatus): a paused
      // row whose bytes already cover total_size renders Complete per
      // spec — tally it as completed so bar counters match the table.
      let st = t.status;
      if (st === "paused" && t.total_size > 0 && t.downloaded >= t.total_size) st = "completed";
      c[st] = (c[st] ?? 0) + 1;
      if (st !== "completed" && st !== "cancelled") c.unfinished = (c.unfinished ?? 0) + 1;
      else if (st === "completed") c.finished = (c.finished ?? 0) + 1;
      if (st === "failed") c.failed = (c.failed ?? 0) + 1;
      if (st === "scheduled") c.scheduled = (c.scheduled ?? 0) + 1;
      // Per-category badge counts. Sidebar renders `cat:<name>` for
      // both built-in and user categories (extension-derived, i.e. the
      // engine's guess_category output).
      c[`cat:${t.category}`] = (c[`cat:${t.category}`] ?? 0) + 1;
    }
    return c;
  }, [tasks]);

  const stats = useMemo(() => {
    let activeSpeed = 0;
    let activeCount = 0;
    let queuedCount = 0;
    let remaining = 0;
    for (const t of tasks) {
      if (t.status === "downloading") {
        activeSpeed += t.speed_bps;
        activeCount++;
        if (t.total_size > t.downloaded) remaining += t.total_size - t.downloaded;
      } else if (t.status === "queued") queuedCount++;
    }
    return { activeSpeed, activeCount, queuedCount, remaining };
  }, [tasks]);

  const version = "1.0.0"; // matches CARGO_PKG_VERSION (tauri.conf 1.0.0)

  return {
    tasks,
    filtered,
    grouped,
    counts,
    stats,
    version,
    settings,
    selected,
    sidebarKey,
    search,
    sort,
    groupBy,
    loaded,
    queues,
    categories,
    sites,
    schedules,
    activeProfiles,
    effectiveSpeed,
    schedulerStatus,
    // Reader function for the speed-graph column. Returns a stable
    // reference to the same array each call (so React doesn't see a
    // new identity on every render and re-render the SVG), but
    // `speedHistoryVersion` is the dependency consumers should use to
    // re-read on progress ticks.
    getSpeedHistory: (id: number) =>
      speedHistoryRef.current.get(id) ?? EMPTY_SAMPLES,
    speedHistoryVersion,
    // Reader for the IDM multi-segment dialog: taskId -> array of
    // segment snapshots sorted by chunk index. Same contract as
    // getSpeedHistory: stable reference, re-read on segmentVersion.
    getSegments: (id: number): SegmentSnapshot[] => {
      const m = segmentsRef.current.get(id);
      if (!m || m.size === 0) return EMPTY_SEGMENTS;
      return [...m.entries()]
        .sort((a, b) => a[0] - b[0])
        .map(([idx, s]) => ({ idx, bytes: s.bytes, size: s.size }));
    },
    segmentVersion,
    setSidebarKey,
    setSearch,
    setSort,
    setGroupBy,
    selectOnly,
    toggleSelect,
    selectRange,
    clearSelection,
    add,
    addBatch,
    countExistingByUrl,
    pause,
    resume,
    cancel,
    remove,
    redownload,
    resumeAll: () => api.resumeAll(),
    pauseAll: () => api.pauseAll(),
    stopAll: () => api.stopAll(),
    retryAll: () => api.retryAll(),
    clearFinished: () => api.clearFinished(),
    bulk,
    moveToQueue,
    setCategory,
    rename,
    setSaveDir,
    setProxyUrl,
    setDescription,
    removeWithFile,
    openFolder,
    openFile,
    forceStart,
    setDownloadPriority,
    recheck,
    analyze,
    grabSite,
    grabSiteWithRules,
    setBandwidth,
    removeKeepFile,
    setQueueBandwidth,
    setQueueEnabled,
    resumeQueueAction,
    pauseQueueAction,
    stopQueueAction,
    updateQueue,
    createQueue,
    deleteQueue,
    reorderQueueAction,
    moveQueueToAction,
    saveCategory,
    deleteCategory,
    saveSchedule,
    deleteSchedule,
    saveSite,
    deleteSite,
    testSiteMatch,
    saveQueuePassword,
    clearQueuePassword,
    testQueueAuth,
    probeUrl,
    updateSetting,
    refresh,
    refreshAux,
    lastNotification,
    dismissNotification,
  };
}

function normalizeTask(t: Partial<DownloadTask> & { id: number }): DownloadTask {
  const now = Math.floor(Date.now() / 1000);
  return {
    id: t.id,
    url: t.url ?? "",
    filename: t.filename ?? "",
    save_dir: t.save_dir ?? "",
    status: (t.status as DownloadTask["status"]) ?? "queued",
    total_size: t.total_size ?? 0,
    downloaded: t.downloaded ?? 0,
    transferred: t.transferred ?? t.downloaded ?? 0,
    speed_bps: t.speed_bps ?? 0,
    threads: t.threads ?? 1,
    format_id: t.format_id ?? null,
    webpage_url: t.webpage_url ?? null,
    referer: t.referer ?? null,
    user_agent: t.user_agent ?? null,
    cookies: t.cookies ?? null,
    proxy_url: t.proxy_url ?? null,
    category: t.category ?? "other",
    expected_sha256: t.expected_sha256 ?? null,
    error: t.error ?? null,
    created_at: t.created_at ?? now,
    updated_at: t.updated_at ?? now,
    // IDM extensions defaulted
    queue: t.queue ?? "general",
    description: t.description ?? null,
    started_at: t.started_at ?? null,
    completed_at: t.completed_at ?? null,
    last_resumed_at: t.last_resumed_at ?? null,
    retry_count: t.retry_count ?? 0,
    max_retries: t.max_retries ?? 0,
    retry_delay_ms: t.retry_delay_ms ?? 0,
    conn_timeout_ms: t.conn_timeout_ms ?? 0,
    read_timeout_ms: t.read_timeout_ms ?? 0,
    peak_speed_bps: t.peak_speed_bps ?? 0,
    avg_speed_bps: t.avg_speed_bps ?? 0,
    segments: t.segments ?? 0,
    resume_support: t.resume_support ?? false,
    http_status: t.http_status ?? 0,
    content_type: t.content_type ?? null,
    checksum_algorithm: t.checksum_algorithm ?? "",
    checksum_value: t.checksum_value ?? "",
    checksum_verified: t.checksum_verified ?? false,
    site_id: t.site_id ?? "",
    error_code: t.error_code ?? 0,
    error_category: t.error_category ?? "",
    on_complete: t.on_complete ?? "",
    // Phase 3B defaults
    priority: t.priority ?? 0,
    force_started: t.force_started ?? false,
    // Mirror/multi-source: Rust serializes `mirrors` as a JSON array
    // (Vec<String>). The defensive branch tolerates a legacy string.
    mirrors: Array.isArray(t.mirrors)
      ? t.mirrors
      : typeof t.mirrors === "string"
        ? (() => {
            try {
              const v = JSON.parse(t.mirrors);
              return Array.isArray(v) ? (v as string[]) : [];
            } catch {
              return [];
            }
          })()
        : [],
    mirror_index: t.mirror_index ?? 0,
  };
}

export type DownloadsStore = ReturnType<typeof useDownloads>;
