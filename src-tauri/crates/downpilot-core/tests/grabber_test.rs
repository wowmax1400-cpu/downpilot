//! Deterministic tests for the Download Grabber.
//!
//! The grabber fetches a real page, extracts href/src links, then
//! filters them through `GrabRules`. We spin up a loopback HTTP server
//! that returns a small page with a known set of links and verify the
//! engine's `grab_site_with_rules` returns the expected subset, in
//! the expected order, with the right inferred categories and
//! `matched` reasons.

use downpilot_core::engine::DownloadManager;
use downpilot_core::models::GrabRules;
use std::io::Write;
use std::net::TcpListener;
use std::thread;

fn spawn_page_server(html: &'static str) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || {
        // One connection — the grabber does a single GET.
        if let Ok((mut stream, _)) = listener.accept() {
            // Set a read timeout so a buggy grabber doesn't hang the test.
            let _ = stream.set_read_timeout(Some(std::time::Duration::from_millis(500)));
            // Read request (ignored) ...
            let mut buf = [0u8; 2048];
            let _ = std::io::Read::read(&mut (&stream as &std::net::TcpStream), &mut buf);
            // ... then reply.
            let body = html.as_bytes();
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(header.as_bytes());
            let _ = stream.write_all(body);
            let _ = stream.flush();
        }
    });
    port
}

const HTML: &str = r#"
<html><body>
<a href="https://example.com/files/ubuntu-22.04.iso">Ubuntu</a>
<a href="https://example.com/files/movie-1080p.mp4">Movie 1080p</a>
<a href="https://example.com/files/movie-720p.mp4">Movie 720p</a>
<a href="https://example.com/files/album.zip">Album</a>
<a href="https://example.com/files/track.mp3">Track</a>
<a href="https://example.com/files/book.pdf">Book</a>
<a href="https://example.com/files/subtitles.srt">Subs</a>
<a href="https://example.com/files/logo.png">Logo</a>
<a href="https://example.com/files/style.css">CSS</a>
<a href="https://example.com/files/script.js">JS</a>
<a href="/relative/relative.zip">Relative</a>
<a href="//cdn.example.com/files/cdn.zip">CDN protocol-relative</a>
<a href="https://other.com/files/external.zip">External</a>
</body></html>
"#;

#[tokio::test]
async fn default_rules_keep_downloads_drop_html_assets() {
    let port = spawn_page_server(HTML);
    let url = format!("http://127.0.0.1:{port}/page.html");
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let state_dir = dir.path().join("state");
    std::fs::create_dir_all(&state_dir).unwrap();
    let (mgr, _rx) = DownloadManager::new(&db_path, state_dir);
    let links = mgr.grab_site(&url, 100).await.unwrap();
    // Keep set: iso, mp4, mp4, zip, mp3, pdf, srt, zip, zip, zip
    // Drop set: png, css, js (excluded)
    let urls: Vec<&str> = links.iter().map(|l| l.url.as_str()).collect();
    assert!(
        urls.iter().any(|u| u.ends_with("ubuntu-22.04.iso")),
        "iso kept"
    );
    assert!(urls.iter().any(|u| u.ends_with("movie-1080p.mp4")));
    assert!(urls.iter().any(|u| u.ends_with("movie-720p.mp4")));
    assert!(urls.iter().any(|u| u.ends_with("album.zip")));
    assert!(urls.iter().any(|u| u.ends_with("track.mp3")));
    assert!(urls.iter().any(|u| u.ends_with("book.pdf")));
    // SRT is NOT in the include list by default — must be dropped.
    assert!(
        !urls.iter().any(|u| u.ends_with("subtitles.srt")),
        "srt not in defaults: must be dropped"
    );
    // HTML assets must be dropped.
    assert!(!urls.iter().any(|u| u.ends_with("logo.png")));
    assert!(!urls.iter().any(|u| u.ends_with("style.css")));
    assert!(!urls.iter().any(|u| u.ends_with("script.js")));
}

#[tokio::test]
async fn categories_are_inferred_from_extension() {
    let port = spawn_page_server(HTML);
    let url = format!("http://127.0.0.1:{port}/page.html");
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let state_dir = dir.path().join("state");
    std::fs::create_dir_all(&state_dir).unwrap();
    let (mgr, _rx) = DownloadManager::new(&db_path, state_dir);
    let links = mgr.grab_site(&url, 100).await.unwrap();
    let cat_for = |suffix: &str| {
        links
            .iter()
            .find(|l| l.url.ends_with(suffix))
            .and_then(|l| l.category.clone())
            .unwrap_or_default()
    };
    assert_eq!(cat_for("ubuntu-22.04.iso"), "compressed"); // iso → compressed in the helper
    assert_eq!(cat_for("movie-1080p.mp4"), "video");
    assert_eq!(cat_for("track.mp3"), "music");
    assert_eq!(cat_for("book.pdf"), "documents");
    assert_eq!(cat_for("album.zip"), "compressed");
}

#[tokio::test]
async fn custom_include_extensions_acts_as_allowlist() {
    let port = spawn_page_server(HTML);
    let url = format!("http://127.0.0.1:{port}/page.html");
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let state_dir = dir.path().join("state");
    std::fs::create_dir_all(&state_dir).unwrap();
    let (mgr, _rx) = DownloadManager::new(&db_path, state_dir);
    // Only allow mp4 + zip — every other extension must be dropped.
    let rules = GrabRules {
        include_extensions: vec!["mp4".into(), "zip".into()],
        ..GrabRules::sensible_defaults()
    };
    let links = mgr.grab_site_with_rules(&url, 100, &rules).await.unwrap();
    let exts: std::collections::HashSet<String> = links.iter().map(|l| l.ext.clone()).collect();
    for e in &exts {
        assert!(e == "mp4" || e == "zip", "unexpected ext: {e}");
    }
    assert!(exts.contains("mp4"));
    assert!(exts.contains("zip"));
}

#[tokio::test]
async fn exclude_extensions_take_priority_over_include() {
    let port = spawn_page_server(HTML);
    let url = format!("http://127.0.0.1:{port}/page.html");
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let state_dir = dir.path().join("state");
    std::fs::create_dir_all(&state_dir).unwrap();
    let (mgr, _rx) = DownloadManager::new(&db_path, state_dir);
    let rules = GrabRules {
        include_extensions: vec!["mp4".into(), "zip".into()],
        exclude_extensions: vec!["zip".into()], // zip excluded → only mp4 remains
        ..GrabRules::sensible_defaults()
    };
    let links = mgr.grab_site_with_rules(&url, 100, &rules).await.unwrap();
    assert!(!links.iter().any(|l| l.ext == "zip"));
    assert!(links.iter().any(|l| l.ext == "mp4"));
}

#[tokio::test]
async fn exclude_domains_drops_external_hosts() {
    let port = spawn_page_server(HTML);
    let url = format!("http://127.0.0.1:{port}/page.html");
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let state_dir = dir.path().join("state");
    std::fs::create_dir_all(&state_dir).unwrap();
    let (mgr, _rx) = DownloadManager::new(&db_path, state_dir);
    let rules = GrabRules {
        exclude_domains: vec!["other.com".into()],
        ..GrabRules::sensible_defaults()
    };
    let links = mgr.grab_site_with_rules(&url, 100, &rules).await.unwrap();
    assert!(!links.iter().any(|l| l.url.contains("other.com")));
    // everything from example.com (or 127.0.0.1) is still there
    assert!(links.iter().any(|l| l.url.contains("example.com")));
}

#[tokio::test]
async fn relative_urls_are_resolved_against_page() {
    let port = spawn_page_server(HTML);
    let url = format!("http://127.0.0.1:{port}/page.html");
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let state_dir = dir.path().join("state");
    std::fs::create_dir_all(&state_dir).unwrap();
    let (mgr, _rx) = DownloadManager::new(&db_path, state_dir);
    let links = mgr.grab_site(&url, 100).await.unwrap();
    // "/relative/relative.zip" should resolve against the page
    assert!(links
        .iter()
        .any(|l| l.url == format!("http://127.0.0.1:{port}/relative/relative.zip")));
    // "//cdn.example.com/files/cdn.zip" should inherit the http scheme
    assert!(links
        .iter()
        .any(|l| l.url == "http://cdn.example.com/files/cdn.zip"));
}

#[tokio::test]
async fn duplicate_links_are_deduped() {
    let html = r#"
<html><body>
<a href="https://example.com/files/a.zip">a</a>
<a href="https://example.com/files/a.zip">a again</a>
<a href="https://example.com/files/b.zip">b</a>
<a href="https://example.com/files/b.zip">b again</a>
</body></html>
"#;
    let port = spawn_page_server(html);
    let url = format!("http://127.0.0.1:{port}/page.html");
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let state_dir = dir.path().join("state");
    std::fs::create_dir_all(&state_dir).unwrap();
    let (mgr, _rx) = DownloadManager::new(&db_path, state_dir);
    let links = mgr.grab_site(&url, 100).await.unwrap();
    let mut urls: Vec<&str> = links.iter().map(|l| l.url.as_str()).collect();
    urls.sort();
    urls.dedup();
    assert_eq!(urls.len(), links.len(), "duplicates must be deduped");
    assert_eq!(links.len(), 2);
}

#[tokio::test]
async fn matched_reason_is_set_for_kept_links() {
    let port = spawn_page_server(HTML);
    let url = format!("http://127.0.0.1:{port}/page.html");
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let state_dir = dir.path().join("state");
    std::fs::create_dir_all(&state_dir).unwrap();
    let (mgr, _rx) = DownloadManager::new(&db_path, state_dir);
    let rules = GrabRules {
        include_extensions: vec!["mp4".into()],
        ..GrabRules::sensible_defaults()
    };
    let links = mgr.grab_site_with_rules(&url, 100, &rules).await.unwrap();
    assert!(links.iter().all(|l| !l.matched.is_empty()));
    // The mp4s are in the include list → "include:ext"
    assert!(links.iter().all(|l| l.matched == "include:ext"));
}

#[tokio::test]
async fn non_success_status_returns_error() {
    // A listener that accepts and immediately closes gives a
    // malformed response — the grabber should error out rather
    // than return a phantom empty list.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let _ = stream.write_all(
                b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            );
        }
    });
    let url = format!("http://127.0.0.1:{port}/page.html");
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let state_dir = dir.path().join("state");
    std::fs::create_dir_all(&state_dir).unwrap();
    let (mgr, _rx) = DownloadManager::new(&db_path, state_dir);
    let res = mgr.grab_site(&url, 100).await;
    assert!(
        res.is_err(),
        "404 must surface as an error, not an empty list"
    );
}
