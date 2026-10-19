// DownPilot Chrome extension — background service worker.
//
// Design (IDM-class, our own branding):
//  - Single source of truth for capture rules lives in the DownPilot app
//    (Settings → Browser Integration). The extension fetches it via the
//    native host `config` message and caches it (refreshed every 15s + on
//    demand).
//  - Capture mode ("off" | "ask" | "auto") comes from the app's
//    `capture_mode` setting; the legacy `auto_intercept` bool is honoured
//    only as a fallback for older app builds.
//  - Auto mode: cancel Chrome's own download and hand the URL to the
//    engine. If the app is down, restore Chrome's download so the user
//    never loses the file.
//  - Ask mode: notification with three buttons — Download, Ignore, Always
//    (Always records a per-domain "skip capture" so the next download from
//    this site is silenced).
//  - Duplicate prevention: the extension tracks every browser download id
//    and only forwards once, even on Chrome retry.
//  - Cookie handling: the extension forwards `document.cookie` only when
//    the user has opted in via the ask-mode notification and never logs
//    it. The backend stores the cookie string in the existing
//    `cookies` column of `downloads`; it is never echoed back through
//    the NMH protocol.

const NMH_NAME = "com.downpilot.nmh";

const config = {
  mode: "ask", // "off" | "ask" | "auto"
  excludeDomains: [],
  excludeExtensions: [],
  allowedExtensions: [],   // When non-empty, ONLY these are captured.
  alwaysAllowExtensions: new Set(), // Persisted in the app via the NMH protocol.
  userPaused: false,
};
let configLoaded = false;

// ---- Native messaging -------------------------------------------------------
let nmhPort = null;
const pendingRequests = new Map();
let requestId = 0;

function connect() {
  try {
    nmhPort = chrome.runtime.connectNative(NMH_NAME);
    nmhPort.onMessage.addListener((msg) => {
      // Accept both `_req_id` (current) and `_reqId` (legacy) so a
      // version mismatch does not silently hang the user.
      const id = msg && (msg._req_id != null ? msg._req_id : msg._reqId);
      if (id != null && pendingRequests.has(id)) {
        const { resolve, reject } = pendingRequests.get(id);
        pendingRequests.delete(id);
        if (msg.ok) resolve(msg);
        else reject(new Error(msg.error || "native message failed"));
      }
    });
    nmhPort.onDisconnect.addListener(() => {
      nmhPort = null;
      for (const [, { reject }] of pendingRequests) {
        reject(new Error("DownPilot app disconnected"));
      }
      pendingRequests.clear();
      configLoaded = false;
      setTimeout(connect, 3000);
    });
  } catch (e) {
    console.warn("DownPilot: NMH connect failed:", e);
    nmhPort = null;
    setTimeout(connect, 5000);
  }
}
connect();

// Default per-request deadline. Probe messages (browserDownloadDetected)
// can take up to PROBE_TIMEOUT_MS on the Rust side, so we leave headroom
// above that. 8s is enough for any single round-trip; longer-running
// operations (downloads) report completion via a separate event.
const SEND_TIMEOUT_MS = 8000;
const SEND_RETRIES = 1; // one bounded retry on timeout/disconnect

function sendMessage(msg) {
  let attempt = 0;
  const type = msg && msg.type ? msg.type : "?";
  const run = () =>
    new Promise((resolve, reject) => {
      const id = ++requestId;
      msg._req_id = id;
      const t = setTimeout(() => {
        if (pendingRequests.has(id)) {
          pendingRequests.delete(id);
          reject(new Error("DownPilot request timed out"));
        }
      }, SEND_TIMEOUT_MS);
      pendingRequests.set(id, {
        resolve: (v) => {
          clearTimeout(t);
          resolve(v);
        },
        reject: (e) => {
          clearTimeout(t);
          reject(e);
        },
      });
      try {
        if (nmhPort) nmhPort.postMessage(msg);
        else {
          pendingRequests.delete(id);
          clearTimeout(t);
          reject(new Error("DownPilot not connected"));
        }
      } catch (e) {
        pendingRequests.delete(id);
        clearTimeout(t);
        reject(e);
      }
    });
  return (async () => {
    let lastErr;
    for (attempt = 0; attempt <= SEND_RETRIES; attempt++) {
      try {
        return await run();
      } catch (e) {
        lastErr = e;
        // Reconnect-on-failure: the disconnect listener already fired,
        // but make sure we have a fresh port before retrying.
        if (!nmhPort) connect();
      }
    }
    throw lastErr || new Error("DownPilot request failed");
  })();
}

function splitList(s) {
  if (!s || typeof s !== "string") return [];
  return s.split(",").map((x) => x.trim().toLowerCase()).filter(Boolean);
}

// ---- Config sync from the app ---------------------------------------------
async function refreshConfig(force = false) {
  if (!force && configLoaded) return config;
  try {
    const resp = await sendMessage({ type: "config" });
    if (resp && resp.ok && resp.data) {
      const d = resp.data;
      if (typeof d.capture_mode === "string") {
        config.mode = d.capture_mode;
      } else if (d.auto_intercept === true) {
        config.mode = "auto";
      } else if (d.auto_intercept === false) {
        config.mode = "off";
      }
      config.excludeDomains = splitList(d.exclude_domains);
      config.excludeExtensions = splitList(d.exclude_extensions).map((e) =>
        e.replace(/^\./, "").toLowerCase()
      );
      config.allowedExtensions = splitList(d.allowed_extensions).map((e) =>
        e.replace(/^\./, "").toLowerCase()
      );
      config.alwaysAllowExtensions = new Set(
        splitList(d.always_allow_extensions).map((e) => e.replace(/^\./, "").toLowerCase())
      );
      configLoaded = true;
    }
  } catch (e) {
    // App unreachable; keep the cached config (or the defaults).
  }
  return config;
}

async function ensureConfig() {
  if (configLoaded) return config;
  return refreshConfig(true);
}

// ---- Capture decision ------------------------------------------------------
function domainOf(url) {
  try {
    return new URL(url).hostname.replace(/^www\./, "").toLowerCase();
  } catch {
    return "";
  }
}
function extOf(url) {
  try {
    const p = new URL(url).pathname;
    const m = p.match(/\.([a-z0-9]{1,6})$/i);
    return m ? m[1].toLowerCase() : "";
  } catch {
    return "";
  }
}

// Hostname-aware matching. A pattern of `example.com` matches the host
// AND any subdomain (`www.example.com`, `cdn.example.com`) but NOT
// `notexample.com`. A leading `.` is allowed and means the same thing
// explicitly. Empty / invalid patterns are ignored.
function hostMatches(host, pattern) {
  if (!host || !pattern) return false;
  const h = host.toLowerCase();
  const p = pattern.toLowerCase();
  if (p.startsWith(".")) return h === p.slice(1) || h.endsWith(p);
  return h === p || h.endsWith("." + p);
}

function isExcluded(url) {
  const dom = domainOf(url);
  const ext = extOf(url);
  if (ext && config.excludeExtensions.includes(ext)) return true;
  for (const d of config.excludeDomains) {
    if (hostMatches(dom, d)) return true;
  }
  return false;
}

// Allow-list: when non-empty, ONLY these extensions are captured. A
// download whose extension is not on the list is treated as excluded.
function isAllowListed(url) {
  if (config.allowedExtensions.length === 0) return true;
  const ext = extOf(url);
  if (!ext) return true; // unknown ext: don't auto-block
  return config.allowedExtensions.includes(ext);
}

// Always-allow: persistent in the app via the NMH `set_always_allow`
// message. Bypasses ask-mode and captures immediately.
function isAlwaysAllow(url) {
  const ext = extOf(url);
  if (ext && config.alwaysAllowExtensions.has(ext)) return true;
  return false;
}

// Always-skip set, populated by the ask-notification "Always" button.
// Keyed by `${domain}` or `${ext}`. Persisted across restarts via
// chrome.storage so the choice survives browser restarts.
const alwaysSkip = { domains: new Set(), exts: new Set() };
chrome.storage?.local.get?.(["alwaysSkip"], (v) => {
  if (v && v.alwaysSkip) {
    alwaysSkip.domains = new Set(v.alwaysSkip.domains || []);
    alwaysSkip.exts = new Set(v.alwaysSkip.exts || []);
  }
});
function persistAlwaysSkip() {
  chrome.storage?.local?.set?.({
    alwaysSkip: {
      domains: Array.from(alwaysSkip.domains),
      exts: Array.from(alwaysSkip.exts),
    },
  });
}

function isAlwaysSkipped(url) {
  const d = domainOf(url);
  const e = extOf(url);
  if (e && alwaysSkip.exts.has(e)) return true;
  if (d && alwaysSkip.domains.has(d)) return true;
  return false;
}

async function decide(url) {
  await ensureConfig();
  if (config.userPaused) return "skip";
  if (config.mode === "off") return "skip";
  if (!isAllowListed(url)) return "skip";
  if (isExcluded(url)) return "skip";
  if (isAlwaysAllow(url)) return "capture";
  if (config.mode === "auto") return "capture";
  return "ask";
}

// ---- Duplicate prevention --------------------------------------------------
// Once we hand a download off to DownPilot, Chrome retries the same URL
// fire a fresh `onCreated` event. We keep a short-lived set of seen
// browser ids + URLs to suppress the second import. Cleared after a minute.
const seenCaptures = new Map(); // key -> ts
function markCaptured(key) {
  seenCaptures.set(key, Date.now());
}
function wasCaptured(key) {
  const t = seenCaptures.get(key);
  if (!t) return false;
  if (Date.now() - t > 60_000) {
    seenCaptures.delete(key);
    return false;
  }
  return true;
}
setInterval(() => {
  const now = Date.now();
  for (const [k, t] of seenCaptures) {
    if (now - t > 60_000) seenCaptures.delete(k);
  }
}, 15_000);

// ---- Chrome download takeover ---------------------------------------------
chrome.downloads.onCreated.addListener((item) => {
  if (!item || !item.url) return;
  if (item.url.startsWith("blob:") || item.url.startsWith("data:")) return;

  const key = `b${item.id}|${item.url}`;
  if (wasCaptured(key)) return;

  decide(item.url).then((verdict) => {
    if (verdict === "skip") return;
    if (verdict === "capture") {
      takeover(item);
    } else {
      askUser(item);
    }
  });
});

function takeover(item, opts) {
  opts = opts || {};
  // Pause Chrome's download while DownPilot decides whether it accepted the
  // handoff. This keeps the original browser copy recoverable on failure.
  try { chrome.downloads.pause(item.id, () => {}); } catch (e) { /* already gone */ }
  const resumeChrome = () => {
    try { chrome.downloads.resume(item.id, () => {}); } catch (e) { /* already gone */ }
  };

  sendMessage({
    type: "submit_browser_download",
    url: item.url,
    filename: item.filename || undefined,
    referer: item.referrer || undefined,
    user_agent: item.userAgent || undefined,
    cookies: undefined, // Never forwarded by default; ask-mode would
                        // attach it only if the user explicitly opted in.
    browser_id: String(item.id),
    try_download_anyway: !!opts.tryAnyway,
  })
    .then((resp) => {
      chrome.downloads.cancel(item.id, () => {
        chrome.downloads.erase({ id: item.id }, () => {});
      });
      markCaptured(`b${item.id}|${item.url}`);
      const id = resp && resp.data ? resp.data.id : null;
      notify("Download captured", item.filename || item.url);
      // Stash the mapping for the popup / "Open in DownPilot" link.
      capturedByUrl.set(item.url, id);
    })
    .catch(() => {
      // App rejected the handoff (network, dedup race, etc.).
      // Fall back to letting Chrome continue so the user still gets
      // the file. (Don't try to re-cancel — the user might already
      // have clicked through.)
      notify("DownPilot didn't accept the download", "Continuing in Chrome");
    });
}

const capturedByUrl = new Map();

function askUser(item) {
  // Pre-flight: ask the app to probe the URL so the notification can
  // show the real filename / size / content-type, and so a 404 doesn't
  // fall through to a broken task. The probe is bounded by the app's
  // PROBE_TIMEOUT_MS (4s) so the ask notification is never slow.
  const probeItem = Object.assign({}, item);
  sendMessage({
    type: "browser_download_detected",
    url: item.url,
    filename: item.filename,
    referer: item.referrer,
    cookies: undefined, // never forwarded on the preview path
    browser_id: String(item.id),
  })
    .then((resp) => {
      const d = resp && resp.data ? resp.data : {};
      if (typeof d.filename === "string" && d.filename) {
        probeItem.filename = d.filename;
      }
      const ok = d.ok === true;
      probeItem._probeFailed = !ok;
      const id = `dp-ask-${item.id}`;
      const sizeBytes = Number(d.total_bytes) || 0;
      const ctxMsg = [
        domainOf(item.url),
        sizeBytes ? humanBytes(sizeBytes) : "",
        d.content_type || "",
      ]
        .filter(Boolean)
        .join(" · ");
      const buttons = ok
        ? [
            { title: "Download" },
            { title: "Ignore" },
            { title: "Always download this type" },
          ]
        : [
            { title: "Try Download Anyway" },
            { title: "Ignore" },
          ];
      chrome.notifications.create(id, {
        type: "basic",
        iconUrl: "icons/128.png",
        title: ok
          ? "Download with DownPilot?"
          : "Probe failed — Download with DownPilot?",
        message: probeItem.filename || item.url,
        contextMessage: ctxMsg || d.error || undefined,
        buttons,
        priority: 1,
      });
      askCallbacks.set(id, probeItem);
    })
    .catch(() => {
      // App unreachable: fall back to a basic ask without a probe.
      const id = `dp-ask-${item.id}`;
      chrome.notifications.create(id, {
        type: "basic",
        iconUrl: "icons/128.png",
        title: "Download with DownPilot?",
        message: item.filename || item.url,
        contextMessage: domainOf(item.url),
        buttons: [
          { title: "Download" },
          { title: "Ignore" },
          { title: "Always download this type" },
        ],
        priority: 1,
      });
      askCallbacks.set(id, item);
    });
}

function humanBytes(n) {
  if (!n || n < 0) return "";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let i = 0;
  let v = n;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return v.toFixed(v >= 10 ? 0 : 1) + " " + units[i];
}

const askCallbacks = new Map();
chrome.notifications.onButtonClicked.addListener((notifId, btnIdx) => {
  const item = askCallbacks.get(notifId);
  if (!item) return;
  askCallbacks.delete(notifId);
  chrome.notifications.clear(notifId);
  if (btnIdx === 0) {
    takeover(item, { tryAnyway: item._probeFailed === true });
  } else if (btnIdx === 2) {
    // "Always download this type": persist the extension to the app's
    // `always_allow_extensions` setting so future downloads of the same
    // type are captured automatically (no ask prompt). The app is the
    // source of truth; we re-fetch config after the round-trip so the
    // in-memory `alwaysAllowExtensions` set stays in sync.
    const e = extOf(item.url);
    if (e) {
      sendMessage({ type: "set_always_allow_extension", extension: e, op: "add" })
        .then((resp) => {
          if (resp && resp.ok && resp.data && Array.isArray(resp.data.always_allow_extensions)) {
            config.alwaysAllowExtensions = new Set(
              resp.data.always_allow_extensions
                .split(",")
                .map((x) => x.trim().toLowerCase())
                .filter(Boolean)
            );
          } else {
            config.alwaysAllowExtensions.add(e);
          }
        })
        .catch(() => {
          // App unreachable — fall back to local-only memory.
          config.alwaysAllowExtensions.add(e);
        })
        .finally(() => {
          alwaysSkip.exts.delete(e);
          persistAlwaysSkip();
          takeover(item, { tryAnyway: item._probeFailed === true });
        });
      return;
    }
    takeover(item, { tryAnyway: item._probeFailed === true });
  } else {
    // Ignore. Remember the url so a Chrome retry doesn't re-prompt.
    markCaptured(`b${item.id}|${item.url}`);
  }
});

chrome.notifications.onClosed.addListener((notifId) => {
  askCallbacks.delete(notifId);
});

// ---- Context menus ---------------------------------------------------------
chrome.runtime.onInstalled.addListener(() => {
  chrome.contextMenus.create({ id: "downpilot-link", title: "Download link with DownPilot", contexts: ["link"] });
  chrome.contextMenus.create({ id: "downpilot-video", title: "Download video with DownPilot", contexts: ["video"] });
  chrome.contextMenus.create({ id: "downpilot-page", title: "Grab media from this page", contexts: ["page"] });
});

chrome.contextMenus.onClicked.addListener(async (info, tab) => {
  const url = info.linkUrl || info.srcUrl || tab?.url;
  if (!url) return;
  if (info.menuItemId === "downpilot-page") {
    sendMessage({ type: "analyze", url })
      .then(() => notify("Analyzing page", url))
      .catch(() => notify("Failed", "Check DownPilot is running"));
  } else {
    sendMessage({
      type: "submit_browser_download",
      url,
      referer: tab?.url,
      browser_id: `ctx-${Date.now()}`,
    })
      .then(() => notify("Download added", url))
      .catch(() => notify("Failed", "Check DownPilot is running"));
  }
});

// ---- Content script / popup RPC -------------------------------------------
chrome.runtime.onMessage.addListener((msg, _sender, sendResponse) => {
  if (msg.action === "setPaused") {
    config.userPaused = !!msg.value;
    sendResponse({ ok: true });
    return;
  }
  if (msg.action === "refreshConfig") {
    refreshConfig(true).then(() => sendResponse({ ok: true, config })).catch(() => sendResponse({ ok: false }));
    return true;
  }
  if (msg.action === "analyze") {
    sendMessage({ type: "analyze", url: msg.url, cookies: msg.cookies, referer: msg.referer, user_agent: msg.user_agent })
      .then((resp) => sendResponse({ ok: true, media: resp.data }))
      .catch((err) => sendResponse({ ok: false, error: err.message }));
    return true;
  }
  if (msg.action === "download") {
    sendMessage({
      type: "submit_browser_download",
      url: msg.url,
      filename: msg.filename,
      format_id: msg.format_id,
      referer: msg.referer,
      user_agent: msg.user_agent,
      cookies: msg.cookies,
      browser_id: `cs-${Date.now()}`,
    })
      .then((resp) => sendResponse({ ok: true, id: resp.data?.id }))
      .catch((err) => sendResponse({ ok: false, error: err.message }));
    return true;
  }
  if (msg.action === "status") {
    sendResponse({ connected: nmhPort !== null, config, alwaysSkip: { domains: Array.from(alwaysSkip.domains), exts: Array.from(alwaysSkip.exts) } });
    return;
  }
  if (msg.action === "getDownloads") {
    sendMessage({ type: "list" })
      .then((resp) => sendResponse({ ok: true, tasks: resp.data || [] }))
      .catch((err) => sendResponse({ ok: false, error: err.message }));
    return true;
  }
  // taskAction: popup sends {action: "taskAction", action: "pause|resume|cancel", id}
  // (the `action` field name is overloaded). The NMH `type` is the verb.
  if (msg.action === "taskAction") {
    const t = msg.type || (msg.action !== "taskAction" ? msg.action : null);
    const valid = new Set(["pause", "resume", "cancel", "start"]);
    if (!valid.has(t) || msg.id == null) {
      sendResponse({ ok: false, error: "missing type or id" });
      return;
    }
    sendMessage({ type: t, id: msg.id })
      .then((resp) => sendResponse({ ok: true, data: resp.data }))
      .catch((err) => sendResponse({ ok: false, error: err.message }));
    return true;
  }
});

function notify(title, message) {
  try {
    chrome.notifications.create({
      type: "basic",
      iconUrl: "icons/128.png",
      title,
      message: String(message || "").slice(0, 200),
    });
  } catch {
    // Some platforms (notably Wayland under GNOME) don't render
    // notifications — silently degrade.
  }
}

// Periodic config refresh (app settings may change while extension is idle).
setInterval(() => {
  if (nmhPort) refreshConfig(false).catch(() => {});
}, 15000);
