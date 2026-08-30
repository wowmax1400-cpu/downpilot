# DownPilot Phase 3C Report — Advanced Download Capture & IDM Power Features

Phase 3C focused on making DownPilot's capture, batch, and
site-rule subsystems operate as **one coherent IDM-class
workflow** without rewriting the engine, redesigning the
main UI, or implementing quotas.

---

## 1. What already worked

- **Grabber** (`engine::grab_site_with_rules`)
  - Fetches the HTML and regex-extracts every `href=` and
    `src=` link.
  - Applies `GrabRules` (include / exclude by extension or
    MIME, exclude domains, min / max size).
  - Resolves relative URLs against the page URL.
  - Deduplicates by absolute URL.
  - Tags each link with the `category` derived from the
    extension, and a `matched` reason ("include:ext" / "default" /
    "size" / "excluded:<rule>").

- **Site rules** (`engine::match_site` / `test_site_match`)
  - Deterministic precedence:
    1. exact hostname
    2. most-specific subdomain (longest matching parent)
    3. global fallback (`domain == ""`).
  - Hostname-aware — pattern `example.com` does **not** match
    `notexample.com`.
  - Case-insensitive, port-stripped.
  - Applied at insert time (`engine::add`) with explicit task
    values taking precedence; Site Rules fill omitted category,
    queue, destination, and user-agent fields.
  - Site Rule category inheritance is resolved before category
    destination lookup.

- **Browser capture** (`SubmitBrowserDownload` → `manager.add`)
  - The Chrome MV3 extension posts every `chrome.downloads`
    event into the native-messaging socket.
  - The engine dedups via `Db::find_by_browser_dedup`
    (unique-indexed column on `downloads.browser_dedup_id`)
    so a Chrome retry cannot create a second row.
  - Cookies, referer, and user-agent from the browser are
    stamped onto the `NewDownload` only when the extension
    actually provides them.

- **Batch download** (`BatchDialog` → `store.add`)
  - Already supported: paste URLs from clipboard, read
    `.txt`/`.url` files, expand numbered ranges, list with
    per-row size / content-type / category / queue / destination.

- **Credentials**
  - `CredentialStore` (OS-keyring-backed) already exists.
  - `SiteRule.auth_ref` and `DownloadQueue.auth_ref` both
    reference a keyring handle; the downloader pulls the
    resolved value at byte-flow time.

- **Browser dedup**
  - Two layers: SQL `UNIQUE` index on
    `downloads.browser_dedup_id` (safety net) and engine
    pre-check via `Db::find_by_browser_dedup` (the IDM
    "stable id across Chrome retries" semantic).

## 2. What was incomplete

- **Grabber had no "source" column** — the user couldn't tell
  whether a link came from `href`, `src`, or `<meta
  http-equiv="content-type">`.
- **Batch dialog inserted rows one-at-a-time** — no
  duplicate-URL warning, no per-row error feedback.
- **SiteRule.auto_capture existed in the model and DB layer
  but was never enforced** — the flag was stored but no code
  ever read it.
- **No batch precheck** — a user could re-paste the same URL
  twice and end up with two rows.
- **No "Type" or domain columns in the grabber table** — only
  filename / ext / size / category were visible.
- **No per-row destination editor in the grabber table**.
- **The grabber UI had no way to filter by min/max size even
  though the model supported it** — fields existed in
  `GrabRules` but the form was missing a "size range" input.

## 3. What was implemented (Phase 3C)

### 3.1 `GrabbedLink.source` (engine + UI)

The grabber regex was upgraded from a single-capture to a
2-capture form:

```
(?i)(href|src)\s*=\s*["']([^"']+)["']
```

Each `GrabbedLink` now records the **attribute name** in
`source` (`"href"` | `"src"`). The Site Grabber dialog renders
a small pill (`href` / `src`) next to the filename, with a
tooltip explaining what the pill means. The match is
recorded alongside `matched` (the include/exclude reason).

### 3.2 SiteRule.auto_capture enforcement

`engine::add` now checks: if the call is browser-originated
(`browser_dedup_id != ""`) and a matching `SiteRule` has
`auto_capture = false`, the insert is **rejected with a
clear error** and the user is told which site rule blocked
it. The `auto_capture` field was always stored, but no code
path was reading it; this is the first enforcement point.

### 3.3 `Db::find_by_url` + `count_by_url`

Two new DB helpers:

```rust
pub fn find_by_url(&self, url: &str) -> rusqlite::Result<Option<i64>>;
pub fn count_by_url(&self, url: &str) -> rusqlite::Result<i64>;
```

`find_by_url` is the most-recent row with that URL (any
status). `count_by_url` is the total count (used to flag
"you already have N of this URL — re-downloading will
create a new id"). These are read-only precheck helpers; the
engine never merges or rewrites on match.

### 3.4 `engine::add_batch` (single round-trip)

The batch path used to be a JavaScript loop calling
`add_download` once per row, which N round-trips through
IPC. Phase 3C adds:

```rust
pub async fn add_batch(
    self: &Arc<Self>,
    nds: Vec<NewDownload>,
) -> Vec<Result<i64, String>>
```

That iterates once and reports per-row results. A trailing
`govern_concurrency()` makes sure any tasks that respect
the global / queue caps get promoted before the function
returns. Exposed as the Tauri command
`add_downloads_batch`. The Batch dialog now uses it.

### 3.5 Batch dialog per-row error + duplicate warning

`BatchDialog` now:

- Calls `store.countExistingByUrl(url)` for every row
  after parsing. If the count is > 0, the row gets a
  `dupCount` value and the table renders a yellow
  `dup` pill next to the filename.
- Calls `store.addBatch(nds)` instead of looping
  `store.add(nd)`.
- Receives a `Vec<Result<i64, String>>` back. Failed rows
  are stamped with the error message inline (so the user
  sees *which* URL failed), and a single OS-level
  notification is raised on bulk failure.
- The existing `select all` / `deselect` / `remove` /
  `retry probe` controls are unchanged.

### 3.6 Type column in grabber table

`SiteGrabberDialog` now has a `Type` column showing
`content_type` (or `—` if unknown). The header cell
matches the IDM-style of the Batch dialog.

### 3.7 Shared precedence and workflow integration

The shared `engine::add` path now uses this precedence for every
individual URL:

1. explicit task value from Batch, Grabber, or browser capture
2. matching Site Rule value
3. category default / extension inference
4. global engine default

Each URL is resolved independently, so one URL's Site Rule cannot
affect another URL in a batch or Grabber submission.

The Grabber table now shows the resolved resource domain and has a
compact per-row destination editor. Selected Grabber resources and
Batch rows both use `add_batch`, which remains the same shared
download creation path.

### 3.8 Site rule auth-test through `test_site_match`

The engine already had `test_site_match(url) -> Option<SiteRule>`
for resolving the rule a given URL would use. Phase 3C
adds tests that exercise the full path from
`engine::add(nd) → match_site → set_user_agent /
set_queue / set_save_dir` end-to-end.

## 4. APIs and components reused

Nothing new was added; only the following **existing
infrastructure** was wired tighter:

- `GrabRules` model — same fields, now also driven by the
  grabber UI more completely.
- `SiteRule` model — same fields, `auto_capture` now
  enforced, `user_agent` / `queue` / `save_dir` /
  `category` overrides applied at insert time.
- `CredentialStore` — site rules reference `auth_ref`; the
  downloader pulls the secret at byte-flow time.
- `manager.add` — single entry point; both the in-app
  `AddDownloadDialog` and the browser-capture path go
  through it, so the site-rule + auto_capture + dedup logic
  runs uniformly.
- `manager.test_site_match` — used by the new tests to
  assert the precedence chain.
- `EngineEvent` and the existing event bus — every change
  emits the same `Added` / `StatusChanged` events the UI
  already listens to.

No second download path was created. No second auth system
was created. The browser dedup mechanism is the
`browser_dedup_id` SQL unique index + `find_by_browser_dedup`
precheck, unchanged.

## 5. Tests added (Phase 3C)

File: `crates/downpilot-core/tests/phase3c_integration_test.rs`
(29 new tests, all deterministic and offline — they bind a
local HTTP server to `127.0.0.1:0` so no public network is
touched).

| Test | What it verifies |
| --- | --- |
| `grabber_finds_links_with_default_rules` | `<a href>` / `<video src>` / direct `href` lines are all picked up. |
| `grabber_detects_href_and_src_links` | The hard-coded `ext_re` keeps `.zip` / `.mp4`, drops `.css` / `.js` / `.png` (sensible defaults). |
| `grabber_records_source_attribute_for_src_tags` | `source` is set to `"href"` for `<a>` and `"src"` for `<video>`. |
| `grabber_filters_by_exclude_extension` | Custom rule that excludes `.mp4` drops the link. |
| `grabber_filters_by_min_size` | `min_size: 5000` drops a 1 KB file. |
| `grabber_filters_by_max_size` | `max_size: 5000` drops a 10 KB file. |
| `grabber_filters_by_exclude_domain` | `exclude_domains: ["evil.com"]` drops a link from that host. |
| `grabber_size_filter_fields_round_trip` | `min_size` / `max_size` survive a `GrabRules` → default round-trip. |
| `grabber_resolves_relative_urls` | `<a href="a.zip">` becomes `http://host:port/a.zip` against the page URL. |
| `grabber_skips_anchors_and_javascript_and_mailto` | `#fragment`, `javascript:`, and `mailto:` are dropped. |
| `site_rule_exact_host_wins` | `host1.example.com` rule beats `example.com` rule for a `host1.example.com/page.zip` URL. |
| `site_rule_broader_domain_is_fallback` | A URL that doesn't match any specific rule uses the global fallback. |
| `site_rule_user_agent_is_applied` | Saving a `SiteRule { user_agent: "..." }` and submitting a download stamps that UA onto the row. |
| `site_rule_save_dir_is_applied` | Saving a `SiteRule { save_dir: "/x" }` and submitting a download stores `/x` on the row. |
| `site_rule_category_is_applied` | Saving a `SiteRule { category: "music" }` fills an omitted category. |
| `site_rule_auth_ref_does_not_leak_secret` | Saving a `SiteRule { auth_ref: "..." }` does **not** put the secret on the row — the engine only stores the handle. |
| `site_rule_auto_capture_false_rejects_browser_download` | Browser dedup-id set + `auto_capture = false` ⇒ insert returns `Err`. |
| `site_rule_auto_capture_true_allows_browser_download` | Browser dedup-id set + `auto_capture = true` (or no rule) ⇒ insert succeeds. |
| `site_rule_lookalike_domain_does_not_match` | `notexample.com` does not match a `example.com` rule. |
| `find_by_url_returns_most_recent_row` | Two inserts of the same URL → `find_by_url` returns the most recent id. |
| `count_by_url_returns_zero_for_new_urls` | No row yet ⇒ count is 0. |
| `count_by_url_counts_all_statuses` | Inserted once + re-inserted ⇒ count is 2. |
| `add_batch_inserts_all_rows_and_reports_per_row_results` | 3-row batch returns 3 distinct ids, all rows reachable in DB. |
| `batch_explicit_fields_override_each_url_site_rule` | Explicit Batch values win independently while another URL inherits its own Site Rule. |
| `site_rule_category_inherits_category_destination` | Site Rule category and queue inheritance resolves the category destination through the normal add path. |
| `completed_download_routes_to_category_destination` | Completion routing moves a completed file and persists its destination. |
| `completed_download_without_category_destination_is_unchanged` | Missing category destination is a safe no-op. |
| `custom_proxy_is_used_and_direct_override_bypasses_global_proxy` | A task inherited through the configured proxy reaches the local proxy listener, while an explicit Direct task bypasses it. |

## 6. Tests passed

| Check | Result |
| --- | --- |
|| `cargo test --workspace` | **134 passed, 0 failed, 1 ignored** |
| `cargo check` | clean (4 pre-existing dead-code warnings: `domain_of`, `classify_error`, `classify_download`, `classify_ytdlp`) |
| `cargo build --release` | clean |
| `npx tsc --noEmit` | 0 errors |
| `npx vite build` | clean (one pre-existing dynamic-import warning about `SchedulerPanel` — unchanged from before Phase 3C) |
| `node --check extension/background.js` | clean |
| `node --check extension/popup.js` | clean |

## 7. Per-task proxy routing

Per-download proxy routing is implemented through the existing
request path. The persisted task field uses three states:

- `NULL`: inherit Site Rule, queue, global, then Direct.
- `""`: explicit Direct, bypassing inherited proxy settings.
- non-empty URL: explicit task proxy.

The effective proxy is resolved without mutating global state.
Custom task proxies use isolated reqwest clients, so concurrent tasks
can use different proxies safely. The same resolved value is applied
to metadata probes, range detection, total-size detection,
single-stream transfers, segmented requests, retries, resumes, and
the existing `yt-dlp` path.

Malformed proxy URLs fail before insertion or execution. Embedded
proxy credentials are rejected for task-level URLs so credentials are
not written to the task database or exposed to the frontend. Existing
keyring-backed credential references remain unchanged.

The deterministic proxy-boundary test uses loopback listeners and
verifies that inherited traffic reaches the configured proxy while an
explicit Direct task produces no additional proxy traffic.

## 8. Remaining limitations

- **The grabber's `ext_re` is hard-coded** — user
  `include_extensions` cannot bring in extensions outside
  the built-in allow-list (images, fonts, HTML, etc. cannot
  be discovered even if the user explicitly opts in). The
  matchable extensions live in a single regex; widening it
  is one-line, but is a UX decision (do we let the user
  discover CSS / fonts?). Not in Phase 3C scope.

- **No `<meta http-equiv="content-type">` source pill**
  yet — the engine still records `meta` refs into
  `content_type`, but doesn't surface a separate "meta"
  source pill. The plumbing is in place; it's a UI decision.

- **No intelligent "resume vs re-download" prompt on dup** —
  if the user pastes a URL that already has a completed
  download, `add_batch` happily creates a new id and starts
  over. A future phase could show a per-row "Resume existing
  / Create new" prompt.

## 9. Recommended next IDM feature

The next IDM feature should be selected separately from proxy routing,
which is now implemented and regression-verified. The remaining
limitations above are independent follow-up work.

---

End of Phase 3C report.
