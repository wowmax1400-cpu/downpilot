// Compact, monochrome icon set for the IDM-style desktop UI.
// All icons accept size/className; drawn with currentColor so they
// inherit text color (and theme accent where appropriate).

import React from "react";

type P = { size?: number; className?: string };

const base = (size: number): React.SVGProps<SVGSVGElement> => ({
  width: size,
  height: size,
  viewBox: "0 0 24 24",
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 1.8,
  strokeLinecap: "round" as const,
  strokeLinejoin: "round" as const,
});

export const IconAdd = ({ size = 16, className }: P) => (
  <svg {...base(size)} className={className}>
    <path d="M12 5v14M5 12h14" />
  </svg>
);
export const IconStart = ({ size = 16, className }: P) => (
  <svg {...base(size)} className={className}>
    <path d="M6 4l14 8-14 8V4z" fill="currentColor" stroke="none" />
  </svg>
);
export const IconResume = IconStart;
export const IconPause = ({ size = 16, className }: P) => (
  <svg {...base(size)} className={className}>
    <rect x="6" y="5" width="4" height="14" fill="currentColor" stroke="none" />
    <rect x="14" y="5" width="4" height="14" fill="currentColor" stroke="none" />
  </svg>
);
export const IconStop = ({ size = 16, className }: P) => (
  <svg {...base(size)} className={className}>
    <rect x="6" y="6" width="12" height="12" fill="currentColor" stroke="none" />
  </svg>
);
export const IconDelete = ({ size = 16, className }: P) => (
  <svg {...base(size)} className={className}>
    <path d="M4 7h16M9 7V4h6v3M6 7l1 13h10l1-13" />
  </svg>
);
export const IconRedownload = ({ size = 16, className }: P) => (
  <svg {...base(size)} className={className}>
    <path d="M3 12a9 9 0 1 1 3 6.7" />
    <path d="M3 19v-5h5" />
  </svg>
);
export const IconRetry = ({ size = 16, className }: P) => (
  <svg {...base(size)} className={className}>
    <path d="M21 12a9 9 0 1 1-3-6.7" />
    <path d="M21 4v5h-5" />
  </svg>
);
export const IconOptions = ({ size = 16, className }: P) => (
  <svg {...base(size)} className={className}>
    <circle cx="12" cy="12" r="3" />
    <path d="M12 2v3M12 19v3M2 12h3M19 12h3M5 5l2 2M17 17l2 2M19 5l-2 2M7 17l-2 2" />
  </svg>
);
export const IconScheduler = ({ size = 16, className }: P) => (
  <svg {...base(size)} className={className}>
    <circle cx="12" cy="12" r="8" />
    <path d="M12 7v5l3 2" />
  </svg>
);
export const IconQueue = ({ size = 16, className }: P) => (
  <svg {...base(size)} className={className}>
    <path d="M4 6h12M4 12h12M4 18h12" />
    <path d="M18 9l3 3-3 3" />
  </svg>
);
export const IconGrabber = ({ size = 16, className }: P) => (
  <svg {...base(size)} className={className}>
    <circle cx="11" cy="11" r="6" />
    <path d="M20 20l-4-4" />
  </svg>
);
export const IconSite = ({ size = 16, className }: P) => (
  <svg {...base(size)} className={className}>
    <circle cx="12" cy="12" r="9" />
    <path d="M3 12h18M12 3c3 3 3 15 0 18M12 3c-3 3-3 15 0 18" />
  </svg>
);
export const IconCategory = ({ size = 16, className }: P) => (
  <svg {...base(size)} className={className}>
    <path d="M4 5h7v7H4zM13 5h7v7h-7zM4 13h7v7H4zM13 13h7v7h-7z" />
  </svg>
);
export const IconSettings = IconOptions;
export const IconRefresh = ({ size = 16, className }: P) => (
  <svg {...base(size)} className={className}>
    <path d="M20 11a8 8 0 0 0-14-5M4 4v4h4" />
    <path d="M4 13a8 8 0 0 0 14 5M20 20v-4h-4" />
  </svg>
);
export const IconSearch = ({ size = 16, className }: P) => (
  <svg {...base(size)} className={className}>
    <circle cx="11" cy="11" r="6" />
    <path d="M20 20l-4-4" />
  </svg>
);
export const IconClipboard = ({ size = 16, className }: P) => (
  <svg {...base(size)} className={className}>
    <rect x="6" y="4" width="12" height="17" rx="2" />
    <path d="M9 4V3h6v1" />
  </svg>
);
export const IconFolder = ({ size = 16, className }: P) => (
  <svg {...base(size)} className={className}>
    <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V7z" />
  </svg>
);
export const IconFile = ({ size = 16, className }: P) => (
  <svg {...base(size)} className={className}>
    <path d="M6 3h8l4 4v14H6z" />
    <path d="M14 3v4h4" />
  </svg>
);
export const IconProperties = ({ size = 16, className }: P) => (
  <svg {...base(size)} className={className}>
    <path d="M4 5h16M4 12h16M4 19h16" />
    <circle cx="8" cy="5" r="1.4" fill="currentColor" stroke="none" />
    <circle cx="8" cy="12" r="1.4" fill="currentColor" stroke="none" />
    <circle cx="8" cy="19" r="1.4" fill="currentColor" stroke="none" />
  </svg>
);
export const IconSpeed = ({ size = 16, className }: P) => (
  <svg {...base(size)} className={className}>
    <path d="M3 17a9 9 0 0 1 18 0" />
    <path d="M12 17l5-6" />
  </svg>
);
export const IconClose = ({ size = 16, className }: P) => (
  <svg {...base(size)} className={className}>
    <path d="M6 6l12 12M18 6L6 18" />
  </svg>
);
export const IconMenu = ({ size = 16, className }: P) => (
  <svg {...base(size)} className={className}>
    <path d="M4 6h16M4 12h16M4 18h16" />
  </svg>
);
export const IconChevron = ({ size = 16, className }: P) => (
  <svg {...base(size)} className={className}>
    <path d="M9 6l6 6-6 6" />
  </svg>
);
export const IconBolt = ({ size = 16, className }: P) => (
  <svg {...base(size)} className={className}>
    <path d="M13 2L4 14h7l-1 8 9-12h-7l1-8z" fill="currentColor" stroke="none" />
  </svg>
);
export const IconBatch = ({ size = 16, className }: P) => (
  <svg {...base(size)} className={className}>
    <path d="M4 6h10M4 12h10M4 18h10" />
    <path d="M18 8v8M14 12h8" />
  </svg>
);
export const IconCheck = ({ size = 16, className }: P) => (
  <svg {...base(size)} className={className}>
    <path d="M5 12l5 5L20 6" />
  </svg>
);
export const IconWarning = ({ size = 16, className }: P) => (
  <svg {...base(size)} className={className}>
    <path d="M12 3l10 17H2L12 3z" />
    <path d="M12 9v5M12 17h.01" />
  </svg>
);
export const IconPauseAll = ({ size = 16, className }: P) => (
  <svg {...base(size)} className={className}>
    <rect x="4" y="5" width="5" height="14" fill="currentColor" stroke="none" />
    <rect x="11" y="5" width="5" height="14" fill="currentColor" stroke="none" />
    <path d="M3 3l18 18" />
  </svg>
);
export const IconStopAll = IconPauseAll;
export const IconPlay = IconStart;

// IDM-specific icons (extra toolbar + context-menu glyphs).
export const IconForceStart = ({ size = 16, className }: P) => (
  <svg {...base(size)} className={className}>
    <path d="M5 4l14 8-14 8V4z" fill="currentColor" stroke="none" />
    <path d="M19 4l-3 8 3 8" />
  </svg>
);
export const IconOpenFile = ({ size = 16, className }: P) => (
  <svg {...base(size)} className={className}>
    <path d="M6 3h8l4 4v14H6z" />
    <path d="M14 3v4h4" />
    <path d="M9 13l6-3-6 3 6 3-6-3z" fill="currentColor" stroke="none" />
  </svg>
);
export const IconDeleteAll = ({ size = 16, className }: P) => (
  <svg {...base(size)} className={className}>
    <path d="M3 6h14M5 6V4h10v2M6 6l1 13h8l1-13" />
    <path d="M9 10v6M13 10v6" />
  </svg>
);
export const IconCopy = ({ size = 16, className }: P) => (
  <svg {...base(size)} className={className}>
    <rect x="8" y="8" width="12" height="12" />
    <path d="M16 8V4H4v12h4" />
  </svg>
);

// ---------------------------------------------------------------------------
// File-type / category icons (IDM-style: file with a small label badge).
// Filled for visual mass; currentColor for theme tinting.
// ---------------------------------------------------------------------------

type FP = { size?: number; className?: string; filled?: boolean };

function fileBase(size: number, filled = false): React.SVGProps<SVGSVGElement> {
  return {
    width: size,
    height: size,
    viewBox: "0 0 24 24",
    fill: filled ? "currentColor" : "none",
    stroke: "currentColor",
    strokeWidth: 1.4,
    strokeLinecap: "round" as const,
    strokeLinejoin: "round" as const,
  };
}

// Per-type color for file icons. Helps distinguish types at small sizes
// (the small inner glyphs get lost below 16px).
export const FILE_ICON_COLOR: Record<string, string> = {
  generic: "text-slate-400",
  video: "text-rose-400",
  audio: "text-fuchsia-400",
  archive: "text-amber-400",
  document: "text-sky-400",
  program: "text-emerald-400",
  image: "text-violet-400",
  disk: "text-orange-400",
};

export const FileIconGeneric = ({ size = 16, className }: FP) => (
  <svg {...fileBase(size)} className={className}>
    <path d="M6 3h8l4 4v14H6z" />
    <path d="M14 3v4h4" />
  </svg>
);

export const FileIconVideo = ({ size = 16, className }: FP) => (
  <svg {...fileBase(size)} className={className}>
    <path d="M6 3h8l4 4v14H6z" />
    <path d="M14 3v4h4" />
    {/* Filled play triangle - bigger so it reads at 16px */}
    <path d="M9 9.5l7 3.5-7 3.5z" fill="currentColor" stroke="none" />
  </svg>
);

export const FileIconAudio = ({ size = 16, className }: FP) => (
  <svg {...fileBase(size)} className={className}>
    <path d="M6 3h8l4 4v14H6z" />
    <path d="M14 3v4h4" />
    {/* Quarter notes */}
    <circle cx="10" cy="16" r="1.4" fill="currentColor" stroke="none" />
    <path d="M11.4 16V10l4-1v6" />
    <circle cx="15" cy="15" r="1.2" fill="currentColor" stroke="none" />
  </svg>
);

export const FileIconArchive = ({ size = 16, className }: FP) => (
  <svg {...fileBase(size)} className={className}>
    <path d="M6 3h8l4 4v14H6z" />
    <path d="M14 3v4h4" />
    {/* Box with cross-tape */}
    <rect x="8" y="11" width="8" height="7" />
    <path d="M8 14h8M12 11v7" />
  </svg>
);

export const FileIconDocument = ({ size = 16, className }: FP) => (
  <svg {...fileBase(size)} className={className}>
    <path d="M6 3h8l4 4v14H6z" />
    <path d="M14 3v4h4" />
    {/* Page lines */}
    <path d="M9 11h6M9 14h6M9 17h4" />
  </svg>
);

export const FileIconProgram = ({ size = 16, className }: FP) => (
  <svg {...fileBase(size)} className={className}>
    <path d="M6 3h8l4 4v14H6z" />
    <path d="M14 3v4h4" />
    {/* Gear / cog */}
    <circle cx="12" cy="14" r="2.5" fill="currentColor" stroke="none" />
    <path d="M12 9.5v1.5M14 10.5l1-1M14.5 14h1.5M14 17.5l1 1M12 18.5V20M10 17.5l-1 1M9.5 14H8M10 10.5l-1-1" />
  </svg>
);

export const FileIconImage = ({ size = 16, className }: FP) => (
  <svg {...fileBase(size)} className={className}>
    <path d="M6 3h8l4 4v14H6z" />
    <path d="M14 3v4h4" />
    <rect x="8" y="12" width="8" height="6" />
    <circle cx="10" cy="14" r="0.8" fill="currentColor" stroke="none" />
  </svg>
);

export const FileIconDisk = ({ size = 16, className }: FP) => (
  <svg {...fileBase(size)} className={className}>
    <path d="M6 3h8l4 4v14H6z" />
    <path d="M14 3v4h4" />
    <circle cx="12" cy="14" r="2.4" />
    <circle cx="12" cy="14" r="0.7" fill="currentColor" stroke="none" />
  </svg>
);

/**
 * Map a filename + category to the right file-type icon.
 * Order of preference: explicit category (video/audio/compressed/documents/
 * programs) then extension-based fallback.
 */
export type FileIconKey =
  | "generic"
  | "video"
  | "audio"
  | "archive"
  | "document"
  | "program"
  | "image"
  | "disk";

export interface FileIconResult {
  Icon: (p: { size?: number; className?: string }) => JSX.Element;
  color: string; // tailwind class
  key: FileIconKey;
}

function pick(key: FileIconKey): FileIconResult {
  const map: Record<FileIconKey, FileIconResult["Icon"]> = {
    generic: FileIconGeneric,
    video: FileIconVideo,
    audio: FileIconAudio,
    archive: FileIconArchive,
    document: FileIconDocument,
    program: FileIconProgram,
    image: FileIconImage,
    disk: FileIconDisk,
  };
  return { Icon: map[key], color: FILE_ICON_COLOR[key], key };
}

export function fileIconFor(filename: string, category?: string | null): FileIconResult {
  const cat = (category ?? "").toLowerCase();
  if (cat === "video") return pick("video");
  if (cat === "music" || cat === "audio") return pick("audio");
  if (cat === "compressed") return pick("archive");
  if (cat === "documents") return pick("document");
  if (cat === "programs") return pick("program");
  const lower = filename.toLowerCase();
  // Compound extensions first (e.g. .tar.gz, .tar.xz, .tar.bz2).
  if (/\.tar\.(gz|xz|bz2|zst)$/.test(lower)) return pick("archive");
  const ext = lower.replace(/.*\./, "");
  if (/^(mp4|mkv|webm|mov|avi|flv|wmv|mpg|mpeg|m4v|ts)$/.test(ext)) return pick("video");
  if (/^(mp3|flac|wav|ogg|m4a|aac|opus|wma)$/.test(ext)) return pick("audio");
  if (/^(zip|rar|7z|tar|gz|bz2|xz|iso|img|dmg)$/.test(ext)) return pick("archive");
  if (/^(pdf|doc|docx|odt|rtf|txt|md|epub|mobi|html|htm|css|js|ts|tsx|jsx|json|xml|yml|yaml)$/.test(ext)) return pick("document");
  if (/^(exe|msi|deb|rpm|apk|appimage|run|sh|bat|jar|py|bin|app)$/.test(ext)) return pick("program");
  if (/^(iso|img|dmg)$/.test(ext)) return pick("disk");
  if (/^(jpg|jpeg|png|gif|webp|bmp|svg|ico)$/.test(ext)) return pick("image");
  return pick("generic");
}
