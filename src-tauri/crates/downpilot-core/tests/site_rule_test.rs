//! Deterministic tests for Site Rule precedence and persistence.

use downpilot_core::engine::DownloadManager;
use downpilot_core::models::{NewDownload, SiteRule};
use std::sync::Arc;

fn new_mgr() -> (Arc<DownloadManager>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let state_dir = dir.path().join("state");
    std::fs::create_dir_all(&state_dir).unwrap();
    let (mgr, _rx) = DownloadManager::new(&db_path, state_dir);
    (mgr, dir)
}

fn rule(domain: &str, queue: &str) -> SiteRule {
    SiteRule {
        domain: domain.to_string(),
        queue: queue.to_string(),
        ..Default::default()
    }
}

#[test]
fn exact_host_wins_over_subdomain_pattern() {
    let (mgr, _dir) = new_mgr();
    mgr.save_site(rule("example.com", "general")).unwrap();
    mgr.save_site(rule("cdn.example.com", "video")).unwrap();
    assert_eq!(
        mgr.test_site_match("https://cdn.example.com/file.mp4").unwrap().queue,
        "video"
    );
    assert_eq!(
        mgr.test_site_match("https://static.example.com/img.png").unwrap().queue,
        "general"
    );
    assert!(mgr.test_site_match("https://other.com/x.zip").is_none());
}

#[test]
fn hostname_aware_matching_rejects_lookalikes() {
    let (mgr, _dir) = new_mgr();
    mgr.save_site(rule("example.com", "general")).unwrap();
    assert!(mgr.test_site_match("https://notexample.com/x.zip").is_none());
    assert!(mgr.test_site_match("https://example.com.evil.tld/x.zip").is_none());
    assert!(mgr.test_site_match("https://www.example.com/x.zip").is_some());
    assert!(mgr.test_site_match("https://a.b.c.example.com/x.zip").is_some());
}

#[test]
fn most_specific_parent_wins() {
    let (mgr, _dir) = new_mgr();
    mgr.save_site(rule("example.com", "general")).unwrap();
    mgr.save_site(rule("cdn.example.com", "video")).unwrap();
    mgr.save_site(rule("static.cdn.example.com", "image")).unwrap();
    assert_eq!(
        mgr.test_site_match("https://static.cdn.example.com/a.png").unwrap().queue,
        "image"
    );
    assert_eq!(
        mgr.test_site_match("https://other.cdn.example.com/a.png").unwrap().queue,
        "video"
    );
    assert_eq!(
        mgr.test_site_match("https://docs.example.com/a.html").unwrap().queue,
        "general"
    );
}

#[test]
fn global_rule_is_fallback_only() {
    let (mgr, _dir) = new_mgr();
    mgr.save_site(rule("example.com", "general")).unwrap();
    mgr.save_site(rule("", "fallback")).unwrap();
    assert_eq!(
        mgr.test_site_match("https://example.com/x").unwrap().queue,
        "general"
    );
    assert_eq!(
        mgr.test_site_match("https://other.com/x").unwrap().queue,
        "fallback"
    );
}

#[test]
fn matching_is_case_insensitive() {
    let (mgr, _dir) = new_mgr();
    mgr.save_site(rule("Example.COM", "general")).unwrap();
    assert!(mgr.test_site_match("https://WWW.example.com/x.zip").is_some());
    assert!(mgr.test_site_match("https://www.EXAMPLE.com/x.zip").is_some());
}

#[test]
fn port_in_url_is_stripped() {
    let (mgr, _dir) = new_mgr();
    mgr.save_site(rule("example.com", "general")).unwrap();
    assert_eq!(
        mgr.test_site_match("https://example.com:8443/x.zip").unwrap().queue,
        "general"
    );
}

#[test]
fn persistence_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let state_dir = dir.path().join("state");
    std::fs::create_dir_all(&state_dir).unwrap();
    {
        let (mgr, _rx) = DownloadManager::new(&db_path, state_dir.clone());
        mgr.save_site(rule("example.com", "video")).unwrap();
        mgr.save_site(rule("", "fallback")).unwrap();
    }
    {
        let (mgr, _rx) = DownloadManager::new(&db_path, state_dir);
        assert_eq!(
            mgr.test_site_match("https://www.example.com/x.zip").unwrap().queue,
            "video"
        );
        assert_eq!(
            mgr.test_site_match("https://other.com/x.zip").unwrap().queue,
            "fallback"
        );
    }
}

#[test]
fn delete_removes_rule_only() {
    let (mgr, _dir) = new_mgr();
    mgr.save_site(rule("example.com", "video")).unwrap();
    mgr.save_site(rule("other.com", "general")).unwrap();
    mgr.delete_site("example.com").unwrap();
    assert!(mgr.test_site_match("https://example.com/x").is_none());
    assert!(mgr.test_site_match("https://other.com/x").is_some());
}

#[tokio::test]
async fn category_and_user_agent_apply_at_insert() {
    let (mgr, _dir) = new_mgr();
    mgr.save_site(SiteRule {
        domain: "rules-test.example".to_string(),
        queue: "video".to_string(),
        save_dir: "/tmp/rules-test".to_string(),
        category: "video".to_string(),
        user_agent: "DownPilot-Test/1.0".to_string(),
        ..Default::default()
    })
    .unwrap();
    let nd = NewDownload {
        url: "https://rules-test.example/foo.zip".to_string(),
        filename: Some("foo.zip".to_string()),
        save_dir: None,
        threads: None,
        format_id: None,
        webpage_url: None,
        referer: None,
        user_agent: None,
        cookies: None,
        expected_sha256: None,
        category: Some("other".to_string()),
        queue: Some("general".to_string()),
        description: None,
        max_retries: None,
        retry_delay_ms: None,
        conn_timeout_ms: None,
        read_timeout_ms: None,
        checksum_algorithm: None,
        on_complete: None,
        start_immediately: false,
        browser_dedup_id: String::new(),
        skip_probe: true,
    };
    let _ = mgr.add(nd).await;
    let list = mgr.list();
    let task = list
        .iter()
        .find(|t| t.url == "https://rules-test.example/foo.zip")
        .expect("row should exist");
    assert_eq!(task.category, "video");
    assert_eq!(task.queue, "video");
    assert_eq!(task.save_dir, "/tmp/rules-test");
    assert_eq!(task.user_agent.as_deref(), Some("DownPilot-Test/1.0"));
}
