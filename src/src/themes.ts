// Theme definitions + runtime application.
// Themes map 1:1 to the palettes in index.css (html[data-theme="..."])
// and only need to define the accent color here (plus a label).

export interface ThemeDef {
  id: string;
  name: string;
  blurb: string;
  accent: string;
  swatch: string; // representative background color for the picker card
}

export const THEMES: ThemeDef[] = [
  {
    id: "deep-midnight",
    name: "Deep Midnight (macOS Dark)",
    blurb: "Cyber neon midnight palette",
    accent: "#06b6d4",
    swatch: "#0a0f1d",
  },
  {
    id: "classic-idm",
    name: "Classic IDM (Light)",
    blurb: "Authentic Win32 IDM paper white",
    accent: "#0284c7",
    swatch: "#f0f0f0",
  },
  {
    id: "modern-dark",
    name: "Modern Dark (Navy)",
    blurb: "Soft slate · easy on the eyes",
    accent: "#38bdf8",
    swatch: "#0f172a",
  },
  {
    id: "cyberpunk",
    name: "Cyberpunk",
    blurb: "Bazzite purple · neon glow",
    accent: "#a855f7",
    swatch: "#130d1f",
  },
];

export const DEFAULT_ACCENTS = [
  "#7c5cff",
  "#a855f7",
  "#3b82f6",
  "#22d3ee",
  "#34d399",
  "#fbbf24",
  "#f87171",
  "#ec4899",
];

export function hexToRgba(hex: string, alpha: number): string {
  const h = hex.replace("#", "");
  const r = parseInt(h.slice(0, 2), 16);
  const g = parseInt(h.slice(2, 4), 16);
  const b = parseInt(h.slice(4, 6), 16);
  return `rgba(${r}, ${g}, ${b}, ${alpha})`;
}

/** Apply a theme id + optional accent hex to the document. */
export function applyTheme(themeId: string, accent?: string) {
  const root = document.documentElement;
  const tid = themeId || "deep-midnight";
  root.dataset.theme = tid;
  if (tid === "classic-idm") {
    root.style.colorScheme = "light";
  } else {
    root.style.colorScheme = "dark";
  }
}
