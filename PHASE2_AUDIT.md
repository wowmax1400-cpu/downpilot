# IDM Workflow Phase — Audit

Audit of the existing codebase against the 16 spec items, before any
work starts. **Most features already exist in some form** — the heavy
lifting (engine, DB schema, completion actions, search, properties) is
done. The work is in **filling UI gaps, polishing, and wiring exposed
but unused backend actions**.

## Already implemented (10 of 16)

| # | Spec item | State | Where |
|---|---|---|---|
| 1 | **Download History** (8/9 columns + 5/8 actions) | Mostly there | `src/src/components/HistoryPanel.tsx` |
| 2 | **Properties Dialog** (6 tabs, all 4 spec sections covered) | Complete | `src/src/components/PropertiesDialog.tsx` |
| 6 | **Global Controls** (Stop All, Delete All already in toolbar) | Mostly | `Toolbar.tsx`; `useDownloads.ts` has `resume_all/stop_all/retry_all` wired |
| 7 | **Search** (all 6 fields) | Complete | `useDownloads.ts:528-545` |
| 8 | **Table Presets** (drag, resize, hide/show columns) | Complete | `columnConfig.ts` + `ColumnsMenu.tsx` |
| 9 | **File Operations** (Open, Folder, Rename, Delete) | Mostly there | `store.openFile`, `api.openInFileManager`, `store.rename`, `store.deleteFile` |
| 11 | **Keyboard Workflow** (all 7 spec shortcuts) | Complete | `App.tsx:180-260` |
| 12 | **Drag & Drop** (URLs, text, files) | Complete | `App.tsx:120-140` |
| 13 | **Error UX** (status pills, error rows) | Mostly | `ErrorClass` model exists, `format.ts` renders labels |
| 14 | **Empty States** (basic `.panel__empty` blocks) | Mostly | scattered, need a polish pass |

## Backend already has, but UI doesn't expose (3 of 16)

| # | Spec item | Backend has | Frontend has |
|---|---|---|---|
| 3 | **Checksum Verification** (SHA-256/SHA-1/MD5) | `checksum_algorithm`, `checksum_value`, `checksum_verified` columns + DB reads. No actual hash computation. | Properties shows the values but never triggers verify |
| 4 | **Completion Actions** (all 10 variants) | `CompletionAction` enum with `None/OpenFile/OpenFolder/Notification/PlaySound/RunCommand/StartQueue/StopQueue/Suspend/Hibernate/Shutdown` | Settings probably has UI for some; not all |
| 5 | **Advanced Queue Controls** (priority, force start, queue ordering, active count, queued count, schedule status) | `priority`, `threads`, `speed_limit_kbps`, `active` | `QueuesPanel` shows most; priority/active count UI may be missing |

## Spec items I CAN'T honestly deliver in this session (3 of 16)

| # | Spec item | Why |
|---|---|---|
| 10 | **Notifications polish** (content, dedup) | Needs running app to test; frontend `notification` calls work but content polish is read-only code review |
| 15 | **Performance Review** (100/500 downloads) | Synthetic test is fine; "scrolling remains smooth" requires a real desktop, not a build |
| 16 | **Regression Testing** | Can run the existing 53 tests, can run `tsc`, can run `cargo check`. Cannot run `cargo tauri build` to verify bundle without 5+ min build time and verifying the resulting .deb/RPM works |

---

## Concrete work list (this session)

### 1. History — add 2 missing columns, 2 missing actions, multi-select
- Add "Date added" column (use `created_at`)
- Add "Queue" column (already in `t.queue`, just not rendered)
- Add "Copy Path" action (use `api.copyTextToClipboard(save_dir + filename)`)
- Add "Delete File" action (use `store.deleteFile(id)`)
- Multi-select with checkboxes
- Sortable columns (click header to sort)
- Persist sort state

### 2. Properties — verify it actually works end-to-end
- No new features; the spec is met. Will spot-check that all 4 required sections are present and editable fields save.

### 3. Checksum Verification — make it actually run
- Wire a `verify_checksum` backend command in Rust (compute SHA-256 of finished file, compare to `expected_sha256` if set)
- Frontend: "Verify" button in Properties → Advanced tab
- Frontend: status badge after verification
- Tests: 1 test that downloads a file, computes its SHA-256, compares

### 4. Completion Actions — wire all 10 in the UI
- Settings: a complete `CompletionAction` picker (already exists, probably partial)
- Wire Suspend/Hibernate/Shutdown via `systemctl` (Bazzite) or `loginctl`
- These are dangerous → require explicit opt-in AND a typed confirmation

### 5. Queue UI — add the missing fields
- Add "Active downloads / Queued count" per queue
- Add "Schedule status" if any schedule covers the queue
- Priority drag-to-reorder (optional; "do not create second queue impl")

### 6. Toolbar — add Start All, Retry Failed, Recheck
- "Start All" — `store.resumeAll()` (already exists)
- "Retry Failed" — `store.retryAll()` (already exists; needs menu item)
- "Recheck Selected" — for each selected, `store.recheckDownload(id)` (already exists)
- These are mixed-selection-aware: Recheck only enabled if all selected are failed/completed

### 7. Search — add date filter
- Date range as a compact sidebar widget (since / until), not a full panel

### 8. Table Presets — view presets (Compact/Detailed/Power User)
- Three named presets, save which one is active in localStorage
- Compact: Name / Status / Size / Speed / ETA
- Detailed: Name / Status / Size / Downloaded / Speed / ETA / Connections / Category / Queue
- Power User: everything visible

### 9. File operations — robust error handling
- All file ops already exist; the gap is **user-visible error messages**
- Wrap each in a try/catch and surface the error via the existing toast/banner system (or a small status bar message)

### 10-12. Notifications / Drag&Drop / Keyboard — no new work, already done

### 13. Error UX — already has ErrorClass
- Add the "Failed / HTTP 404 — File not found" rich text rendering where status is "failed"
- Properties dialog already shows `task.error`; add `error_category` to the General tab

### 14. Empty States — polish pass
- Sidebar All / Unfinished / History / Queue / Schedules each get a one-line empty message

### 15-16. Performance + Regression
- Synthetic 200-task test (insert 200 dummy tasks, measure search/filter time)
- `cargo test`, `cargo check`, `tsc --noEmit`, `node --check` on extension files
- Try `cargo tauri build` if time permits; otherwise document it as the next-step

---

## Approach

1. Single thread, **focused incremental changes**, real verification after each step (cargo / tsc)
2. **No parallel agents** for this — most changes touch `useDownloads.ts` and `App.tsx`, both of which have grown complex and conflict-prone
3. **Honest reporting at the end** with a list of what actually works vs. what's left
