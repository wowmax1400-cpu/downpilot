// DownPilot content script: injects a floating button on video/audio
// players. Detection is deterministic — a periodic scan finds the largest
// playing media element in the viewport and pins the button to it. This is
// the XDM-style approach: no hover wiring, no event hooks to lose, works in
// shadow DOMs via a shadow-piercing walk, and survives SPA navigation.

(function () {
  "use strict";

  // ---- Shadow DOM helper --------------------------------------------------
  let shadowHost = null;
  let shadowRoot = null;

  function createUI() {
    if (shadowHost) return;
    shadowHost = document.createElement("div");
    shadowHost.id = "downpilot-overlay";
    shadowHost.style.cssText =
      "position:fixed;top:0;left:0;width:0;height:0;z-index:2147483647;pointer-events:none";
    document.body.appendChild(shadowHost);
    shadowRoot = shadowHost.attachShadow({ mode: "closed" });
    shadowRoot.innerHTML = `
      <style>
        @import url('data:text/css,${encodeURIComponent(
          `
          .dp-btn {
            display:flex;align-items:center;gap:6px;padding:6px 12px;
            border:none;border-radius:8px;cursor:grab;font-size:13px;font-weight:600;
            background:rgba(124,92,255,0.92);color:#fff;pointer-events:auto;
            box-shadow:0 4px 16px rgba(0,0,0,0.45);transition:all 0.15s;
            white-space:nowrap;user-select:none;-webkit-user-select:none;
          }
          .dp-btn:hover { background:#7c5cff; transform:scale(1.04); }
          .dp-btn svg { width:16px;height:16px; }
          .dp-close {
            position:absolute;top:-8px;right:-8px;width:18px;height:18px;
            border:none;border-radius:50%;padding:0;
            background:rgba(56,56,66,0.95);color:#c9c9d1;
            font-size:11px;line-height:1;font-family:system-ui,sans-serif;
            cursor:pointer;pointer-events:auto;
            display:flex;align-items:center;justify-content:center;
            box-shadow:0 2px 6px rgba(0,0,0,0.4);transition:background 0.15s;
            z-index:1;
          }
          .dp-close:hover { background:#b03a3a; color:#fff; }
          .dp-dropdown {
            position:absolute;top:100%;right:0;margin-top:6px;min-width:200px;
            border-radius:10px;overflow:hidden;
            background:rgba(10,10,14,0.95);border:1px solid rgba(124,92,255,0.25);
            box-shadow:0 8px 32px rgba(0,0,0,0.6);pointer-events:auto;
          }
          .dp-option {
            display:flex;align-items:center;justify-content:space-between;
            gap:8px;width:100%;padding:8px 12px;border:none;background:transparent;
            color:#e8e8ec;font-size:12px;cursor:pointer;text-align:left;
            transition:background 0.1s;
          }
          .dp-option:hover { background:rgba(124,92,255,0.15); }
          .dp-option span { color:#9d9da8;font-size:11px; }
          .dp-option .size { font-variant-numeric:tabular-nums; }
          .dp-spinner { width:14px;height:14px;border:2px solid rgba(255,255,255,0.2);border-top-color:#fff;border-radius:50%;animation:dp-spin 0.6s linear infinite; }
          @keyframes dp-spin { to { transform:rotate(360deg); } }
          .dp-tip {
            position:absolute;bottom:100%;left:50%;transform:translateX(-50%);
            margin-bottom:8px;min-width:180px;max-width:320px;padding:8px 12px;
            border-radius:8px;font-size:11px;line-height:1.5;color:#e8e8ec;
            background:rgba(10,10,14,0.96);border:1px solid rgba(124,92,255,0.3);
            box-shadow:0 6px 24px rgba(0,0,0,0.6);pointer-events:none;white-space:normal;
            text-align:left;z-index:2147483647;
          }
          .dp-tip .t-title { font-weight:600;font-size:11.5px;color:#fff;word-break:break-all;margin-bottom:3px; }
          .dp-tip .t-row { display:flex;justify-content:space-between;gap:12px;color:#9d9da8; }
          .dp-tip .t-row b { color:#c4b5fd;font-weight:600; }
          `
        )}');
      </style>
      <div id="dp" style="position:absolute;pointer-events:none"></div>
    `;
  }

  // ---- Shadow-piercing media search ----------------------------------------
  // document.querySelectorAll can't see into shadow roots. Walk them.
  function collectMedia() {
    const found = [];
    function walk(root) {
      root.querySelectorAll("video, audio").forEach((el) => found.push(el));
      root.querySelectorAll("*").forEach((el) => {
        if (el.shadowRoot) walk(el.shadowRoot);
      });
    }
    walk(document);
    return found;
  }

  // ---- State --------------------------------------------------------------
  let currentMedia = null;
  let visible = false;
  let buttonEl = null;
  let lastPinnedAt = 0;
  // User clicked the × on the button — hide the overlay for this page load.
  let dismissed = false;

  // ---- Position the button near the element -------------------------------
  function positionButton(el) {
    const rect = el.getBoundingClientRect();
    if (rect.width < 40 || rect.height < 20) return false;
    const container = shadowRoot.getElementById("dp");
    const btnW = 120;
    const btnH = 34;
    const pad = 8;
    let left = Math.min(rect.right - btnW - pad, window.innerWidth - btnW - pad);
    // Pin the button to the TOP edge of the media element (IDM-style),
    // not the bottom. Falls back to the viewport top when the element's
    // top is off-screen.
    let top = Math.min(rect.top + pad, window.innerHeight - btnH - pad);
    left = Math.max(pad, left);
    top = Math.max(pad, top);
    container.style.left = left + "px";
    container.style.top = top + "px";
    container.style.pointerEvents = "none";
    currentMedia = el;
    return true;
  }

  // ---- Build the dropdown --------------------------------------------------
  function buildDropdown(formats) {
    const div = document.createElement("div");
    div.className = "dp-dropdown";
    (formats || []).forEach((f) => {
      const btn = document.createElement("button");
      btn.className = "dp-option";
      const label = f.resolution || f.note || f.format_id;
      const extra = f.filesize > 0 ? formatBytes(f.filesize) : f.ext;
      btn.innerHTML = `<span>${label}</span><span class="size">${extra}</span>`;
      btn.addEventListener("click", (e) => {
        e.stopPropagation();
        doDownload(f.format_id);
        closeDropdown();
      });
      div.appendChild(btn);
    });
    return div;
  }

  function closeDropdown() {
    const container = shadowRoot.getElementById("dp");
    const old = container.querySelector(".dp-dropdown");
    if (old) old.remove();
  }

  // ---- Analyze + download --------------------------------------------------
  function currentSrc() {
    if (!currentMedia) return "";
    return (
      currentMedia.currentSrc ||
      currentMedia.src ||
      (currentMedia.querySelector && currentMedia.querySelector("source")?.src) ||
      ""
    );
  }

  // URL to send to the backend. A `blob:` / `data:` value is an in-memory
  // object the yt-dlp backend cannot fetch — resolve to the page URL instead
  // (yt-dlp extracts the real stream from the page, e.g. MSE/HLS on YouTube).
  function backendUrl(url) {
    if (!url) return url;
    if (url.startsWith("blob:") || url.startsWith("data:")) {
      try {
        const p = new URL(window.location.href);
        if (p.protocol === "http:" || p.protocol === "https:") return p.href;
      } catch { /* unresolvable — fall through */ }
      return window.location.href;
    }
    return url;
  }

  function isDirectFile(url) {
    try {
      const p = new URL(url).pathname;
      return /\.[a-z0-9]{2,5}$/i.test(p);
    } catch {
      return false;
    }
  }

  // Plain "download now" — sends straight to the desktop app (chunked engine).
  // No yt-dlp round-trip, so it's instant for direct file URLs.
  function quickDownload() {
    const url = currentSrc();
    if (!url) return;
    closeDropdown();
    if (isDirectFile(url)) {
      sendDownload(url, null);
      return;
    }
    // Media *page* (YouTube, etc.): need a format picker -> analyze.
    analyzeMedia();
  }

  function sendDownload(url, formatId) {
    const backend = backendUrl(url);
    const note = document.createElement("div");
    note.className = "dp-dropdown";
    note.innerHTML = '<div style="padding:10px;font-size:12px;color:#9d9da8">Sending to DownPilot…</div>';
    shadowRoot.getElementById("dp").appendChild(note);
    setTimeout(() => note.remove(), 2000);
    chrome.runtime.sendMessage(
      {
        action: "download",
        url: backend,
        format_id: formatId,
        referer: window.location.href,
        user_agent: navigator.userAgent,
        cookies: document.cookie,
      },
      (resp) => {
        note.remove();
        if (!resp || !resp.ok) {
          const err = document.createElement("div");
          err.className = "dp-dropdown";
          err.innerHTML = `<div style="padding:10px;font-size:12px;color:#f87171">${resp?.error || "Failed — is DownPilot running?"}</div>`;
          shadowRoot.getElementById("dp").appendChild(err);
          setTimeout(() => err.remove(), 2500);
        }
      }
    );
  }

  function analyzeMedia() {
    const url = currentSrc();
    if (!url) return;
    const container = shadowRoot.getElementById("dp");
    closeDropdown();

    const loading = document.createElement("div");
    loading.className = "dp-dropdown";
    loading.innerHTML = '<div style="padding:12px;display:flex;align-items:center;gap:8px;font-size:12px;color:#9d9da8"><div class="dp-spinner"></div> Analyzing…</div>';
    container.appendChild(loading);

    let done = false;
    const to = setTimeout(() => {
      if (done) return;
      done = true;
      loading.remove();
      // yt-dlp slow/unavailable: fall back to a direct download of the URL.
      sendDownload(url, null);
    }, 8000);

    chrome.runtime.sendMessage(
      { action: "analyze", url: backendUrl(url), cookies: document.cookie, referer: window.location.href, user_agent: navigator.userAgent },
      (resp) => {
        if (done) return;
        done = true;
        clearTimeout(to);
        loading.remove();
        if (resp && resp.ok && resp.media && resp.media.best_formats && resp.media.best_formats.length) {
          const dropdown = buildDropdown(resp.media.best_formats);
          container.appendChild(dropdown);
        } else if (resp && resp.ok && isDirectFile(url)) {
          sendDownload(url, null);
        } else {
          const err = document.createElement("div");
          err.className = "dp-dropdown";
          err.innerHTML = `<div style="padding:12px;font-size:12px;color:#f87171">${resp?.error || "Failed to analyze"}</div>`;
          container.appendChild(err);
          setTimeout(() => err.remove(), 2500);
        }
      }
    );
  }

  function doDownload(formatId) {
    const url = currentSrc();
    if (!url) return;
    sendDownload(url, formatId);
  }

  // ---- Main button (IDM-style: draggable) ---------------------------------
  let dragState = null; // { startX, startY, origLeft, origTop, moved, startTime }
  let userPos = null; // { x, y } viewport position the user dragged to

  function ensureButton() {
    const container = shadowRoot.getElementById("dp");
    if (!buttonEl) {
      buttonEl = document.createElement("button");
      buttonEl.className = "dp-btn";
      buttonEl.title = "Drag to move · Click to download";
      buttonEl.innerHTML = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" y1="15" x2="12" y2="3"/></svg> DownPilot`;

      // --- Drag to move (IDM-style) ---
      buttonEl.addEventListener("mousedown", (e) => {
        if (e.button !== 0) return; // left button only
        e.stopPropagation();
        e.preventDefault();
        const c = shadowRoot.getElementById("dp");
        const rect = c.getBoundingClientRect();
        dragState = {
          startX: e.clientX,
          startY: e.clientY,
          origLeft: rect.left,
          origTop: rect.top,
          moved: false,
          startTime: Date.now(),
        };
        buttonEl.style.cursor = "grabbing";
        document.addEventListener("mousemove", onDragMove);
        document.addEventListener("mouseup", onDragEnd);
      });

      function onDragMove(e) {
        if (!dragState) return;
        const dx = e.clientX - dragState.startX;
        const dy = e.clientY - dragState.startY;
        if (Math.abs(dx) > 3 || Math.abs(dy) > 3) dragState.moved = true;
        if (!dragState.moved) return;
        const c = shadowRoot.getElementById("dp");
        const btnW = 120;
        const btnH = 34;
        let left = dragState.origLeft + dx;
        let top = dragState.origTop + dy;
        left = Math.max(4, Math.min(left, window.innerWidth - btnW - 4));
        top = Math.max(4, Math.min(top, window.innerHeight - btnH - 4));
        c.style.left = left + "px";
        c.style.top = top + "px";
        c.style.pointerEvents = "auto";
      }

      function onDragEnd(e) {
        if (!dragState) return;
        const wasDrag = dragState.moved;
        const c = shadowRoot.getElementById("dp");
        if (wasDrag) {
          // Snap to the position the user left the button at.
          const r = c.getBoundingClientRect();
          userPos = { x: r.left, y: r.top };
          lastPinnedAt = Date.now();
        }
        dragState = null;
        buttonEl.style.cursor = "grab";
        document.removeEventListener("mousemove", onDragMove);
        document.removeEventListener("mouseup", onDragEnd);
        if (wasDrag) {
          // Suppress the click that follows a drag (don't open the dropdown).
          e.preventDefault();
          e.stopPropagation();
          // The click event fires after mouseup; swallow it once.
          const swallow = (ce) => {
            ce.preventDefault();
            ce.stopPropagation();
            buttonEl.removeEventListener("click", swallow, true);
          };
          buttonEl.addEventListener("click", swallow, true);
        }
      }

      buttonEl.addEventListener("click", (e) => {
        e.stopPropagation();
        e.preventDefault();
        // If the user just dragged, a swallow handler already intercepted.
        if (dragState) return;
        quickDownload();
      });

      // --- Hover info tooltip (IDM-style: name, size, format) ---
      let tipEl = null;
      let tipVisible = false;
      let tipTimer = null;

      function ensureTip() {
        if (!tipEl) {
          tipEl = document.createElement("div");
          tipEl.className = "dp-tip";
          tipEl.style.display = "none";
          container.appendChild(tipEl);
        }
        return tipEl;
      }

      function hideTip() {
        tipVisible = false;
        clearTimeout(tipTimer);
        if (tipEl) tipEl.style.display = "none";
      }

      // Show instantly from the media element's own metadata, then upgrade
      // with real format/size data from the engine (cached per URL).
      buttonEl.addEventListener("mouseenter", () => {
        if (!currentMedia) return;
        clearTimeout(tipTimer);
        ensureTip();
        tipEl.innerHTML = `<div class="t-title">${escapeHtml(mediaTitle())}</div>
          <div class="t-row"><span>Size</span><b>${formatBytes(estimateSize()) || "—"}</b></div>
          <div class="t-row"><span>Quality</span><b>${resolution()}</b></div>
          <div class="t-row"><span>Format</span><b>${mediaExt()}</b></div>`;
        tipEl.style.display = "block";
        tipVisible = true;
        // Fire a cached analyze to get accurate size/format data.
        fetchMediaInfo();
      });

      buttonEl.addEventListener("mouseleave", () => {
        tipTimer = setTimeout(hideTip, 400);
      });

      // Cache of real media info per URL (avoid hammering the engine).
      let infoCache = new Map();
      function fetchMediaInfo() {
        const url = currentSrc();
        if (!url || infoCache.has(url)) {
          if (infoCache.has(url)) applyInfo(url);
          return;
        }
        chrome.runtime.sendMessage(
          { action: "analyze", url: backendUrl(url), cookies: document.cookie, referer: window.location.href, user_agent: navigator.userAgent },
          (resp) => {
            if (!resp || !resp.ok || !resp.media) return;
            infoCache.set(url, resp.media);
            applyInfo(url);
          }
        );
      }

      function applyInfo(url) {
        const info = infoCache.get(url);
        if (!info || !tipVisible || url !== currentSrc()) return;
        const best = (info.best_formats || [])[0];
        const rows = [];
        const title = info.title || mediaTitle();
        if (best && best.filesize > 0) {
          rows.push(`<div class="t-row"><span>Size</span><b>${formatBytes(best.filesize)}</b></div>`);
        }
        const res = best?.resolution || resolution();
        if (res && res !== "—") {
          rows.push(`<div class="t-row"><span>Quality</span><b>${res}</b></div>`);
        }
        const ext = best?.ext || mediaExt();
        if (ext && ext !== "—") {
          rows.push(`<div class="t-row"><span>Format</span><b>${ext.toUpperCase()}</b></div>`);
        }
        if (best?.format_id && best.format_id !== "original") {
          rows.push(`<div class="t-row"><span>ID</span><b>${best.format_id}</b></div>`);
        }
        tipEl.innerHTML = `<div class="t-title">${escapeHtml(title)}</div>${rows.join("")}`;
      }

      container.appendChild(buttonEl);

      // --- Close (×) button — dismiss the overlay for this page ---
      const closeEl = document.createElement("button");
      closeEl.className = "dp-close";
      closeEl.title = "Hide download button";
      closeEl.setAttribute("aria-label", "Hide download button");
      closeEl.textContent = "×";
      closeEl.addEventListener("mousedown", (e) => {
        e.stopPropagation(); // don't start a drag on the main button
        e.preventDefault();
      });
      closeEl.addEventListener("click", (e) => {
        e.stopPropagation();
        e.preventDefault();
        dismissed = true;
        hide();
      });
      container.appendChild(closeEl);
    }
    return buttonEl;
  }

  // ---- Media metadata helpers ---------------------------------------------
  function mediaTitle() {
    if (!currentMedia) return "Video";
    // Prefer the page <title> (usually the video name) trimmed to ~60 chars.
    const t = document.title?.trim();
    if (t && t.length > 1 && t.length < 200) return t;
    const u = currentSrc();
    if (u) {
      try {
        const name = decodeURIComponent(new URL(u).pathname.split("/").pop() || "");
        if (name) return name;
      } catch (_) {}
    }
    return "Video";
  }

  function resolution() {
    if (!currentMedia) return "—";
    if (currentMedia.tagName === "AUDIO") return "Audio";
    const w = currentMedia.videoWidth;
    const h = currentMedia.videoHeight;
    if (!w || !h) return "—";
    if (h >= 2160) return "4K";
    if (h >= 1440) return "1440p";
    if (h >= 1080) return "1080p";
    if (h >= 720) return "720p";
    if (h >= 480) return "480p";
    return `${h}p`;
  }

  function mediaExt() {
    if (!currentMedia) return "—";
    const u = currentSrc();
    if (!u) return "—";
    if (u.startsWith("blob:")) return "Stream";
    try {
      const p = new URL(u).pathname;
      const m = p.match(/\.([a-z0-9]{2,5})$/i);
      if (m) return m[1];
    } catch (_) {}
    return "Video";
  }

  function estimateSize() {
    // Rough instant estimate from duration × bitrate when real size is
    // unknown yet; the engine analyze call will replace it with exact data.
    if (!currentMedia || !currentMedia.duration) return 0;
    const dur = currentMedia.duration;
    // Assume ~2.5 Mbps for video; audio-only ~128 kbps.
    const bps = currentMedia.tagName === "AUDIO" ? 128000 : 2500000;
    return Math.round((dur * bps) / 8);
  }

  function escapeHtml(s) {
    return String(s)
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/"/g, "&quot;");
  }

  // ---- Show/hide ----------------------------------------------------------
  function show(el) {
    if (!el || el === shadowHost) return;
    createUI();
    if (!positionButton(el)) return;
    visible = true;
    ensureButton();
    const container = shadowRoot.getElementById("dp");
    container.style.opacity = "1";
    container.style.pointerEvents = "auto";
  }

  function hide() {
    if (!visible) return;
    visible = false;
    const container = shadowRoot.getElementById("dp");
    if (container) {
      container.style.opacity = "0";
      container.style.pointerEvents = "none";
      closeDropdown();
    }
    currentMedia = null;
  }

  // ---- THE CORE: periodic viewport scan (XDM-style) ------------------------
  // Every 500ms find the largest *playing* media element in the viewport and
  // pin the button to it. No hover, no event wiring — it just works.
  function tick() {
    // User dismissed the overlay via the × button — stay hidden this page load.
    if (dismissed) return;
    // If the dropdown is open, leave everything alone so the user can pick.
    const container = shadowRoot && shadowRoot.getElementById("dp");
    if (container && container.querySelector(".dp-dropdown")) return;

    const vw = window.innerWidth;
    const vh = window.innerHeight;

    let best = null;
    let bestArea = 0;
    for (const el of collectMedia()) {
      // Consider only elements that are actually rendering content.
      const ready = el.readyState > 0;
      const playing = !el.paused;
      if (!ready && !playing) continue;
      const r = el.getBoundingClientRect();
      // Must be at least partially visible in the viewport.
      if (r.bottom < 0 || r.right < 0 || r.top > vh || r.left > vw) continue;
      const area = r.width * r.height;
      if (area > bestArea) {
        bestArea = area;
        best = el;
      }
    }

    if (best) {
      // Re-pin position as the video moves/scrolls, but don't fight the user
      // hovering the overlay (max 2 repositions per second).
      const now = Date.now();
      const same = best === currentMedia;
      if (!visible) {
        show(best);
        // If the user had dragged the button, move it to their position
        // immediately after show() places it on the video.
        if (userPos) {
          const c = shadowRoot.getElementById("dp");
          if (c) { c.style.left = userPos.x + "px"; c.style.top = userPos.y + "px"; }
        }
      } else if (same && now - lastPinnedAt < 500) {
        // keep current position (it's already there)
      } else {
        createUI();
        ensureButton();
        if (userPos && same) {
          // User dragged it — keep at their viewport position.
          const c = shadowRoot.getElementById("dp");
          if (c) { c.style.left = userPos.x + "px"; c.style.top = userPos.y + "px"; }
          lastPinnedAt = now;
        } else {
          // New video or fresh pin — snap to the video.
          userPos = null;
          positionButton(best);
          lastPinnedAt = now;
        }
        // Ensure visible
        const c = shadowRoot.getElementById("dp");
        if (c) { c.style.opacity = "1"; c.style.pointerEvents = "auto"; }
        visible = true;
      }
      return;
    }

    // Nothing playing in view: keep the button only while the pointer is on it
    // (dropdown interaction), otherwise hide.
    if (container && container.style.opacity === "1") {
      // give a moment in case the video is between frames
      if (Date.now() - lastPinnedAt > 1200) hide();
    } else {
      hide();
    }
  }

  setInterval(tick, 500);
  setTimeout(tick, 400);

  // Also react to DOM changes (SPA navigation) with a fast rescan.
  let scanTimer = null;
  new MutationObserver(() => {
    clearTimeout(scanTimer);
    scanTimer = setTimeout(tick, 300);
  }).observe(document.documentElement, { childList: true, subtree: true });

  // Keep the button alive while the mouse is over it (so the dropdown works).
  document.addEventListener(
    "mouseover",
    (e) => {
      const onOverlay = e.composedPath().some(
        (n) => n === shadowHost || (n && n.id === "downpilot-overlay")
      );
      if (onOverlay) {
        lastPinnedAt = Date.now();
        return;
      }
    },
    true
  );

  // ---- Small helpers ------------------------------------------------------
  function formatBytes(n) {
    if (!n || n <= 0) return "—";
    const units = ["B", "KB", "MB", "GB"];
    let i = 0;
    let v = n;
    while (v >= 1024 && i < units.length - 1) {
      v /= 1024;
      i++;
    }
    return `${v >= 100 ? v.toFixed(0) : v >= 10 ? v.toFixed(1) : v.toFixed(2)} ${units[i]}`;
  }
})();