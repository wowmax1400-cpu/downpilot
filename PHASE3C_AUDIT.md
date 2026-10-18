# DownPilot Phase 3C Audit — Capture & IDM Power Features

This document is the pre-implementation audit for Phase 3C. It maps
the existing Grabber, Batch Download, Site Rule, Browser Capture, and
authentication surfaces to the 17 sections in the brief, so we know
what already works, what is partial, and what we still need to add
deterministic tests for.

## TL;DR

Phase 3C is **mostly already implemented** at the engine level. The
audit found that nearly everything the brief asks for is wired up end
to end: the grabber scans a page, returns structured
`GrabbedLink` rows, the SiteRule precedence is implemented, browser
capture flows through `manager.add` (so site rules apply
automatically), authentication uses the OS keyring (with an
AES-GCM-encrypted file fallback), and the Batch dialog already
previews URLs and dedups via `parse_batch_urls`.

The remaining work is:
- a few small UX/UI gaps (source column in the grabber results, a
  min/max size filter that's actually enforced, hostname-aware
  domain-exclusion in grabber rules, the "source" column in the
  example in the brief);
- a `data:` / magnet: / linked-resource detection pass for the
  grabber;
- per-site-rule auth (currently site rules have an `auth_ref` field
  but it is not yet wired to the downloader at byte-flow time —
  per-queue auth via `DownloadQueue.auth_user/auth_ref` is the
  currently-working path);
- additional deterministic tests covering the integration points
  the brief calls out (grabber, batch, site rules, browser
  integration).

## Section-by-section

### 1. Audit existing grabber

`engine::grab_site_with_rules` (in
`crates/downpilot-core/src/engine.rs`) is the implementation.

- Direct file links: ✅
  Scans for `href="..."` and `src="..."` attributes via a regex
  (`(?i)(?:href|src)\s*=\s*["']([^"']+)["']`). The candidate is
  resolved against the page URL with `resolve_url`.
- Downloadable documents / archives / images / audio / video
  resources: ✅
  An extension allowlist regex
  (`\.(zip|rar|7z|tar|gz|bz2|xz|iso|...|mp3|...|mp4|...|pdf|...)`)
  keeps the candidate. The result is classified into the
  engine's standard categories via `category_for_ext`.
- Page-linked downloadable resources: ✅
  The regex is `href|src`, so `<a href="...">`, `<link href="...">`,
  and `<img src="...">`, `<video src="...">`, `<source src="...">`,
  `<script src="...">`, `<iframe src="...">`, etc. all contribute.
- The implementation **does not** attempt to detect media streams
  (HLS / DASH / progressive MP4 inside a JSON manifest). The brief
  explicitly warns against claiming that; we do not claim it.
- We do not bypass DRM, paywalls, or access restrictions. The
  grabber fetches only what the user is legitimately able to fetch.

### 2. Grabber analysis

`SiteGrabberDialog` (in `src/src/components/SiteGrabberDialog.tsx`)
already implements everything the brief calls out:

- result table with: filename, URL (tooltip), category, size,
  source (via the `matched` field as a small chip in the name
  column), selected state. The brief asks for a *content type*
  column — we don't show it as a separate column. It is included in
  the tooltip on the URL and (when the user probes the row in
  BatchDialog) shown as a "Type" column. We should add a content
  type column to the grabber table too — see Improvements.
- Select All / Deselect All / Search / Sort (Name/Ext/Size/Category) /
  per-row Queue dropdown / per-row Category dropdown / common-queue
  apply-to-selected.
- The "Download Selected" path goes through `onAdd` → `store.add` →
  `manager.add`. That is the same code path the user-driven Add
  Download dialog uses, so the orchestrator (queue/concurrency/
  scheduler) applies.

### 3. Grabber filter rules

`GrabRules` (in `crates/downpilot-core/src/models.rs`) plus
`grabber_filter` (in `engine.rs`):

- extension allow / deny: ✅ via `include_extensions` and
  `exclude_extensions`. `include_extensions` is an allow-list when
  non-empty; `exclude_extensions` is always a deny-list and wins.
- MIME: ✅ via `include_mime` and `exclude_mime` (the grabber's
  `content_type` field comes from `<meta http-equiv="...">` only;
  full HEAD probing is the Batch dialog's job).
- domain: ✅ via `exclude_domains` with hostname-aware matching
  (same `host_matches_pattern` engine the Site Rules use).
- minimum / maximum size: ⚠️
  The fields exist and are stored, but the grabber doesn't probe,
  so the size filter is documented as "deferred to the UI's probe
  pass" — and the grabber dialog does *not* actually probe. This
  is a small follow-up: see Improvements.

### 4. Batch download

`BatchDialog` (in `src/src/components/BatchDialog.tsx`) plus
`parse_batch_urls` (in `engine.rs`):

- multiple URLs, newline-separated, clipboard, text-file: ✅
  `parse_batch_urls` accepts newlines OR (when there are no
  newlines) whitespace + commas, so clipboard-paste of
  space-separated URLs works too.
- numbered URL ranges: ✅ via `parse_batch_urls` (the
  `file-001..010.zip` expansion).
- preview before insert: ✅ via the BatchDialog table.
- select all / deselect all / remove: ✅.
- retry metadata detection: ✅ via the "Probe server" toggle
  (calls `probe_url` per row).
- per-row category / queue / destination dropdowns: ✅.

### 5. Batch duplicate handling

- `browser_dedup_id`: ✅ via the `downloads.browser_dedup_id` UNIQUE
  index (the brief says "continue using" — we do).
- per-URL duplicate detection: ⚠️
  `parse_batch_urls` deduplicates by string equality of the *input*
  line (so `file-001..010.zip` only expands once, and a URL pasted
  twice on adjacent lines is collapsed). It does not yet check
  against the *existing DB*. The BatchDialog preview lets the user
  manually remove rows; for true server-side dedup we need to add a
  `Db::find_by_url` precheck at the engine entry. The brief calls
  this out, so we'll add it.

### 6. Site rules

`SiteRule` model (in `models.rs`) plus `match_site` (in `engine.rs`):

- domain: ✅ `domain: String` (empty string = global default).
- category: ✅ `category: String` (empty = no override).
- queue: ✅ `queue: String`.
- save directory: ✅ `save_dir: String`.
- bandwidth: ✅ `bandwidth_kbps: i64` (0 = no override).
- proxy: ✅ `proxy_url: String` (the field is stored; the brief says
  the downloader reads it via the queue at byte-flow time — that is
  true for `DownloadQueue.proxy_url`. The `SiteRule.proxy_url` is
  reserved for per-site overrides and is not yet applied at
  byte-flow time. See Improvements.)
- user-agent: ✅ `user_agent: String` and applied at insert time
  via `db.set_user_agent` (the test
  `category_and_user_agent_apply_at_insert` in
  `site_rule_test.rs` proves this).
- authentication: ✅ `auth_ref: String` (the value of the secret is
  stored in the OS keyring by queue name; the `auth_ref` field on
  the task is a *label* — the secret is looked up at byte-flow
  time via `auth_header_for(task)`. The brief asks for per-site
  auth: this works today by attaching the credential to the queue
  the site rule maps to, and that is the design the engine uses
  end to end. See Improvements for the small UI gap.)
- capture behavior: ✅ `auto_capture: bool` (the brief mentions this
  field on site rules; the current code persists it but does not
  yet gate `submit_browser_download` on it. The Chrome extension
  always asks. See Improvements.)

### 7. Site rule priority

`match_site` (engine.rs) implements the brief's order:

1. exact hostname (`s.domain == host`),
2. subdomain pattern (most specific parent wins, by descending
   `domain.len()`),
3. global fallback (the rule with empty `domain`).

Verified by `exact_host_wins_over_subdomain_pattern`,
`most_specific_parent_wins`, and `global_rule_is_fallback_only` in
`site_rule_test.rs`.

### 8. Site rule authentication

`CredentialStore` (in `credentials.rs`) is the single credential
system. The brief says "Use the existing CredentialStore. Do not
build another credential system." — we don't.

`queue_auth_test.rs` exercises:

- configured credentials: ✅
- missing credentials: ✅
- invalid credentials: ✅ (the 401 path).
- correct credentials: ✅.
- `auth_ref` resolution: ✅ (the value lives in the keyring; the
  field on the queue is the *label*).
- secure credential lookup: ✅ (OS keyring → AES-GCM fallback).

`auth_header_for` is called at byte-flow time and returns an HTTP
Basic header. Cookies are passed through the task (from the browser
extension or from the queue's `cookies` field) and never logged.

### 9. Browser capture + site rules

`SubmitBrowserDownload` in `src/lib.rs` builds a `NewDownload`,
passes it to `manager.add`, and `manager.add` calls
`match_site(url)` and applies the rule's queue / save_dir /
category / user_agent. The brief says "no additional user
interaction should be required in Automatic mode" — the
architecture already does this. We do not gate on `auto_capture`
yet (see Improvements).

### 10. Browser capture + categories

`NewDownload::category` and `DownloadTask::category` exist.
The insert path is:

- browser capture → `NewDownload { category: Some(extension_inferred) }`,
- site rule, if any, **overrides** the category (because it is
  applied after the row is inserted),
- default category fallback (`other`) is the
  `guess_category(filename)` path.

Documented precedence (in `match_site` and `add`):

1. explicit `NewDownload::category` (set by the Grabber /
   BatchDialog / extension),
2. site rule's `category` (overrides #1),
3. extension-based `category_for_ext`,
4. `"other"`.

### 11. Authenticated browser downloads

`SubmitBrowserDownload` carries the `cookies` field from the
extension into `NewDownload::cookies`. The downloader passes them
through as a header on the first request only, scoped to the task's
host. They are not logged, not stored, not exposed to the frontend
(`Task.cookies` is `Option<String>` on the wire and is *not* in the
`DownloadTask` DTO that the React UI sees — see `models.rs` —
frontends see the `auth_configured` boolean, not the secret). When
the task is removed, the cookies go with it.

The brief asks us to verify the security requirements. They are
met. The only thing we are *not* doing is one-drive / google-drive
OAuth flows — the brief says to skip those.

### 12. Grabber → queue / category integration

Already covered by `onAdd → store.add → manager.add` (see §2). The
selected `queue` and `category` from the grabber table flow into
`NewDownload` and from there into the normal queue system.

### 13. Batch → queue / category integration

Same: `BatchDialog.startSelected` calls `store.add` per row.

### 14. IDM-style power workflow

All three paths (Grabber, Batch, Browser) converge on
`manager.add`, which applies site rules, places the task in the
queue, and lets the existing scheduler/concurrency code promote it
according to the precedence chain from Phase 3B. There is no
second download path. There is no second authentication system.

### 15. UX

Both `SiteGrabberDialog` and `BatchDialog` are dense tables, no
cards, no glassmorphism. They fit the brief.

### 16. Testing

Tests already exist for:

- `grabber_test.rs` — URL parsing, hostname rules.
- `site_rule_test.rs` — exact host, subdomain, lookalike, most
  specific parent, global fallback, case insensitivity, port
  stripping, persistence round-trip, delete-removes-only,
  category/user-agent at insert.
- `capture_rules_test.rs` — host matching, dedup, probe paths.
- `browser_capture_test.rs` — UNIQUE-index dedup safety net.
- `queue_auth_test.rs` — credential round-trip and 401 handling.

We will add a new `phase3c_integration_test.rs` that exercises the
*integration* points the brief specifically asks for, so the test
suite reflects the wiring rather than the individual primitives.

### 17. Regression

We will run the full matrix at the end:
`cargo test --workspace`, `cargo check`, `cargo build --release`,
`npx tsc --noEmit`, `node --check extension/{background,popup}.js`.
The 16 Phase 3B tests must remain green.

## What we will add in Phase 3C

1. **`GrabbedLink` source field** (column in the grabber table
   showing where the link came from: `href`, `src`, or `meta`).
2. **Size-aware grabber filter**: the `min_size` / `max_size`
   fields get applied at the BatchDialog probe step (which is the
   only place we ever have a real size to filter on). The grabber
   keeps its current "deferred to UI" comment but the BatchDialog
   gets a real `exclude below / above` toggle that hides rows.
3. **`Db::find_by_url(url)` and a `BatchDialog` precheck** that
   warns when a URL the user is about to add already exists in the
   DB.
4. **`SiteRule.auto_capture` enforcement** in
   `SubmitBrowserDownload` (the extension always sends
   `BrowserDownloadDetected`; the `SubmitBrowserDownload` path now
   looks up the matching `SiteRule` and refuses with a clear
   error if `auto_capture == false`, so users can mark a domain
   as "do not capture from").
5. **Per-queue and per-site `user_agent` actually applied at the
   request level**, not just inserted. Today the task's
   `user_agent` is set, but the downloader reads its own default.
   The downloader's `http_client_for(task)` will be taught to use
   `task.user_agent` (with the site rule's UA, if any, already
   stamped by `manager.add`).
6. **Site-rule-driven `proxy_url` on the request**. Same pattern
   as user_agent: the engine stamps the proxy on the task (or via
   a per-task override struct), the downloader honors it.
7. **`add` body returns `Vec<i64>` for batch insert** (or, more
   pragmatically, a new `manager.add_batch(nds) -> Vec<Result<i64,
   String>>` that we can call from the BatchDialog in a single
   round-trip — useful when the user has 200 URLs).
8. **New integration test file `phase3c_integration_test.rs`**
   that covers everything the brief calls out and that we know is
   currently working (so a future refactor cannot silently break
   the wiring).

We do **not** rewrite the engine. We do not change the architecture
of any of the three paths. We only (a) close a few small gaps the
audit found and (b) add tests so the gaps, once closed, stay
closed.
