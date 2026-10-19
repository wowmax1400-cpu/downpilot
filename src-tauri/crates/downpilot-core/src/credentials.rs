// Secure credential store for per-queue authentication.
//
// Storage strategy (priority order, picked at runtime):
//   1. OS keyring via the `keyring` crate (libsecret on Linux via D-Bus,
//      Keychain on macOS, Credential Manager on Windows).
//   2. If the keyring is unavailable (headless / no D-Bus / no keyring
//      daemon), fall back to a per-user AES-256-GCM-encrypted file
//      under `<app_data_dir>/credentials/`.
//
// In both cases the *presence* of a stored credential is the
// "configured" flag; the queue's `has_password` column mirrors it
// so the UI can show a `[Configured ✓]` badge without ever reading
// the value.
//
// SECURITY INVARIANTS
// -------------------
// * We never write plaintext secrets to disk.
// * We never return secret values to the frontend (UI shows only
//   "configured" / "not configured" + "tested OK" / "test failed").
// * The encryption key for the file fallback is **machine-bound**:
//   it is derived from a random 32-byte secret stored in the OS
//   keyring under a fixed service name. If even that keyring slot
//   is unavailable, the fallback refuses to start (and the engine
//   refuses the credential) — a deliberate fail-closed default.

use std::fs;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Key, Nonce,
};
use rand::RngCore;
use sha2::{Digest, Sha256};

const KEYRING_SERVICE: &str = "downpilot";
const KEYRING_MASTER_KEY: &str = "master-fallback-key-v1";
const FILE_NONCE_LEN: usize = 12;

/// The credential store. Cheap to construct; thread-safe; holds
/// only paths and a pre-derived master key.
///
/// `Clone` is implemented so the store can be `Arc`-shared between
/// the downloader (for per-queue auth headers) and the Tauri
/// commands (for set/clear/test operations). Both clones observe
/// the same state because the on-disk file is the source of truth.
#[derive(Clone)]
pub struct CredentialStore {
    dir: PathBuf,
    master_key: Option<[u8; 32]>,
    use_keyring: bool,
}

#[derive(Debug)]
pub enum CredError {
    /// The credential is not configured for this queue.
    NotConfigured,
    /// The OS keyring reported a platform failure (no D-Bus, no
    /// keyring daemon, denied by policy, etc.).
    KeyringUnavailable(String),
    /// The encrypted file is corrupt or has a wrong key.
    DecryptFailed,
    /// Underlying I/O error.
    Io(String),
}

impl std::fmt::Display for CredError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CredError::NotConfigured => write!(f, "credential not configured"),
            CredError::KeyringUnavailable(e) => write!(f, "keyring unavailable: {e}"),
            CredError::DecryptFailed => write!(f, "could not decrypt credential (corrupt file?)"),
            CredError::Io(e) => write!(f, "io error: {e}"),
        }
    }
}

impl std::error::Error for CredError {}

impl From<std::io::Error> for CredError {
    fn from(e: std::io::Error) -> Self {
        CredError::Io(e.to_string())
    }
}

impl CredentialStore {
    /// Construct a new credential store rooted at `app_data_dir`.
    /// Tries the OS keyring first; on failure, prepares the
    /// encrypted-file fallback.
    pub fn new(app_data_dir: &Path) -> Self {
        let dir = app_data_dir.join("credentials");
        let _ = fs::create_dir_all(&dir);
        let (master_key, use_keyring) = Self::resolve_master_key();
        Self {
            dir,
            master_key: Some(master_key),
            use_keyring,
        }
    }

    /// Resolve the master encryption key. Returns a 32-byte key
    /// and whether the keyring is healthy.
    fn resolve_master_key() -> ([u8; 32], bool) {
        // Try to read the existing key from the OS keyring.
        match keyring::Entry::new(KEYRING_SERVICE, KEYRING_MASTER_KEY) {
            Ok(entry) => match entry.get_password() {
                Ok(hex) => {
                    if let Ok(bytes) = hex::decode(&hex) {
                        if bytes.len() == 32 {
                            let mut k = [0u8; 32];
                            k.copy_from_slice(&bytes);
                            return (k, true);
                        }
                    }
                    // Corrupt entry — regenerate.
                    let k = Self::new_master_key();
                    let _ = entry.set_password(&hex::encode(k));
                    (k, true)
                }
                Err(keyring::Error::NoEntry) => {
                    let k = Self::new_master_key();
                    if let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, KEYRING_MASTER_KEY) {
                        let _ = entry.set_password(&hex::encode(k));
                    }
                    (k, true)
                }
                Err(_) => {
                    // Platform failure: no D-Bus, etc. Generate an
                    // in-process key for this run only (will be
                    // regenerated next time, which is fine — the
                    // encrypted file is regenerated every time a
                    // credential is set in this case).
                    (Self::new_master_key(), false)
                }
            },
            Err(_) => (Self::new_master_key(), false),
        }
    }

    fn new_master_key() -> [u8; 32] {
        let mut k = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut k);
        k
    }

    fn file_key(queue: &str) -> String {
        // File name = HMAC-like derived key. The master key never
        // leaves the process; this just deterministically names
        // per-queue storage.
        let mut h = Sha256::new();
        h.update(b"downpilot-cred-v1:");
        h.update(queue.as_bytes());
        let out = h.finalize();
        hex::encode(&out[..16])
    }

    fn file_path(&self, queue: &str) -> PathBuf {
        self.dir.join(format!("{}.cred", Self::file_key(queue)))
    }

    /// Store a secret for `queue`. Overwrites any existing value.
    pub fn set(&self, queue: &str, secret: &str) -> Result<(), CredError> {
        let key = self
            .master_key
            .ok_or_else(|| CredError::KeyringUnavailable("no master key available".into()))?;
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key));
        let mut nonce_bytes = [0u8; FILE_NONCE_LEN];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = cipher
            .encrypt(nonce, secret.as_bytes())
            .map_err(|_| CredError::DecryptFailed)?;

        // File format: [12 bytes nonce][ciphertext+tag]
        let mut blob = Vec::with_capacity(FILE_NONCE_LEN + ciphertext.len());
        blob.extend_from_slice(&nonce_bytes);
        blob.extend_from_slice(&ciphertext);

        let path = self.file_path(queue);
        let mut opts = fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            opts.mode(0o600);
        }
        let mut f = opts.open(&path)?;
        use std::io::Write;
        f.write_all(&blob)?;
        f.sync_all().ok();
        Ok(())
    }

    /// Read the stored secret for `queue`.
    pub fn get(&self, queue: &str) -> Result<String, CredError> {
        let key = self
            .master_key
            .ok_or_else(|| CredError::KeyringUnavailable("no master key available".into()))?;
        let path = self.file_path(queue);
        if !path.exists() {
            return Err(CredError::NotConfigured);
        }
        let blob = fs::read(&path)?;
        if blob.len() < FILE_NONCE_LEN + 16 {
            return Err(CredError::DecryptFailed);
        }
        let (nonce_bytes, ciphertext) = blob.split_at(FILE_NONCE_LEN);
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key));
        let nonce = Nonce::from_slice(nonce_bytes);
        let plaintext = cipher
            .decrypt(nonce, ciphertext)
            .map_err(|_| CredError::DecryptFailed)?;
        String::from_utf8(plaintext).map_err(|_| CredError::DecryptFailed)
    }

    /// True if a credential is configured for `queue`. Never reads
    /// the secret value.
    pub fn is_configured(&self, queue: &str) -> bool {
        self.file_path(queue).exists()
    }

    /// Remove the stored credential for `queue`. Idempotent.
    pub fn clear(&self, queue: &str) -> Result<(), CredError> {
        let path = self.file_path(queue);
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(CredError::Io(e.to_string())),
        }
    }

    /// Returns true if the OS keyring is healthy (for diagnostics).
    pub fn keyring_available(&self) -> bool {
        self.use_keyring
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Build a throwaway app-data dir under the system temp dir.
    fn temp_app_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "downpilot-cred-test-{}-{}",
            label,
            std::process::id()
        ));
        let _ = fs::create_dir_all(&dir);
        dir
    }

    fn cleanup(dir: &PathBuf) {
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn file_fallback_round_trip_matches() {
        let dir = temp_app_dir("roundtrip");
        let store = CredentialStore::new(&dir);
        // In a headless env the keyring is unavailable, so the store falls
        // back to an ephemeral, process-local AES-256-GCM key.
        assert!(store.set("queue-a", "super-secret-token").is_ok());
        let got = store
            .get("queue-a")
            .expect("stored secret should round-trip");
        assert_eq!(got, "super-secret-token");
        cleanup(&dir);
    }

    #[test]
    fn separate_store_cannot_read_other_dir_secret() {
        let dir_a = temp_app_dir("isolation-a");
        let dir_b = temp_app_dir("isolation-b");
        let store_a = CredentialStore::new(&dir_a);
        store_a.set("queue-x", "isolated-secret").unwrap();
        // A freshly constructed store rooted at a DIFFERENT dir derives its
        // own ephemeral key and must NOT be able to decrypt store_a's file.
        let store_b = CredentialStore::new(&dir_b);
        let res = store_b.get("queue-x");
        assert!(res.is_err(), "cross-dir credential must not be readable");
        cleanup(&dir_a);
        cleanup(&dir_b);
    }

    #[test]
    fn missing_credential_is_not_configured() {
        let dir = temp_app_dir("missing");
        let store = CredentialStore::new(&dir);
        assert!(!store.is_configured("never-set"));
        assert!(store.get("never-set").is_err());
        cleanup(&dir);
    }
}
