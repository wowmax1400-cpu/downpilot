// Popup script: shows connection status and settings toggle.

(function () {
  const dot = document.getElementById("dot");
  const statusText = document.getElementById("statusText");
  const autoIntercept = document.getElementById("autoIntercept");
  const openBtn = document.getElementById("openBtn");

  // Check connection status
  chrome.runtime.sendMessage({ action: "status" }, (resp) => {
    if (resp && resp.connected) {
      dot.className = "dot on";
      statusText.textContent = "Connected";
    } else {
      dot.className = "dot off";
      statusText.textContent = "DownPilot not running";
    }
  });

  // Load auto-intercept setting
  chrome.storage.local.get("autoIntercept", (data) => {
    autoIntercept.checked = !!data.autoIntercept;
  });

  autoIntercept.addEventListener("change", () => {
    chrome.storage.local.set({ autoIntercept: autoIntercept.checked });
    // Notify background
    chrome.runtime.sendMessage({ action: "setAutoIntercept", value: autoIntercept.checked });
  });

  openBtn.addEventListener("click", () => {
    // Try to open the DownPilot app (or its dashboard URL)
    // Usually the app runs on localhost:19851; we can't really open it from
    // a browser extension, but the user can click to focus.
    chrome.tabs.create({ url: "http://localhost:19851" }).catch(() => {});
  });
})();