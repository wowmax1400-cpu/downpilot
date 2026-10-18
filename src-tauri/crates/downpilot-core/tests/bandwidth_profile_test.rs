use downpilot_core::engine::DownloadManager;
use tempfile::tempdir;

#[test]
fn bandwidth_profile_overrides_and_restores_queue_speed() {
    use downpilot_core::models::Schedule;
    let tmp = tempdir().unwrap();
    let db_path = tmp.path().join("test.db");
    let state_dir = tmp.path().join("state");
    std::fs::create_dir_all(&state_dir).unwrap();
    let (manager, _rx) = DownloadManager::new(&db_path, state_dir.clone());

    let baseline = manager.effective_queue_speed_kbps("general");
    assert_eq!(baseline, 0, "default cap should be 0 (unlimited)");

    let sch = Schedule {
        id: 0,
        name: "Night".to_string(),
        queue: String::new(),
        enabled: true,
        kind: "bandwidth".to_string(),
        start_time: "00:00".to_string(),
        end_time: "23:59".to_string(),
        days: 127,
        action: "bandwidth".to_string(),
        bandwidth_kbps: 1234,
        on_complete: "none".to_string(),
        on_complete_command: String::new(),
        priority: 10,
        overrides_json: r#"{"general":1234}"#.to_string(),
    };
    let id = manager.save_schedule(sch).expect("save_schedule ok");
    assert!(id > 0);

    manager.enforce_bandwidth_profiles();
    let active = manager.active_profiles();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].schedule_id, id);
    assert!(active[0].affected_queues.iter().any(|q| q == "general"));

    let eff = manager.effective_queue_speed_kbps("general");
    assert_eq!(
        eff, 1234,
        "override should set effective speed to 1234 kbps"
    );

    let mut sch2 = manager
        .list_schedules()
        .into_iter()
        .find(|s| s.id == id)
        .expect("schedule present");
    sch2.enabled = false;
    manager.save_schedule(sch2).expect("re-save ok");
    manager.enforce_bandwidth_profiles();
    let active = manager.active_profiles();
    assert!(active.is_empty());
    let eff2 = manager.effective_queue_speed_kbps("general");
    assert_eq!(eff2, 0, "disabling should restore baseline (unlimited)");
}

#[test]
fn overlapping_profiles_resolve_by_priority() {
    use downpilot_core::models::Schedule;
    let tmp = tempdir().unwrap();
    let db_path = tmp.path().join("test.db");
    let state_dir = tmp.path().join("state");
    std::fs::create_dir_all(&state_dir).unwrap();
    let (manager, _rx) = DownloadManager::new(&db_path, state_dir.clone());

    let lo = Schedule {
        id: 0,
        name: "Day".to_string(),
        queue: String::new(),
        enabled: true,
        kind: "bandwidth".to_string(),
        start_time: "00:00".to_string(),
        end_time: "23:59".to_string(),
        days: 127,
        action: "bandwidth".to_string(),
        bandwidth_kbps: 1000,
        on_complete: "none".to_string(),
        on_complete_command: String::new(),
        priority: 1,
        overrides_json: r#"{"general":1000}"#.to_string(),
    };
    manager.save_schedule(lo).unwrap();
    let hi = Schedule {
        id: 0,
        name: "Night".to_string(),
        queue: String::new(),
        enabled: true,
        kind: "bandwidth".to_string(),
        start_time: "00:00".to_string(),
        end_time: "23:59".to_string(),
        days: 127,
        action: "bandwidth".to_string(),
        bandwidth_kbps: 5000,
        on_complete: "none".to_string(),
        on_complete_command: String::new(),
        priority: 100,
        overrides_json: r#"{"general":5000}"#.to_string(),
    };
    manager.save_schedule(hi).unwrap();

    manager.enforce_bandwidth_profiles();
    let eff = manager.effective_queue_speed_kbps("general");
    assert_eq!(eff, 5000, "higher priority wins on overlap");
}

#[test]
fn per_queue_auth_metadata_round_trips() {
    use downpilot_core::models::DownloadQueue;
    let tmp = tempdir().unwrap();
    let db_path = tmp.path().join("test.db");
    let state_dir = tmp.path().join("state");
    std::fs::create_dir_all(&state_dir).unwrap();
    let (manager, _rx) = DownloadManager::new(&db_path, state_dir.clone());

    let q = DownloadQueue {
        name: "alpha".to_string(),
        label: "Alpha".to_string(),
        proxy_url: "http://127.0.0.1:8080".to_string(),
        user_agent: "DownPilot-Test/1.0".to_string(),
        cookies: "session=abc".to_string(),
        auth_ref: "alpha:user:secret".to_string(),
        auth_user: "alice".to_string(),
        has_password: true,
        ..Default::default()
    };
    manager.create_queue(q.clone()).expect("create_queue");

    let list = manager.list_queues();
    let back = list
        .iter()
        .find(|x| x.name == "alpha")
        .expect("alpha present");
    assert_eq!(back.proxy_url, "http://127.0.0.1:8080");
    assert_eq!(back.user_agent, "DownPilot-Test/1.0");
    assert_eq!(back.cookies, "session=abc");
    assert_eq!(back.auth_ref, "alpha:user:secret");
    assert_eq!(back.auth_user, "alice");
    assert!(back.has_password);

    let store = downpilot_core::credentials::CredentialStore::new(&state_dir.join("credentials"));
    store.set("alpha", "user:secret").expect("set password");
    assert!(store.is_configured("alpha"));
    store.clear("alpha").unwrap();
    assert!(!store.is_configured("alpha"));
}
