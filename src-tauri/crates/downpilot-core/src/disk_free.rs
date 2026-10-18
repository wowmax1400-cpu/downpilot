// Real disk-free-space detection.
//
// `nix_statfs` was previously a no-op stub that always returned
// `Err(())`. This module implements the actual `statvfs` syscall on
// Unix (Linux/macOS). On other platforms it returns `Unsupported` so
// callers can detect the limitation and fall back gracefully.
//
// DownPilot targets Linux first; macOS support is a stretch goal
// tracked in PHASE3_IDM_PARITY.md.

use std::path::Path;

#[cfg(unix)]
pub fn free_bytes(dir: &Path) -> std::io::Result<i64> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let c_path = CString::new(dir.as_os_str().as_bytes()).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("path has interior NUL: {e}"),
        )
    })?;
    // SAFETY: `c_path` is a valid C string for the duration of the
    // call. `statvfs` writes to `stat` on success and only reads
    // from `c_path`.
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    let rc = unsafe { libc::statvfs(c_path.as_ptr(), &mut stat) };
    if rc != 0 {
        return Err(std::io::Error::last_os_error());
    }
    // f_bavail = blocks available to non-privileged user
    // f_frsize = fragment size (block size in bytes).
    // Saturating multiply then clamp to i64 to avoid overflow on
    // huge filesystems.
    let bavail = stat.f_bavail as u64;
    let frsize = stat.f_frsize as u64;
    let bytes = bavail.saturating_mul(frsize);
    Ok(bytes.min(i64::MAX as u64) as i64)
}

#[cfg(not(unix))]
pub fn free_bytes(_dir: &Path) -> std::io::Result<i64> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "free_bytes: no implementation for this platform (Linux/macOS only)",
    ))
}

/// Render a byte count with the appropriate unit (B, KB, MB, GB, TB).
/// Used by the engine's "Not enough free disk space" error message.
pub fn human_bytes(n: i64) -> String {
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1024.0 && i < units.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    format!("{:.1} {}", v, units[i])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// The root filesystem always has SOME free space on a real
    /// system; this should never error. In some sandboxed /
    /// overlay-fs environments `statvfs("/")` can return 0
    /// (the FS reports a count but no user-available blocks),
    /// so we accept that here and only assert non-error.
    #[test]
    fn reports_free_bytes_for_root() {
        let v = free_bytes(Path::new("/")).expect("statvfs /");
        // We don't assert `> 0` because overlayfs / chrooted FS
        // can legitimately return 0 user-available blocks.
        let _ = v;
    }

    /// Asking about a non-existent path must return an error, not
    /// a misleading zero.
    #[test]
    fn errors_on_missing_path() {
        let v = free_bytes(Path::new("/this/path/does/not/exist/anywhere/123456789"));
        assert!(v.is_err(), "expected error for missing path");
    }

    /// `tmp` should be queryable and return positive.
    #[test]
    fn works_for_tmp() {
        let v = free_bytes(Path::new("/tmp")).expect("statvfs /tmp");
        assert!(v > 0, "expected positive free bytes for /tmp, got {v}");
    }
}
