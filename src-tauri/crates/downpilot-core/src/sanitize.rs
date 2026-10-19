//! Proxy-URL sanitization for anything that leaves the app (NMH browser
//! feed, logs, debug dumps).
//!
//! DownPilot stores proxy credentials directly in the proxy URL string in
//! the `user:pass@host:port` form (reqwest's `Proxy::all` parses and applies
//! them as HTTP Basic automatically). That means the raw proxy_url value
//! contains a secret. Any serialization that goes outside the local DB —
//! especially the native-messaging feed the Chrome extension reads — must
//! strip the userinfo so a proxy password never leaks to the browser or
//! to logs.

/// Strip the `userinfo@` (username/password) portion of a proxy URL.
///
/// Returns the scheme+host+port with any `user:pass@` removed, and the
/// extracted password for callers that need to log a masked form.
///
/// - `http://user:pass@host:8080` → `http://host:8080`
/// - `socks5://user@host:1080`   → `socks5://host:1080`
/// - `http://host:8080`          → unchanged
/// - garbage / no `://`          → unchanged (best-effort, never fail)
///
/// The password is deliberately NOT preserved; callers that need it should
/// read it from the DB directly. This function exists to redact, not to
/// extract.
pub fn sanitize_proxy_url(proxy: &str) -> String {
    let Some(scheme_end) = proxy.find("://") else {
        return proxy.to_string();
    };
    let host_start = scheme_end + 3;
    let host_part = &proxy[host_start..];
    let Some(at) = host_part.find('@') else {
        return proxy.to_string();
    };
    if at == 0 {
        return proxy.to_string();
    }
    // Only strip if there is actually userinfo before the '@'.
    if at > 0 && (host_part[..at].contains(':') || !host_part[..at].is_empty()) {
        // `host_part[..at]` is non-empty here (at > 0) so the clause is
        // "strip whenever there is any userinfo, with or without ':'".
        let stripped = format!("{}{}", &proxy[..host_start], &host_part[at + 1..]);
        stripped
    } else {
        proxy.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::sanitize_proxy_url;

    #[test]
    fn strips_user_and_password() {
        assert_eq!(
            sanitize_proxy_url("http://user:secret@proxy.example:8080"),
            "http://proxy.example:8080"
        );
    }

    #[test]
    fn strips_user_without_password() {
        assert_eq!(
            sanitize_proxy_url("http://user@proxy.example:8080"),
            "http://proxy.example:8080"
        );
    }

    #[test]
    fn strips_socks5_userinfo() {
        assert_eq!(
            sanitize_proxy_url("socks5://user:pass@proxy.example:1080"),
            "socks5://proxy.example:1080"
        );
    }

    #[test]
    fn leaves_plain_proxy_untouched() {
        assert_eq!(
            sanitize_proxy_url("http://proxy.example:8080"),
            "http://proxy.example:8080"
        );
    }

    #[test]
    fn leaves_host_with_port_untouched_when_no_userinfo() {
        // A host containing a colon (IPv6 or port) must NOT be treated as
        // userinfo:pass@host — there is no '@' after it, so it is fine.
        assert_eq!(
            sanitize_proxy_url("http://[::1]:8080"),
            "http://[::1]:8080"
        );
    }

    #[test]
    fn empty_or_garbage_is_passthrough() {
        assert_eq!(sanitize_proxy_url(""), "");
        assert_eq!(sanitize_proxy_url("not-a-url"), "not-a-url");
    }

    #[test]
    fn percent_encoded_userinfo_is_stripped() {
        assert_eq!(
            sanitize_proxy_url("http://user%40elsewhere:p%40ss@proxy.example:8080"),
            "http://proxy.example:8080"
        );
    }
}