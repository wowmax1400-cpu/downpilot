// English strings — the source of truth for the i18n key shape.
// Keys are grouped by area; fa.ts mirrors this object exactly.

export const en = {
  // ---- Menu bar ----
  "menu.file": "File",
  "menu.downloads": "Downloads",
  "menu.view": "View",
  "menu.tasks": "Tasks",
  "menu.tools": "Tools",
  "menu.help": "Help",

  "menu.file.newDownload": "New Download…",
  "menu.file.addBatch": "Add Batch Download…",
  "menu.file.import": "Import Downloads…",
  "menu.file.export": "Export Downloads…",
  "menu.file.viewHistory": "View History",
  "menu.file.close": "Close",

  "menu.downloads.resume": "Resume",
  "menu.downloads.pause": "Pause",
  "menu.downloads.stop": "Stop",
  "menu.downloads.forceStart": "Force Start",
  "menu.downloads.retry": "Retry",
  "menu.downloads.redownload": "Redownload",
  "menu.downloads.openFile": "Open File",
  "menu.downloads.openFolder": "Open Folder",
  "menu.downloads.copyUrl": "Copy URL",
  "menu.downloads.copyPath": "Copy Path",
  "menu.downloads.rename": "Rename",
  "menu.downloads.moveToQueue": "Move to Queue…",
  "menu.downloads.changeCategory": "Change Category…",
  "menu.downloads.properties": "Properties",
  "menu.downloads.delete": "Delete",
  "menu.downloads.deleteWithFile": "Delete + File",

  "menu.view.all": "All Downloads",
  "menu.view.unfinished": "Unfinished",
  "menu.view.finished": "Finished",
  "menu.view.paused": "Paused",
  "menu.view.errors": "Errors",

  "menu.tasks.addUrl": "Add URL…",
  "menu.tasks.batchDownload": "Batch Download…",
  "menu.tasks.grabber": "Grabber…",
  "menu.tasks.siteManager": "Site Manager…",
  "menu.tasks.categories": "Categories…",
  "menu.tasks.scheduler": "Scheduler…",
  "menu.tasks.queueManager": "Queue Manager…",
  "menu.tasks.startAll": "Start All",
  "menu.tasks.pauseAll": "Pause All",
  "menu.tasks.stopAll": "Stop All",
  "menu.tasks.retryAllFailed": "Retry All Failed",
  "menu.tasks.clearFinished": "Clear Finished",

  "menu.tools.options": "Options…",
  "menu.tools.browserIntegration": "Browser Integration…",
  "menu.tools.about": "About DownPilot",

  "menu.help.shortcuts": "Keyboard Shortcuts",
  "menu.help.about": "About DownPilot",

  "menu.search.placeholder": "Search filename, URL, domain… (Ctrl+F)",
  "menu.search.clear": "Clear search",

  // ---- Prompts / confirms (App + MenuBar) ----
  "prompt.renameTo": "Rename to:",
  "prompt.moveToQueue": "Move to queue (name):",
  "prompt.changeCategory": "Change category:",
  "confirm.deleteFile": "and the on-disk file?",
  "confirm.deleteAllCompleted": "Delete all completed downloads?",

  // ---- Keyboard shortcuts dialog (Help menu) ----
  "shortcuts.title": "DownPilot keyboard shortcuts",
  "shortcuts.newDownload": "New download",
  "shortcuts.focusSearch": "Focus search",
  "shortcuts.selectAll": "Select all",
  "shortcuts.properties": "Properties",
  "shortcuts.options": "Options",
  "shortcuts.openOrProps": "Open / properties",
  "shortcuts.removeSelected": "Remove selected",
  "shortcuts.rename": "Rename",
  "shortcuts.pauseResume": "Pause/Resume",
  "shortcuts.doubleClickHint": "Open file if done, else Properties",

  // ---- Toolbar ----
  "toolbar.addUrl": "Add URL",
  "toolbar.addUrl.title": "Add a new download (Ctrl+N)",
  "toolbar.grabber": "Grabber",
  "toolbar.resume": "Resume",
  "toolbar.start": "Start",
  "toolbar.pause": "Pause",
  "toolbar.stop": "Stop",
  "toolbar.forceStart": "Force Start",
  "toolbar.delete": "Delete",
  "toolbar.stopAll": "Stop All",
  "toolbar.deleteAll": "Delete All",
  "toolbar.options": "Options",
  "toolbar.scheduler": "Scheduler",
  "toolbar.queues": "Queues",
  "toolbar.bandwidth.unlimited": "Unlimited",
  "toolbar.bandwidth.title": "Global bandwidth cap (applies to all downloads)",

  // ---- Sidebar ----
  "sidebar.section.categories": "Categories",
  "sidebar.section.grabber": "Grabber",
  "sidebar.section.queues": "Queues",
  "sidebar.allDownloads": "All Downloads",
  "sidebar.unfinished": "Unfinished",
  "sidebar.finished": "Finished",
  "sidebar.paused": "Paused",
  "sidebar.errors": "Errors",
  "sidebar.siteGrabber": "Site Grabber",
  "sidebar.noQueues": "No queues yet",
  "sidebar.queued": "queued",

  // ---- Status bar ----
  "status.downloads": "downloads",
  "status.paused": "paused",
  "status.active": "active",
  "status.remaining": "remaining",
  "status.cap": "cap",
  "status.unlimited": "Unlimited",
  "status.cappedAt": "Capped at {speed}",

  // ---- Drop overlay ----
  "drop.title": "Drop URL or URL-list file",
  "drop.sub1": "Drop a single URL to open Add Download.",
  "drop.sub2": "Drop multiple URLs or a .txt list to open Batch.",

  // ---- Common ----
  "common.close": "Close",
  "common.cancel": "Cancel",
  "common.save": "Save",
  "common.edit": "Edit",
  "common.delete": "Del",
  "common.open": "Open",
  "common.builtIn": "built-in",
  "common.none": "(none)",
  "common.default": "(default)",
  "common.global": "(global)",
  "common.useDefault": "(use default)",

  // ---- Settings dialog ----
  "settings.title": "Options",
  "settings.search.placeholder": "Search settings...",
  "settings.noMatch": "No settings match",
  "settings.section.general": "General",
  "settings.section.downloads": "Downloads",
  "settings.section.connections": "Connections",
  "settings.section.proxy": "Proxy",
  "settings.section.browser": "Browser Integration",
  "settings.section.clipboard": "Clipboard",
  "settings.section.completion": "Completion Actions",
  "settings.section.notifications": "Notifications",
  "settings.section.categories": "Categories",
  "settings.section.schedules": "Schedules",
  "settings.section.schedulesFull": "Schedules & Bandwidth Profiles",
  "settings.section.capture": "Browser Capture",
  "settings.section.sites": "Site Rules",
  "settings.section.language": "Language",

  "settings.general.downloadDir": "Default download folder",
  "settings.general.theme": "Theme",
  "settings.general.accent": "Accent color",

  "settings.downloads.defaultThreads": "Default connections per download",
  "settings.downloads.maxThreads": "Max connections per download",
  "settings.downloads.maxSimultaneous": "Max simultaneous downloads",
  "settings.downloads.speedLimit": "Global speed limit (kbps, 0=unlimited)",

  "settings.connections.connTimeout": "Connection timeout (ms)",
  "settings.connections.readTimeout": "Read timeout (ms)",
  "settings.connections.maxRetries": "Max retries",
  "settings.connections.retryDelay": "Retry delay (ms)",

  "settings.proxy.url": "Proxy URL (http/https/socks5)",
  "settings.proxy.urlHint": "Blank = no proxy",

  "settings.browser.autoIntercept": "Automatically capture browser downloads",
  "settings.browser.ytdlp": "Use yt-dlp for media links",

  "settings.clipboard.monitor": "Monitor clipboard for download URLs",

  "settings.completion.defaultAction": "Default action",

  "settings.notifications.scheduleEnabled": "Enable scheduled windows",
  "settings.notifications.scheduleStart": "Schedule start (HH:MM)",
  "settings.notifications.scheduleEnd": "Schedule end (HH:MM)",

  // ---- Language switcher (inside General section) ----
  "settings.language.label": "Language",
  "settings.language.hint": "Interface language. Persian flips the layout to right-to-left.",
  "settings.language.en": "English",
  "settings.language.fa": "فارسی",

  // ---- Dialog chrome ----
  "dialog.close": "Close",
};

export type StringKey = keyof typeof en;
