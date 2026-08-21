import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { applyTheme } from "./themes";
import "./index.css";

// Load persisted theme before first paint to avoid a flash of the default
// theme. The real value comes from the backend in the Tauri app; in browser
// preview we read localStorage directly.
(function initTheme() {
  const saved = localStorage.getItem("dp.theme");
  const accent = localStorage.getItem("dp.accent");
  if (saved || accent) {
    applyTheme(saved || "deep-midnight", accent || undefined);
  }
})();

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
