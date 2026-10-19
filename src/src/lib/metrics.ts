// Authoritative download-metrics derivation.
//
// ONE source of truth for current speed, average speed, remaining bytes,
// ETA and percent — consumed by DownloadTable, DetailsPanel and StatusBar
// so no component computes contradictory values independently.
//
// Current speed is a windowed mean over the store's per-task sample ring
// (samples pushed by the store on every Progress event), NOT a single
// delta: single-delta speeds jump wildly on bursty chunk delivery.

export interface TaskLike {
  id: number;
  status: string;
  total_size: number;
  downloaded: number;
  transferred: number;
  speed_bps: number;
  avg_speed_bps: number;
}

export interface TaskMetrics {
  /** Windowed current speed in bytes/sec; 0 when no meaningful sample. */
  speedBps: number;
  /** Backend-recorded average speed (bytes/sec); 0 when unknown. */
  avgSpeedBps: number;
  /** total - downloaded, clamped at 0; -1 when total is unknown. */
  remaining: number;
  /** ETA seconds from remaining/windowed speed; null when not computable. */
  etaSecs: number | null;
  /** 0-100 rounded percent; -1 when total unknown. */
  pct: number;
}

/**
 * Windowed mean of recent speed samples. Filters non-finite and negative
 * entries; returns 0 when no meaningful sample exists.
 */
export function computeSpeedBps(history: number[]): number {
  if (!history.length) return 0;
  let sum = 0;
  let n = 0;
  for (const s of history) {
    if (Number.isFinite(s) && s >= 0) {
      sum += s;
      n += 1;
    }
  }
  return n > 0 ? sum / n : 0;
}

/**
 * ETA in whole seconds. Returns null whenever the value would be
 * meaningless: unknown/zero speed, unknown/zero remaining, or negative.
 * Deliberately does NOT return 0/∞/negative — callers render "—".
 */
export function computeEtaSeconds(
  remaining: number,
  speedBps: number,
): number | null {
  if (!Number.isFinite(remaining) || remaining <= 0) return null;
  if (!Number.isFinite(speedBps) || speedBps <= 0) return null;
  const secs = remaining / speedBps;
  if (!Number.isFinite(secs) || secs <= 0) return null;
  return Math.max(1, Math.round(secs));
}

/**
 * The one derivation every UI surface uses. `history` is the store's
 * recent-samples ring for the task (may be empty).
 *
 * State handling:
 * - downloading: speed = windowed mean; ETA from remaining ÷ speed.
 * - completed:   speed = backend average (if recorded), ETA = null,
 *                remaining = 0, pct = 100.
 * - failed:      keeps last known downloaded count; speed = backend
 *                average as the "last meaningful" rate, ETA = null.
 * - paused/queued/etc: speed 0, ETA null; downloaded/percent preserved.
 */
export function deriveTaskMetrics(
  t: TaskLike,
  history: number[] = [],
): TaskMetrics {
  const totalKnown = t.total_size > 0;
  const pct = totalKnown
    ? Math.min(100, Math.max(0, Math.round((t.downloaded / t.total_size) * 100)))
    : -1;

  if (t.status === "completed") {
    return {
      speedBps: 0,
      avgSpeedBps: t.avg_speed_bps > 0 ? t.avg_speed_bps : 0,
      remaining: 0,
      etaSecs: null,
      pct: totalKnown ? 100 : pct,
    };
  }

  if (t.status === "downloading") {
    const speedBps = computeSpeedBps(history);
    const remaining = totalKnown
      ? Math.max(0, t.total_size - t.downloaded)
      : -1;
    const etaSecs =
      totalKnown && speedBps > 0 ? computeEtaSeconds(remaining, speedBps) : null;
    return { speedBps, avgSpeedBps: t.avg_speed_bps, remaining, etaSecs, pct };
  }

  // paused / queued / failed / stopped / scheduled
  const remaining = totalKnown
    ? Math.max(0, t.total_size - t.downloaded)
    : -1;
  return {
    // failed tasks surface the backend-recorded average as "last speed"
    speedBps: 0,
    avgSpeedBps: t.avg_speed_bps > 0 ? t.avg_speed_bps : 0,
    remaining,
    etaSecs: null,
    pct,
  };
}

/** "12.8 MB/s" style — shared with the speed column. */
export function metricsSpeedLabel(bps: number): string {
  return bps > 0 ? formatBytesPerSec(bps) : "";
}

function formatBytesPerSec(bps: number): string {
  const units = ["B", "KB", "MB", "GB", "TB"];
  let v = bps;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v >= 100 ? v.toFixed(0) : v >= 10 ? v.toFixed(1) : v.toFixed(2)} ${units[i]}/s`;
}
