// Deterministic tests for the authoritative metrics module (lib/metrics.ts).
// No timers, no network: every value is computed from controlled inputs.

import { describe, it, expect } from "vitest";
import {
  computeSpeedBps,
  computeEtaSeconds,
  deriveTaskMetrics,
  type TaskLike,
} from "./metrics";

function task(over: Partial<TaskLike> = {}): TaskLike {
  return {
    id: 1,
    status: "downloading",
    total_size: 6_500_000_000,
    downloaded: 4_200_000_000,
    transferred: 4_200_000_000,
    speed_bps: 0,
    avg_speed_bps: 0,
    ...over,
  };
}

describe("computeSpeedBps", () => {
  it("returns 0 for empty history (placeholder, not fake data)", () => {
    expect(computeSpeedBps([])).toBe(0);
  });

  it("returns the windowed mean of samples", () => {
    expect(computeSpeedBps([10, 20, 30])).toBe(20);
  });

  it("ignores negative and non-finite samples", () => {
    expect(computeSpeedBps([10, -5, NaN, Infinity, 30])).toBe(20);
  });

  it("is stable across bursty samples (rolling window)", () => {
    // Bursty 2 Hz samples around ~12.8 MB/s stay near the mean.
    const hist = [12e6, 14e6, 11e6, 15e6, 12.5e6, 12e6];
    const mean = hist.reduce((a, b) => a + b, 0) / hist.length;
    expect(Math.abs(computeSpeedBps(hist) - mean) / mean).toBeLessThan(0.01);
  });
});

describe("computeEtaSeconds", () => {
  it("computes whole seconds from remaining ÷ speed", () => {
    // 2.3 GB at 12.8 MB/s = 179.7 s → rounds to 180
    const eta = computeEtaSeconds(2_300_000_000, 12.8e6);
    expect(eta).toBe(180);
  });

  it("returns null for zero/unknown speed (never 0s or ∞)", () => {
    expect(computeEtaSeconds(1000, 0)).toBeNull();
    expect(computeEtaSeconds(1000, NaN)).toBeNull();
    expect(computeEtaSeconds(1000, Infinity)).toBeNull();
  });

  it("returns null for zero/unknown remaining", () => {
    expect(computeEtaSeconds(0, 12e6)).toBeNull();
    expect(computeEtaSeconds(-5, 12e6)).toBeNull();
  });

  it("never returns negative or sub-second-zero values", () => {
    expect(computeEtaSeconds(1, 12e6)).toBe(1);
  });
});

describe("deriveTaskMetrics", () => {
  it("active download: windowed speed, remaining, ETA, percent", () => {
    const m = deriveTaskMetrics(task(), [12e6, 14e6]);
    expect(m.speedBps).toBe(13e6);
    expect(m.remaining).toBe(2_300_000_000);
    expect(m.etaSecs).not.toBeNull();
    expect(m.pct).toBe(65);
  });

  it("active download with no samples yet: speed 0, ETA null (placeholder)", () => {
    const m = deriveTaskMetrics(task(), []);
    expect(m.speedBps).toBe(0);
    expect(m.etaSecs).toBeNull();
  });

  it("unknown total: pct -1, remaining -1, ETA null", () => {
    const m = deriveTaskMetrics(task({ total_size: 0 }), [12e6]);
    expect(m.pct).toBe(-1);
    expect(m.remaining).toBe(-1);
    expect(m.etaSecs).toBeNull();
  });

  it("completed: 100%, remaining 0, ETA null, backend average surfaced", () => {
    const m = deriveTaskMetrics(
      task({ status: "completed", downloaded: 6_500_000_000, avg_speed_bps: 10.8e6 }),
    );
    expect(m.pct).toBe(100);
    expect(m.remaining).toBe(0);
    expect(m.etaSecs).toBeNull();
    expect(m.avgSpeedBps).toBe(10.8e6);
    expect(m.speedBps).toBe(0);
  });

  it("failed: keeps downloaded count and last (average) speed, no ETA", () => {
    const m = deriveTaskMetrics(
      task({ status: "failed", avg_speed_bps: 12.1e6 }),
      [],
    );
    expect(m.remaining).toBe(2_300_000_000);
    expect(m.avgSpeedBps).toBe(12.1e6);
    expect(m.etaSecs).toBeNull();
    expect(m.pct).toBe(65);
  });

  it("paused: speed 0, ETA null, downloaded/percent preserved", () => {
    const m = deriveTaskMetrics(task({ status: "paused" }), [12e6, 99e9]);
    expect(m.speedBps).toBe(0);
    expect(m.etaSecs).toBeNull();
    expect(m.remaining).toBe(2_300_000_000);
    expect(m.pct).toBe(65);
  });
});
