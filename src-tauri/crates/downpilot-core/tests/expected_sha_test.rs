// Tests for the `set_expected_sha256` path introduced during the
// Phase 3A hardening pass. The user-visible fix: a SHA-256 the
// user enters in the Properties dialog must:
//   1. round-trip through the DB
//   2. survive a "restart" (open a fresh Db on the same file)
//   3. lower-case normalize "AB..." to "ab..."
//   4. trim whitespace
//   5. be cleared by passing None or empty

use downpilot_core::db::Db;
use downpilot_core::models::{NewDownload, Status};
use tempfile::tempdir;

fn nd(url: &str) -> NewDownload {
    NewDownload {
        url: url.to_string(),
        filename: Some("x.bin".into()),
        save_dir: Some("/tmp".into()),
        threads: None,
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
        browser_dedup_id: String::new(),
        skip_probe: false,
        mirrors: Vec::new(),
    }
}

#[test]
fn set_expected_sha256_round_trips() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("test.db");
    let db = Db::open(&path).unwrap();
    let id = db.insert(&nd("https://example.com/x"), "/tmp").unwrap();
    db.set_expected_sha256(id, Some("ABCDEF1234")).unwrap();
    let task = db.get(id).unwrap().expect("task");
    assert_eq!(task.expected_sha256.as_deref(), Some("abcdef1234"));
}

#[test]
fn set_expected_sha256_survives_db_reopen() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("test.db");
    {
        let db = Db::open(&path).unwrap();
        let id = db.insert(&nd("https://example.com/x"), "/tmp").unwrap();
        db.set_expected_sha256(id, Some("DEADBEEF")).unwrap();
    }
    // Reopen the same DB. The value must still be there.
    let db = Db::open(&path).unwrap();
    let mut found = false;
    for t in db.list().unwrap_or_default() {
        if t.status == Status::Queued {
            assert_eq!(t.expected_sha256.as_deref(), Some("deadbeef"));
            found = true;
        }
    }
    assert!(found, "task not found after restart");
}

#[test]
fn set_expected_sha256_normalizes_whitespace_and_case() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("test.db");
    let db = Db::open(&path).unwrap();
    let id = db.insert(&nd("https://example.com/x"), "/tmp").unwrap();
    db.set_expected_sha256(id, Some("  ABC123  ")).unwrap();
    let task = db.get(id).unwrap().expect("task");
    assert_eq!(task.expected_sha256.as_deref(), Some("abc123"));
}

#[test]
fn set_expected_sha256_clears_with_empty_string() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("test.db");
    let db = Db::open(&path).unwrap();
    let id = db.insert(&nd("https://example.com/x"), "/tmp").unwrap();
    db.set_expected_sha256(id, Some("anything")).unwrap();
    db.set_expected_sha256(id, Some("")).unwrap();
    let task = db.get(id).unwrap().expect("task");
    assert!(task.expected_sha256.is_none(), "empty string should clear");
}

#[test]
fn set_expected_sha256_clears_with_none() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("test.db");
    let db = Db::open(&path).unwrap();
    let id = db.insert(&nd("https://example.com/x"), "/tmp").unwrap();
    db.set_expected_sha256(id, Some("set")).unwrap();
    db.set_expected_sha256(id, None).unwrap();
    let task = db.get(id).unwrap().expect("task");
    assert!(task.expected_sha256.is_none(), "None should clear");
}
