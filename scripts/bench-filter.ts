// @ts-nocheck — standalone perf bench, not type-checked by the project.
// Performance micro-benchmark for the search/filter/sort pipeline in
// the downloads store. Run with:  npx tsx scripts/bench-filter.ts
// (or `node --import tsx scripts/bench-filter.ts`).

import { performance } from "node:perf_hooks";

interface Task {
  id: number;
  filename: string;
  url: string;
  category: string;
  queue: string;
  description: string;
  status: string;
  created_at: number;
  total_size: number;
  downloaded: number;
  speed_bps: number;
  avg_speed_bps: number;
  threads: number;
}

function domainOf(url: string): string {
  try {
    return new URL(url).hostname;
  } catch {
    return "";
  }
}

// Replicate the exact filter+sort logic from useDownloads.ts
function filterAndSort(
  tasks: Task[],
  search: string,
  sortField: keyof Task = "created_at",
  sortDir: "asc" | "desc" = "desc",
): Task[] {
  let list = tasks;
  if (search) {
    const tokens = search.split(/\s+/).filter(Boolean);
    const dayMs = 24 * 60 * 60;
    const todayStart = Math.floor(Date.now() / 1000 / dayMs) * dayMs;
    const weekStart = todayStart - 6 * dayMs;
    const dateFilters: ((t: Task) => boolean)[] = [];
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
        const ts = Math.floor(
          new Date(sinceMatch[1] + "T00:00:00Z").getTime() / 1000,
        );
        if (!isNaN(ts)) {
          dateFilters.push((t) => (t.created_at ?? 0) >= ts);
          continue;
        }
      }
      const untilMatch = /^until:(\d{4}-\d{2}-\d{2})$/i.exec(tok);
      if (untilMatch) {
        const ts = Math.floor(
          new Date(untilMatch[1] + "T23:59:59Z").getTime() / 1000,
        );
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
            (t.description ?? "").toLowerCase().includes(q),
        ),
      );
    }
  }
  const dir = sortDir === "asc" ? 1 : -1;
  return [...list].sort((a, b) => {
    const va = a[sortField] as any;
    const vb = b[sortField] as any;
    if (va < vb) return -1 * dir;
    if (va > vb) return 1 * dir;
    return a.id - b.id;
  });
}

function makeTasks(n: number): Task[] {
  const cats = ["software", "video", "music", "docs", "other"];
  const statuses = [
    "downloading",
    "completed",
    "failed",
    "paused",
    "queued",
    "cancelled",
  ];
  const now = Math.floor(Date.now() / 1000);
  return Array.from({ length: n }, (_, i) => ({
    id: i + 1,
    filename: `file-${i}-${(i % 7) ? ".iso" : ".zip"}`,
    url: `https://cdn${(i % 19) + 1}.example.com/path/${i}/file${i}.bin`,
    category: cats[i % cats.length],
    queue: i % 3 === 0 ? "default" : i % 3 === 1 ? "media" : "archives",
    description: i % 4 === 0 ? "important backup" : "",
    status: statuses[i % statuses.length],
    // Spread creation times across the last 30 days
    created_at: now - (i * 30 * 60) % (30 * 24 * 3600),
    total_size: 1_000_000 + i * 12345,
    downloaded: 500_000 + i * 1000,
    speed_bps: i % 5 === 0 ? 1024 * 1024 : 0,
    avg_speed_bps: i % 5 === 0 ? 512 * 1024 : 0,
    threads: (i % 4) + 1,
  }));
}

function bench(name: string, n: number, iter: number, fn: () => void): number {
  // Warm up
  for (let i = 0; i < 3; i++) fn();
  const start = performance.now();
  for (let i = 0; i < iter; i++) fn();
  const elapsed = performance.now() - start;
  const perIter = elapsed / iter;
  console.log(
    `  ${name.padEnd(30)} n=${n.toString().padStart(3)}  ${iter}x  ` +
      `total=${elapsed.toFixed(2)}ms  avg=${perIter.toFixed(3)}ms/pass`,
  );
  return perIter;
}

const SIZES = [100, 200, 500];
const SCENARIOS: Array<[string, string]> = [
  ["no search", ""],
  ["single text token", "video"],
  ["two text tokens", "video media"],
  ["week filter", "week"],
  ["since + until", "since:2026-08-01 until:2026-08-29"],
  ["3 tokens + since", "video media since:2026-08-15"],
];

console.log("DownPilot filter/benchmark\n");

let allPass = true;
for (const n of SIZES) {
  console.log(`\n== ${n} tasks ==`);
  const tasks = makeTasks(n);
  for (const [name, search] of SCENARIOS) {
    const ms = bench(name, n, 50, () => filterAndSort(tasks, search));
    // Soft targets — flag if we regress badly. These are deliberately
    // generous so a slow CI machine doesn't break the build.
    const limit = n <= 200 ? 10 : 25;
    if (ms > limit) {
      console.log(`    !! above soft target of ${limit}ms — investigate`);
      allPass = false;
    }
  }
}

console.log(
  allPass
    ? "\nAll scenarios under soft target. UI filter pipeline is healthy."
    : "\nSome scenarios exceeded soft targets; see warnings above.",
);
