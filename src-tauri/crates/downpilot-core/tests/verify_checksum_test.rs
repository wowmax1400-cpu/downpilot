// Integration test for `DownloadManager::verify_checksum`.
//
// We don't need a full download pipeline here — `verify_checksum` reads
// the file at `save_dir/filename` regardless of how it got there. So
// the test seeds a temp dir with a known file, inserts a task row that
// points at it (with and without an expected digest), and checks that
// the verification result matches the file's actual SHA-256.
//
// This catches:
//   - Hash math regressions (the worst case — silent corruption detection)
//   - Off-by-one in the read loop (the most common bug in hashers)
//   - The `expected` normalization (lowercase + trim) working right

use downpilot_core::engine::DownloadManager;
use downpilot_core::models::{ChecksumResult, NewDownload};
use sha2::{Digest, Sha256};
use std::io::Write;
use tempfile::TempDir;

const FIXTURE: &[u8] = b"the quick brown fox jumps over the lazy dog\n\
                        DownPilot verify_checksum test fixture v1\n\
                        -----------------------------------------\n";

fn expected_sha256_of(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    hex::encode(h.finalize())
}

fn write_fixture(path: &std::path::Path) {
    let mut f = std::fs::File::create(path).expect("create fixture");
    f.write_all(FIXTURE).expect("write fixture");
}

fn make_manager(tmp: &TempDir) -> std::sync::Arc<DownloadManager> {
    let db = tmp.path().join("db.sqlite");
    let parts = tmp.path().join("parts");
    std::fs::create_dir_all(&parts).expect("parts dir");
    let (mgr, _rx) = DownloadManager::new(&db, parts);
    mgr
}

/// Build a `NewDownload` that points at `save_dir/filename` and is
/// inserted as `Queued` (not `Downloading`) so the verify path can
/// operate on a stable on-disk file without a downloader racing it.
fn new_task(
    url: &str,
    filename: &str,
    save_dir: &std::path::Path,
    expected_sha256: Option<String>,
) -> NewDownload {
    NewDownload {
        url: url.to_string(),
        filename: Some(filename.to_string()),
        save_dir: Some(save_dir.to_string_lossy().into_owned()),
        threads: Some(1),
        format_id: None,
        webpage_url: None,
        referer: None,
        user_agent: None,
        cookies: None,
        proxy_url: None,
        expected_sha256,
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
        skip_probe: true,
        mirrors: Vec::new(),
    }
}

#[tokio::test]
async fn verify_checksum_matches_a_correct_expected_digest() {
    let tmp = TempDir::new().expect("tmp");
    let mgr = make_manager(&tmp);
    let save_dir = tmp.path().join("downloads");
    std::fs::create_dir_all(&save_dir).expect("downloads dir");
    let file_path = save_dir.join("fox.txt");
    write_fixture(&file_path);
    let expected = expected_sha256_of(FIXTURE);

    let id = mgr
        .add(new_task(
            "https://example.com/fox.txt",
            "fox.txt",
            &save_dir,
            Some(expected.clone()),
        ))
        .await
        .expect("add");

    let res: ChecksumResult = mgr.verify_checksum(id).expect("verify");
    assert_eq!(res.algorithm, "sha256");
    assert_eq!(res.computed, expected, "computed digest must match file");
    assert_eq!(
        res.expected, expected,
        "expected must be normalized to lowercase"
    );
    assert_eq!(res.matches, Some(true), "match flag should be true");
    assert_eq!(res.file_size, FIXTURE.len() as u64);
}

#[tokio::test]
async fn verify_checksum_detects_a_mismatch() {
    let tmp = TempDir::new().expect("tmp");
    let mgr = make_manager(&tmp);
    let save_dir = tmp.path().join("downloads");
    std::fs::create_dir_all(&save_dir).expect("downloads dir");
    let file_path = save_dir.join("fox.txt");
    write_fixture(&file_path);
    // A wrong-but-syntactically-valid 64-char hex digest. Generated
    // by repeating "deadbeef" 8 times; definitely not the file's hash.
    let wrong = "deadbeef".repeat(8);

    let id = mgr
        .add(new_task(
            "https://example.com/fox.txt",
            "fox.txt",
            &save_dir,
            Some(wrong.clone()),
        ))
        .await
        .expect("add");

    let res = mgr.verify_checksum(id).expect("verify");
    assert_eq!(res.matches, Some(false), "wrong digest must not match");
    // The recorded `computed` is still the real file hash; that's the
    // useful thing to surface to the user so they can re-download.
    assert_eq!(res.computed, expected_sha256_of(FIXTURE));
}

#[tokio::test]
async fn verify_checksum_without_expected_still_records_actual() {
    let tmp = TempDir::new().expect("tmp");
    let mgr = make_manager(&tmp);
    let save_dir = tmp.path().join("downloads");
    std::fs::create_dir_all(&save_dir).expect("downloads dir");
    write_fixture(&save_dir.join("fox.txt"));

    let id = mgr
        .add(new_task(
            "https://example.com/fox.txt",
            "fox.txt",
            &save_dir,
            None,
        ))
        .await
        .expect("add");

    let res = mgr.verify_checksum(id).expect("verify");
    assert_eq!(res.matches, None, "no expected → matches is None");
    assert_eq!(res.computed, expected_sha256_of(FIXTURE));
    // And the DB should now contain the recorded value so a future
    // UI read doesn't need to recompute.
    let task = mgr.list().into_iter().find(|t| t.id == id).expect("task");
    assert_eq!(task.checksum_algorithm, "sha256");
    assert_eq!(task.checksum_value, expected_sha256_of(FIXTURE));
    // No expected was supplied, so the verified flag stays false —
    // we don't claim verification without an expected to compare to.
    assert!(!task.checksum_verified);
}

#[tokio::test]
async fn verify_checksum_accepts_uppercase_expected() {
    // Real users copy/paste SHA-256 digests in mixed case all the
    // time. The comparison must be case-insensitive.
    let tmp = TempDir::new().expect("tmp");
    let mgr = make_manager(&tmp);
    let save_dir = tmp.path().join("downloads");
    std::fs::create_dir_all(&save_dir).expect("downloads dir");
    write_fixture(&save_dir.join("fox.txt"));
    let lc = expected_sha256_of(FIXTURE);
    let upper = lc.to_uppercase();

    let id = mgr
        .add(new_task(
            "https://example.com/fox.txt",
            "fox.txt",
            &save_dir,
            Some(upper),
        ))
        .await
        .expect("add");

    let res = mgr.verify_checksum(id).expect("verify");
    assert_eq!(res.matches, Some(true), "uppercase expected must match");
    assert_eq!(res.expected, lc, "expected is normalized to lowercase");
}

#[tokio::test]
async fn verify_checksum_errors_on_missing_file() {
    let tmp = TempDir::new().expect("tmp");
    let mgr = make_manager(&tmp);
    let save_dir = tmp.path().join("downloads");
    std::fs::create_dir_all(&save_dir).expect("downloads dir");
    // NB: do NOT write the file. The task points at a nonexistent path.

    let id = mgr
        .add(new_task(
            "https://example.com/missing.bin",
            "missing.bin",
            &save_dir,
            None,
        ))
        .await
        .expect("add");

    let err = mgr.verify_checksum(id).expect_err("should error");
    // We don't assert the exact text — the OS-level error string varies
    // by platform — but it should be a present-tense complaint, not a
    // crash. A crash would fail the test process entirely.
    assert!(
        !err.is_empty(),
        "error message should explain what went wrong"
    );
}

#[tokio::test]
async fn verify_checksum_handles_a_binary_blob() {
    // Guard against silent read bugs (off-by-one, alignment, etc.) by
    // hashing 1 MiB of pseudo-random data and comparing to a known-good
    // implementation's result.
    let tmp = TempDir::new().expect("tmp");
    let mgr = make_manager(&tmp);
    let save_dir = tmp.path().join("downloads");
    std::fs::create_dir_all(&save_dir).expect("downloads dir");

    // Deterministic 1 MiB pseudo-random blob. We don't need crypto
    // quality — we just need bytes that don't compress into a hash
    // shortcut.
    let mut data = Vec::with_capacity(1024 * 1024);
    let mut state: u32 = 0x1234_5678;
    for _ in 0..(1024 * 1024 / 4) {
        // xorshift32
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        data.extend_from_slice(&state.to_le_bytes());
    }
    let file_path = save_dir.join("blob.bin");
    std::fs::write(&file_path, &data).expect("write blob");
    let expected = expected_sha256_of(&data);

    let id = mgr
        .add(new_task(
            "https://example.com/blob.bin",
            "blob.bin",
            &save_dir,
            Some(expected.clone()),
        ))
        .await
        .expect("add");

    let res = mgr.verify_checksum(id).expect("verify");
    assert_eq!(res.computed, expected);
    assert_eq!(res.matches, Some(true));
    assert_eq!(res.file_size, 1024 * 1024);
}
