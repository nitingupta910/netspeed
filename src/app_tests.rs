use crate::app::{App, AppState, HISTORY_SIZE};

fn make_app() -> App {
    App::new(
        "eth0".to_string(),
        vec!["eth0".to_string(), "wlan0".to_string()],
    )
}

// ── App::new ─────────────────────────────────────────────────────────────────

#[test]
fn new_initial_state_is_monitoring() {
    assert_eq!(make_app().state, AppState::Monitoring);
}

#[test]
fn new_counters_start_at_zero() {
    let app = make_app();
    assert_eq!(app.download_speed, 0.0);
    assert_eq!(app.upload_speed, 0.0);
    assert_eq!(app.peak_download, 0.0);
    assert_eq!(app.peak_upload, 0.0);
    assert_eq!(app.total_rx, 0);
    assert_eq!(app.total_tx, 0);
}

#[test]
fn new_history_starts_empty() {
    let app = make_app();
    assert!(app.download_history.is_empty());
    assert!(app.upload_history.is_empty());
}

#[test]
fn new_optional_fields_are_none() {
    let app = make_app();
    assert!(app.speed_test_result.is_none());
    assert!(app.speed_test_progress.is_none());
    assert!(!app.interface_selector_open);
}

#[test]
fn new_finds_correct_interface_index_first() {
    assert_eq!(make_app().selected_interface_idx, 0);
}

#[test]
fn new_finds_correct_interface_index_second() {
    let app = App::new(
        "wlan0".to_string(),
        vec!["eth0".to_string(), "wlan0".to_string()],
    );
    assert_eq!(app.selected_interface_idx, 1);
}

#[test]
fn new_defaults_idx_to_zero_when_iface_not_in_list() {
    let app = App::new(
        "tun0".to_string(),
        vec!["eth0".to_string(), "wlan0".to_string()],
    );
    assert_eq!(app.selected_interface_idx, 0);
}

#[test]
fn new_works_with_empty_interface_list() {
    let app = App::new("eth0".to_string(), vec![]);
    assert_eq!(app.selected_interface_idx, 0);
}

// ── push_speeds ───────────────────────────────────────────────────────────────

#[test]
fn push_speeds_updates_current_speeds() {
    let mut app = make_app();
    app.push_speeds(42.5, 11.1);
    assert_eq!(app.download_speed, 42.5);
    assert_eq!(app.upload_speed, 11.1);
}

#[test]
fn push_speeds_updates_peaks_on_higher_value() {
    let mut app = make_app();
    app.push_speeds(50.0, 10.0);
    app.push_speeds(100.0, 20.0);
    assert_eq!(app.peak_download, 100.0);
    assert_eq!(app.peak_upload, 20.0);
}

#[test]
fn push_speeds_does_not_lower_peak() {
    let mut app = make_app();
    app.push_speeds(100.0, 20.0);
    app.push_speeds(25.0, 5.0);
    assert_eq!(app.peak_download, 100.0);
    assert_eq!(app.peak_upload, 20.0);
}

#[test]
fn push_speeds_peak_tracks_independently() {
    let mut app = make_app();
    app.push_speeds(80.0, 5.0);
    app.push_speeds(20.0, 30.0);
    assert_eq!(app.peak_download, 80.0); // download peak from first call
    assert_eq!(app.peak_upload, 30.0);   // upload peak from second call
}

#[test]
fn push_speeds_appends_to_history() {
    let mut app = make_app();
    app.push_speeds(10.0, 1.0);
    app.push_speeds(20.0, 2.0);
    app.push_speeds(30.0, 3.0);
    let dl: Vec<f64> = app.download_history.iter().cloned().collect();
    assert_eq!(dl, vec![10.0, 20.0, 30.0]);
}

#[test]
fn push_speeds_history_is_oldest_first() {
    let mut app = make_app();
    for i in 1u64..=5 {
        app.push_speeds(i as f64, 0.0);
    }
    assert_eq!(*app.download_history.front().unwrap(), 1.0);
    assert_eq!(*app.download_history.back().unwrap(), 5.0);
}

#[test]
fn push_speeds_caps_at_history_size() {
    let mut app = make_app();
    for i in 0..HISTORY_SIZE + 10 {
        app.push_speeds(i as f64, i as f64);
    }
    assert_eq!(app.download_history.len(), HISTORY_SIZE);
    assert_eq!(app.upload_history.len(), HISTORY_SIZE);
}

#[test]
fn push_speeds_evicts_oldest_when_full() {
    let mut app = make_app();
    for i in 0..HISTORY_SIZE {
        app.push_speeds(i as f64, 0.0);
    }
    // 0.0 should be the front
    assert_eq!(*app.download_history.front().unwrap(), 0.0);

    app.push_speeds(999.0, 0.0);

    // 0.0 evicted; 1.0 is now the front
    assert_eq!(*app.download_history.front().unwrap(), 1.0);
    assert_eq!(*app.download_history.back().unwrap(), 999.0);
}

// ── reset_for_interface ───────────────────────────────────────────────────────

#[test]
fn reset_clears_speeds_and_peaks() {
    let mut app = make_app();
    app.push_speeds(100.0, 50.0);
    app.reset_for_interface();
    assert_eq!(app.download_speed, 0.0);
    assert_eq!(app.upload_speed, 0.0);
    assert_eq!(app.peak_download, 0.0);
    assert_eq!(app.peak_upload, 0.0);
}

#[test]
fn reset_clears_history() {
    let mut app = make_app();
    for i in 0..10 {
        app.push_speeds(i as f64, i as f64);
    }
    app.reset_for_interface();
    assert!(app.download_history.is_empty());
    assert!(app.upload_history.is_empty());
}

#[test]
fn reset_clears_byte_totals() {
    let mut app = make_app();
    app.total_rx = 1_000_000;
    app.total_tx = 500_000;
    app.reset_for_interface();
    assert_eq!(app.total_rx, 0);
    assert_eq!(app.total_tx, 0);
}

#[test]
fn reset_does_not_change_interface_name() {
    let mut app = make_app();
    app.interface = "wlan0".to_string();
    app.reset_for_interface();
    assert_eq!(app.interface, "wlan0");
}

// ── AppState ──────────────────────────────────────────────────────────────────

#[test]
fn app_state_variants_are_distinct() {
    assert_ne!(AppState::Monitoring, AppState::SpeedTesting);
    assert_ne!(AppState::SpeedTesting, AppState::SpeedTestDone);
    assert_ne!(AppState::Monitoring, AppState::SpeedTestDone);
}
