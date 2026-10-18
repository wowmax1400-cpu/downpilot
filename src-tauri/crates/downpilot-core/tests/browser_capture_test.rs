//! Tests for the browser-capture dedup pipeline.
//!
//! Two layers cooperate:
//!   1. A UNIQUE index on `downloads.browser_dedup_id` makes the SQL
//!      layer a safety net — even if the engine's pre-check is bypassed
//!      (a future refactor, a different process, etc.) the database
//!      still refuses to double-insert.
//!   2. The engine's pre-check via `find_by_browser_dedup` returns the
//!      existing id to the caller without re-inserting or re-probing —
//!      the `submit_browser_download` NMH handler relies on this so the
//!      extension sees a stable id even when Chrome retries the same
//!      download.

use downpilot_core::db::Db;
use downpilot_core::models::NewDownload;
use tempfile::tempdir;

fn nd(url: &str, dedup: &str) -> NewDownload {
    NewDownload {
        url: url.to_string(),
        filename: Some(format!("test-{dedup}.bin")),
        save_dir: Some("/tmp".to_string()),
        threads: Some(2),
        format_id: None,
        webpage_url: None,
        referer: None,
        user_agent: None,
        cookies: None,
        proxy_url: None,
        expected_sha256: None,
        category: None,
        queue: None,
        description: None,
        max_retries: None,
        retry_delay_ms: None,
        conn_timeout_ms: None,
        read_timeout_ms: None,
        checksum_algorithm: None,
        on_complete: None,
        priority: None,
        start_immediately: false,
        browser_dedup_id: dedup.to_string(),
        skip_probe: false,
        mirrors: Vec::new(),
    }
}

#[test]
fn first_insert_with_dedup_id_succeeds_and_is_findable() {
    let tmp = tempdir().unwrap();
    let db = Db::open(&tmp.path().join("test.db")).unwrap();
    let id = db
        .insert(&nd("https://example.com/a.zip", "ext-1"), "/tmp")
        .unwrap();
    let found = db.find_by_browser_dedup("ext-1").unwrap();
    assert_eq!(found, Some(id));
}

#[test]
fn sql_layer_rejects_duplicate_dedup_id() {
    // The safety net: even if the engine pre-check is skipped, the UNIQUE
    // index guarantees we never end up with two rows for the same
    // browser download.
    let tmp = tempdir().unwrap();
    let db = Db::open(&tmp.path().join("test.db")).unwrap();
    db.insert(&nd("https://example.com/a.zip", "ext-A"), "/tmp")
        .unwrap();
    let second = db.insert(&nd("https://example.com/a.zip", "ext-A"), "/tmp");
    assert!(
        second.is_err(),
        "UNIQUE index must reject the second insert"
    );
}

#[test]
fn empty_dedup_id_always_inserts() {
    // The "no browser context" path (eg. user added a download via the
    // app's own Add dialog) must always create a new row, even for the
    // same URL.
    let tmp = tempdir().unwrap();
    let db = Db::open(&tmp.path().join("test.db")).unwrap();
    let a = db
        .insert(&nd("https://example.com/a.zip", ""), "/tmp")
        .unwrap();
    let b = db
        .insert(&nd("https://example.com/a.zip", ""), "/tmp")
        .unwrap();
    assert_ne!(a, b);
}

#[test]
fn different_dedup_ids_create_independent_rows() {
    let tmp = tempdir().unwrap();
    let db = Db::open(&tmp.path().join("test.db")).unwrap();
    let a = db
        .insert(&nd("https://example.com/a.zip", "ext-A"), "/tmp")
        .unwrap();
    let b = db
        .insert(&nd("https://example.com/a.zip", "ext-B"), "/tmp")
        .unwrap();
    assert_ne!(a, b);
    assert_eq!(db.find_by_browser_dedup("ext-A").unwrap(), Some(a));
    assert_eq!(db.find_by_browser_dedup("ext-B").unwrap(), Some(b));
}

#[test]
fn find_returns_none_for_unknown_id() {
    let tmp = tempdir().unwrap();
    let db = Db::open(&tmp.path().join("test.db")).unwrap();
    assert!(db.find_by_browser_dedup("never-seen").unwrap().is_none());
}
