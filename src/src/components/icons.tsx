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
// Authentic 32x32 Win32 / IDM Classic toolbar icons
export const IdmIconAdd = ({ size = 32 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 32 32">
    <defs>
      <linearGradient id="idm-plus-shade" x1="0" y1="0" x2="0" y2="1">
        <stop offset="0%" stopColor="#6ee7b7" />
        <stop offset="25%" stopColor="#22c55e" />
        <stop offset="75%" stopColor="#15803d" />
        <stop offset="100%" stopColor="#14532d" />
      </linearGradient>
    </defs>
    {/* 3D Green Plus */}
    <path d="M12 3h8v8h8v8h-8v8h-8v-8H4v-8h8z" fill="url(#idm-plus-shade)" stroke="#14532d" strokeWidth="1" />
    <path d="M13 4h6v8h8v6h-8v8h-6v-8H5v-6h8z" fill="none" stroke="#bbf7d0" strokeWidth="1" opacity="0.8" />
  </svg>
);

export const IdmIconResume = ({ size = 32 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 32 32">
    <defs>
      <linearGradient id="idm-down-shade" x1="0" y1="0" x2="0" y2="1">
        <stop offset="0%" stopColor="#93c5fd" />
        <stop offset="30%" stopColor="#3b82f6" />
        <stop offset="80%" stopColor="#1d4ed8" />
        <stop offset="100%" stopColor="#1e3a8a" />
      </linearGradient>
    </defs>
    {/* 3D Blue Arrow */}
    <path d="M11 3h10v12h7L16 29 4 15h7z" fill="url(#idm-down-shade)" stroke="#1e3a8a" strokeWidth="1" />
    <path d="M12 4h8v12h5l-9 10-9-10h5z" fill="none" stroke="#dbeafe" strokeWidth="1" opacity="0.8" />
  </svg>
);

export const IdmIconStop = ({ size = 32 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 32 32">
    <defs>
      <linearGradient id="idm-stop-shade" x1="0" y1="0" x2="0" y2="1">
        <stop offset="0%" stopColor="#fca5a5" />
        <stop offset="30%" stopColor="#ef4444" />
        <stop offset="80%" stopColor="#b91c1c" />
        <stop offset="100%" stopColor="#7f1d1d" />
      </linearGradient>
    </defs>
    {/* Red Pause Bars */}
    <rect x="6" y="4" width="8" height="24" fill="url(#idm-stop-shade)" stroke="#7f1d1d" strokeWidth="1" />
    <rect x="7" y="5" width="6" height="22" fill="none" stroke="#fecaca" strokeWidth="1" opacity="0.8" />
    <rect x="18" y="4" width="8" height="24" fill="url(#idm-stop-shade)" stroke="#7f1d1d" strokeWidth="1" />
    <rect x="19" y="5" width="6" height="22" fill="none" stroke="#fecaca" strokeWidth="1" opacity="0.8" />
  </svg>
);

export const IdmIconStopAll = ({ size = 32 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 32 32">
    <defs>
      <linearGradient id="idm-hand-shade" x1="0" y1="0" x2="0" y2="1">
        <stop offset="0%" stopColor="#fca5a5" />
        <stop offset="40%" stopColor="#ef4444" />
        <stop offset="90%" stopColor="#b91c1c" />
        <stop offset="100%" stopColor="#7f1d1d" />
      </linearGradient>
    </defs>
    {/* Stop Hand */}
    <path d="M9 13v7c0 4 3 8 7 8s7-4 7-8v-5a1.5 1.5 0 0 0-3 0v2h-1v-7a1.5 1.5 0 0 0-3 0v4h-1V7a1.5 1.5 0 0 0-3 0v8h-1v-2a1.5 1.5 0 0 0-3 0z" fill="url(#idm-hand-shade)" stroke="#7f1d1d" strokeWidth="1" />
    <path d="M10 14v6c0 3.5 2.5 7 6 7s6-3.5 6-7v-4h-1v-7h-1v-3h-1v7h-1v-2h-1z" fill="none" stroke="#fee2e2" strokeWidth="0.8" opacity="0.6" />
  </svg>
);

export const IdmIconDelete = ({ size = 32 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 32 32">
    <defs>
      <linearGradient id="idm-bin-shade" x1="0" y1="0" x2="0" y2="1">
        <stop offset="0%" stopColor="#f472b6" />
        <stop offset="35%" stopColor="#db2777" />
        <stop offset="85%" stopColor="#9d174d" />
        <stop offset="100%" stopColor="#831843" />
      </linearGradient>
    </defs>
    {/* Pink Trash Bin */}
    <path d="M7 10h18l-2 19H9z" fill="url(#idm-bin-shade)" stroke="#831843" strokeWidth="1" />
    <path d="M5 7h22v3H5zM12 4h8v3h-8z" fill="#be185d" stroke="#831843" strokeWidth="1" />
    <line x1="12" y1="13" x2="12" y2="25" stroke="#fce7f3" strokeWidth="1.5" />
    <line x1="16" y1="13" x2="16" y2="25" stroke="#fce7f3" strokeWidth="1.5" />
    <line x1="20" y1="13" x2="20" y2="25" stroke="#fce7f3" strokeWidth="1.5" />
  </svg>
);

export const IdmIconOptions = ({ size = 32 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 32 32">
    <defs>
      <linearGradient id="idm-gear-shade" x1="0" y1="0" x2="0" y2="1">
        <stop offset="0%" stopColor="#d8b4fe" />
        <stop offset="30%" stopColor="#a855f7" />
        <stop offset="80%" stopColor="#7e22ce" />
        <stop offset="100%" stopColor="#581c87" />
      </linearGradient>
    </defs>
    {/* Purple Gear */}
    <path d="M16 3l2 4 4-1 2 4-3 3 2 4 4 1v4l-4 1-2 4 3 3-2 4-4-1-2 4h-4l-2-4-4 1-2-4 3-3-2-4-4-1v-4l4-1 2-4-3-3 2-4 4 1 2-4z" fill="url(#idm-gear-shade)" stroke="#581c87" strokeWidth="1" />
    <circle cx="16" cy="16" r="5" fill="#faf5ff" stroke="#581c87" strokeWidth="1" />
  </svg>
);

export const IdmIconScheduler = ({ size = 32 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 32 32">
    <defs>
      <radialGradient id="idm-clock-shade" cx="40%" cy="40%" r="60%">
        <stop offset="0%" stopColor="#fef08a" />
        <stop offset="60%" stopColor="#eab308" />
        <stop offset="100%" stopColor="#a16207" />
      </radialGradient>
    </defs>
    {/* Gold Clock */}
    <circle cx="16" cy="16" r="13" fill="url(#idm-clock-shade)" stroke="#713f12" strokeWidth="1.2" />
    <circle cx="16" cy="16" r="10.5" fill="#ffffff" stroke="#ca8a04" strokeWidth="0.8" />
    <circle cx="16" cy="16" r="1.5" fill="#dc2626" />
    <polyline points="16,8 16,16 22,16" fill="none" stroke="#111827" strokeWidth="2.2" strokeLinecap="round" />
  </svg>
);

export const IdmIconGrabber = ({ size = 32 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 32 32">
    <defs>
      <radialGradient id="idm-grab-globe" cx="40%" cy="40%" r="60%">
        <stop offset="0%" stopColor="#7dd3fc" />
        <stop offset="60%" stopColor="#0284c7" />
        <stop offset="100%" stopColor="#075985" />
      </radialGradient>
    </defs>
    {/* Blue Globe with Yellow Arrow */}
    <circle cx="16" cy="16" r="13" fill="url(#idm-grab-globe)" stroke="#0c4a6e" strokeWidth="1.2" />
    <ellipse cx="16" cy="16" rx="6" ry="13" fill="none" stroke="#e0f2fe" strokeWidth="0.8" />
    <line x1="3" y1="16" x2="29" y2="16" stroke="#e0f2fe" strokeWidth="0.8" />
    <path d="M12 22l8-11v5h5l-8 11v-5h-5z" fill="#facc15" stroke="#a16207" strokeWidth="1" />
  </svg>
);

export const IdmIconDeleteCompleted = ({ size = 32 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 32 32">
    <defs>
      <linearGradient id="idm-delc-grad" x1="0" y1="0" x2="0" y2="1">
        <stop offset="0%" stopColor="#f87171" />
        <stop offset="50%" stopColor="#dc2626" />
        <stop offset="100%" stopColor="#991b1b" />
      </linearGradient>
    </defs>
    {/* Red trash bin with checkmark */}
    <path d="M7 10h18l-2 19H9z" fill="url(#idm-delc-grad)" stroke="#7f1d1d" strokeWidth="1" />
    <path d="M5 7h22v3H5zM12 4h8v3h-8z" fill="#b91c1c" stroke="#7f1d1d" strokeWidth="1" />
    <polyline points="11,19 14,22 21,15" fill="none" stroke="#ffffff" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" />
  </svg>
);

export const IdmIconStartQueue = ({ size = 32 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 32 32">
    <defs>
      <linearGradient id="idm-doc-blue" x1="0" y1="0" x2="0" y2="1">
        <stop offset="0%" stopColor="#93c5fd" />
        <stop offset="100%" stopColor="#2563eb" />
      </linearGradient>
    </defs>
    {/* Double page document with green play badge */}
    <rect x="5" y="4" width="13" height="18" rx="1" fill="#e0e7ff" stroke="#4338ca" strokeWidth="1" />
    <rect x="9" y="8" width="14" height="19" rx="1" fill="url(#idm-doc-blue)" stroke="#1e3a8a" strokeWidth="1" />
    <path d="M19 19l8 5-8 5z" fill="#22c55e" stroke="#15803d" strokeWidth="1" />
  </svg>
);

export const IdmIconStopQueue = ({ size = 32 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 32 32">
    <defs>
      <linearGradient id="idm-doc-purple" x1="0" y1="0" x2="0" y2="1">
        <stop offset="0%" stopColor="#e9d5ff" />
        <stop offset="100%" stopColor="#9333ea" />
      </linearGradient>
    </defs>
    {/* Double page document with red stop square */}
    <rect x="5" y="4" width="13" height="18" rx="1" fill="#f3e8ff" stroke="#7e22ce" strokeWidth="1" />
    <rect x="9" y="8" width="14" height="19" rx="1" fill="url(#idm-doc-purple)" stroke="#581c87" strokeWidth="1" />
    <rect x="19" y="20" width="8" height="8" rx="1" fill="#ef4444" stroke="#991b1b" strokeWidth="1" />
  </svg>
);

export const IdmIconTellAFriend = ({ size = 32 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 32 32">
    <defs>
      <linearGradient id="idm-thumbs" x1="0" y1="0" x2="0" y2="1">
        <stop offset="0%" stopColor="#fde047" />
        <stop offset="60%" stopColor="#eab308" />
        <stop offset="100%" stopColor="#ca8a04" />
      </linearGradient>
    </defs>
    {/* Thumbs up badge */}
    <circle cx="16" cy="16" r="13" fill="#ecfccb" stroke="#65a30d" strokeWidth="1" />
    <path d="M10 14h3v10h-3zM13 18l3-8a2 2 0 0 1 2 2v2h5a2 2 0 0 1 2 2.3l-1.5 6a2 2 0 0 1-2 1.7H13z" fill="url(#idm-thumbs)" stroke="#854d0e" strokeWidth="1" />
  </svg>
);

export const IdmQueueClock = ({ size = 16 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 16 16">
    <circle cx="8" cy="8" r="6.5" fill="#fef08a" stroke="#ca8a04" strokeWidth="1" />
    <circle cx="8" cy="8" r="5" fill="#ffffff" stroke="#eab308" strokeWidth="0.6" />
    <circle cx="8" cy="8" r="1" fill="#dc2626" />
    <polyline points="8,4 8,8 11,8" fill="none" stroke="#1f2937" strokeWidth="1.2" strokeLinecap="round" />
  </svg>
);
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
  image: "text-cyan-400",
  disk: "text-orange-400",
};

export const FileIconGeneric = ({ size = 16, className }: FP) => (
  <svg {...fileBase(size)} className={className}>
    <path d="M6 3h8l4 4v14H6z" />
    <path d="M14 3v4h4" />
  </svg>
);

export const FileIconVideo = ({ size = 16 }: FP) => (
  <svg width={size} height={size} viewBox="0 0 16 16">
    {/* Filmstrip / video reel */}
    <rect x="2" y="2" width="12" height="12" rx="1" fill="#475569" stroke="#1e293b" strokeWidth="1" />
    <rect x="4" y="4" width="8" height="8" fill="#0f172a" />
    <polygon points="6,6 11,8 6,10" fill="#38bdf8" />
    <rect x="3" y="3" width="1.5" height="1.5" fill="#f8fafc" />
    <rect x="3" y="11.5" width="1.5" height="1.5" fill="#f8fafc" />
    <rect x="11.5" y="3" width="1.5" height="1.5" fill="#f8fafc" />
    <rect x="11.5" y="11.5" width="1.5" height="1.5" fill="#f8fafc" />
  </svg>
);

export const FileIconAudio = ({ size = 16 }: FP) => (
  <svg width={size} height={size} viewBox="0 0 16 16">
    {/* Audio note */}
    <circle cx="5" cy="11.5" r="2.5" fill="#ca8a04" stroke="#854d0e" strokeWidth="0.8" />
    <circle cx="11" cy="9.5" r="2.5" fill="#ca8a04" stroke="#854d0e" strokeWidth="0.8" />
    <path d="M7.5 11.5V4l6-1.5v7" fill="none" stroke="#854d0e" strokeWidth="1.5" strokeLinecap="round" />
    <polygon points="7.5,4 13.5,2.5 13.5,4.5 7.5,6" fill="#eab308" />
  </svg>
);

export const FileIconArchive = ({ size = 16 }: FP) => (
  <svg width={size} height={size} viewBox="0 0 16 16">
    {/* Zip package / folder */}
    <rect x="2" y="3" width="12" height="10.5" rx="0.5" fill="#fbbf24" stroke="#b45309" strokeWidth="1" />
    <line x1="8" y1="3" x2="8" y2="13.5" stroke="#78350f" strokeWidth="1.2" strokeDasharray="1,1" />
    <rect x="6.5" y="8" width="3" height="3" fill="#d97706" stroke="#78350f" strokeWidth="0.8" />
  </svg>
);

export const FileIconDocument = ({ size = 16 }: FP) => (
  <svg width={size} height={size} viewBox="0 0 16 16">
    {/* Classic document */}
    <path d="M3 1.5h7l3.5 3.5V14.5H3z" fill="#f8fafc" stroke="#64748b" strokeWidth="1" />
    <polygon points="10,1.5 10,5 13.5,5" fill="#cbd5e1" stroke="#64748b" strokeWidth="0.8" />
    <line x1="5" y1="7" x2="11" y2="7" stroke="#94a3b8" strokeWidth="1" />
    <line x1="5" y1="9" x2="11" y2="9" stroke="#94a3b8" strokeWidth="1" />
    <line x1="5" y1="11" x2="9" y2="11" stroke="#94a3b8" strokeWidth="1" />
  </svg>
);

export const FileIconProgram = ({ size = 16 }: FP) => (
  <svg width={size} height={size} viewBox="0 0 16 16">
    {/* Win32 window application icon */}
    <rect x="2" y="2" width="12" height="12" rx="0.5" fill="#f1f5f9" stroke="#334155" strokeWidth="1" />
    <rect x="2" y="2" width="12" height="3.5" fill="#2563eb" />
    <rect x="4" y="8" width="3" height="3" fill="#3b82f6" />
    <rect x="9" y="8" width="3" height="3" fill="#ef4444" />
  </svg>
);

export const FileIconImage = ({ size = 16 }: FP) => (
  <svg width={size} height={size} viewBox="0 0 16 16">
    {/* Photo picture icon */}
    <rect x="2" y="2" width="12" height="12" rx="0.5" fill="#ecfdf5" stroke="#059669" strokeWidth="1" />
    <circle cx="5" cy="5" r="1.2" fill="#fbbf24" />
    <polygon points="3,12 7,7 10,10 13,6 13,12" fill="#10b981" />
  </svg>
);

export const FileIconDisk = ({ size = 16 }: FP) => (
  <svg width={size} height={size} viewBox="0 0 16 16">
    {/* Optical disk */}
    <circle cx="8" cy="8" r="6" fill="#e2e8f0" stroke="#475569" strokeWidth="1" />
    <circle cx="8" cy="8" r="2" fill="#ffffff" stroke="#475569" strokeWidth="0.8" />
    <circle cx="8" cy="8" r="0.8" fill="#475569" />
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
