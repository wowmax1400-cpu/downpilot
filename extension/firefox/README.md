# Firefox extension

This directory contains the **Firefox MV3** build of the DownPilot
extension. It lives separately from the Chrome build because Firefox
and Chrome differ on three manifest keys:

| Concern | Chrome | Firefox |
|---|---|---|
| Action | `action` | `browser_action` |
| Background | `background.service_worker` | `background.scripts` (array) |
| Identity | nothing (key in manifest) | `browser_specific_settings.gecko.id` |

## Install

Firefox MV3 requires **all** referenced files (background, content
scripts, icons, popup) to live in the **same directory as the
manifest.json**. They cannot be referenced via `../`. So:

```bash
# 1. Make a Firefox build directory that mirrors the Chrome layout.
mkdir -p /tmp/downpilot-firefox
cp extension/firefox/manifest.json /tmp/downpilot-firefox/
cp extension/background.js /tmp/downpilot-firefox/
cp extension/content.js /tmp/downpilot-firefox/
cp extension/content.css /tmp/downpilot-firefox/
cp extension/popup.html /tmp/downpilot-firefox/
cp extension/popup.js /tmp/downpilot-firefox/
cp -r extension/icons /tmp/downpilot-firefox/

# 2. Load as a temporary add-on: about:debugging#/runtime/this-firefox
#    → "Load Temporary Add-on..." → pick the manifest.json.
```

For a permanent install, zip the dir and submit to
[addons.mozilla.org](https://addons.mozilla.org/) — the AMO signing
process stamps a real `gecko.id` for you, replacing
`downpilot@local`.

## Native messaging

The Tauri app's NMH server on `127.0.0.1:19851` is identical for
Chrome and Firefox — same frame protocol, same JSON message shapes.
The only thing the browser cares about is that the manifest at
`native-host/firefox/downpilot-firefox-nmh.json` is installed under
`~/.mozilla/native-messaging-hosts/` (Linux) with the same binary
path as the Chrome variant.

The `allowed_extensions` field in the Firefox NMH manifest is
`["downpilot@local"]` — this **must** match
`browser_specific_settings.gecko.id` in the extension manifest.
Firefox silently refuses to spawn the host if these don't match;
there is no console error, the message just disappears.
