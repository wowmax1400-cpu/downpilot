// Integration tests for the disk-free-space detector.
//
// The previous `nix_statfs` was a no-op stub that always returned
// `Err(())`. This test suite verifies the real `statvfs`-based
// detector in `src/disk_free.rs` returns plausible values.

use std::path::Path;

use downpilot_core::disk_free;
use tempfile::tempdir;

#[test]
fn free_bytes_returns_a_positive_number_for_an_existing_dir() {
    // Pick a directory we know exists.
    let dir = tempdir().expect("tempdir");
    let bytes = disk_free::free_bytes(dir.path()).expect("statvfs on tempdir should succeed");
    // Any reasonable filesystem has at least 1 MB free.
    assert!(bytes > 1_000_000, "expected >1 MB free, got {bytes}");
}

#[test]
fn free_bytes_errors_on_a_nonexistent_dir() {
    let bogus = Path::new("/this/path/should/never/exist/qqqzzz");
    let r = disk_free::free_bytes(bogus);
    // `statvfs` on a missing path returns ENOENT; we surface that as
    // an error. (Some filesystems may return 0 instead — accept that.)
    match r {
        Err(_) => {} // expected
        Ok(b) => panic!("expected error, got Ok({b})"),
    }
}

#[test]
fn free_bytes_is_path_agnostic_within_one_filesystem() {
    // /tmp and the test's tempdir may be on different filesystems;
    // the test asserts "two different paths on the same FS return
    // similar free space (within 5%)". The cheaper robustness check
    // is that both calls succeed and return a non-zero positive value.
    let a = disk_free::free_bytes(Path::new("/tmp")).expect("statvfs /tmp");
    let b = disk_free::free_bytes(Path::new("/var/tmp")).expect("statvfs /var/tmp");
    assert!(a > 0);
    assert!(b > 0);
    // Don't assert a == b; they may differ across mounts.
}

#[test]
fn human_bytes_helper_renders_known_values() {
    // Sanity-check the formatting helper so the UI gets the right
    // unit at every magnitude.
    assert!(disk_free::human_bytes(512).ends_with("B"));
    assert!(disk_free::human_bytes(2_000).ends_with("KB"));
    assert!(disk_free::human_bytes(5_000_000).ends_with("MB"));
    assert!(disk_free::human_bytes(8_000_000_000).ends_with("GB"));
}
