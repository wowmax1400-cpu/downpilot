// Secure credential store for per-queue authentication.
//
// DownPilot never stores password material in SQLite. Passwords live
// in per-queue files under `<app_data_dir>/credentials/<queue>.cred`,
// created with mode 0600 on Unix. The presence of the file is the
// "configured" flag; the queue's `has_password` column mirrors it so
// the UI can show a `[Configured ✓]` badge without ever reading the
// value.
//
// If the `secret-service` crate is available on the target platform,
// callers can wire it in here — for now the file-based store keeps
// the build matrix simple and still gives per-user OS isolation.

use std::fs;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

/// Filename-safe digest of a queue name. The store on disk uses this
/// instead of the raw name so that queue-name leakage through a
/// directory listing isn't useful.
fn safe_key(queue: &str) -> String {
    // FxHash-style mix (no extra deps). Stable across runs.
    let mut h: u64 = 0xcbf29ce484222325;
    for b in queue.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{:016x}", h)
}

pub struct CredentialStore {
    dir: PathBuf,
}

impl CredentialStore {
    pub fn new(app_data_dir: &Path) -> Self {
        let dir = app_data_dir.join("credentials");
        let _ = fs::create_dir_all(&dir);
        Self { dir }
    }

    fn path_for(&self, queue: &str) -> PathBuf {
        self.dir.join(format!("{}.cred", safe_key(queue)))
    }

    /// Store a secret for `queue`. Creates the file with mode 0600
    /// (Unix). Replaces any existing value.
    pub fn set(&self, queue: &str, secret: &str) -> std::io::Result<()> {
        let path = self.path_for(queue);
        let mut opts = fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            opts.mode(0o600);
        }
        let mut f = opts.open(&path)?;
        use std::io::Write;
        f.write_all(secret.as_bytes())?;
        f.sync_all().ok();
        Ok(())
    }

    /// Read the stored secret for `queue`. Returns None if no
    /// credential is configured.
    pub fn get(&self, queue: &str) -> Option<String> {
        let path = self.path_for(queue);
        fs::read_to_string(&path).ok()
    }

    /// Returns true if a credential is currently configured for
    /// `queue`. The UI calls this to show `[Configured ✓]` without
    /// ever reading the value.
    pub fn is_configured(&self, queue: &str) -> bool {
        self.path_for(queue).exists()
    }

    /// Remove the stored credential for `queue`. Idempotent.
    pub fn clear(&self, queue: &str) -> std::io::Result<()> {
        let path = self.path_for(queue);
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        }
    }
}
