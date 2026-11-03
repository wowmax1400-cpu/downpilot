import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { applyTheme } from "./themes";
import { I18nProvider } from "./i18n";
import "./index.css";

// Initialize theme from persisted setting or default to deep-midnight
(function initTheme() {
  const saved = localStorage.getItem("downpilot.theme") || "deep-midnight";
  applyTheme(saved);
})();

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <I18nProvider>
      <App />
    </I18nProvider>
  </React.StrictMode>
);
