// SpeedGraph — minimal inline SVG sparkline for per-task download speed.
//
// No charting library: SVG path with two line segments (the curve + a soft
// fill below). We auto-scale so the peak in the visible window is the top
// of the chart, and we show a faint "baseline" so single-sample windows
// don't render as a flat line at the top.
//
// Renders inline with the row so it can sit next to the numeric "Speed"
// column. Width and height are props so the column config controls them.

import React from "react";

export interface SpeedGraphProps {
  /** Recent speed samples in B/s, oldest first, newest last. */
  samples: number[];
  /** SVG viewport width in px (column width). */
  width: number;
  /** SVG viewport height in px (column height). */
  height: number;
  /** Optional theme color (defaults to a calm cyan). */
  color?: string;
  /** Optional fill color (defaults to the line color at low alpha). */
  fillColor?: string;
}

const PADDING_X = 2;
const PADDING_Y = 3;

export function SpeedGraph({
  samples,
  width,
  height,
  color = "#22d3ee",
  fillColor = "rgba(34, 211, 238, 0.18)",
}: SpeedGraphProps) {
  // Empty / no-samples state: faint dashed baseline.
  if (samples.length === 0) {
    return (
      <svg
        width={width}
        height={height}
        viewBox={`0 0 ${width} ${height}`}
        aria-hidden
      >
        <line
          x1={PADDING_X}
          y1={height - PADDING_Y}
          x2={width - PADDING_X}
          y2={height - PADDING_X}
          stroke="currentColor"
          strokeOpacity={0.15}
          strokeDasharray="2 3"
        />
      </svg>
    );
  }

  // Auto-scale: max sample in the window sets the top. A minimum peak
  // of 1 B/s keeps the chart from going vertical when the task is at a
  // literal standstill but we still have one transient nonzero sample.
  const peak = Math.max(1, ...samples);
  const w = width - PADDING_X * 2;
  const h = height - PADDING_Y * 2;
  const step = samples.length > 1 ? w / (samples.length - 1) : 0;

  const points: [number, number][] = samples.map((v, i) => {
    const x = PADDING_X + i * step;
    // Invert Y so higher speed = higher on screen.
    const y = PADDING_Y + h - (v / peak) * h;
    return [x, y];
  });

  const linePath = points
    .map(([x, y], i) => `${i === 0 ? "M" : "L"}${x.toFixed(1)},${y.toFixed(1)}`)
    .join(" ");

  // Fill area: walk the line forward, then close along the baseline.
  const first = points[0];
  const last = points[points.length - 1];
  const fillPath =
    points.length === 1
      ? `M${first[0]},${first[1]} L${first[0]},${
          PADDING_Y + h
        } L${first[0]},${first[1]} Z`
      : `M${first[0]},${first[1]} ${points
          .slice(1)
          .map(([x, y]) => `L${x.toFixed(1)},${y.toFixed(1)}`)
          .join(" ")} L${last[0]},${PADDING_Y + h} L${first[0]},${
          PADDING_Y + h
        } Z`;

  return (
    <svg
      width={width}
      height={height}
      viewBox={`0 0 ${width} ${height}`}
      preserveAspectRatio="none"
      aria-label={`${samples.length} speed samples, peak ${Math.round(peak)} B/s`}
    >
      <path d={fillPath} fill={fillColor} />
      <path
        d={linePath}
        fill="none"
        stroke={color}
        strokeWidth={1.2}
        strokeLinejoin="round"
        strokeLinecap="round"
      />
    </svg>
  );
}
