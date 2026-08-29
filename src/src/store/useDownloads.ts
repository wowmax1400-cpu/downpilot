// Central state hook: polls the backend, listens for push events,
// and exposes actions + derived views (filtered, sorted, grouped) for the UI.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import * as api from "../lib/api";
import { domainOf } from "../lib/format";
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
  /** Effective (current) speed limit for each queue, in kbps. Cached
   *  after each refresh so the UI can show "Scheduled: X" without a
   *  roundtrip per row. */
  const [effectiveSpeed, setEffectiveSpeed] = useState<Record<string, number>>({});
  const tasksRef = useRef<DownloadTask[]>([]);
  const selectedRef = useRef(selected);
  selectedRef.current = selected;

  const refresh = useCallback(async () => {
    const list = await api.listDownloads();
    // Backend may omit extension fields; coerce to a full DownloadTask.
    const norm = list.map(normalizeTask);
    tasksRef.current = norm;
    setTasks(norm);
    setLoaded(true);
  }, []);

  const applyEvent = useCallback((raw: EngineEvent) => {
    // Rust serializes events as {"type":"added", ...} (tag = "type",
    // snake_case). The store's EngineEvent type uses `kind`, so read both.
    const ev = raw as unknown as Record<string, any>;
    const kind: string = ev.type ?? ev.kind;
    if (kind === "removed") {
      tasksRef.current = tasksRef.current.filter((t) => t.id !== ev.id);
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
      tasksRef.current = tasksRef.current.map((t) =>
        t.id === ev.id
          ? {
              ...t,
              downloaded: ev.downloaded,
              total_size: ev.total ?? t.total_size,
              speed_bps: ev.speed_bps,
            }
          : t
      );
    }
    setTasks([...tasksRef.current]);
  }, []);

  const refreshAux = useCallback(async () => {
    try {
      const [q, c, st, ap, sites] = await Promise.all([
        api.listQueues(),
        api.listCategories(),
        api.listSchedules(),
        api.activeProfiles(),
        api.listSites(),
      ]);
      setQueues(q);
      setCategories(c);
      setSites(sites);
      setSchedules(st);
      setActiveProfiles(ap);
      // Refresh effective speeds for every queue we know about.
      const map: Record<string, number> = {};
      await Promise.all(
        q.map(async (qq) => {
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
  const remove = useCallback((id: number) => api.removeDownload(id), []);
  /** Remove the history row but keep the file on disk. */
  const removeKeepFile = useCallback((id: number) => api.removeKeepFile(id), []);
  /** Remove the row AND delete the file on disk. Two-step so the
   *  file deletion failure doesn't leave a dangling DB row. */
  const removeWithFile = useCallback(
    async (id: number) => {
      try {
        await api.deleteFile(id);
      } catch {
        /* ignore — file may not exist */
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
    (id: number, name: string) => api.renameDownload(id, name),
    []
  );
  const setSaveDir = useCallback(
    (id: number, dir: string) => api.setSaveDir(id, dir),
    []
  );
  const setDescription = useCallback(
    (id: number, description: string) => api.setDescription(id, description),
    []
  );
  const openFolder = useCallback(async (task: DownloadTask) => {
    const path = task.save_dir + (task.filename ? "/" + task.filename : "");
    await api.openInFileManager(path);
  }, []);
  const openFile = useCallback(async (task: DownloadTask) => {
    const path = task.save_dir + (task.filename ? "/" + task.filename : "");
    await api.openFile(path);
  }, []);
  /** Force-start a queued download by removing it from the queue gate.
   *  We do this by sending a redownload (which re-enqueues), but
   *  since IDM uses Force Start to bypass the global+queue limits,
   *  we just call resume with a small hack: cancel+resume forces a
   *  fresh start. For now, alias to `redownload` (creates a new id
   *  if needed) since the engine always starts on add(). */
  const forceStart = useCallback((id: number) => api.resumeDownload(id).catch(() => api.redownload(id)), []);
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
      const q = search.toLowerCase();
      list = list.filter(
        (t) =>
          (t.filename ?? "").toLowerCase().includes(q) ||
          t.url.toLowerCase().includes(q) ||
          domainOf(t.url).includes(q) ||
          (t.category ?? "").toLowerCase().includes(q) ||
          (t.queue ?? "").toLowerCase().includes(q) ||
          (t.description ?? "").toLowerCase().includes(q)
      );
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
      c[t.status] = (c[t.status] ?? 0) + 1;
      if (t.status !== "completed" && t.status !== "cancelled")
        c.unfinished = (c.unfinished ?? 0) + 1;
      else if (t.status === "completed") c.finished = (c.finished ?? 0) + 1;
      if (t.status === "failed") c.failed = (c.failed ?? 0) + 1;
      if (t.status === "paused" || t.status === "stopped") c.paused = (c.paused ?? 0) + 1;
      if (t.status === "scheduled") c.scheduled = (c.scheduled ?? 0) + 1;
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

  const version = "1.0.0"; // read from backend if exposed

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
    setSidebarKey,
    setSearch,
    setSort,
    setGroupBy,
    selectOnly,
    toggleSelect,
    selectRange,
    clearSelection,
    add,
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
    setDescription,
    removeWithFile,
    openFolder,
    openFile,
    forceStart,
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
  };
}

export type DownloadsStore = ReturnType<typeof useDownloads>;
