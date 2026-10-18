//! Regression tests for the authoritative average-speed persistence
//! (Part A/C of the v1.1 spec): `finish_complete` must write
//! `avg_speed_bps` = downloaded ÷ (completed_at − started_at) exactly
//! once, so the table/details "final speed" is real, not zero.

use downpilot_core::db::Db;
use tempfile::TempDir;

#[test]
fn finish_complete_records_average_speed_from_timestamps() {
    let tmp = TempDir::new().unwrap();
    let db = Db::open(&tmp.path().join("t.db")).unwrap();

    let id = db
        .insert(
            &downpilot_core::models::NewDownload {
                url: "http://example.com/f.bin".into(),
                filename: Some("f.bin".into()),
                save_dir: None,
                threads: Some(1),
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
                max_retries: Some(0),
                retry_delay_ms: None,
                conn_timeout_ms: None,
                read_timeout_ms: None,
                checksum_algorithm: None,
                on_complete: None,
                priority: None,
                start_immediately: false,
                browser_dedup_id: String::new(),
                skip_probe: true,
                mirrors: Vec::new(),
            },
            &tmp.path().to_string_lossy(),
        )
        .unwrap();

    // Simulate a download that started 100 s ago and moved 1_000_000 bytes.
    // started_at is set by the engine at first run; write it directly here
    // (the unit under test is finish_complete's arithmetic, not the engine).
    db.set_started_at(id, now_minus(100));
    db.set_downloaded_for_tests(id, 1_000_000);

    db.finish_complete(id, 1_000_000, 1_000_000).unwrap();

    let t = db.get(id).unwrap().unwrap();
    assert_eq!(t.status, downpilot_core::models::Status::Completed);
    assert!(
        t.avg_speed_bps > 0.0,
        "avg_speed_bps must be recorded on completion, got {}",
        t.avg_speed_bps
    );
    // ~1_000_000 bytes / 100 s = 10_000 B/s (allow timestamp rounding).
    assert!(
        (9_000.0..11_000.0).contains(&t.avg_speed_bps),
        "expected ~10000 B/s, got {}",
        t.avg_speed_bps
    );
}

fn now_minus(secs: i64) -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        - secs
}
