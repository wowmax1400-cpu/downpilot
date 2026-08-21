// DownPilot Chrome extension background service worker.
// Manages native messaging host connection, context menus, download
// interception, and forwards analyze/download requests from content scripts.

const NMH_NAME = "com.downpilot.nmh";

// ---- State -----------------------------------------------------------------
let nmhPort = null;
let pendingRequests = new Map();
let requestId = 0;
// Download interception is ON by default (IDM behavior). Users who want
// Chrome's own downloads back can untick it in the popup.
const settings = { autoIntercept: true };

// Restore persisted settings (user's explicit choice wins)
chrome.storage.local.get("autoIntercept", (data) => {
  if (typeof data.autoIntercept === "boolean") {
    settings.autoIntercept = data.autoIntercept;
  } else {
    // First run: default ON, persist it.
    chrome.storage.local.set({ autoIntercept: true });
  }
});

// ---- Native messaging ------------------------------------------------------
function connect() {
  try {
    nmhPort = chrome.runtime.connectNative(NMH_NAME);
    nmhPort.onMessage.addListener((msg) => {
      const id = msg._reqId;
      if (id != null && pendingRequests.has(id)) {
        const { resolve, reject } = pendingRequests.get(id);
        pendingRequests.delete(id);
        if (msg.ok) resolve(msg);
        else reject(new Error(msg.error || "native message failed"));
      }
    });
    nmhPort.onDisconnect.addListener(() => {
      nmhPort = null;
      // Reject all pending requests
      for (const [id, { reject }] of pendingRequests) {
        reject(new Error("DownPilot app disconnected"));
      }
      pendingRequests.clear();
      // Retry connection in 3s
      setTimeout(connect, 3000);
    });
  } catch (e) {
    console.warn("DownPilot NMH connection failed:", e);
    setTimeout(connect, 5000);
  }
}
connect();

function sendMessage(msg) {
  return new Promise((resolve, reject) => {
    const id = ++requestId;
    msg._reqId = id;
    pendingRequests.set(id, { resolve, reject });
    try {
      if (nmhPort) {
        nmhPort.postMessage(msg);
      } else {
        reject(new Error("DownPilot not connected"));
      }
    } catch (e) {
      reject(e);
    }
  });
}

// ---- Context menus ---------------------------------------------------------
chrome.runtime.onInstalled.addListener(() => {
  chrome.contextMenus.create({
    id: "downpilot-link",
    title: "Download link with DownPilot",
    contexts: ["link"],
  });
  chrome.contextMenus.create({
    id: "downpilot-video",
    title: "Download video with DownPilot",
    contexts: ["video"],
  });
  chrome.contextMenus.create({
    id: "downpilot-page",
    title: "Grab media from this page",
    contexts: ["page"],
  });
});

chrome.contextMenus.onClicked.addListener((info, tab) => {
  const url = info.linkUrl || info.srcUrl || tab?.url;
  if (!url) return;
  if (info.menuItemId === "downpilot-page") {
    // Analyze page URL
    sendMessage({ type: "analyze", url })
      .then(() => {})
      .catch(() => {});
  } else {
    sendMessage({ type: "download", url, referer: tab?.url })
      .then(() => showNotification("Download added", url))
      .catch(() => showNotification("Failed", "Check DownPilot is running"));
  }
});

// ---- Download interception (takeover) -------------------------------------
// When enabled, Chrome's own download is CANCELLED and erased from the shelf,
// then the URL is handed to DownPilot so it downloads with the multi-threaded
// engine instead (paused/resumable, speed-limited). This is the classic
// IDM/XDM takeover pattern.
chrome.downloads.onCreated.addListener((item) => {
  if (!settings.autoIntercept || !item.url) return;
  // Never intercept our own handoff — DownPilot downloads via the engine,
  // not through chrome.downloads, so there's no loop, but guard blob:/data:
  // URLs which the engine can't re-fetch (no stable HTTP URL to resume).
  if (item.url.startsWith("blob:") || item.url.startsWith("data:")) return;

  // Cancel Chrome's download first so it doesn't double-download.
  chrome.downloads.cancel(item.id, () => {
    // Remove the cancelled entry from the download shelf (no red X remnant).
    // erase() takes a DownloadQuery object, not a bare ID.
    chrome.downloads.erase({ id: item.id }, () => {
      if (chrome.runtime.lastError) {
        // Download may not be erased if still mid-cancel; harmless — the
        // shelf entry disappears on its own once the state settles.
      }
    });
  });

  // Hand the URL to DownPilot.
  sendMessage({
    type: "download",
    url: item.url,
    referer: item.referrer || undefined,
    user_agent: item.userAgent || undefined,
  })
    .then(() => {
      showNotification("Download intercepted", item.filename || item.url);
    })
    .catch(() => {
      // DownPilot isn't running — restore Chrome's download so the user
      // doesn't lose the file. (cancel may have already fired; re-initiate
      // only if the original is gone.)
      showNotification("DownPilot not running", "Using Chrome download instead");
      chrome.downloads.search({ id: item.id }, (results) => {
        const dl = results[0];
        if (dl && (dl.state === "interrupted" || dl.state === "cancelled")) {
          chrome.downloads.download({ url: item.url, filename: item.filename, saveAs: false }).catch(() => {});
        }
      });
    });
});

// ---- Content script RPC ----------------------------------------------------
chrome.runtime.onMessage.addListener((msg, sender, sendResponse) => {
  if (msg.action === "setAutoIntercept") {
    settings.autoIntercept = !!msg.value;
    chrome.storage.local.set({ autoIntercept: settings.autoIntercept });
    sendResponse({ ok: true });
    return;
  }
  if (msg.action === "analyze") {
    sendMessage({
      type: "analyze",
      url: msg.url,
      cookies: msg.cookies,
      referer: msg.referer,
      user_agent: msg.user_agent,
    })
      .then((resp) => sendResponse({ ok: true, media: resp.data }))
      .catch((err) => sendResponse({ ok: false, error: err.message }));
    return true; // keep channel open for async reply
  }
  if (msg.action === "download") {
    sendMessage({
      type: "download",
      url: msg.url,
      format_id: msg.format_id,
      referer: msg.referer,
      user_agent: msg.user_agent,
      cookies: msg.cookies,
    })
      .then((resp) => sendResponse({ ok: true, id: resp.data?.id }))
      .catch((err) => sendResponse({ ok: false, error: err.message }));
    return true;
  }
  if (msg.action === "status") {
    sendResponse({ connected: nmhPort !== null });
  }
});

function showNotification(title, message) {
  chrome.notifications.create({
    type: "basic",
    iconUrl: "icons/128.png",
    title,
    message,
  });
}