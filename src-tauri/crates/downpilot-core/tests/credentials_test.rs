// Unit tests for the secure credential store.
//
// The previous file-based plaintext store has been replaced with a
// tiered backend:
//   1. OS keyring via the `keyring` crate (libsecret/D-Bus on Linux)
//   2. AES-256-GCM-encrypted file fallback when no D-Bus session is
//      available (e.g. headless, CI)
// The two backends share a common API. The tests below verify that
// the public surface — `set`/`get`/`is_configured`/`clear` — works
// correctly regardless of which backend is active.

use downpilot_core::credentials::CredentialStore;
use tempfile::TempDir;

fn store() -> (TempDir, CredentialStore) {
    let dir = TempDir::new().expect("tempdir");
    let s = CredentialStore::new(dir.path());
    (dir, s)
}

#[test]
fn round_trip_secret() {
    let (_dir, s) = store();
    s.set("queue-a", "alice:hunter2").expect("set");
    let got = s.get("queue-a").expect("get ok");
    assert_eq!(got, "alice:hunter2");
}

#[test]
fn is_configured_distinguishes_set_vs_unset() {
    let (_dir, s) = store();
    assert!(!s.is_configured("queue-a"));
    s.set("queue-a", "x").expect("set");
    assert!(s.is_configured("queue-a"));
    s.clear("queue-a").expect("clear");
    assert!(!s.is_configured("queue-a"));
}

#[test]
fn get_returns_none_for_unknown_queue() {
    let (_dir, s) = store();
    // No insert; `get` returns `Err(NotFound)`. We don't pin the
    // specific error variant because it may differ between the OS
    // keyring and the file backend.
    assert!(s.get("nope").is_err());
}

#[test]
fn replace_overwrites() {
    let (_dir, s) = store();
    s.set("q", "v1").unwrap();
    s.set("q", "v2").unwrap();
    assert_eq!(s.get("q").unwrap(), "v2");
}

#[test]
fn clear_is_idempotent() {
    let (_dir, s) = store();
    // Clearing an unset queue is OK (no error).
    s.clear("q").expect("clear unset");
    s.set("q", "v").unwrap();
    s.clear("q").expect("clear set");
    s.clear("q").expect("clear again");
    assert!(s.get("q").is_err());
}

#[test]
fn different_queues_are_independent() {
    let (_dir, s) = store();
    s.set("alpha", "1").unwrap();
    s.set("beta", "2").unwrap();
    assert_eq!(s.get("alpha").unwrap(), "1");
    assert_eq!(s.get("beta").unwrap(), "2");
    s.clear("alpha").unwrap();
    assert!(s.get("alpha").is_err());
    assert_eq!(s.get("beta").unwrap(), "2");
}

#[test]
fn arcs_share_state() {
    // The downloader holds a clone; a separate path writing through
    // a different handle should be visible to the first.
    let (_dir, s) = store();
    let s2 = s.clone();
    s2.set("q", "shared").unwrap();
    assert_eq!(s.get("q").unwrap(), "shared");
    s.clear("q").unwrap();
    assert!(s2.get("q").is_err());
}

#[test]
fn secret_value_is_not_written_in_plaintext_to_disk_for_known_queues() {
    // For the file fallback, the on-disk blob must NOT contain the
    // raw secret bytes. The keyring backend doesn't write to disk,
    // so we skip the assertion if no fallback file is present.
    let (dir, s) = store();
    s.set("queue-x", "supersecret-marker-7c4a9d").unwrap();
    // Walk the credentials dir and look for the marker.
    let creds_dir = dir.path().join("credentials");
    let mut found_plaintext = false;
    if let Ok(read) = std::fs::read_dir(&creds_dir) {
        for e in read.flatten() {
            if let Ok(bytes) = std::fs::read(e.path()) {
                if bytes
                    .windows(b"supersecret-marker-7c4a9d".len())
                    .any(|w| w == b"supersecret-marker-7c4a9d")
                {
                    found_plaintext = true;
                    break;
                }
            }
        }
    }
    assert!(
        !found_plaintext,
        "raw secret found in on-disk credentials file — encryption is broken"
    );
}
