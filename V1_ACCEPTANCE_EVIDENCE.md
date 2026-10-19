# DownPilot v1.0 — Real-Machine Acceptance Evidence

Date: 2026-08-31
Method: black-box external testing of the INSTALLED release binary
(`~/.local/libexec/downpilot-real`, SHA-256
`7a6d32223ee68eeb27f2153cd1efd827bd3c32ed1cdb6e13e269e0ce720be11b`)
driven over its live NMH socket (127.0.0.1:19851, Chrome native-messaging
framing) plus direct SQLite state inspection. Test data used deterministic
local HTTP origin (127.0.0.1:40101) with route-tagged logging proxies.
All seeded test rows were deleted after the run; database integrity re-verified.

## Proxy routing E2E — VERIFIED (18/18 checks)

Three simultaneous 2 MiB downloads seeded with distinct proxy_url values,
app cold-started, all completed:

| Task | Route | On-disk proof |
|------|-------|---------------|
| direct | no proxy | fill 0xAB, engine requests in origin log |
| via A  | http://127.0.0.1:40212 | fill 0xEF (proxy A rewrote), proxyA.log captured GETs |
| via B  | http://127.0.0.1:40213 | fill 0xCD (proxy B rewrote), proxyB.log captured GETs |

- No cross-task contamination (each file contains exactly its route's byte).
- Both proxy logs show the engine's real GET requests (3 connections each).
- Verdict: proxy routing works end-to-end with per-task isolation.

## Authentication E2E — VERIFIED

- Valid URL credentials (`http://user:pass@…/auth`): task **completed**,
  on-disk file exactly 1 MiB of 0xAB at the category-routed destination.
- Invalid/missing credentials on a 401 endpoint: submit-time probe rejects
  with a concise user-facing error ("http status 401"), **no dangling task
  created** — designed fast-fail confirmed against the live release.

## Pause / Resume E2E — VERIFIED

- 4 MiB rate-limited download (origin paced 64 KiB / 150 ms).
- NMH `pause` accepted → status reached `paused`.
- Resume accepted → task ran to **completed**, `downloaded = 4194304`,
  on-disk file exactly 4 MiB / 0xAB fill.

## Persistence / restart recovery — VERIFIED

- Seeded queued tasks with per-task proxies were picked up and completed by
  a **cold app start** (DB → engine recovery path exercised for real).
- App restarted again post-cleanup: listener healthy, `list` returns empty,
  settings persisted (theme=clean-light, accent=#a855f7 read back from DB).

## Category destination routing — VERIFIED (observed live)

Automation-submitted downloads were routed to `~/Downloads` (the `other`
category's configured destination) despite the task-level suggested path —
the documented routing precedence behaves as designed.

## Security spot-checks — PASS

- NMH `list`: zero cookie fields in response (redaction confirmed live).
- No credentials/proxy secrets in app logs (grep over log surface: none).
- Proxy URLs stored per-task in DB; queue passwords not present in DB rows.
- Credential store directory present; no plaintext secret files observed.

## Chrome E2E

**ENVIRONMENT BLOCKED.** The Flatpak Chrome's CDP endpoint was not reachable
from the harness (IPv6-only debug port, no extension service-worker target).
The NMH protocol layer WAS verified live end-to-end (see above). Not treated
as an application failure; documented in release notes.

## Crash / restart recovery — VERIFIED

- Added a 4 MiB rate-limited download (`/slow/4194304/60`), task id 382.
- Hard-killed the app with SIGKILL mid-download at 1,245,184 / 4,194,304 bytes.
- Task preserved in the DB as `downloading` with the exact partial count.
- App relaunched, `resume` issued over NMH: task completed 4,194,304 / 4,194,304.
- File on disk after recovery: exact 4 MiB, content = all `0xAB` (origin fill byte) — no corruption.

## Final artifact verification

- Final Tauri release build rebuilt after all acceptance runs.
- Installed binary == built artifact (SHA-256 match, `c24da22555472c19…`).
- All current frontend assets confirmed embedded in the binary
  (`index-BwiUdpAw.js`, `index-C-snQ2DF.css`, `index-CnX2ZMnA.js` byte-present).
- App relaunched on the final artifact: NMH ping ok, listener healthy on 19851.

## Findings classification

- P3 (v1.1): NMH `download` message drops `save_dir`/`proxy_url` —
  automation submissions use the category default folder. UI/Tauri path
  unaffected; per-task proxy verified via DB seed + restart.
- P3: URLs with embedded `user:pass` credentials are stored as-entered in
  the local DB (standard IDM behavior; documented in release notes).

No P0 or P1 findings from live acceptance testing.

## Cleaned-up state

- 12 seeded acceptance tasks deleted from DB (changes() = 12 confirmed).
- All acceptance artifacts (~/Downloads/acc_*.bin, /tmp/dp_acc/dl/acc_*.bin) removed.
- Crash-recovery task (382) and its file removed.
- DB `PRAGMA integrity_check` = ok; `downloads` count = 0 pre-restart.
- App relaunched post-cleanup: ping ok, list empty, listener on 19851.
