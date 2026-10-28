// DownPilot popup: connection + capture-mode control + live downloads list
// with per-row pause/resume/cancel. All NMH traffic is funneled through
// background.js (which owns the persistent native-messaging port).

(function () {
  const dot = document.getElementById("dot");
  const statusText = document.getElementById("statusText");
  const modeText = document.getElementById("modeText");
  const pauseBtn = document.getElementById("pauseBtn");
  const openBtn = document.getElementById("openBtn");
  const listEl = document.getElementById("list");
  const emptyMsg = document.getElementById("emptyMsg");
  const refreshListBtn = document.getElementById("refreshList");
  const chips = Array.from(document.querySelectorAll(".chip"));

  let paused = false;
  let connected = false;
  let filter = "active"; // "active" | "all" | "done"
  let lastTasks = [];

  // ---- chrome.runtime helpers ---------------------------------------------
  function send(action, payload) {
    return new Promise((resolve) => {
      try {
        chrome.runtime.sendMessage({ action, ...(payload || {}) }, (resp) => {
          resolve(resp);
        });
      } catch (_) {
        resolve(null);
      }
    });
  }

  // ---- Status row (connection + capture mode) -----------------------------
  send("status").then((resp) => {
    connected = !!(resp && resp.connected);
    dot.className = "dot " + (connected ? "on" : "off");
    statusText.textContent = connected ? "Connected" : "DownPilot not running";
    if (connected && resp.config) {
      paused = !!resp.config.userPaused;
      const c = resp.config;
      applyPaused();
      modeText.textContent = (c.userPaused ? "PAUSED" : (c.mode || "ask").toUpperCase());
    } else {
      setEmpty(connected ? "No tasks yet" : "Start DownPilot to see downloads", !connected);
    }
    renderSkips();
    if (connected) refreshList();
  });

  function applyPaused() {
    pauseBtn.textContent = paused ? "Resume capture" : "Pause capture";
    if (modeText && paused) modeText.textContent = "PAUSED";
  }

  // Row for a skipped domain/extension, with an un-skip button.
  function renderSkips() {
    const el = document.getElementById("skips");
    if (!el) return;
    send("status").then((resp) => {
      const sk = (resp && resp.alwaysSkip) || { domains: [], exts: [] };
      const dd = sk.domains || [], de = sk.exts || [];
      el.innerHTML = "";
      if (dd.length === 0 && de.length === 0) {
        const n = document.createElement("li");
        n.textContent = "None — sites aren't being skipped";
        n.style.color = "#8b8b95";
        el.appendChild(n);
      } else {
        dd.forEach((d) => el.appendChild(skipRow(d, "domain")));
        de.forEach((e) => el.appendChild(skipRow(e, "ext")));
      }
    });
  }

  function skipRow(value, kind) {
    const li = document.createElement("li");
    li.className = "skip-row";
    const label = document.createElement("span");
    label.className = "skip-label";
    label.textContent = (kind === "domain" ? "site " : "ext ") + value;
    const btn = document.createElement("button");
    btn.className = "icon-btn";
    btn.title = "Remove from skip list";
    btn.textContent = "✕";
    btn.addEventListener("click", () => {
      btn.disabled = true;
      send("removeSkip", { [kind]: value }).then(() => renderSkips());
    });
    li.appendChild(label);
    li.appendChild(btn);
    return li;
  }

  pauseBtn.addEventListener("click", () => {
    paused = !paused;
    applyPaused();
    send("setPaused", { value: paused });
  });

  openBtn.addEventListener("click", (e) => {
    e.preventDefault();
    chrome.tabs.create({ url: "chrome://extensions/?id=onmpegdpcecafeebchmmmohbdbjfcepb" });
  });

  // ---- Filter chips --------------------------------------------------------
  chips.forEach((c) => {
    c.addEventListener("click", () => {
      filter = c.dataset.filter;
      chips.forEach((x) => x.classList.toggle("active", x === c));
      render(lastTasks);
    });
  });

  refreshListBtn.addEventListener("click", () => refreshList());

  // ---- Downloads list ------------------------------------------------------
  function refreshList() {
    send("getDownloads").then((resp) => {
      if (!resp || !resp.ok) {
        setEmpty(connected ? "Failed to load" : "Start DownPilot to see downloads", !connected);
        return;
      }
      lastTasks = Array.isArray(resp.tasks) ? resp.tasks : [];
      render(lastTasks);
    });
  }

  function setEmpty(text, isError) {
    emptyMsg.textContent = text;
    emptyMsg.classList.toggle("error", !!isError);
    emptyMsg.style.display = "block";
  }

  function render(tasks) {
    listEl.innerHTML = "";
    if (!tasks || tasks.length === 0) {
      setEmpty(connected ? "No downloads" : "Start DownPilot to see downloads", !connected);
      return;
    }
    const filtered = filterTasks(tasks);
    if (filtered.length === 0) {
      setEmpty(filter === "done" ? "No completed downloads" : "No active downloads", false);
      return;
    }
    // Cap to 20 most-recent rows for sanity (full list is in the app).
    filtered.slice(0, 20).forEach((t) => {
      listEl.appendChild(buildRow(t));
    });
    if (filtered.length > 20) {
      const more = document.createElement("div");
      more.className = "empty";
      more.textContent = `+ ${filtered.length - 20} more (open DownPilot for full list)`;
      listEl.appendChild(more);
    }
  }

  function filterTasks(tasks) {
    // Sort by updated_at desc — most-recent activity first.
    const sorted = tasks.slice().sort((a, b) => (b.updated_at || 0) - (a.updated_at || 0));
    if (filter === "all") return sorted;
    if (filter === "done") {
      return sorted.filter((t) =>
        t.status === "completed" || t.status === "failed" || t.status === "cancelled"
      );
    }
    // "active" = everything still in flight or resumable.
    return sorted.filter((t) =>
      t.status === "downloading" || t.status === "paused" ||
      t.status === "queued" || t.status === "scheduled" ||
      t.status === "stopped" || t.status === "failed"
    );
  }

  function buildRow(t) {
    const row = document.createElement("div");
    row.className = "item";
    row.dataset.id = String(t.id);

    const main = document.createElement("div");
    main.className = "item-main";

    const name = document.createElement("div");
    name.className = "item-name";
    name.textContent = t.filename || "(unnamed)";
    name.title = t.filename || t.url || "";
    main.appendChild(name);

    const meta = document.createElement("div");
    meta.className = "item-meta";

    // Progress bar (only when we know the total)
    const bar = document.createElement("div");
    bar.className = "item-bar";
    const fill = document.createElement("div");
    const total = Number(t.total_size) || 0;
    const done = Number(t.downloaded) || 0;
    if (total > 0) {
      const pct = Math.max(0, Math.min(100, (done / total) * 100));
      fill.style.width = pct.toFixed(1) + "%";
    } else {
      bar.classList.add("indeterminate");
      fill.style.width = "30%";
    }
    bar.appendChild(fill);
    meta.appendChild(bar);

    const pct = document.createElement("span");
    pct.className = "item-pct";
    if (total > 0) {
      pct.textContent = Math.round((done / total) * 100) + "%";
    } else if (t.status === "downloading") {
      pct.textContent = "···";
    } else {
      pct.textContent = "";
    }
    meta.appendChild(pct);

    // Speed + ETA (only meaningful when downloading)
    if (t.status === "downloading") {
      const speed = document.createElement("span");
      speed.className = "item-speed";
      speed.title = "Current speed";
      const bps = Number(t.peak_speed_bps) || Number(t.avg_speed_bps) || 0;
      speed.textContent = bps > 0 ? formatBytes(bps) + "/s" : "";
      meta.appendChild(speed);
      if (total > 0) {
        const eta = document.createElement("span");
        eta.className = "item-eta";
        eta.title = "Time remaining";
        const remaining = Math.max(0, total - done);
        const sec = bps > 0 ? Math.round(remaining / bps) : 0;
        eta.textContent = sec > 0 ? formatEta(sec) : "";
        meta.appendChild(eta);
      }
    }

    const badge = document.createElement("span");
    badge.className = "badge " + badgeClass(t.status);
    badge.textContent = t.status;
    meta.appendChild(badge);

    main.appendChild(meta);
    row.appendChild(main);

    // ---- Action buttons (only for active states) ----
    const actions = document.createElement("div");
    actions.className = "item-actions";

    // Pause / Resume (toggle)
    if (canPause(t)) {
      const btn = document.createElement("button");
      btn.className = "icon-btn";
      btn.title = "Pause";
      btn.textContent = "⏸";
      btn.addEventListener("click", () => doAction("pause", t.id, btn));
      actions.appendChild(btn);
    } else if (canResume(t)) {
      const btn = document.createElement("button");
      btn.className = "icon-btn";
      btn.title = "Resume";
      btn.textContent = "▶";
      btn.addEventListener("click", () => doAction("resume", t.id, btn));
      actions.appendChild(btn);
    }

    // Cancel / Stop (only for non-terminal)
    if (canCancel(t)) {
      const btn = document.createElement("button");
      btn.className = "icon-btn danger";
      btn.title = "Cancel";
      btn.textContent = "✕";
      btn.addEventListener("click", () => doAction("cancel", t.id, btn));
      actions.appendChild(btn);
    }

    if (actions.childElementCount > 0) row.appendChild(actions);
    return row;
  }

  function badgeClass(s) {
    if (s === "downloading") return "dl";
    if (s === "paused") return "paused";
    if (s === "queued" || s === "scheduled") return "queued";
    if (s === "completed") return "done";
    if (s === "failed") return "failed";
    if (s === "stopped") return "stopped";
    if (s === "cancelled") return "cancelled";
    return "queued";
  }

  function canPause(t) {
    return t.status === "downloading" || t.status === "queued" || t.status === "scheduled";
  }
  function canResume(t) {
    return t.status === "paused" || t.status === "stopped" || t.status === "failed";
  }
  function canCancel(t) {
    return t.status !== "completed" && t.status !== "cancelled";
  }

  function doAction(action, id, btn) {
    if (btn) btn.disabled = true;
    send("taskAction", { type: action, id }).then((resp) => {
      if (!resp || !resp.ok) {
        if (btn) btn.disabled = false;
        // Show a brief inline error (replace badge area)
        const row = listEl.querySelector(`[data-id="${id}"]`);
        if (row) {
          const err = document.createElement("div");
          err.className = "empty error";
          err.textContent = (resp && resp.error) || "Action failed";
          row.appendChild(err);
          setTimeout(() => err.remove(), 1800);
        }
        return;
      }
      // Refresh after a short delay so the engine settles.
      setTimeout(refreshList, 250);
    });
  }

  // ---- Auto-refresh while the popup is open ----
  // 2s tick — dense enough to feel live, light enough to be cheap.
  const tick = setInterval(() => {
    if (connected) refreshList();
  }, 2000);
  window.addEventListener("unload", () => clearInterval(tick));
})();
