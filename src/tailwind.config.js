/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        // All colors resolve to CSS custom properties so the multi-theme
        // engine can swap entire palettes at runtime (see themes.ts + index.css)
        bg: "var(--bg)",
        surface: "var(--surface)",
        "surface-2": "var(--surface-2)",
        "surface-3": "var(--surface-3)",
        line: "var(--line)",
        txt: "var(--text)",
        "txt-dim": "var(--text-dim)",
        "txt-faint": "var(--text-faint)",
        accent: "var(--accent)",
        "accent-soft": "var(--accent-soft)",
        "accent-text": "var(--accent-text)",
        success: "var(--success)",
        warning: "var(--warning)",
        danger: "var(--danger)",
        info: "var(--info)",
        "progress-track": "var(--progress-track)",
      },
      fontFamily: {
        sans: ["Inter", "ui-sans-serif", "system-ui", "sans-serif"],
        mono: ["'JetBrains Mono'", "ui-monospace", "SFMono-Regular", "monospace"],
      },
      fontSize: {
        xs: ["11px", "15px"],
        sm: ["12px", "17px"],
        base: ["13px", "19px"],
        lg: ["15px", "21px"],
      },
    },
  },
  plugins: [],
};
