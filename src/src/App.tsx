import { useCallback, useEffect, useState } from "react";
import { useDownloads } from "./store/useDownloads";
import { Sidebar } from "./components/Sidebar";
import { Toolbar } from "./components/Toolbar";
import { DownloadTable } from "./components/DownloadTable";
import { StatusBar } from "./components/StatusBar";
import { AddDownloadDialog } from "./components/AddDownloadDialog";
import { ThemePicker } from "./components/ThemePicker";
import { SettingsPanel } from "./components/SettingsPanel";
import { SiteGrabberDialog } from "./components/SiteGrabberDialog";
import { ClipboardPrompt } from "./components/ClipboardPrompt";
import { applyTheme } from "./themes";
import * as api from "./lib/api";
import type { GrabbedLink } from "./types";

export default function App() {
  const store = useDownloads();
  const [addOpen, setAddOpen] = useState(false);
  const [initialUrl, setInitialUrl] = useState<string | undefined>();
  const [themeOpen, setThemeOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [grabberOpen, setGrabberOpen] = useState(false);
  const [clipboardUrl, setClipboardUrl] = useState<string | null>(null);
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);

  // Apply persisted theme once settings arrive from the backend
  useEffect(() => {
    if (store.settings) {
      applyTheme(store.settings.theme, store.settings.accent || undefined);
    }
  }, [store.settings]);

  // Clipboard URL monitor (Tauri emits "downloads://clipboard")
  useEffect(() => {
    const unsub = api.subscribeToClipboard((url) => {
      setClipboardUrl((prev) => (prev === url ? prev : url));
    });
    return () => {
      unsub();
    };
  }, []);

  // Drag & drop URL onto the main area
  const handleDrop = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    const text =
      e.dataTransfer.getData("text/uri-list") ||
      e.dataTransfer.getData("text/plain");
    const url = text.trim().split("\n")[0].trim();
    if (url && (url.startsWith("http://") || url.startsWith("https://"))) {
      setInitialUrl(url);
      setAddOpen(true);
    }
  }, []);
  const handleDragOver = useCallback((e: React.DragEvent) => {
    e.preventDefault();
  }, []);

  // Open the Add dialog with a specific URL (from clipboard prompt)
  const handleOpenAdd = useCallback((url: string) => {
    setInitialUrl(url);
    setAddOpen(true);
  }, []);

  // Add selected links from site grabber
  const handleGrabAdd = useCallback(
    async (links: GrabbedLink[]) => {
      for (const l of links) {
        await store.add({
          url: l.url,
          filename: l.filename,
          start_immediately: true,
        });
      }
    },
    [store]
  );

  return (
    <div
      className="flex h-full flex-col bg-bg text-txt"
      onDrop={handleDrop}
      onDragOver={handleDragOver}
    >
      {/* Distinct application header — full width, above sidebar */}
      <Toolbar
        store={store}
        onAdd={() => {
          setInitialUrl(undefined);
          setAddOpen(true);
        }}
        onOpenTheme={() => setThemeOpen(true)}
        onOpenGrabber={() => setGrabberOpen(true)}
      />

      <div className="flex min-h-0 flex-1">
        <Sidebar
          store={store}
          collapsed={sidebarCollapsed}
          onToggleCollapse={() => setSidebarCollapsed((c) => !c)}
          onOpenSettings={() => setSettingsOpen(true)}
        />
        {/* Main content: card container floats on the background */}
        <main className="flex min-w-0 flex-1 flex-col p-4">
          <DownloadTable store={store} onAdd={() => setAddOpen(true)} />
        </main>
      </div>

      <StatusBar store={store} />

      {/* Clipboard prompt toast */}
      <ClipboardPrompt
        url={clipboardUrl}
        onDismiss={() => setClipboardUrl(null)}
        onOpen={handleOpenAdd}
      />

      {addOpen && (
        <AddDownloadDialog
          store={store}
          onClose={() => setAddOpen(false)}
          initialUrl={initialUrl}
        />
      )}
      {themeOpen && (
        <ThemePicker store={store} onClose={() => setThemeOpen(false)} />
      )}
      {settingsOpen && (
        <SettingsPanel store={store} onClose={() => setSettingsOpen(false)} />
      )}
      {grabberOpen && (
        <SiteGrabberDialog
          open={grabberOpen}
          onClose={() => setGrabberOpen(false)}
          onGrab={store.grabSite}
          onAdd={handleGrabAdd}
        />
      )}
    </div>
  );
}