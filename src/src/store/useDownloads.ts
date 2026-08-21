// Central state hook: polls the backend, listens for push events,
// and exposes actions for the whole UI.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import * as api from "../lib/api";
import type { AppSettings, DownloadTask, EngineEvent, GrabbedLink, MediaInfo, NewDownload } from "../types";

export type CategoryFilter =
  | "all"
  | "downloading"
  | "paused"
  | "completed"
  | "compressed"
  | "documents"
  | "music"
  | "video"
  | "programs"
  | "other";

export function useDownloads() {
  const [tasks, setTasks] = useState<DownloadTask[]>([]);
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [filter, setFilter] = useState<CategoryFilter>("all");
  const [search, setSearch] = useState("");
  const [loaded, setLoaded] = useState(false);
  const tasksRef = useRef<DownloadTask[]>([]);

  const refresh = useCallback(async () => {
    const list = await api.listDownloads();
    tasksRef.current = list;
    setTasks(list);
    setLoaded(true);
  }, []);

  const applyEvent = useCallback((raw: unknown) => {
    const ev = raw as EngineEvent;
    if (ev.type === "removed" && ev.id != null) {
      tasksRef.current = tasksRef.current.filter((t) => t.id !== ev.id);
    } else if (ev.type === "added" && ev.task) {
      tasksRef.current = [ev.task, ...tasksRef.current];
    } else if (ev.type === "status_changed" && ev.id != null) {
      tasksRef.current = tasksRef.current.map((t) =>
        t.id === ev.id ? { ...t, status: (ev.status as DownloadTask["status"]) ?? t.status, error: ev.error ?? t.error } : t
      );
    } else if (ev.type === "progress" && ev.id != null) {
      tasksRef.current = tasksRef.current.map((t) =>
        t.id === ev.id
          ? {
              ...t,
              downloaded: ev.downloaded ?? t.downloaded,
              total_size: ev.total ?? t.total_size,
              speed_bps: ev.speed_bps ?? t.speed_bps,
            }
          : t
      );
    }
    setTasks([...tasksRef.current]);
  }, []);

  useEffect(() => {
    void refresh();
    const unsub = api.subscribeToEvents(applyEvent);
    const iv = window.setInterval(() => {
      // Never poll while the tab is hidden (fixes freeze when switching
      // to another browser/app while the preview is open).
      if (!document.hidden) void refresh();
    }, 2000);
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
  }, [refresh, applyEvent]);

  const add = useCallback(
    async (nd: NewDownload) => {
      const id = await api.addDownload(nd);
      await refresh();
      return id;
    },
    [refresh]
  );

  const pause = useCallback(async (id: number) => {
    await api.pauseDownload(id);
  }, []);
  const resume = useCallback(async (id: number) => {
    await api.resumeDownload(id);
  }, []);
  const cancel = useCallback(async (id: number) => {
    await api.cancelDownload(id);
  }, []);
  const remove = useCallback(async (id: number) => {
    await api.removeDownload(id);
  }, []);
  const openFolder = useCallback(async (task: DownloadTask) => {
    const path = task.save_dir + (task.filename ? "/" + task.filename : "");
    await api.openInFileManager(path);
  }, []);
  const analyze = useCallback((url: string, cookies?: string | null) => api.analyzeUrl(url, cookies), []);
  const grabSite = useCallback((url: string, limit?: number) => api.grabSite(url, limit), []);
  const updateSetting = useCallback(async (key: string, value: string) => {
    await api.setSetting(key, value);
    setSettings((s) => (s ? { ...s, [key]: key === "threads" || key === "speed_limit_kbps" ? Number(value) : value } : s));
  }, []);

  const filtered = useMemo(() => {
    let list = tasks;
    switch (filter) {
      case "downloading":
        list = list.filter((t) => t.status === "downloading" || t.status === "queued");
        break;
      case "paused":
        list = list.filter((t) => t.status === "paused");
        break;
      case "completed":
        list = list.filter((t) => t.status === "completed");
        break;
      default:
        if (filter !== "all") list = list.filter((t) => t.category === filter);
    }
    if (search) {
      const q = search.toLowerCase();
      list = list.filter((t) => (t.filename ?? t.url).toLowerCase().includes(q));
    }
    return [...list].sort((a, b) => b.created_at - a.created_at);
  }, [tasks, filter, search]);

  const counts = useMemo(() => {
    const c: Record<string, number> = {
      all: tasks.length,
      downloading: 0,
      paused: 0,
      completed: 0,
    };
    for (const t of tasks) {
      if (t.status === "downloading" || t.status === "queued") c.downloading++;
      else if (t.status === "paused") c.paused++;
      else if (t.status === "completed") c.completed++;
    }
    return c;
  }, [tasks]);

  const stats = useMemo(() => {
    let activeSpeed = 0;
    let activeCount = 0;
    for (const t of tasks) {
      if (t.status === "downloading") {
        activeSpeed += t.speed_bps;
        activeCount++;
      }
    }
    return { activeSpeed, activeCount };
  }, [tasks]);

  return {
    tasks: filtered,
    allTasks: tasks,
    counts,
    stats,
    settings,
    filter,
    setFilter,
    search,
    setSearch,
    loaded,
    add,
    pause,
    resume,
    cancel,
    remove,
    openFolder,
    analyze,
    grabSite,
    updateSetting,
  };
}

export type DownloadsStore = ReturnType<typeof useDownloads>;
